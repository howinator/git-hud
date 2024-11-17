use crate::error::HudError;
use anyhow::Result;
use async_trait::async_trait;

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

#[async_trait]
impl Summarizer for ClaudeSummarizer {
    async fn summarize(&self, diff: &str) -> Result<String> {
        // We'll implement this next
        todo!()
    }
}
