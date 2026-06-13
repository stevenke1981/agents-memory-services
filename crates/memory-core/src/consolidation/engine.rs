use crate::consolidation::decay::{calculate_retention, initial_stability, reinforce_stability};
use crate::consolidation::dedup::{entity_overlap, is_exact_duplicate, is_near_duplicate};
use crate::consolidation::entity::link_entities;
use crate::error::Result;
use crate::extraction::ExtractedMemory;
use crate::models::{Memory, MemoryScope};
use crate::storage::{SqliteStore, TextIndex, VectorStore};
use std::sync::Arc;
use uuid::Uuid;

pub struct ConsolidationEngine {
    sqlite: Arc<SqliteStore>,
    vector_store: Arc<VectorStore>,
    text_index: Arc<TextIndex>,
    dedup_threshold: f64,
    near_dedup_threshold: f64,
    decay_lambda: f64,
}

impl ConsolidationEngine {
    pub fn new(
        sqlite: Arc<SqliteStore>,
        vector_store: Arc<VectorStore>,
        text_index: Arc<TextIndex>,
        dedup_threshold: f64,
        near_dedup_threshold: f64,
        decay_lambda: f64,
    ) -> Self {
        Self {
            sqlite,
            vector_store,
            text_index,
            dedup_threshold,
            near_dedup_threshold,
            decay_lambda,
        }
    }

    /// Consolidates a single extracted memory.
    pub async fn consolidate_single(
        &self,
        ext: ExtractedMemory,
        vector: Vec<f32>,
        scope: MemoryScope,
        project_id: Option<String>,
        session_id: String,
        metadata_custom: Option<serde_json::Value>,
    ) -> Result<Option<Memory>> {
        let now_ms = chrono::Utc::now().timestamp_millis();

        // 1. Search top 5 similar candidates in vector store
        let candidates = self.vector_store.search(&vector, 5)?;

        // 2. Scan candidates for duplicates
        for (vector_id, similarity) in candidates {
            let similarity_f64 = similarity as f64;
            if is_exact_duplicate(similarity_f64, self.dedup_threshold) {
                // Exact duplicate
                // Retrieve the memory by vector_id from SQLite.
                // Wait! Since SQLite table has an index on vector_id, we can query by vector_id.
                // Let's find memory by vector_id in SQLite.
                if let Some(mem) = self.sqlite.get_memory_by_vector_id(vector_id).await? {
                    if !same_scope(&mem, &scope, project_id.as_deref()) {
                        continue;
                    }
                    // Update access stats
                    self.sqlite
                        .update_access_stats(std::slice::from_ref(&mem.id))
                        .await?;
                    return Ok(None); // Deduplicated
                }
            } else if is_near_duplicate(
                similarity_f64,
                self.dedup_threshold,
                self.near_dedup_threshold,
            ) {
                // Near duplicate: compare entity overlap
                if let Some(mem) = self.sqlite.get_memory_by_vector_id(vector_id).await? {
                    if !same_scope(&mem, &scope, project_id.as_deref()) {
                        continue;
                    }
                    let existing_entities: Vec<String> =
                        serde_json::from_str(&mem.entities).unwrap_or_default();
                    let overlap = entity_overlap(&ext.entities, &existing_entities);
                    if overlap > 0.5 {
                        // Synonym
                        self.sqlite
                            .update_access_stats(std::slice::from_ref(&mem.id))
                            .await?;
                        return Ok(None); // Deduplicated
                    }
                }
            }
        }

        // 3. Insert as a new memory
        let llm_score = ext.importance as f64 / 5.0;
        // Formula: 0.5 * llm_score + 0.3 * access_factor + 0.2 * recency_factor
        // For new memory: access_factor = 0.0, recency_factor = 1.0 (since delta_t is 0)
        let importance_score = (0.5 * llm_score + 0.2 * 1.0).clamp(0.0, 1.0);

        let next_vector_id = self.sqlite.get_max_vector_id().await? + 1;

        let memory_id = Uuid::new_v4().to_string();

        let entities_str = serde_json::to_string(&ext.entities)?;

        let mut meta_map = if let Some(serde_json::Value::Object(m)) = metadata_custom {
            m
        } else {
            serde_json::Map::new()
        };
        meta_map.insert("confidence".to_string(), ext.confidence.into());
        meta_map.insert("llm_importance".to_string(), ext.importance.into());
        meta_map.insert("archived".to_string(), false.into());
        let metadata_str = serde_json::to_string(&meta_map)?;

        let memory = Memory {
            id: memory_id.clone(),
            content: ext.content.clone(),
            category: ext.category.as_str().to_string(),
            scope: scope.as_str().to_string(),
            project_id: project_id.clone(),
            agent_id: None,
            source_session: session_id,
            created_at: now_ms,
            updated_at: now_ms,
            last_accessed_at: now_ms,
            access_count: 0,
            importance_score,
            retention_factor: 1.0,
            entities: entities_str,
            vector_id: next_vector_id,
            metadata: metadata_str,
        };

        // Add to stores
        self.sqlite.insert_memory(&memory).await?;
        self.vector_store.add(next_vector_id, &vector)?;
        self.text_index.add_document(
            &memory.id,
            &memory.content,
            &memory.category,
            &memory.entities,
        )?;

        // Entity linking
        link_entities(&self.sqlite, &memory.id, &ext.entities, now_ms).await?;

        Ok(Some(memory))
    }

    /// Batch decay consolidation executed periodically (e.g. daily, or on session end).
    pub async fn batch_consolidate(
        &self,
        scope: Option<MemoryScope>,
        project_id: Option<&str>,
    ) -> Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let memories = self
            .sqlite
            .get_memories_for_decay(scope.as_ref().map(MemoryScope::as_str), project_id)
            .await?;

        for mut mem in memories {
            // Check if already archived
            let mut meta: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&mem.metadata).unwrap_or_default();
            if meta
                .get("archived")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                continue;
            }

            // Calculate elapsed days
            let elapsed_ms = now_ms - mem.last_accessed_at;
            let elapsed_days = elapsed_ms as f64 / 86_400_000.0;

            // Stability S = importance_score * 30 days. Reinforced by access_count.
            let mut stability = initial_stability(mem.importance_score);
            for _ in 0..mem.access_count {
                stability = reinforce_stability(stability);
            }

            // Ebbinghaus decay
            let retention = calculate_retention(stability, elapsed_days);
            mem.retention_factor = retention;

            // Update importance score
            let llm_importance = meta
                .get("llm_importance")
                .and_then(|v| v.as_f64())
                .unwrap_or(2.5)
                / 5.0;
            let access_score = (mem.access_count as f64 / 10.0).min(1.0);
            let recency_score = (-self.decay_lambda * elapsed_days).exp();
            let new_importance =
                (0.5 * llm_importance + 0.3 * access_score + 0.2 * recency_score).clamp(0.0, 1.0);
            mem.importance_score = new_importance;

            if retention < 0.1 {
                // Archive the memory
                meta.insert("archived".to_string(), true.into());
                mem.metadata = serde_json::to_string(&meta)?;
            }

            mem.updated_at = now_ms;

            // Save to SQLite
            self.sqlite
                .update_decay_parameters(
                    &mem.id,
                    mem.importance_score,
                    mem.retention_factor,
                    mem.updated_at,
                    &mem.metadata,
                )
                .await?;
            // Note: Since this is decay parameter update only, we don't modify memory content.
        }

        // Compact Tantivy index after decay updates
        if let Err(e) = self.text_index.compact() {
            tracing::warn!("Failed to compact text index: {e}");
        }

        Ok(())
    }
}

fn same_scope(memory: &Memory, scope: &MemoryScope, project_id: Option<&str>) -> bool {
    memory.scope == scope.as_str() && memory.project_id.as_deref() == project_id
}
