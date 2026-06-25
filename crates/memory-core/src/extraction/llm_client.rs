use crate::error::{MemoryError, Result};
use serde::{Deserialize, Serialize};

pub struct LlmClient {
    client: reqwest::Client,
    api_base: String,
    embedding_api_base: String,
    api_key: String,
}

#[derive(Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: Option<u32>,
    response_format: Option<ChatResponseFormat>,
}

#[derive(Serialize)]
struct ChatResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl LlmClient {
    pub fn new(api_base: &str, embedding_api_base: &str, api_key: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_base: api_base.trim_end_matches('/').to_string(),
            embedding_api_base: embedding_api_base.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
        }
    }

    pub async fn complete(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        model: &str,
        max_tokens: u32,
        temperature: f32,
    ) -> Result<String> {
        if self.api_key == "mock" || (self.api_key == "local" && self.api_base == "mock") {
            // Mock response for testing
            return Ok(r#"
            {
              "memories": [
                {
                  "content": "User prefers using tokio::spawn for background tasks in Rust.",
                  "category": "Preference",
                  "entities": ["tokio::spawn", "Rust", "background tasks"],
                  "importance": 4,
                  "confidence": 0.95
                }
              ]
            }
            "#
            .to_string());
        }

        let url = format!("{}/chat/completions", self.api_base);

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ];

        let req = ChatCompletionRequest {
            model: model.to_string(),
            messages,
            temperature,
            max_tokens: Some(max_tokens),
            response_format: Some(ChatResponseFormat {
                format_type: "json_object".to_string(),
            }),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(MemoryError::ExtractionFailed(format!(
                "HTTP Status {}: {}",
                status, err_text
            )));
        }

        let resp: ChatCompletionResponse = response.json().await?;
        let text = resp
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| {
                MemoryError::ExtractionFailed(
                    "No choices returned from LLM completions".to_string(),
                )
            })?;

        Ok(text)
    }

    pub async fn embed(&self, text: &str, model: &str) -> Result<Vec<f32>> {
        if self.api_key == "mock" || (self.api_key == "local" && self.api_base == "mock") {
            // Return dummy vector of dimension 1536 (default)
            return Ok(vec![0.1; 1024]);
        }

        let url = format!("{}/embeddings", self.embedding_api_base);
        let req = EmbeddingRequest {
            model: model.to_string(),
            input: text.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let err_text = response.text().await.unwrap_or_default();
            return Err(MemoryError::Other(format!(
                "Embedding API error {}: {}",
                status, err_text
            )));
        }

        let resp: EmbeddingResponse = response.json().await?;
        let vector = resp
            .data
            .first()
            .map(|d| d.embedding.clone())
            .ok_or_else(|| {
                MemoryError::Other("No embedding returned from embedding API".to_string())
            })?;

        Ok(vector)
    }
}
