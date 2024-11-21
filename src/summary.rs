use crate::error::HudError;
use anyhow::Result;
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue};
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
        let api_key = std::env::var("ANTHROPIC_API_KEY")
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
        let c_summarizer = ClaudeSummarizer::new()?;
        let mut headers = HeaderMap::new();
        let base_prompt = "Your job is to give a single sentence summary of the changes in this diff";
        let full_prompt = [base_prompt, diff].join(" ");
        headers.insert("X-Api-Key", HeaderValue::from_str(&self.api_key)?);
        headers.insert("anthropic-version", "2023-06-01".parse()?);
        headers.insert("content-type", "application/json".parse()?);
        let request_body = r#"{
            "model": "claude-3-5-haiku-20241022",
            "max_tokens": "256",
            "messages": [
                {"role": "user", "content": full_prompt },
            ]
        }"#;
        let response = self.client.post("https://api.anthropic.com/v1/messages")
            .headers(headers)
            .json(&request_body)
            .send().await?;

        let message: AnthropicAPIResponse = response.json().await?;

        return Ok(message.content[0].text.clone());

        // We'll implement this next
    }
}
