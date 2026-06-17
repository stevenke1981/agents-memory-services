use clap::{Parser, Subcommand};
use memory_core::{
    config::MemoryConfig,
    models::{MemoryScope, SearchQuery},
    service::MemoryService,
};
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "memory-cli")]
#[command(about = "OpenCode Memory CLI Debugging Utility", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a memory from raw text
    Add {
        /// Raw conversation text or statement
        #[arg(short, long)]
        content: String,

        /// Scope of the memory: Global, Project, Session, Agent
        #[arg(short, long, default_value = "Global")]
        scope: String,

        /// Optional Project ID
        #[arg(short, long)]
        project_id: Option<String>,
    },
    /// Search memories using Hybrid retrieval
    Search {
        /// Query string to search
        #[arg(short, long)]
        query: String,

        /// Return top K results
        #[arg(short, long, default_value_t = 5)]
        top_k: usize,

        /// Optional Scope filter
        #[arg(short, long)]
        scope: Option<String>,

        /// Optional Project ID filter
        #[arg(short, long)]
        project_id: Option<String>,
    },
    /// List all memories
    List {
        /// Optional Scope filter
        #[arg(short, long)]
        scope: Option<String>,

        /// Optional Project ID filter
        #[arg(short, long)]
        project_id: Option<String>,

        /// Limit number of returned results
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
    },
    /// Get memory system statistics
    Stats,
    /// Trigger Ebbinghaus decay and consolidation
    Consolidate,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let args = Cli::parse();

    let config = MemoryConfig::from_env()?;
    let service = Arc::new(MemoryService::new(config).await?);

    match args.command {
        Commands::Add {
            content,
            scope,
            project_id,
        } => {
            let memory_scope = scope
                .parse::<MemoryScope>()
                .map_err(|e| anyhow::anyhow!("Invalid scope: {e}"))?;

            println!("Extracting and consolidating memory...");
            let memories = service
                .add_memory(
                    &content,
                    memory_scope,
                    project_id,
                    None,
                    "cli-session".to_string(),
                    None,
                )
                .await?;

            if memories.is_empty() {
                println!("No new memories added (either duplicate or low relevance).");
            } else {
                println!("Successfully added {} memories:", memories.len());
                for mem in memories {
                    println!("- [{}] (ID: {}): {}", mem.category, mem.id, mem.content);
                }
            }
        }
        Commands::Search {
            query,
            top_k,
            scope,
            project_id,
        } => {
            let memory_scope = scope.and_then(|s| s.parse::<MemoryScope>().ok());

            let search_query = SearchQuery {
                query,
                top_k,
                scope: memory_scope,
                project_id,
                categories: None,
                created_after: None,
                min_importance: None,
                include_decayed: false,
                session_id: None,
                weights: None,
            };

            let results = service.search_memories(&search_query).await?;
            if results.is_empty() {
                println!("No matching memories found.");
            } else {
                println!("Found {} results:", results.len());
                for res in results {
                    println!(
                        "Score: {:.4} [Semantic: {:.4}, BM25: {:.4}, Recency: {:.4}]",
                        res.score_final, res.score_semantic, res.score_bm25, res.score_temporal
                    );
                    println!(
                        "- [{}] (ID: {}): {}",
                        res.memory.category, res.memory.id, res.memory.content
                    );
                }
            }
        }
        Commands::List {
            scope,
            project_id,
            limit,
        } => {
            let memory_scope = scope.and_then(|s| s.parse::<MemoryScope>().ok());

            let memories = service
                .get_memories(None, memory_scope, project_id, limit)
                .await?;
            if memories.is_empty() {
                println!("No memories stored.");
            } else {
                println!("List of memories (limit {}):", limit);
                for mem in memories {
                    println!(
                        "- [{}] (ID: {}) (Accessed: {} times): {}",
                        mem.category, mem.id, mem.access_count, mem.content
                    );
                }
            }
        }
        Commands::Stats => {
            let stats = service.get_stats().await?;
            println!("{}", serde_json::to_string_pretty(&stats)?);
        }
        Commands::Consolidate => {
            println!("Running batch decay consolidation...");
            service.consolidate_memories(None, None).await?;
            println!("Consolidation complete.");
        }
    }

    Ok(())
}
