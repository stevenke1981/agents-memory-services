use memory_core::{
    config::MemoryConfig,
    models::{MemoryScope, SearchQuery},
    service::MemoryService,
};
use tempfile::tempdir;

fn test_config(tmp: &tempfile::TempDir) -> MemoryConfig {
    MemoryConfig {
        db_path: tmp.path().join("memory.db").to_string_lossy().into_owned(),
        vector_path: tmp
            .path()
            .join("vectors.usearch")
            .to_string_lossy()
            .into_owned(),
        tantivy_path: tmp.path().join("tantivy").to_string_lossy().into_owned(),
        llm_api_base: "mock".to_string(),
        llm_api_key: "mock".to_string(),
        embedding_model: "text-embedding-3-small".to_string(),
        embedding_dim: 1536,
        extraction_model: "claude-sonnet-4-6".to_string(),
        extraction_max_tokens: 2048,
        dedup_threshold: 0.92,
        near_dedup_threshold: 0.75,
        top_k: 5,
        decay_lambda: 0.001,
        temporal_mu: 0.05,
        max_records: 1000,
        min_confidence: 0.60,
        min_importance: 2,
    }
}

#[tokio::test]
async fn test_full_memory_lifecycle() {
    // 1. Create temporary directory for databases & indexes
    let tmp = tempdir().unwrap();
    let config = test_config(&tmp);

    let service = MemoryService::new(config).await.unwrap();

    // 2. Add memory (using the mock responder)
    let conversation = "User: I prefer using tokio::spawn for background tasks in Rust.\n\
                        Assistant: Good practice. I'll remember that preference.";
    let added = service
        .add_memory(
            conversation,
            MemoryScope::Global,
            None,
            None,
            "test-session".to_string(),
            None,
        )
        .await
        .unwrap();

    assert!(!added.is_empty(), "Should extract at least one memory");
    assert_eq!(
        added[0].content,
        "User prefers using tokio::spawn for background tasks in Rust."
    );
    assert_eq!(added[0].category, "Preference");

    // 3. Verify ADD-only constraint: same content is deduplicated and not added again
    let added2 = service
        .add_memory(
            conversation,
            MemoryScope::Global,
            None,
            None,
            "test-session".to_string(),
            None,
        )
        .await
        .unwrap();
    assert!(added2.is_empty(), "Duplicate should be deduplicated");

    // 4. Verify Hybrid Retrieval
    let query = SearchQuery {
        query: "Rust async background task preference".to_string(),
        top_k: 5,
        scope: None,
        project_id: None,
        categories: None,
        created_after: None,
        min_importance: None,
        include_decayed: false,
        session_id: None,
        weights: None,
    };

    let results = service.search_memories(&query).await.unwrap();
    assert!(!results.is_empty(), "Should find the stored memory");
    assert!(results[0].score_final > 0.5, "Score should be significant");

    // 5. Verify batch consolidation (decay calculation)
    service.consolidate_memories(None, None).await.unwrap();

    // Clean up temp files automatically by dropping tmp
}

#[tokio::test]
async fn deduplicates_at_the_exact_similarity_threshold() {
    let tmp = tempdir().unwrap();
    let mut config = test_config(&tmp);
    config.dedup_threshold = 1.0;

    let service = MemoryService::new(config).await.unwrap();
    let conversation = "User: I prefer using tokio::spawn for background tasks in Rust.";

    let first = service
        .add_memory(
            conversation,
            MemoryScope::Global,
            None,
            None,
            "first".to_string(),
            None,
        )
        .await
        .unwrap();
    let second = service
        .add_memory(
            conversation,
            MemoryScope::Global,
            None,
            None,
            "second".to_string(),
            None,
        )
        .await
        .unwrap();

    assert_eq!(first.len(), 1);
    assert!(
        second.is_empty(),
        "similarity equal to the threshold must deduplicate"
    );
}

#[tokio::test]
async fn keeps_identical_memories_isolated_between_projects() {
    let tmp = tempdir().unwrap();
    let service = MemoryService::new(test_config(&tmp)).await.unwrap();
    let conversation = "User: I prefer using tokio::spawn for background tasks in Rust.";

    let project_a = service
        .add_memory(
            conversation,
            MemoryScope::Project,
            Some("project-a".to_string()),
            None,
            "session-a".to_string(),
            None,
        )
        .await
        .unwrap();
    let project_b = service
        .add_memory(
            conversation,
            MemoryScope::Project,
            Some("project-b".to_string()),
            None,
            "session-b".to_string(),
            None,
        )
        .await
        .unwrap();

    assert_eq!(project_a.len(), 1);
    assert_eq!(
        project_b.len(),
        1,
        "project-scoped memories must not deduplicate across projects"
    );
}

#[tokio::test]
async fn delete_cleans_vector_and_entity_indexes() {
    let tmp = tempdir().unwrap();
    let service = MemoryService::new(test_config(&tmp)).await.unwrap();
    let added = service
        .add_memory(
            "User: I prefer using tokio::spawn for background tasks in Rust.",
            MemoryScope::Global,
            None,
            None,
            "delete-test".to_string(),
            None,
        )
        .await
        .unwrap();
    let id = &added[0].id;

    let before = service.get_stats().await.unwrap();
    assert_eq!(before["vector_count"], 1);
    assert!(before["entity_count"].as_i64().unwrap() > 0);

    assert!(service.delete_memory(id).await.unwrap());

    let after = service.get_stats().await.unwrap();
    assert_eq!(after["total_memories"], 0);
    assert_eq!(after["vector_count"], 0);
    assert_eq!(after["entity_count"], 0);
}
