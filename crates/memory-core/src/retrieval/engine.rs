use crate::error::Result;
use crate::extraction::LlmClient;
use crate::models::{HybridWeights, Memory, SearchQuery, SearchResult};
use crate::retrieval::{bm25::Bm25Retriever, semantic::SemanticRetriever};
use crate::storage::SqliteStore;
use std::sync::Arc;

pub struct RetrievalEngine {
    semantic: SemanticRetriever,
    bm25: Bm25Retriever,
    sqlite: Arc<SqliteStore>,
    llm_client: Arc<LlmClient>,
    embedding_model: String,
    default_weights: HybridWeights,
    temporal_mu: f64,
}

impl RetrievalEngine {
    pub fn new(
        sqlite: Arc<SqliteStore>,
        vector_store: Arc<crate::storage::VectorStore>,
        text_index: Arc<crate::storage::TextIndex>,
        llm_client: Arc<LlmClient>,
        embedding_model: &str,
        default_weights: HybridWeights,
        temporal_mu: f64,
    ) -> Self {
        Self {
            semantic: SemanticRetriever::new(vector_store),
            bm25: Bm25Retriever::new(text_index),
            sqlite,
            llm_client,
            embedding_model: embedding_model.to_string(),
            default_weights,
            temporal_mu,
        }
    }

    #[tracing::instrument(skip(self), fields(query = %query.query, top_k = query.top_k))]
    pub async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        query.validate()?;
        let weights = query
            .weights
            .clone()
            .unwrap_or_else(|| self.default_weights.clone());
        let fetch_k = query.top_k * 3;

        // 1. Run embedding and BM25 search in parallel (they have no dependency)
        let (embed_result, bm25_results) = tokio::join!(
            self.llm_client.embed(&query.query, &self.embedding_model),
            async { self.bm25.search_normalized(&query.query, fetch_k) },
        );

        let query_vec = embed_result?;
        let bm25_results = bm25_results?;

        let sem_results = self.semantic.search(&query_vec, fetch_k)?;

        // 2. Fetch all candidates from SQLite
        let mut candidate_ids = std::collections::HashSet::new();

        // Retrieve memory IDs for semantic results by searching SQLite for matching vector_ids
        let sem_vector_ids: Vec<i64> = sem_results.iter().map(|(vid, _)| *vid).collect();
        let sem_memories = if !sem_vector_ids.is_empty() {
            self.sqlite
                .get_memories_by_vector_ids(&sem_vector_ids)
                .await?
        } else {
            Vec::new()
        };

        for m in &sem_memories {
            candidate_ids.insert(m.id.clone());
        }
        for (mid, _) in &bm25_results {
            candidate_ids.insert(mid.clone());
        }

        let candidate_ids_vec: Vec<String> = candidate_ids.into_iter().collect();
        let all_memories = self.sqlite.get_by_ids(&candidate_ids_vec).await?;

        // 3. Fusion scoring
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut scored = Vec::new();

        for mem in all_memories {
            if !self.passes_filters(&mem, query) {
                continue;
            }

            // Semantic score
            let s_sem = sem_results
                .iter()
                .find(|(vid, _)| *vid == mem.vector_id)
                .map(|(_, score)| *score as f64)
                .unwrap_or(0.0);

            // BM25 score
            let s_bm25 = bm25_results
                .iter()
                .find(|(mid, _)| mid == &mem.id)
                .map(|(_, score)| *score as f64)
                .unwrap_or(0.0);

            // Temporal score
            let elapsed_ms = now_ms - mem.last_accessed_at;
            let elapsed_days = elapsed_ms as f64 / 86_400_000.0;
            let s_temp = (-self.temporal_mu * elapsed_days).exp();

            // Weighted combination
            let score_final =
                weights.semantic * s_sem + weights.bm25 * s_bm25 + weights.temporal * s_temp;

            scored.push(SearchResult {
                memory: mem,
                score_final,
                score_semantic: s_sem,
                score_bm25: s_bm25,
                score_temporal: s_temp,
            });
        }

        // Sort by final score descending
        scored.sort_by(|a, b| {
            b.score_final
                .partial_cmp(&a.score_final)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(query.top_k);

        tracing::debug!(count = scored.len(), "hybrid search completed");

        // 4. Update access statistics asynchronously for matched memories
        let hit_ids: Vec<String> = scored.iter().map(|r| r.memory.id.clone()).collect();
        if !hit_ids.is_empty() {
            let sqlite = self.sqlite.clone();
            tokio::spawn(async move {
                let _ = sqlite.update_access_stats(&hit_ids).await;
            });
        }

        Ok(scored)
    }

    fn passes_filters(&self, mem: &Memory, query: &SearchQuery) -> bool {
        // Scope filter
        if let Some(ref sc) = query.scope {
            if mem.scope != sc.as_str() {
                return false;
            }
        }

        // Project ID filter
        if let Some(ref pid) = query.project_id {
            if mem.project_id.as_ref() != Some(pid) {
                return false;
            }
        }

        // Categories filter
        if let Some(ref cats) = query.categories {
            if cats.is_empty() {
                // empty list means no filter
            } else {
                let mut found = false;
                for cat in cats {
                    if mem.category == cat.as_str() {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
            }
        }

        // Created after filter
        if let Some(created_after) = query.created_after {
            if mem.created_at < created_after {
                return false;
            }
        }

        // Min importance score filter
        if let Some(min_imp) = query.min_importance {
            if mem.importance_score < min_imp {
                return false;
            }
        }

        // Decayed filter
        if !query.include_decayed && mem.is_archived() {
            return false;
        }

        true
    }
}
