use crate::config::MemoryConfig;
use crate::consolidation::ConsolidationEngine;
use crate::error::{MemoryError, Result};
use crate::extraction::{ExtractionConfig, ExtractionEngine, LlmClient};
use crate::models::{HybridWeights, Memory, MemoryScope, SearchQuery, SearchResult};
use crate::retrieval::RetrievalEngine;
use crate::storage::{SqliteStore, TextIndex, VectorStore};
use std::sync::Arc;

pub struct MemoryService {
    config: MemoryConfig,
    sqlite: Arc<SqliteStore>,
    vector_store: Arc<VectorStore>,
    text_index: Arc<TextIndex>,
    _llm_client: Arc<LlmClient>,
    extraction: Arc<ExtractionEngine>,
    consolidation: Arc<ConsolidationEngine>,
    retrieval: Arc<RetrievalEngine>,
}

impl MemoryService {
    pub async fn new(config: MemoryConfig) -> Result<Self> {
        // Ensure parent directories exist for database and indexes
        if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(parent) = std::path::Path::new(&config.vector_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Some(parent) = std::path::Path::new(&config.tantivy_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let sqlite = Arc::new(SqliteStore::new(&config.db_path).await?);

        let vector_store = Arc::new(VectorStore::new(&config.vector_path, config.embedding_dim)?);

        let text_index = Arc::new(TextIndex::new(&config.tantivy_path)?);

        let llm_client = Arc::new(LlmClient::new(&config.llm_api_base, &config.embedding_api_base, &config.llm_api_key));

        let extraction_config = ExtractionConfig {
            model: config.extraction_model.clone(),
            max_tokens: config.extraction_max_tokens,
            temperature: 0.1,
            min_confidence: config.min_confidence,
            min_importance: config.min_importance,
        };

        let extraction = Arc::new(ExtractionEngine::new(
            llm_client.clone(),
            &config.embedding_model,
            extraction_config,
        ));

        let consolidation = Arc::new(ConsolidationEngine::new(
            sqlite.clone(),
            vector_store.clone(),
            text_index.clone(),
            config.dedup_threshold,
            config.near_dedup_threshold,
            config.decay_lambda,
        ));

        let default_weights = HybridWeights {
            semantic: 0.60,
            bm25: 0.30,
            temporal: 0.10,
        };

        let retrieval = Arc::new(RetrievalEngine::new(
            sqlite.clone(),
            vector_store.clone(),
            text_index.clone(),
            llm_client.clone(),
            &config.embedding_model,
            default_weights,
            config.temporal_mu,
        ));

        // Persist actual embedding metadata into system_config
        let _ = sqlite
            .set_system_config("vector_dimensions", &config.embedding_dim.to_string())
            .await;
        let _ = sqlite
            .set_system_config("embedding_model", &config.embedding_model)
            .await;

        Ok(Self {
            config: config.clone(),
            sqlite,
            vector_store,
            text_index,
            _llm_client: llm_client,
            extraction,
            consolidation,
            retrieval,
        })
    }

    /// Add memory from conversation content.
    ///
    /// Extraction errors are gracefully degraded: if the LLM extraction or embedding
    /// fails, a warning is logged and the call returns an empty result instead of
    /// propagating the error. This ensures the session is not interrupted by transient
    /// API failures or malformed extraction responses.
    #[tracing::instrument(skip(self, content, metadata), fields(content_len = %content.len(), scope = ?scope))]
    pub async fn add_memory(
        &self,
        content: &str,
        scope: MemoryScope,
        project_id: Option<String>,
        agent_id: Option<String>,
        session_id: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<Vec<Memory>> {
        // 1. Extract memory chunks from content (gracefully degraded)
        let extracted_chunks = match self.extraction.extract(content).await {
            Ok(chunks) => chunks,
            Err(e @ MemoryError::ExtractionFailed(_))
            | Err(e @ MemoryError::ExtractionParseFailed(_))
            | Err(e @ MemoryError::HttpClient(_)) => {
                tracing::warn!("Memory extraction degraded (will not block session): {e}");
                return Ok(Vec::new());
            }
            Err(e) => return Err(e),
        };

        // 2. Enforce max_records limit
        let current_count = self.sqlite.memory_count().await?;
        if current_count >= self.config.max_records as i64 {
            tracing::warn!(
                "Memory store at capacity ({} >= {}), rejecting new memory",
                current_count,
                self.config.max_records
            );
            return Ok(Vec::new());
        }

        // 3. Ensure session_stats row exists
        let _ = self
            .sqlite
            .ensure_session(&session_id, project_id.as_deref())
            .await;

        let extracted_count = extracted_chunks.len() as i64;
        let mut added_count = 0i64;
        let mut dedup_count = 0i64;
        let mut added = Vec::new();
        for chunk in extracted_chunks {
            // 2. Embed content (skip on failure)
            let vector = match self.extraction.embed(&chunk.content).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("Memory embedding degraded (skipping chunk): {e}");
                    continue;
                }
            };

            // 3. Consolidate and insert
            match self
                .consolidation
                .consolidate_single(
                    chunk,
                    vector,
                    scope.clone(),
                    project_id.clone(),
                    agent_id.clone(),
                    session_id.clone(),
                    metadata.clone(),
                )
                .await
            {
                Ok(Some(mem)) => {
                    added.push(mem);
                    added_count += 1;
                }
                Ok(None) => {
                    dedup_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Memory consolidation degraded (skipping chunk): {e}");
                }
            }
        }

        // Flush text index batch
        if let Err(e) = self.text_index.flush() {
            tracing::warn!("Failed to flush text index: {e}");
        }

        // Update session stats
        let _ = self
            .sqlite
            .update_session_stats(&session_id, extracted_count, added_count, dedup_count, 0, 0)
            .await;

        Ok(added)
    }

    /// Search memories using Hybrid retrieval
    pub async fn search_memories(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let results = self.retrieval.search(query).await?;
        // Track retrieval in session_stats if session_id is set
        if let Some(session_id) = query.session_id.as_deref() {
            let _ = self
                .sqlite
                .update_session_stats(session_id, 0, 0, 0, results.len() as i64, 0)
                .await;
        }
        Ok(results)
    }

    /// Retrieve memories with filters
    pub async fn get_memories(
        &self,
        ids: Option<Vec<String>>,
        scope: Option<MemoryScope>,
        project_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        if let Some(ids_list) = ids {
            self.sqlite.get_by_ids(&ids_list).await
        } else {
            let scope_str = scope.map(|s| s.as_str().to_string());
            self.sqlite
                .list_memories(scope_str.as_deref(), project_id.as_deref(), limit)
                .await
        }
    }

    /// Delete memory by ID
    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let Some(memory) = self.sqlite.get_memory(id).await? else {
            return Ok(false);
        };
        let deleted = self.sqlite.delete_memory(id).await?;
        if deleted {
            self.vector_store.remove(memory.vector_id)?;
            self.text_index.delete_document(id)?;
            self.sqlite.unlink_memory_from_entities(id).await?;
        }
        Ok(deleted)
    }

    /// Consolidate memories (decay calculations)
    #[tracing::instrument(skip(self))]
    pub async fn consolidate_memories(
        &self,
        scope: Option<MemoryScope>,
        project_id: Option<&str>,
    ) -> Result<()> {
        self.consolidation
            .batch_consolidate(scope, project_id)
            .await
    }

    /// Expose the consolidation engine for background scheduling
    pub fn consolidation_engine(&self) -> Arc<ConsolidationEngine> {
        self.consolidation.clone()
    }

    /// End a session by ID (sets ended_at timestamp)
    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        self.sqlite.end_session(session_id).await
    }

    /// Get stats
    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let mut stats = self.sqlite.get_stats().await?;
        if let Some(object) = stats.as_object_mut() {
            object.insert("vector_count".to_string(), self.vector_store.size().into());
        }
        Ok(stats)
    }
}
