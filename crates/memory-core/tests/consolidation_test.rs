use memory_core::consolidation::ConsolidationEngine;
use memory_core::extraction::ExtractedMemory;
use memory_core::models::MemoryCategory;
use memory_core::models::{Memory, MemoryScope};
use memory_core::storage::{SqliteStore, TextIndex, VectorStore};
use std::sync::Arc;
use tempfile::tempdir;

fn memory(id: &str, vector_id: i64, project_id: &str, updated_at: i64) -> Memory {
    Memory {
        id: id.to_string(),
        content: format!("memory for {project_id}"),
        category: "Fact".to_string(),
        scope: "Project".to_string(),
        project_id: Some(project_id.to_string()),
        agent_id: None,
        source_session: "test".to_string(),
        created_at: updated_at,
        updated_at,
        last_accessed_at: updated_at,
        access_count: 0,
        importance_score: 0.5,
        retention_factor: 1.0,
        entities: "[]".to_string(),
        vector_id,
        metadata: r#"{"llm_importance":3,"archived":false}"#.to_string(),
    }
}

#[tokio::test]
async fn batch_consolidation_only_updates_requested_project() {
    let tmp = tempdir().unwrap();
    let sqlite = Arc::new(
        SqliteStore::new(&tmp.path().join("memory.db").to_string_lossy())
            .await
            .unwrap(),
    );
    let vector_store = Arc::new(
        VectorStore::new(&tmp.path().join("vectors.usearch").to_string_lossy(), 3).unwrap(),
    );
    let text_index =
        Arc::new(TextIndex::new(&tmp.path().join("tantivy").to_string_lossy()).unwrap());
    let old = chrono::Utc::now().timestamp_millis() - 86_400_000;
    sqlite
        .insert_memory(&memory("a", 1, "project-a", old))
        .await
        .unwrap();
    sqlite
        .insert_memory(&memory("b", 2, "project-b", old))
        .await
        .unwrap();

    let engine =
        ConsolidationEngine::new(sqlite.clone(), vector_store, text_index, 0.92, 0.75, 0.001);
    engine
        .batch_consolidate(Some(MemoryScope::Project), Some("project-a"))
        .await
        .unwrap();

    let project_a = sqlite.get_memory("a").await.unwrap().unwrap();
    let project_b = sqlite.get_memory("b").await.unwrap().unwrap();
    assert!(project_a.updated_at > old);
    assert_eq!(project_b.updated_at, old);
}

#[tokio::test]
async fn failed_vector_insert_does_not_leave_a_sqlite_orphan() {
    let tmp = tempdir().unwrap();
    let sqlite = Arc::new(
        SqliteStore::new(&tmp.path().join("memory.db").to_string_lossy())
            .await
            .unwrap(),
    );
    let vector_store = Arc::new(
        VectorStore::new(&tmp.path().join("vectors.usearch").to_string_lossy(), 3).unwrap(),
    );
    let text_index =
        Arc::new(TextIndex::new(&tmp.path().join("tantivy").to_string_lossy()).unwrap());
    let engine =
        ConsolidationEngine::new(sqlite.clone(), vector_store, text_index, 0.92, 0.75, 0.001);

    let result = engine
        .consolidate_single(
            ExtractedMemory {
                content: "User prefers Rust.".to_string(),
                category: MemoryCategory::Preference,
                entities: vec!["Rust".to_string()],
                importance: 4,
                confidence: 0.9,
            },
            vec![1.0, 0.0],
            MemoryScope::Global,
            None,
            None,
            "test".to_string(),
            None,
        )
        .await;

    assert!(result.is_err());
    assert!(sqlite
        .list_memories(None, None, 10)
        .await
        .unwrap()
        .is_empty());
}
