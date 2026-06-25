use crate::error::Result;
use crate::models::Memory;
use sha2::Digest;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

#[derive(Clone)]
pub struct SqliteStore {
    pub pool: SqlitePool,
}

impl SqliteStore {
    pub async fn new(db_path: &str) -> Result<Self> {
        let connection_options = SqliteConnectOptions::from_str(&format!("sqlite:{}", db_path))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePool::connect_with(connection_options).await?;

        // Run migrations
        sqlx::migrate!("src/storage/migrations").run(&pool).await?;

        let store = Self { pool };

        // One-time post-migration backfill for pre-v2 memories
        let backfilled = store.backfill_embedding_metadata().await?;
        if backfilled > 0 {
            tracing::info!("Backfilled embedding metadata for {backfilled} pre-v2 memories");
        }

        Ok(store)
    }

    pub async fn insert_memory(&self, memory: &Memory) -> Result<()> {
        sqlx::query(
            "INSERT INTO memories (id, content, category, scope, project_id, agent_id, source_session, created_at, updated_at, last_accessed_at, access_count, importance_score, retention_factor, entities, vector_id, metadata, status, embedding_model, embedding_dim, content_hash)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&memory.id)
        .bind(&memory.content)
        .bind(&memory.category)
        .bind(&memory.scope)
        .bind(&memory.project_id)
        .bind(&memory.agent_id)
        .bind(&memory.source_session)
        .bind(memory.created_at)
        .bind(memory.updated_at)
        .bind(memory.last_accessed_at)
        .bind(memory.access_count)
        .bind(memory.importance_score)
        .bind(memory.retention_factor)
        .bind(&memory.entities)
        .bind(memory.vector_id)
        .bind(&memory.metadata)
        .bind(&memory.status)
        .bind(&memory.embedding_model)
        .bind(memory.embedding_dim)
        .bind(&memory.content_hash)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_memory(&self, id: &str) -> Result<Option<Memory>> {
        let memory = sqlx::query_as::<_, Memory>("SELECT * FROM memories WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(memory)
    }

    pub async fn get_memory_by_vector_id(&self, vector_id: i64) -> Result<Option<Memory>> {
        let memory = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE vector_id = ? AND status = 'active'",
        )
        .bind(vector_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(memory)
    }

    pub async fn get_by_ids(&self, ids: &[String]) -> Result<Vec<Memory>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite has parameter limits, but for typical top_k <= 50, it is well within limits.
        let mut query_builder =
            sqlx::QueryBuilder::new("SELECT * FROM memories WHERE status = 'active' AND id IN (");
        let mut separated = query_builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(") ");

        let query = query_builder.build_query_as::<Memory>();
        let memories = query.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    /// Soft-delete: set status = 'deleted' instead of hard DELETE.
    /// Returns true if a row was updated.
    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let rows_affected = sqlx::query(
            "UPDATE memories SET status = 'deleted', updated_at = ? WHERE id = ? AND status = 'active'",
        )
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();

        Ok(rows_affected > 0)
    }

    /// Permanent undelete: restore a soft-deleted memory to active status.
    pub async fn undelete_memory(&self, id: &str) -> Result<bool> {
        let rows_affected = sqlx::query(
            "UPDATE memories SET status = 'active', updated_at = ? WHERE id = ? AND status = 'deleted'",
        )
        .bind(chrono::Utc::now().timestamp_millis())
        .bind(id)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(rows_affected > 0)
    }

    /// Permanently remove all soft-deleted memories and return the count.
    pub async fn compact_deleted(&self) -> Result<i64> {
        let deleted: Vec<(String, i64)> = sqlx::query_as(
            "SELECT id, COALESCE(vector_id, -1) FROM memories WHERE status = 'deleted'",
        )
        .fetch_all(&self.pool)
        .await?;
        let count = deleted.len() as i64;
        for (id, _vector_id) in &deleted {
            sqlx::query("DELETE FROM memories WHERE id = ?")
                .bind(id)
                .execute(&self.pool)
                .await?;
        }
        Ok(count)
    }

    /// List soft-deleted memories (for admin/debug)
    pub async fn get_deleted_memories(&self, limit: usize) -> Result<Vec<Memory>> {
        let memories = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE status = 'deleted' ORDER BY updated_at DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(memories)
    }

    pub async fn unlink_memory_from_entities(&self, memory_id: &str) -> Result<()> {
        let entities: Vec<EntityRecord> = sqlx::query_as("SELECT * FROM entities")
            .fetch_all(&self.pool)
            .await?;

        for mut entity in entities {
            let mut memory_ids: Vec<String> =
                serde_json::from_str(&entity.memory_ids).unwrap_or_default();
            let original_len = memory_ids.len();
            memory_ids.retain(|id| id != memory_id);
            if memory_ids.len() == original_len {
                continue;
            }

            if memory_ids.is_empty() {
                sqlx::query("DELETE FROM entities WHERE id = ?")
                    .bind(entity.id)
                    .execute(&self.pool)
                    .await?;
            } else {
                entity.memory_ids = serde_json::to_string(&memory_ids)?;
                entity.frequency = memory_ids.len() as i32;
                entity.updated_at = chrono::Utc::now().timestamp_millis();
                self.upsert_entity(&entity).await?;
            }
        }
        Ok(())
    }

    pub async fn list_memories(
        &self,
        scope: Option<&str>,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let mut query_builder =
            sqlx::QueryBuilder::new("SELECT * FROM memories WHERE status = 'active' ");

        if let Some(sc) = scope {
            query_builder.push(" AND scope = ");
            query_builder.push_bind(sc);
        }

        if let Some(pid) = project_id {
            query_builder.push(" AND project_id = ");
            query_builder.push_bind(pid);
        }

        query_builder.push(" ORDER BY created_at DESC LIMIT ");
        query_builder.push_bind(limit as i64);

        let query = query_builder.build_query_as::<Memory>();
        let memories = query.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    pub async fn update_access_stats(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }

        let now_ms = chrono::Utc::now().timestamp_millis();

        let mut query_builder = sqlx::QueryBuilder::new("UPDATE memories SET ");
        query_builder.push("access_count = access_count + 1, ");
        query_builder.push("last_accessed_at = ");
        query_builder.push_bind(now_ms);
        query_builder.push(" WHERE id IN (");

        let mut separated = query_builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");

        let query = query_builder.build();
        query.execute(&self.pool).await?;
        Ok(())
    }

    pub async fn update_decay_parameters(
        &self,
        id: &str,
        importance_score: f64,
        retention_factor: f64,
        updated_at: i64,
        metadata: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE memories SET importance_score = ?, retention_factor = ?, updated_at = ?, metadata = ? WHERE id = ?")
            .bind(importance_score)
            .bind(retention_factor)
            .bind(updated_at)
            .bind(metadata)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_max_vector_id(&self) -> Result<i64> {
        let res: Option<(i64,)> =
            sqlx::query_as("SELECT COALESCE(MAX(vector_id), 0) FROM memories")
                .fetch_optional(&self.pool)
                .await?;
        Ok(res.map(|r| r.0).unwrap_or(0))
    }

    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let total_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories")
            .fetch_one(&self.pool)
            .await?;

        let active_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memories WHERE status = 'active'")
                .fetch_one(&self.pool)
                .await?;

        let deleted_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM memories WHERE status = 'deleted'")
                .fetch_one(&self.pool)
                .await?;

        let categories: Vec<(String, i64)> = sqlx::query_as(
            "SELECT category, COUNT(*) FROM memories WHERE status = 'active' GROUP BY category",
        )
        .fetch_all(&self.pool)
        .await?;

        let scopes: Vec<(String, i64)> = sqlx::query_as(
            "SELECT scope, COUNT(*) FROM memories WHERE status = 'active' GROUP BY scope",
        )
        .fetch_all(&self.pool)
        .await?;

        let entity_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entities")
            .fetch_one(&self.pool)
            .await?;

        let mut cat_map = serde_json::Map::new();
        for (cat, count) in categories {
            cat_map.insert(cat, serde_json::Value::Number(count.into()));
        }

        let mut scope_map = serde_json::Map::new();
        for (sc, count) in scopes {
            scope_map.insert(sc, serde_json::Value::Number(count.into()));
        }

        let mut stats = serde_json::Map::new();
        stats.insert("total_memories".to_string(), total_count.0.into());
        stats.insert("active_memories".to_string(), active_count.0.into());
        stats.insert("deleted_memories".to_string(), deleted_count.0.into());
        stats.insert("categories".to_string(), serde_json::Value::Object(cat_map));
        stats.insert("scopes".to_string(), serde_json::Value::Object(scope_map));
        stats.insert("entity_count".to_string(), entity_count.0.into());

        Ok(serde_json::Value::Object(stats))
    }

    // Entity table helpers
    pub async fn get_entity(&self, name: &str) -> Result<Option<EntityRecord>> {
        let row = sqlx::query_as::<_, EntityRecord>("SELECT * FROM entities WHERE name = ?")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row)
    }

    pub async fn upsert_entity(&self, entity: &EntityRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO entities (name, aliases, memory_ids, frequency, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
             aliases = excluded.aliases,
             memory_ids = excluded.memory_ids,
             frequency = excluded.frequency,
             updated_at = excluded.updated_at",
        )
        .bind(&entity.name)
        .bind(&entity.aliases)
        .bind(&entity.memory_ids)
        .bind(entity.frequency)
        .bind(entity.created_at)
        .bind(entity.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Data consistency & repair helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Count memories with a specific status ('active', 'archived', etc.)
    pub async fn count_by_status(&self, status: &str) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories WHERE status = ?")
            .bind(status)
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    /// Count unresolved repair queue entries
    pub async fn count_unresolved_repairs(&self) -> Result<i64> {
        let row: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM index_repair_queue WHERE resolved_at IS NULL")
                .fetch_one(&self.pool)
                .await?;
        Ok(row.0)
    }

    /// Add an issue to the repair queue
    pub async fn enqueue_repair_issue(
        &self,
        memory_id: Option<&str>,
        issue_type: &str,
        details: &str,
    ) -> Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO index_repair_queue (memory_id, issue_type, details, created_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(memory_id)
        .bind(issue_type)
        .bind(details)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Scan entities table and remove references to memories that no longer exist.
    /// Returns the number of entity references cleaned up.
    pub async fn repair_entity_references(&self) -> Result<i64> {
        let entities: Vec<EntityRecord> = sqlx::query_as("SELECT * FROM entities")
            .fetch_all(&self.pool)
            .await?;

        let mut total_cleaned = 0i64;
        for mut entity in entities {
            let memory_ids: Vec<String> =
                serde_json::from_str(&entity.memory_ids).unwrap_or_default();
            if memory_ids.is_empty() {
                continue;
            }

            // Check which IDs still exist
            let mut valid_ids = Vec::new();
            for mid in &memory_ids {
                let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM memories WHERE id = ?")
                    .bind(mid)
                    .fetch_optional(&self.pool)
                    .await?;
                if exists.is_some() {
                    valid_ids.push(mid.clone());
                }
            }

            if valid_ids.len() != memory_ids.len() {
                total_cleaned += (memory_ids.len() - valid_ids.len()) as i64;
                if valid_ids.is_empty() {
                    sqlx::query("DELETE FROM entities WHERE id = ?")
                        .bind(entity.id)
                        .execute(&self.pool)
                        .await?;
                } else {
                    entity.memory_ids = serde_json::to_string(&valid_ids)?;
                    entity.frequency = valid_ids.len() as i32;
                    entity.updated_at = chrono::Utc::now().timestamp_millis();
                    self.upsert_entity(&entity).await?;
                }
            }
        }
        Ok(total_cleaned)
    }

    /// Migrate pre-v2 schema: fill embedding_model & embedding_dim from system_config
    /// for memories that have NULL in those columns.
    pub async fn backfill_embedding_metadata(&self) -> Result<i64> {
        // Read current defaults from system_config
        let model_row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM system_config WHERE key = 'embedding_model'")
                .fetch_optional(&self.pool)
                .await?;
        let dim_row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM system_config WHERE key = 'vector_dimensions'")
                .fetch_optional(&self.pool)
                .await?;

        let model = model_row
            .map(|r| r.0)
            .unwrap_or_else(|| "unknown".to_string());
        let dim: i64 = dim_row.and_then(|r| r.0.parse().ok()).unwrap_or(1536);

        // Compute content_hash for memories that don't have one
        let memories: Vec<Memory> = sqlx::query_as(
            "SELECT * FROM memories WHERE embedding_model IS NULL OR content_hash IS NULL",
        )
        .fetch_all(&self.pool)
        .await?;

        let count = memories.len() as i64;
        for mem in memories {
            let content_hash = if mem.content_hash.is_none() {
                let mut hasher = sha2::Sha256::new();
                hasher.update(mem.content.as_bytes());
                Some(format!("{:x}", hasher.finalize()))
            } else {
                None
            };

            sqlx::query(
                "UPDATE memories SET embedding_model = COALESCE(embedding_model, ?),
                 embedding_dim = COALESCE(embedding_dim, ?),
                 content_hash = COALESCE(content_hash, ?)
                 WHERE id = ?",
            )
            .bind(&model)
            .bind(dim)
            .bind(&content_hash)
            .bind(&mem.id)
            .execute(&self.pool)
            .await?;
        }
        Ok(count)
    }

    pub async fn get_memories_for_decay(
        &self,
        scope: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<Memory>> {
        let mut query_builder =
            sqlx::QueryBuilder::new("SELECT * FROM memories WHERE status = 'active'");
        if let Some(scope) = scope {
            query_builder.push(" AND scope = ").push_bind(scope);
        }
        if let Some(project_id) = project_id {
            query_builder
                .push(" AND project_id = ")
                .push_bind(project_id);
        }
        let memories = query_builder
            .build_query_as::<Memory>()
            .fetch_all(&self.pool)
            .await?;
        Ok(memories)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EntityRecord {
    pub id: i64,
    pub name: String,
    pub aliases: String,    // JSON array
    pub memory_ids: String, // JSON array
    pub frequency: i32,
    pub created_at: i64,
    pub updated_at: i64,
}
