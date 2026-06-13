use crate::error::Result;
use crate::storage::VectorStore;
use std::sync::Arc;

/// Semantic Retriever — wraps USearch HNSW index for dense vector similarity search.
pub struct SemanticRetriever {
    vector_store: Arc<VectorStore>,
}

impl SemanticRetriever {
    pub fn new(vector_store: Arc<VectorStore>) -> Self {
        Self { vector_store }
    }

    /// Search for top-k semantically similar vectors.
    /// Returns `Vec<(vector_id, cosine_similarity)>`.
    pub fn search(&self, query_vec: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        self.vector_store.search(query_vec, top_k)
    }
}
