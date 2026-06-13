use std::env;
use std::path::PathBuf;
use crate::error::{MemoryError, Result};

#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub db_path: String,
    pub vector_path: String,
    pub tantivy_path: String,
    pub llm_api_base: String,
    pub llm_api_key: String,
    pub embedding_model: String,
    pub embedding_dim: usize,
    pub extraction_model: String,
    pub extraction_max_tokens: u32,
    pub dedup_threshold: f64,
    pub near_dedup_threshold: f64,
    pub top_k: usize,
    pub decay_lambda: f64,
    pub decay_mu: f64,
    pub max_records: usize,
    pub min_confidence: f64,
    pub min_importance: u8,
}

impl MemoryConfig {
    pub fn from_env() -> Result<Self> {
        // Resolve .opencode directory locally or in absolute path
        let base_dir = env::var("PROJECT_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join(".opencode");

        let db_path = env::var("MEMORY_DB_PATH")
            .unwrap_or_else(|_| base_dir.join("memory.db").to_string_lossy().into_owned());

        let vector_path = env::var("MEMORY_VECTOR_PATH")
            .unwrap_or_else(|_| base_dir.join("vectors.usearch").to_string_lossy().into_owned());

        let tantivy_path = env::var("MEMORY_TANTIVY_PATH")
            .unwrap_or_else(|_| base_dir.join("tantivy").to_string_lossy().into_owned());

        let llm_api_base = env::var("LLM_API_BASE")
            .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string());

        let llm_api_key = env::var("LLM_API_KEY")
            .unwrap_or_else(|_| "local".to_string());

        let embedding_model = env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".to_string());

        let embedding_dim = env::var("EMBEDDING_DIM")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(1536);

        let extraction_model = env::var("EXTRACTION_MODEL")
            .unwrap_or_else(|_| "claude-sonnet-4-6".to_string());

        let extraction_max_tokens = env::var("EXTRACTION_MAX_TOKENS")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(2048);

        let dedup_threshold = env::var("MEMORY_DEDUP_THRESHOLD")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(0.92);

        let near_dedup_threshold = env::var("MEMORY_NEAR_DEDUP_THRESHOLD")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(0.75);

        let top_k = env::var("MEMORY_TOP_K")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(10);

        let decay_lambda = env::var("MEMORY_DECAY_LAMBDA")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(0.001);

        let decay_mu = env::var("MEMORY_DECAY_MU")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(0.05);

        let max_records = env::var("MEMORY_MAX_RECORDS")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(50000);

        let min_confidence = env::var("MEMORY_MIN_CONFIDENCE")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(0.60);

        let min_importance = env::var("MEMORY_MIN_IMPORTANCE")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(2);

        Ok(Self {
            db_path,
            vector_path,
            tantivy_path,
            llm_api_base,
            llm_api_key,
            embedding_model,
            embedding_dim,
            extraction_model,
            extraction_max_tokens,
            dedup_threshold,
            near_dedup_threshold,
            top_k,
            decay_lambda,
            decay_mu,
            max_records,
            min_confidence,
            min_importance,
        })
    }
}
