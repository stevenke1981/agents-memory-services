use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use std::sync::Arc;
use crate::error::{MemoryError, Result};
use crate::models::Memory;

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
        sqlx::migrate!("src/storage/migrations")
            .run(&pool)
            .await?;

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
        let memory = sqlx::query_as::<_, Memory>(
            "SELECT * FROM memories WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(memory)
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

    pub async fn update_decay_parameters(&self, id: &str, importance_score: f64, retention_factor: f64, updated_at: i64) -> Result<()> {
        sqlx::query("UPDATE memories SET importance_score = ?, retention_factor = ?, updated_at = ? WHERE id = ?")
            .bind(importance_score)
            .bind(retention_factor)
            .bind(updated_at)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_max_vector_id(&self) -> Result<i64> {
        let res: Option<(i64,)> = sqlx::query_as("SELECT COALESCE(MAX(vector_id), 0) FROM memories")
            .fetch_optional(&self.pool)
            .await?;
        Ok(res.map(|r| r.0).unwrap_or(0))
    }

    pub async fn get_stats(&self) -> Result<serde_json::Value> {
        let total_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM memories")
            .fetch_one(&self.pool)
            .await?;

        let categories: Vec<(String, i64)> = sqlx::query_as(
            "SELECT category, COUNT(*) FROM memories GROUP BY category"
        )
        .fetch_all(&self.pool)
        .await?;

        let scopes: Vec<(String, i64)> = sqlx::query_as(
            "SELECT scope, COUNT(*) FROM memories GROUP BY scope"
        )
        .fetch_all(&self.pool)
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
             updated_at = excluded.updated_at"
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

    pub async fn get_all_memories_for_decay(&self) -> Result<Vec<Memory>> {
        let memories = sqlx::query_as::<_, Memory>("SELECT * FROM memories")
            .fetch_all(&self.pool)
            .await?;
        Ok(memories)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EntityRecord {
    pub id: i64,
    pub name: String,
    pub aliases: String, // JSON array
    pub memory_ids: String, // JSON array
    pub frequency: i32,
    pub created_at: i64,
    pub updated_at: i64,
}
