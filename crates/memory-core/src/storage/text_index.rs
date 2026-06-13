use crate::error::{MemoryError, Result};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tantivy::schema::{Schema, STORED, STRING, TEXT};
use tantivy::{
    collector::TopDocs, doc, query::QueryParser, Index, IndexReader, IndexWriter, SegmentId, Term,
};

pub struct TextIndex {
    index: Index,
    reader: IndexReader,
    writer: Arc<Mutex<IndexWriter>>,
    id_field: tantivy::schema::Field,
    content_field: tantivy::schema::Field,
    category_field: tantivy::schema::Field,
    entities_field: tantivy::schema::Field,
}

impl TextIndex {
    pub fn new(dir_path: &str) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let category_field = schema_builder.add_text_field("category", STRING | STORED);
        let entities_field = schema_builder.add_text_field("entities", TEXT | STORED);
        let schema = schema_builder.build();

        let path = Path::new(dir_path);
        if !path.exists() {
            std::fs::create_dir_all(path)?;
        }

        let mmap_dir = tantivy::directory::MmapDirectory::open(path)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let index = Index::open_or_create(mmap_dir, schema)?;

        // Create reader
        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        // Create writer with 30MB buffer
        let writer = index.writer(30_000_000)?;

        Ok(Self {
            index,
            reader,
            writer: Arc::new(Mutex::new(writer)),
            id_field,
            content_field,
            category_field,
            entities_field,
        })
    }

    pub fn add_document(
        &self,
        id: &str,
        content: &str,
        category: &str,
        entities: &str,
    ) -> Result<()> {
        let mut writer_guard = self.writer.lock().map_err(|e| {
            MemoryError::Other(format!("Failed to acquire text index lock: {:?}", e))
        })?;

        let doc = doc!(
            self.id_field => id,
            self.content_field => content,
            self.category_field => category,
            self.entities_field => entities
        );

        writer_guard.add_document(doc)?;
        writer_guard.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn delete_document(&self, id: &str) -> Result<()> {
        let mut writer_guard = self.writer.lock().map_err(|e| {
            MemoryError::Other(format!("Failed to acquire text index lock: {:?}", e))
        })?;

        let term = Term::from_field_text(self.id_field, id);
        writer_guard.delete_term(term);
        writer_guard.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Compact the Tantivy index by merging segments into a single segment.
    /// This improves search performance and reduces disk usage.
    /// Should be called periodically (e.g., during batch consolidation).
    pub fn compact(&self) -> Result<()> {
        let segment_ids: Vec<SegmentId> = self
            .index
            .searchable_segments()?
            .iter()
            .map(|s| s.id())
            .collect();
        if segment_ids.len() > 1 {
            // Create merge future, drop the lock, then block on the merge.
            let merge_future = {
                let mut writer_guard = self.writer.lock().map_err(|e| {
                    MemoryError::Other(format!("Failed to acquire text index lock: {:?}", e))
                })?;
                // merge() returns FutureResult<Option<SegmentMeta>> directly (not wrapped in Result)
                writer_guard.merge(&segment_ids)
            };
            // Lock is dropped here; merge runs asynchronously.
            // FutureResult<Option<SegmentMeta>> implements Future<Output = Option<SegmentMeta>>
            let _ = tokio::runtime::Handle::current().block_on(merge_future);

            // Re-acquire lock to commit
            let mut writer_guard = self.writer.lock().map_err(|e| {
                MemoryError::Other(format!("Failed to acquire text index lock: {:?}", e))
            })?;
            writer_guard.commit()?;
            self.reader.reload()?;
        }
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        let searcher = self.reader.searcher();

        // Search in content and entities fields
        let query_parser =
            QueryParser::for_index(&self.index, vec![self.content_field, self.entities_field]);
        let query = query_parser
            .parse_query(query_str)
            .map_err(|e| MemoryError::Other(format!("Failed to parse query: {:?}", e)))?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;
            if let Some(tantivy::schema::OwnedValue::Str(id_str)) =
                retrieved_doc.get_first(self.id_field)
            {
                results.push((id_str.clone(), score));
            }
        }

        Ok(results)
    }
}
