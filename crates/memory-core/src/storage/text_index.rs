use std::path::Path;
use std::sync::{Arc, Mutex};
use tantivy::schema::{Schema, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, Term, collector::TopDocs, query::QueryParser};
use crate::error::{MemoryError, Result};

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
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let index = Index::open_or_create(mmap_dir, schema)?;
        
        // Create reader
        let reader = index.reader_builder().reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay).try_into()?;

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

    pub fn add_document(&self, id: &str, content: &str, category: &str, entities: &str) -> Result<()> {
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
        Ok(())
    }

    pub fn delete_document(&self, id: &str) -> Result<()> {
        let mut writer_guard = self.writer.lock().map_err(|e| {
            MemoryError::Other(format!("Failed to acquire text index lock: {:?}", e))
        })?;

        let term = Term::from_field_text(self.id_field, id);
        writer_guard.delete_term(term);
        writer_guard.commit()?;
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<(String, f32)>> {
        let searcher = self.reader.searcher();
        
        // Search in content and entities fields
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field, self.entities_field]);
        let query = query_parser.parse_query(query_str).map_err(|e| {
            MemoryError::Other(format!("Failed to parse query: {:?}", e))
        })?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
        
        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;
            if let Some(id_val) = retrieved_doc.get_first(self.id_field) {
                if let tantivy::schema::OwnedValue::Str(id_str) = id_val {
                    results.push((id_str.clone(), score));
                }
            }
        }

        Ok(results)
    }
}
