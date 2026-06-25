use crate::config::MemoryConfig;
use crate::consolidation::ConsolidationEngine;
use crate::error::{MemoryError, Result};
use crate::extraction::{ExtractionConfig, ExtractionEngine, LlmClient};
use crate::models::{HybridWeights, Memory, MemoryScope, SearchQuery, SearchResult};
use crate::retrieval::RetrievalEngine;
use crate::storage::{SqliteStore, TextIndex, VectorStore};
use std::sync::Arc;

#[allow(dead_code)]
pub struct MemoryService {
    sqlite: Arc<SqliteStore>,
    vector_store: Arc<VectorStore>,
    text_index: Arc<TextIndex>,
    llm_client: Arc<LlmClient>,
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

        let llm_client = Arc::new(LlmClient::new(
            &config.llm_api_base,
            &config.llm_api_key,
            config.embedding_dim,
        ));

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
            config.embedding_model.clone(),
            config.embedding_dim,
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
            config.decay_mu,
        ));

        Ok(Self {
            sqlite,
            vector_store,
            text_index,
            llm_client,
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
                Ok(Some(mem)) => added.push(mem),
                Ok(None) => {} // deduplicated
                Err(e) => {
                    tracing::warn!("Memory consolidation degraded (skipping chunk): {e}");
                }
            }
        }

        Ok(added)
    }

    /// Search memories using Hybrid retrieval
    pub async fn search_memories(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.retrieval.search(query).await
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

    /// Soft-delete memory by ID: marks status='deleted' in SQLite, removes
    /// vector/text index entries immediately. The SQLite row is preserved
    /// for potential recovery via undelete_memory.
    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let Some(memory) = self.sqlite.get_memory(id).await? else {
            return Ok(false);
        };
        // Only active memories can be deleted
        if memory.status != "active" {
            return Ok(false);
        }
        let deleted = self.sqlite.delete_memory(id).await?;
        if deleted {
            self.vector_store.remove(memory.vector_id)?;
            self.text_index.delete_document(id)?;
            self.sqlite.unlink_memory_from_entities(id).await?;
        }
        Ok(deleted)
    }

    /// Restore a soft-deleted memory. Note: vector/text index entries are not
    /// automatically re-created — the memory will be found by ID/listing but
    /// will not appear in hybrid search results until re-embedding occurs.
    pub async fn undelete_memory(&self, id: &str) -> Result<bool> {
        self.sqlite.undelete_memory(id).await
    }

    /// Permanently remove all soft-deleted memories. Returns count of purged rows.
    /// Vector/text cleanup is applied if entries still exist.
    pub async fn compact_deleted(&self) -> Result<i64> {
        let deleted_list = self.sqlite.get_deleted_memories(10_000).await?;
        let _count = deleted_list.len() as i64;
        for mem in &deleted_list {
            // Attempt vector/text cleanup (may already be removed by delete flow)
            let _ = self.vector_store.remove(mem.vector_id);
            let _ = self.text_index.delete_document(&mem.id);
        }
        self.sqlite.compact_deleted().await
    }

    /// Consolidate memories (decay calculations)
    pub async fn consolidate_memories(
        &self,
        scope: Option<MemoryScope>,
        project_id: Option<&str>,
    ) -> Result<()> {
        self.consolidation
            .batch_consolidate(scope, project_id)
            .await
    }

    /// Count soft-deleted memories
    pub async fn count_deleted(&self) -> Result<i64> {
        self.sqlite.count_by_status("deleted").await
    }

    /// Get stats (includes active_memories, deleted_memories, vector_count, unresolved_repairs)
    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let mut stats = self.sqlite.get_stats().await?;
        if let Some(object) = stats.as_object_mut() {
            object.insert("vector_count".to_string(), self.vector_store.size().into());
            object.insert(
                "unresolved_repairs".to_string(),
                self.sqlite.count_unresolved_repairs().await?.into(),
            );
        }
        Ok(stats)
    }

    /// Run diagnostic checks and optionally repair index consistency issues.
    ///
    /// Returns a JSON summary of:
    /// - `issues_found`: number of issues detected
    /// - `issues_fixed`: number of issues automatically repaired
    /// - `details`: list of per-issue descriptions
    /// - `stats_before` / `stats_after`: memory/vector/entity counts before and after repair
    pub async fn repair_indexes(&self) -> Result<serde_json::Value> {
        let stats_before = self.get_stats().await?;
        let mut details: Vec<serde_json::Value> = Vec::new();
        let mut issues_found = 0i64;
        let mut issues_fixed = 0i64;

        // 1. Entity reference cleanup
        let cleaned_refs = self.sqlite.repair_entity_references().await?;
        if cleaned_refs > 0 {
            issues_found += cleaned_refs;
            issues_fixed += cleaned_refs;
            details.push(serde_json::json!({
                "issue": "orphan_entity_references",
                "severity": "warning",
                "fixed": cleaned_refs,
                "detail": format!("Removed {cleaned_refs} entity references to deleted memories")
            }));
        }

        // 2. Vector store consistency: count active memories vs vector store size
        let active_count = self.sqlite.count_by_status("active").await?;
        let vector_count = self.vector_store.size() as i64;
        if active_count != vector_count {
            let diff = (active_count - vector_count).abs();
            issues_found += diff;
            details.push(serde_json::json!({
                "issue": "memory_vector_count_mismatch",
                "severity": "warning",
                "detail": format!(
                    "Active memories: {active_count}, Vector index entries: {vector_count}. Diff: {diff}"
                )
            }));

            // Log to repair queue
            self.sqlite
                .enqueue_repair_issue(
                    None,
                    "memory_vector_count_mismatch",
                    &format!("active_memories={active_count}, vector_entries={vector_count}"),
                )
                .await?;
        }

        let stats_after = self.get_stats().await?;

        Ok(serde_json::json!({
            "issues_found": issues_found,
            "issues_fixed": issues_fixed,
            "details": details,
            "stats_before": stats_before,
            "stats_after": stats_after,
        }))
    }
}
