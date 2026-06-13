use thiserror::Error;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Vector index error: {0}")]
    VectorIndex(String),

    #[error("Text index error: {0}")]
    TextIndex(#[from] tantivy::TantivyError),

    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    #[error("LLM extraction failed: {0}")]
    ExtractionFailed(String),

    #[error("JSON parsing from LLM failed: {0}")]
    ExtractionParseFailed(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Unknown/Other error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, MemoryError>;
