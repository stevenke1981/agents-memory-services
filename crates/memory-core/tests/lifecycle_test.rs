use memory_core::{
    config::MemoryConfig,
    service::MemoryService,
    models::{MemoryScope, SearchQuery},
};
use tempfile::tempdir;

#[tokio::test]
async fn test_full_memory_lifecycle() {
    // 1. Create temporary directory for databases & indexes
    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("memory.db").to_string_lossy().into_owned();
    let vector_path = tmp.path().join("vectors.usearch").to_string_lossy().into_owned();
    let tantivy_path = tmp.path().join("tantivy").to_string_lossy().into_owned();

    // Configure test environment variables to mock
    let config = MemoryConfig {
        db_path,
        vector_path,
        tantivy_path,
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
        decay_mu: 0.05,
        max_records: 1000,
        min_confidence: 0.60,
        min_importance: 2,
    };

    let service = MemoryService::new(config).await.unwrap();

    // 2. Add memory (using the mock responder)
    let conversation = "User: I prefer using tokio::spawn for background tasks in Rust.\n\
                        Assistant: Good practice. I'll remember that preference.";
    let added = service.add_memory(
        conversation,
        MemoryScope::Global,
        None,
        "test-session".to_string(),
        None,
    ).await.unwrap();

    assert!(!added.is_empty(), "Should extract at least one memory");
    assert_eq!(added[0].content, "User prefers using tokio::spawn for background tasks in Rust.");
    assert_eq!(added[0].category, "Preference");

    // 3. Verify ADD-only constraint: same content is deduplicated and not added again
    let added2 = service.add_memory(
        conversation,
        MemoryScope::Global,
        None,
        "test-session".to_string(),
        None,
    ).await.unwrap();
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
        weights: None,
    };

    let results = service.search_memories(&query).await.unwrap();
    assert!(!results.is_empty(), "Should find the stored memory");
    assert!(results[0].score_final > 0.5, "Score should be significant");

    // 5. Verify batch consolidation (decay calculation)
    service.consolidate_memories().await.unwrap();
    
    // Clean up temp files automatically by dropping tmp
}
