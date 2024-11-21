use crate::error::HudError;
use crate::strings;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

#[async_trait]
pub trait Summarizer {
    async fn summarize(&self, diff: &str) -> Result<String>;
}

pub struct ClaudeSummarizer {
    client: reqwest::Client,
    api_key: String,
}

impl ClaudeSummarizer {
    pub fn new() -> Result<Self> {
        let api_key = std::env::var(strings::ANTHROPIC_API_KEY)
            .map_err(|_| HudError::Api("ANTHROPIC_API_KEY not set".to_string()))?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct ContentAPIResponse {
    text: String,
    #[serde(rename = "type")]
    response_type: String,
}
#[derive(Serialize, Deserialize)]
struct TokenUsageAPIResponse {
    input_tokens: u32,
    output_tokens: u32,
}
#[derive(Serialize, Deserialize)]
struct AnthropicAPIResponse {
    content: Vec<ContentAPIResponse>,
    id: String,
    model: String,
    role: String,
    stop_reason: String,
    stop_sequence: String,
    #[serde(rename = "type")]
    response_type: String,
    usage: TokenUsageAPIResponse,
}

#[async_trait]
impl Summarizer for ClaudeSummarizer {
    async fn summarize(&self, diff: &str) -> Result<String> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&*self.api_key)?,
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

        let request_body = serde_json::json!({
            "model": "claude-3-haiku-20240307",
            "max_tokens": 1024,
            "messages": [{
                "role": "user",
                "content": format!(
                    "Summarize this git diff in ONE SHORT LINE (max 50 chars). Focus on the semantic changes, not the mechanical ones. Here's the diff:\n\n{}",
                    diff
                )
            }]
        });
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .headers(headers)
            .json(&request_body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow::anyhow!("Claude API error: {}", error_text));
        }

        let response = response.json::<serde_json::Value>().await?;

        // Extract the content from the response
        let content = response["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Unexpected API response format"))?
            .trim();

        Ok(content.to_string())

        // We'll implement this next
    }
}
