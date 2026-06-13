use memory_core::{config::MemoryConfig, service::MemoryService};
use std::sync::Arc;
use tracing_subscriber::fmt::format::FmtSpan;

mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to stderr (MCP requires stdout to be clean for JSON-RPC messages)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_span_events(FmtSpan::CLOSE)
        .init();

    tracing::info!("Memory MCP Server starting...");

    // Read config from env
    let config = MemoryConfig::from_env()?;
    tracing::info!("DB path: {}", config.db_path);

    // Initialize Memory Service (creates DB, HNSW vector index, Tantivy index)
    let service = Arc::new(MemoryService::new(config).await?);
    tracing::info!("Memory service initialized");

    // Launch custom MCP server on stdio
    let server = server::MemoryMcpServer::new(service);
    server.serve_stdio().await?;

    Ok(())
}
