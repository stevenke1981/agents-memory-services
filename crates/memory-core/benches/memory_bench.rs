use criterion::{criterion_group, criterion_main, Criterion};
use memory_core::config::MemoryConfig;
use memory_core::models::{Memory, MemoryScope, SearchQuery};
use memory_core::service::MemoryService;
use std::sync::Arc;

/// Build a MemoryService with mock LLM and a temp directory.
async fn build_service() -> Arc<MemoryService> {
    let dir = tempfile::tempdir().expect("temp dir");

    let config = MemoryConfig {
        db_path: dir.path().join("bench.db").to_string_lossy().to_string(),
        vector_path: dir
            .path()
            .join("bench.usearch")
            .to_string_lossy()
            .to_string(),
        tantivy_path: dir.path().join("tantivy").to_string_lossy().to_string(),
        llm_api_base: "mock".to_string(),
        embedding_api_base: "mock".to_string(),
        llm_api_key: "mock".to_string(),
        ..MemoryConfig::from_env().expect("config")
    };

    Arc::new(MemoryService::new(config).await.expect("init service"))
}

fn bench_add_memory(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio rt");
    let service = rt.block_on(build_service());

    c.bench_function("add_memory_single", |b| {
        b.to_async(&rt).iter(|| {
            let svc = service.clone();
            let content = "User prefers Rust for backend and TypeScript for frontend.".to_string();
            async move {
                let _: Vec<Memory> = svc
                    .add_memory(
                        &content,
                        MemoryScope::Session,
                        None,
                        None,
                        "bench-session".to_string(),
                        None,
                    )
                    .await
                    .unwrap_or_default();
            }
        });
    });
}

fn bench_search_memory(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().expect("tokio rt");
    let service = rt.block_on(build_service());

    // Pre-populate some memories
    rt.block_on(async {
        for i in 0..10 {
            let content = format!("User preference #{i}: prefers using Rust for backend work and TypeScript for frontend.");
            let _ = service
                .add_memory(
                    &content,
                    MemoryScope::Session,
                    None,
                    None,
                    "bench-session".to_string(),
                    None,
                )
                .await;
        }
    });

    let query = SearchQuery {
        query: "Rust backend TypeScript frontend".to_string(),
        top_k: 5,
        ..Default::default()
    };

    c.bench_function("search_memories_top5", |b| {
        b.to_async(&rt).iter(|| {
            let svc = service.clone();
            let q = query.clone();
            async move {
                let _ = svc.search_memories(&q).await;
            }
        });
    });
}

criterion_group!(benches, bench_add_memory, bench_search_memory);
criterion_main!(benches);
