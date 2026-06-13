use std::sync::Arc;
use serde::{Deserialize, Serialize};
use crate::error::{MemoryError, Result};
use crate::models::MemoryCategory;
use crate::extraction::llm_client::LlmClient;
use crate::extraction::prompt::{EXTRACTION_SYSTEM_PROMPT, extraction_user_prompt};

#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub min_confidence: f64,
    pub min_importance: u8,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-6".to_string(),
            max_tokens: 2048,
            temperature: 0.1,
            min_confidence: 0.60,
            min_importance: 2,
        }
    }
}

pub struct ExtractionEngine {
    llm_client: Arc<LlmClient>,
    embedding_model: String,
    config: ExtractionConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractionResponse {
    pub memories: Vec<ExtractedMemory>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExtractedMemory {
    pub content: String,
    pub category: MemoryCategory,
    pub entities: Vec<String>,
    pub importance: u8,
    pub confidence: f64,
}

impl ExtractionEngine {
    pub fn new(llm_client: Arc<LlmClient>, embedding_model: &str, config: ExtractionConfig) -> Self {
        Self {
            llm_client,
            embedding_model: embedding_model.to_string(),
            config,
        }
    }

    pub async fn extract(&self, conversation: &str) -> Result<Vec<ExtractedMemory>> {
        let user_prompt = extraction_user_prompt(conversation);
        
        let raw_json = self.llm_client
            .complete(
                EXTRACTION_SYSTEM_PROMPT,
                &user_prompt,
                &self.config.model,
                self.config.max_tokens,
                self.config.temperature,
            )
            .await?;

        // Handle possible markdown codeblock wrapping
        let mut cleaned = raw_json.trim();
        if cleaned.starts_with("```") {
            cleaned = cleaned.trim_start_matches("```");
            if cleaned.starts_with("json") {
                cleaned = cleaned.trim_start_matches("json");
            }
            cleaned = cleaned.trim_end_matches("```").trim();
        }

        let response: ExtractionResponse = serde_json::from_str(cleaned)
            .map_err(|e| MemoryError::ExtractionParseFailed(format!("Failed to parse JSON: {}. Cleaned text was: {}", e, cleaned)))?;

        // Apply quality filters
        let filtered: Vec<_> = response.memories
            .into_iter()
            .filter(|m| m.confidence >= self.config.min_confidence)
            .filter(|m| m.importance >= self.config.min_importance)
            .filter(|m| !m.content.trim().is_empty())
            .collect();

        Ok(filtered)
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.llm_client.embed(text, &self.embedding_model).await
    }
}
