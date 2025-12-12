//! LLM integration module
//!
//! This module provides functionality for interacting with Large Language Models:
//! - `OpenAI` GPT models
//! - Ollama local models
//! - Custom endpoints
//! - Streaming responses
//!
//! # Examples
//!
//! ```rust,no_run
//! use snaprag::llm::LlmService;
//! use snaprag::config::AppConfig;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = AppConfig::load()?;
//!     let service = LlmService::new(&config)?;
//!     
//!     let response = service.generate("What is Farcaster?").await?;
//!     println!("Response: {}", response);
//!     
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod prompts;
pub mod streaming;

pub use client::ChatMessage;
pub use client::LlmClient;
pub use client::LlmProvider;
pub use prompts::PromptTemplate;
pub use streaming::StreamingResponse;

use crate::errors::Result;

/// Configuration for LLM service
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub model: String,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub temperature: f32,
    pub max_tokens: usize,
}

impl LlmConfig {
    #[must_use]
    pub fn from_app_config(config: &crate::config::AppConfig) -> Self {
        // Determine provider based on endpoint
        let provider = if config.llm_endpoint().contains("api.openai.com") {
            LlmProvider::OpenAI
        } else if config.llm_key() == "ollama" {
            LlmProvider::Ollama
        } else {
            LlmProvider::Custom
        };

        // Get model from config or use default based on provider
        let model = if config.llm_model().is_empty() {
            match provider {
                LlmProvider::OpenAI => "gpt-4".to_string(),
                LlmProvider::Ollama => "gemma3:27b".to_string(),
                LlmProvider::Custom => "default".to_string(),
            }
        } else {
            config.llm_model().to_string()
        };

        Self {
            provider,
            model,
            endpoint: config.llm_endpoint().to_string(),
            api_key: if provider == LlmProvider::OpenAI || provider == LlmProvider::Custom {
                // Both OpenAI and Custom providers may need API keys
                // For Custom, use the key if it's not "ollama" (which is the Ollama marker)
                if provider == LlmProvider::Custom && config.llm_key() == "ollama" {
                    None
                } else {
                    Some(config.llm_key().to_string())
                }
            } else {
                None
            },
            temperature: 0.7,
            max_tokens: 2000,
        }
    }
}

/// Main LLM service
#[derive(Clone)]
pub struct LlmService {
    client: LlmClient,
    config: LlmConfig,
}

impl LlmService {
    /// Create a new LLM service from app config
    pub fn new(config: &crate::config::AppConfig) -> Result<Self> {
        let llm_config = LlmConfig::from_app_config(config);
        let client = LlmClient::new(
            llm_config.provider,
            llm_config.model.clone(),
            llm_config.endpoint.clone(),
            llm_config.api_key.clone(),
        )?;

        Ok(Self {
            client,
            config: llm_config,
        })
    }

    /// Create from custom config
    pub fn from_config(config: LlmConfig) -> Result<Self> {
        let client = LlmClient::new(
            config.provider,
            config.model.clone(),
            config.endpoint.clone(),
            config.api_key.clone(),
        )?;

        Ok(Self { client, config })
    }

    /// Generate a response from a prompt
    pub async fn generate(&self, prompt: &str) -> Result<String> {
        self.client
            .generate(prompt, self.config.temperature, self.config.max_tokens)
            .await
    }

    /// Generate a response with custom parameters
    pub async fn generate_with_params(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        self.client.generate(prompt, temperature, max_tokens).await
    }

    /// Generate a streaming response
    pub async fn generate_stream(&self, prompt: &str) -> Result<StreamingResponse> {
        self.client
            .generate_stream(prompt, self.config.temperature, self.config.max_tokens)
            .await
    }

    /// Chat completion with message history
    pub async fn chat(&self, messages: Vec<crate::llm::client::ChatMessage>) -> Result<String> {
        self.client
            .chat(messages, self.config.temperature, self.config.max_tokens)
            .await
    }

    /// Get the model name
    #[must_use]
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Get the provider
    #[must_use]
    pub const fn provider(&self) -> LlmProvider {
        self.config.provider
    }
}
