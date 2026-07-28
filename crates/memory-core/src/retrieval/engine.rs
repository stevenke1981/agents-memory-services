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
        let fetch_k = query.top_k.saturating_mul(4);

        // Run both retrieval paths independently. Either side may fail without taking
        // the other side down, which keeps memory retrieval fail-open for the host app.
        let (embed_result, bm25_result) = tokio::join!(
            self.llm_client.embed(&query.query, &self.embedding_model),
            async { self.bm25.search_normalized(&query.query, fetch_k) },
        );

        let bm25_results = match bm25_result {
            Ok(results) => results,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "BM25 retrieval degraded; continuing with semantic search"
                );
                Vec::new()
            }
        };

        let sem_results = match embed_result {
            Ok(query_vec) => match self.semantic.search(&query_vec, fetch_k) {
                Ok(results) => results,
                Err(error) => {
                    tracing::warn!(
                        %error,
                        "semantic vector search degraded; continuing with BM25"
                    );
                    Vec::new()
                }
            },
            Err(error) => {
                tracing::warn!(%error, "query embedding degraded; continuing with BM25");
                Vec::new()
            }
        };

        if sem_results.is_empty() && bm25_results.is_empty() {
            tracing::debug!("all retrieval paths returned no candidates");
            return Ok(Vec::new());
        }

        // Fetch all candidates from SQLite.
        let mut candidate_ids = std::collections::HashSet::new();

        // Retrieve memory IDs for semantic results by searching SQLite for matching vector_ids.
        let sem_vector_ids: Vec<i64> = sem_results.iter().map(|(vid, _)| *vid).collect();
        let sem_memories = if !sem_vector_ids.is_empty() {
            self.sqlite
                .get_memories_by_vector_ids(&sem_vector_ids)
                .await?
        } else {
            Vec::new()
        };

        for memory in &sem_memories {
            candidate_ids.insert(memory.id.clone());
        }
        for (memory_id, _) in &bm25_results {
            candidate_ids.insert(memory_id.clone());
        }

        if candidate_ids.is_empty() {
            return Ok(Vec::new());
        }

        let candidate_ids_vec: Vec<String> = candidate_ids.into_iter().collect();
        let all_memories = self.sqlite.get_by_ids(&candidate_ids_vec).await?;

        // Fusion scoring.
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut scored = Vec::new();

        for memory in all_memories {
            if !self.passes_filters(&memory, query) {
                continue;
            }

            // Cosine similarity is clamped so a negative vector score cannot reduce
            // otherwise valid lexical matches.
            let semantic_score = sem_results
                .iter()
                .find(|(vector_id, _)| *vector_id == memory.vector_id)
                .map(|(_, score)| (*score as f64).clamp(0.0, 1.0))
                .unwrap_or(0.0);

            let bm25_score = bm25_results
                .iter()
                .find(|(memory_id, _)| memory_id == &memory.id)
                .map(|(_, score)| (*score as f64).clamp(0.0, 1.0))
                .unwrap_or(0.0);

            let elapsed_ms = now_ms.saturating_sub(memory.last_accessed_at);
            let elapsed_days = elapsed_ms as f64 / 86_400_000.0;
            let temporal_score = (-self.temporal_mu * elapsed_days)
                .exp()
                .clamp(0.0, 1.0);

            let base_score = weights.semantic * semantic_score
                + weights.bm25 * bm25_score
                + weights.temporal * temporal_score;

            // Importance is a small quality multiplier, not a substitute for relevance.
            let importance_multiplier =
                0.85 + 0.15 * memory.importance_score.clamp(0.0, 1.0);
            let final_score = (base_score * importance_multiplier).clamp(0.0, 1.0);

            scored.push(SearchResult {
                memory,
                score_final: final_score,
                score_semantic: semantic_score,
                score_bm25: bm25_score,
                score_temporal: temporal_score,
            });
        }

        scored.sort_by(|left, right| {
            right
                .score_final
                .partial_cmp(&left.score_final)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(query.top_k);

        tracing::debug!(count = scored.len(), "hybrid search completed");

        // Update access statistics asynchronously for matched memories.
        let hit_ids: Vec<String> = scored
            .iter()
            .map(|result| result.memory.id.clone())
            .collect();
        if !hit_ids.is_empty() {
            let sqlite = self.sqlite.clone();
            tokio::spawn(async move {
                let _ = sqlite.update_access_stats(&hit_ids).await;
            });
        }

        Ok(scored)
    }

    fn passes_filters(&self, memory: &Memory, query: &SearchQuery) -> bool {
        if let Some(ref scope) = query.scope {
            if memory.scope != scope.as_str() {
                return false;
            }
        }

        if let Some(ref project_id) = query.project_id {
            if memory.project_id.as_ref() != Some(project_id) {
                return false;
            }
        }

        if let Some(ref categories) = query.categories {
            if !categories.is_empty()
                && !categories
                    .iter()
                    .any(|category| memory.category == category.as_str())
            {
                return false;
            }
        }

        if let Some(created_after) = query.created_after {
            if memory.created_at < created_after {
                return false;
            }
        }

        if let Some(min_importance) = query.min_importance {
            if memory.importance_score < min_importance {
                return false;
            }
        }

        if !query.include_decayed && memory.is_archived() {
            return false;
        }

        true
    }
}
