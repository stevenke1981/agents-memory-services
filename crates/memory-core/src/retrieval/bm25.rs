use crate::error::Result;
use crate::retrieval::hybrid::normalize_bm25;
use crate::storage::TextIndex;
use std::sync::Arc;

/// BM25 Retriever — wraps Tantivy text index for keyword-based full-text search.
pub struct Bm25Retriever {
    text_index: Arc<TextIndex>,
}

impl Bm25Retriever {
    pub fn new(text_index: Arc<TextIndex>) -> Self {
        Self { text_index }
    }

    /// Search for top-k BM25-scored documents.
    /// Returns raw Tantivy BM25 scores (not normalized).
    pub fn search_raw(&self, query: &str, top_k: usize) -> Result<Vec<(String, f32)>> {
        self.text_index.search(query, top_k)
    }

    /// Search and return min-max normalized BM25 scores in [0.0, 1.0].
    pub fn search_normalized(&self, query: &str, top_k: usize) -> Result<Vec<(String, f32)>> {
        let raw = self.search_raw(query, top_k)?;
        Ok(normalize_bm25(&raw))
    }
}
