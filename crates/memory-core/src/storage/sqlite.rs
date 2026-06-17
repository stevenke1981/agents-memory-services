use crate::error::Result;
use crate::models::Memory;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
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

        Ok(Self { pool })
    }

    pub async fn insert_memory(&self, memory: &Memory) -> Result<()> {
        sqlx::query(
            "INSERT INTO memories (id, content, category, scope, project_id, agent_id, source_session, created_at, updated_at, last_accessed_at, access_count, importance_score, retention_factor, entities, vector_id, metadata)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        let memory = sqlx::query_as::<_, Memory>("SELECT * FROM memories WHERE vector_id = ?")
            .bind(vector_id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(memory)
    }

    pub async fn get_memories_by_vector_ids(&self, vector_ids: &[i64]) -> Result<Vec<Memory>> {
        if vector_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut query_builder =
            sqlx::QueryBuilder::new("SELECT * FROM memories WHERE vector_id IN (");
        let mut separated = query_builder.separated(", ");
        for vid in vector_ids {
            separated.push_bind(vid);
        }
        separated.push_unseparated(") ");

        let memories = query_builder
            .build_query_as::<Memory>()
            .fetch_all(&self.pool)
            .await?;
        Ok(memories)
    }

    pub async fn get_by_ids(&self, ids: &[String]) -> Result<Vec<Memory>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite has parameter limits, but for typical top_k <= 50, it is well within limits.
        let mut query_builder = sqlx::QueryBuilder::new("SELECT * FROM memories WHERE id IN (");
        let mut separated = query_builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(") ");

        let query = query_builder.build_query_as::<Memory>();
        let memories = query.fetch_all(&self.pool).await?;
        Ok(memories)
    }

    pub async fn delete_memory(&self, id: &str) -> Result<bool> {
        let rows_affected = sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?
            .rows_affected();

        Ok(rows_affected > 0)
    }

    pub async fn unlink_memory_from_entities(&self, memory_id: &str) -> Result<()> {
        let pattern = format!("%{}%", memory_id);
        let entities: Vec<EntityRecord> =
            sqlx::query_as("SELECT * FROM entities WHERE memory_ids LIKE ?")
                .bind(&pattern)
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

    pub async fn memory_count(&self) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.0)
    }

    pub async fn list_memories(
        &self,
        scope: Option<&str>,
        project_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let mut query_builder = sqlx::QueryBuilder::new("SELECT * FROM memories WHERE 1=1 ");

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

        let categories: Vec<(String, i64)> =
            sqlx::query_as("SELECT category, COUNT(*) FROM memories GROUP BY category")
                .fetch_all(&self.pool)
                .await?;

        let scopes: Vec<(String, i64)> =
            sqlx::query_as("SELECT scope, COUNT(*) FROM memories GROUP BY scope")
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

    /// Ensure a session_stats row exists for the given session.
    pub async fn ensure_session(&self, session_id: &str, project_id: Option<&str>) -> Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT OR IGNORE INTO session_stats (session_id, project_id, started_at)
             VALUES (?, ?, ?)",
        )
        .bind(session_id)
        .bind(project_id)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Increment counter(s) for an existing session_stats row.
    /// End a session by setting ended_at timestamp.
    pub async fn end_session(&self, session_id: &str) -> Result<()> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE session_stats SET ended_at = ? WHERE session_id = ?")
            .bind(now_ms)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_session_stats(
        &self,
        session_id: &str,
        extracted: i64,
        added: i64,
        deduplicated: i64,
        retrieved: i64,
        tokens: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE session_stats SET
             memories_extracted = memories_extracted + ?,
             memories_added = memories_added + ?,
             memories_deduplicated = memories_deduplicated + ?,
             memories_retrieved = memories_retrieved + ?,
             total_tokens_used = total_tokens_used + ?
             WHERE session_id = ?",
        )
        .bind(extracted)
        .bind(added)
        .bind(deduplicated)
        .bind(retrieved)
        .bind(tokens)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Upsert a key-value pair in system_config.
    pub async fn set_system_config(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO system_config (key, value) VALUES (?, ?)")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Get a value from system_config by key.
    pub async fn get_system_config(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM system_config WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| r.0))
    }

    pub async fn get_memories_for_decay(
        &self,
        scope: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<Memory>> {
        let mut query_builder = sqlx::QueryBuilder::new("SELECT * FROM memories WHERE 1=1");
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

    /// Paginated variant of get_memories_for_decay to avoid loading all memories into memory.
    pub async fn get_memories_for_decay_paginated(
        &self,
        scope: Option<&str>,
        project_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Memory>> {
        let mut query_builder = sqlx::QueryBuilder::new("SELECT * FROM memories WHERE 1=1");
        if let Some(scope) = scope {
            query_builder.push(" AND scope = ").push_bind(scope);
        }
        if let Some(project_id) = project_id {
            query_builder
                .push(" AND project_id = ")
                .push_bind(project_id);
        }
        query_builder
            .push(" ORDER BY rowid LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        let memories = query_builder
            .build_query_as::<Memory>()
            .fetch_all(&self.pool)
            .await?;
        Ok(memories)
    }

    /// Count memories matching decay scope/project filters.
    pub async fn count_memories_for_decay(
        &self,
        scope: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<i64> {
        let mut query_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM memories WHERE 1=1");
        if let Some(scope) = scope {
            query_builder.push(" AND scope = ").push_bind(scope);
        }
        if let Some(project_id) = project_id {
            query_builder
                .push(" AND project_id = ")
                .push_bind(project_id);
        }
        let count: i64 = query_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
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
