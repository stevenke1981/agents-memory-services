use crate::error::{MemoryError, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

const DEFAULT_CONNECT_TIMEOUT_MS: u64 = 3_000;
const DEFAULT_CHAT_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_EMBEDDING_TIMEOUT_MS: u64 = 5_000;

pub struct LlmClient {
    client: reqwest::Client,
    api_base: String,
    embedding_api_base: String,
    api_key: String,
    embedding_dim: usize,
    chat_timeout: Duration,
    embedding_timeout: Duration,
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
        let embedding_dim = read_env_usize("EMBEDDING_DIM", 1024, 1, 65_536);
        Self::new_with_embedding_dim(api_base, embedding_api_base, api_key, embedding_dim)
    }

    pub fn new_with_embedding_dim(
        api_base: &str,
        embedding_api_base: &str,
        api_key: &str,
        embedding_dim: usize,
    ) -> Self {
        let connect_timeout = read_env_duration(
            "LLM_CONNECT_TIMEOUT_MS",
            DEFAULT_CONNECT_TIMEOUT_MS,
            100,
            120_000,
        );
        let chat_timeout =
            read_env_duration("LLM_CHAT_TIMEOUT_MS", DEFAULT_CHAT_TIMEOUT_MS, 500, 600_000);
        let embedding_timeout = read_env_duration(
            "EMBEDDING_TIMEOUT_MS",
            DEFAULT_EMBEDDING_TIMEOUT_MS,
            100,
            120_000,
        );
        let client = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .build()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "failed to configure HTTP client; using reqwest defaults");
                reqwest::Client::new()
            });

        Self {
            client,
            api_base: api_base.trim_end_matches('/').to_string(),
            embedding_api_base: embedding_api_base.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            embedding_dim,
            chat_timeout,
            embedding_timeout,
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
        if self.is_mock() {
            if self.api_key == "mock-fail-parse" {
                return Ok("not valid json { broken".to_string());
            }

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
        let request = ChatCompletionRequest {
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
            .timeout(self.chat_timeout)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MemoryError::ExtractionFailed(format!(
                "HTTP Status {}: {}",
                status, error_text
            )));
        }

        let response: ChatCompletionResponse = response.json().await?;
        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .ok_or_else(|| {
                MemoryError::ExtractionFailed(
                    "No choices returned from LLM completions".to_string(),
                )
            })
    }

    pub async fn embed(&self, text: &str, model: &str) -> Result<Vec<f32>> {
        if self.is_mock() {
            return Ok(vec![0.1; self.embedding_dim]);
        }

        let url = format!("{}/embeddings", self.embedding_api_base);
        let request = EmbeddingRequest {
            model: model.to_string(),
            input: text.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .timeout(self.embedding_timeout)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(MemoryError::Other(format!(
                "Embedding API error {}: {}",
                status, error_text
            )));
        }

        let response: EmbeddingResponse = response.json().await?;
        response
            .data
            .first()
            .map(|data| data.embedding.clone())
            .ok_or_else(|| {
                MemoryError::Other("No embedding returned from embedding API".to_string())
            })
    }

    fn is_mock(&self) -> bool {
        self.api_key.starts_with("mock")
            || (self.api_key == "local"
                && (self.api_base == "mock" || self.embedding_api_base == "mock"))
    }
}

fn read_env_usize(name: &str, fallback: usize, min: usize, max: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(min, max))
        .unwrap_or(fallback)
}

fn read_env_duration(name: &str, fallback_ms: u64, min_ms: u64, max_ms: u64) -> Duration {
    let milliseconds = env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.clamp(min_ms, max_ms))
        .unwrap_or(fallback_ms);
    Duration::from_millis(milliseconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embedding_respects_configured_dimension() {
        let client = LlmClient::new_with_embedding_dim("mock", "mock", "mock", 8);
        let vector = client.embed("test", "mock-model").await.unwrap();
        assert_eq!(vector.len(), 8);
    }

    #[tokio::test]
    async fn supports_mock_parse_failures() {
        let client = LlmClient::new_with_embedding_dim("mock", "mock", "mock-fail-parse", 8);
        let response = client
            .complete("system", "user", "mock-model", 100, 0.1)
            .await
            .unwrap();
        assert_eq!(response, "not valid json { broken");
    }
}
