use crate::error::Result;
use crate::storage::sqlite::{SqliteStore, EntityRecord};

pub async fn link_entities(
    sqlite: &SqliteStore,
    memory_id: &str,
    entities: &[String],
    now_ms: i64,
) -> Result<()> {
    for entity_name in entities {
        let name_trimmed = entity_name.trim();
        if name_trimmed.is_empty() {
            continue;
        }

        let existing = sqlite.get_entity(name_trimmed).await?;
        match existing {
            Some(mut rec) => {
                let mut memory_ids: Vec<String> = serde_json::from_str(&rec.memory_ids)
                    .unwrap_or_default();
                if !memory_ids.contains(&memory_id.to_string()) {
                    memory_ids.push(memory_id.to_string());
                }
                rec.memory_ids = serde_json::to_string(&memory_ids)?;
                rec.frequency += 1;
                rec.updated_at = now_ms;
                sqlite.upsert_entity(&rec).await?;
            }
            None => {
                let memory_ids = vec![memory_id.to_string()];
                let rec = EntityRecord {
                    id: 0, // Auto-incremented
                    name: name_trimmed.to_string(),
                    aliases: "[]".to_string(),
                    memory_ids: serde_json::to_string(&memory_ids)?,
                    frequency: 1,
                    created_at: now_ms,
                    updated_at: now_ms,
                };
                sqlite.upsert_entity(&rec).await?;
            }
        }
    }
    Ok(())
}
