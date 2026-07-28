use crate::error::Result;
use std::env;
use std::path::PathBuf;

fn is_placeholder(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || value.contains("${")
        || value.contains("$(")
        || value.starts_with('$')
        || (value.len() > 2 && value.starts_with('%') && value.ends_with('%'))
}

#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub db_path: String,
    pub vector_path: String,
    pub tantivy_path: String,
    pub llm_api_base: String,
    pub embedding_api_base: String,
    pub llm_api_key: String,
    pub embedding_model: String,
    pub embedding_dim: usize,
    pub extraction_model: String,
    pub extraction_max_tokens: u32,
    pub dedup_threshold: f64,
    pub near_dedup_threshold: f64,
    pub top_k: usize,
    pub decay_lambda: f64,
    pub temporal_mu: f64,
    pub max_records: usize,
    pub min_confidence: f64,
    pub min_importance: u8,
}

impl MemoryConfig {
    pub fn from_env() -> Result<Self> {
        // Reject empty or unexpanded template values so a copied client config cannot
        // create a literal `${PROJECT_ROOT}/.opencode` directory inside the host app.
        let project_root = env::var("PROJECT_ROOT")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !is_placeholder(value))
            .map(PathBuf::from)
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let base_dir = project_root.join(".opencode");

        let db_path = env::var("MEMORY_DB_PATH")
            .unwrap_or_else(|_| base_dir.join("memory.db").to_string_lossy().into_owned());

        let vector_path = env::var("MEMORY_VECTOR_PATH").unwrap_or_else(|_| {
            base_dir
                .join("vectors.usearch")
                .to_string_lossy()
                .into_owned()
        });

        let tantivy_path = env::var("MEMORY_TANTIVY_PATH")
            .unwrap_or_else(|_| base_dir.join("tantivy").to_string_lossy().into_owned());

        let llm_api_base =
            env::var("LLM_API_BASE").unwrap_or_else(|_| "http://localhost:8080/v1".to_string());

        let embedding_api_base =
            env::var("EMBEDDING_API_BASE").unwrap_or_else(|_| llm_api_base.clone());

        let llm_api_key = env::var("LLM_API_KEY").unwrap_or_else(|_| "local".to_string());

        let embedding_model =
            env::var("EMBEDDING_MODEL").unwrap_or_else(|_| "qwen3-embedding:0.6b".to_string());

        let embedding_dim = env::var("EMBEDDING_DIM")
            .ok()
            .and_then(|val| val.parse().ok())
            .unwrap_or(1024);

        let extraction_model = env::var("EXTRACTION_MODEL").unwrap_or_else(|_| "phi-4".to_string());

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

        let temporal_mu = env::var("MEMORY_TEMPORAL_MU")
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
            embedding_api_base,
            llm_api_key,
            embedding_model,
            embedding_dim,
            extraction_model,
            extraction_max_tokens,
            dedup_threshold,
            near_dedup_threshold,
            top_k,
            decay_lambda,
            temporal_mu,
            max_records,
            min_confidence,
            min_importance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unexpanded_project_root_placeholders() {
        for value in [
            "",
            "   ",
            "${PROJECT_ROOT}",
            "$PROJECT_ROOT",
            "$(pwd)",
            "%PROJECT_ROOT%",
        ] {
            assert!(is_placeholder(value), "expected placeholder: {value:?}");
        }
    }

    #[test]
    fn accepts_normal_project_paths() {
        for value in [
            "/absolute/project",
            "relative/project",
            r"C:\Users\owner\project",
        ] {
            assert!(!is_placeholder(value), "expected normal path: {value:?}");
        }
    }
}
