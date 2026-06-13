use std::fs::OpenOptions;
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::sync::RwLock;
use crate::error::{MemoryError, Result};

pub struct VectorRecord {
    pub id: i64,
    pub vector: Vec<f32>,
}

pub struct VectorStore {
    records: RwLock<Vec<VectorRecord>>,
    dimensions: usize,
    path: String,
}

impl VectorStore {
    pub fn new(path: &str, dimensions: usize) -> Result<Self> {
        let mut records = Vec::new();

        if Path::new(path).exists() {
            let mut file = OpenOptions::new().read(true).open(path)?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;

            let record_size = 8 + dimensions * 4; // 8 bytes for id (i64) + dimensions * 4 bytes for f32
            let num_records = buffer.len() / record_size;

            for i in 0..num_records {
                let offset = i * record_size;
                let id = i64::from_le_bytes(buffer[offset..offset + 8].try_into().unwrap());
                
                let mut vector = Vec::with_capacity(dimensions);
                for d in 0..dimensions {
                    let float_offset = offset + 8 + d * 4;
                    let float_val = f32::from_le_bytes(buffer[float_offset..float_offset + 4].try_into().unwrap());
                    vector.push(float_val);
                }
                records.push(VectorRecord { id, vector });
            }
        } else {
            // Ensure directory exists
            if let Some(parent) = Path::new(path).parent() {
                std::fs::create_dir_all(parent)?;
            }
            // Create an empty file
            OpenOptions::new().write(true).create(true).open(path)?;
        }

        Ok(Self {
            records: RwLock::new(records),
            dimensions,
            path: path.to_string(),
        })
    }

    pub fn add(&self, id: i64, vector: &[f32]) -> Result<()> {
        if vector.len() != self.dimensions {
            return Err(MemoryError::VectorIndex(format!(
                "Dimension mismatch. Expected {}, got {}",
                self.dimensions,
                vector.len()
            )));
        }

        // 1. Append to file
        let mut file = OpenOptions::new().write(true).append(true).open(&self.path)?;
        file.write_all(&id.to_le_bytes())?;
        for &val in vector {
            file.write_all(&val.to_le_bytes())?;
        }
        file.flush()?;

        // 2. Append to memory records
        let mut records_guard = self.records.write().unwrap();
        records_guard.push(VectorRecord {
            id,
            vector: vector.to_vec(),
        });

        Ok(())
    }

    pub fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        if query.len() != self.dimensions {
            return Err(MemoryError::VectorIndex(format!(
                "Dimension mismatch. Expected {}, got {}",
                self.dimensions,
                query.len()
            )));
        }

        let records_guard = self.records.read().unwrap();
        let mut results = Vec::new();

        for rec in records_guard.iter() {
            let similarity = cosine_similarity(&rec.vector, query);
            results.push((rec.id, similarity));
        }

        // Sort by similarity descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        Ok(results)
    }

    pub fn remove(&self, _id: i64) -> Result<()> {
        // Dynamic deletion can be filtered by SQLite lookup, but we can also implement in-memory removal
        // and rewrite the file if needed. Since deletion is rare, in-memory filtering by SQLite is enough.
        Ok(())
    }

    pub fn size(&self) -> usize {
        let records_guard = self.records.read().unwrap();
        records_guard.len()
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..a.len() {
        dot_product += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}
