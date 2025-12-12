//! LLM API clients for various providers

use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::info;

use crate::errors::Result;
use crate::errors::SnapragError;
use crate::llm::streaming::StreamingResponse;

/// Supported LLM providers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProvider {
    /// `OpenAI` GPT models
    OpenAI,
    /// Ollama local models
    Ollama,
    /// Custom endpoint
    Custom,
}

/// Chat message for conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

/// Client for interacting with LLMs
#[derive(Clone)]
pub struct LlmClient {
    provider: LlmProvider,
    model: String,
    endpoint: String,
    api_key: Option<String>,
    client: Client,
}

impl LlmClient {
    /// Create a new LLM client
    ///
    /// # Errors
    /// - HTTP client creation errors
    pub fn new(
        provider: LlmProvider,
        model: String,
        endpoint: String,
        api_key: Option<String>,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| SnapragError::HttpError(e.to_string()))?;

        Ok(Self {
            provider,
            model,
            endpoint,
            api_key,
            client,
        })
    }

    /// Generate a response from a prompt
    ///
    /// # Errors
    /// - LLM API call errors (network, authentication, rate limits)
    /// - Invalid response format
    /// - JSON parsing errors
    pub async fn generate(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        match self.provider {
            LlmProvider::OpenAI => self.generate_openai(prompt, temperature, max_tokens).await,
            LlmProvider::Ollama => self.generate_ollama(prompt, temperature, max_tokens).await,
            LlmProvider::Custom => self.generate_custom(prompt, temperature, max_tokens).await,
        }
    }

    /// Generate a streaming response
    ///
    /// # Errors
    /// - LLM API call errors (network, authentication, rate limits)
    /// - Streaming connection errors
    /// - Not supported for custom provider
    pub async fn generate_stream(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<StreamingResponse> {
        match self.provider {
            LlmProvider::OpenAI => {
                self.generate_openai_stream(prompt, temperature, max_tokens)
                    .await
            }
            LlmProvider::Ollama => {
                self.generate_ollama_stream(prompt, temperature, max_tokens)
                    .await
            }
            LlmProvider::Custom => Err(SnapragError::LlmError(
                "Streaming not supported for custom provider".to_string(),
            )),
        }
    }

    /// Chat completion with message history
    ///
    /// # Errors
    /// - LLM API call errors (network, authentication, rate limits)
    /// - Invalid response format
    /// - JSON parsing errors
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        match self.provider {
            LlmProvider::OpenAI => self.chat_openai(messages, temperature, max_tokens).await,
            LlmProvider::Ollama => self.chat_ollama(messages, temperature, max_tokens).await,
            LlmProvider::Custom => self.chat_custom(messages, temperature, max_tokens).await,
        }
    }

    /// `OpenAI` completion
    async fn generate_openai(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| SnapragError::ConfigError("OpenAI API key not provided".to_string()))?;

        #[derive(Serialize)]
        struct OpenAIRequest<'a> {
            model: &'a str,
            messages: Vec<ChatMessage>,
            temperature: f32,
            max_tokens: usize,
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ChatMessage,
        }

        let url = format!("{}/chat/completions", self.endpoint);
        debug!("Calling OpenAI API: {}", url);

        let request = OpenAIRequest {
            model: &self.model,
            messages: vec![ChatMessage::user(prompt)],
            temperature,
            max_tokens,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| SnapragError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SnapragError::LlmError(format!(
                "OpenAI API error ({status}): {error_text}"
            )));
        }

        let result: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| SnapragError::LlmError(format!("Failed to parse response: {e}")))?;

        result
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| SnapragError::LlmError("No response from OpenAI".to_string()))
    }

    /// `OpenAI` chat completion
    async fn chat_openai(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| SnapragError::ConfigError("OpenAI API key not provided".to_string()))?;

        #[derive(Serialize)]
        struct OpenAIRequest<'a> {
            model: &'a str,
            messages: Vec<ChatMessage>,
            temperature: f32,
            max_tokens: usize,
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ChatMessage,
        }

        let url = format!("{}/chat/completions", self.endpoint);

        let request = OpenAIRequest {
            model: &self.model,
            messages,
            temperature,
            max_tokens,
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| SnapragError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SnapragError::LlmError(format!(
                "OpenAI API error ({status}): {error_text}"
            )));
        }

        let result: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| SnapragError::LlmError(format!("Failed to parse response: {e}")))?;

        result
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| SnapragError::LlmError("No response from OpenAI".to_string()))
    }

    /// `OpenAI` streaming
    async fn generate_openai_stream(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<StreamingResponse> {
        // Use unified fallback-to-stream pattern
        let response = self
            .generate_openai(prompt, temperature, max_tokens)
            .await?;
        Ok(Self::wrap_in_stream(response))
    }

    /// Ollama completion
    async fn generate_ollama(
        &self,
        prompt: &str,
        temperature: f32,
        _max_tokens: usize,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct OllamaRequest<'a> {
            model: &'a str,
            prompt: &'a str,
            stream: bool,
            options: OllamaOptions,
        }

        #[derive(Serialize)]
        struct OllamaOptions {
            temperature: f32,
        }

        #[derive(Deserialize)]
        struct OllamaResponse {
            response: String,
        }

        let url = format!("{}/api/generate", self.endpoint);
        debug!("Calling Ollama API: {}", url);

        let request = OllamaRequest {
            model: &self.model,
            prompt,
            stream: false,
            options: OllamaOptions { temperature },
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| SnapragError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SnapragError::LlmError(format!(
                "Ollama API error ({status}): {error_text}"
            )));
        }

        let result: OllamaResponse = response
            .json()
            .await
            .map_err(|e| SnapragError::LlmError(format!("Failed to parse response: {e}")))?;

        Ok(result.response)
    }

    /// Ollama chat completion
    async fn chat_ollama(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f32,
        _max_tokens: usize,
    ) -> Result<String> {
        #[derive(Serialize)]
        struct OllamaRequest<'a> {
            model: &'a str,
            messages: Vec<ChatMessage>,
            stream: bool,
            options: OllamaOptions,
        }

        #[derive(Serialize)]
        struct OllamaOptions {
            temperature: f32,
        }

        #[derive(Deserialize)]
        struct OllamaResponse {
            message: ChatMessage,
        }

        let url = format!("{}/api/chat", self.endpoint);

        let request = OllamaRequest {
            model: &self.model,
            messages,
            stream: false,
            options: OllamaOptions { temperature },
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| SnapragError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SnapragError::LlmError(format!(
                "Ollama API error ({status}): {error_text}"
            )));
        }

        let result: OllamaResponse = response
            .json()
            .await
            .map_err(|e| SnapragError::LlmError(format!("Failed to parse response: {e}")))?;

        Ok(result.message.content)
    }

    /// Ollama streaming
    async fn generate_ollama_stream(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<StreamingResponse> {
        // Use unified fallback-to-stream pattern
        let response = self
            .generate_ollama(prompt, temperature, max_tokens)
            .await?;
        Ok(Self::wrap_in_stream(response))
    }

    /// Custom provider completion (OpenAI-compatible API)
    async fn generate_custom(
        &self,
        prompt: &str,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        // Use chat API with single user message
        self.chat_custom(vec![ChatMessage::user(prompt)], temperature, max_tokens)
            .await
    }

    /// Custom provider chat completion (OpenAI-compatible API)
    async fn chat_custom(
        &self,
        messages: Vec<ChatMessage>,
        temperature: f32,
        max_tokens: usize,
    ) -> Result<String> {
        // Custom provider uses OpenAI-compatible API format
        let api_key = self.api_key.as_ref();

        #[derive(Serialize)]
        struct OpenAIRequest<'a> {
            model: &'a str,
            messages: Vec<ChatMessage>,
            temperature: f32,
            max_tokens: usize,
        }

        #[derive(Deserialize)]
        struct OpenAIResponse {
            choices: Vec<Choice>,
        }

        #[derive(Deserialize)]
        struct Choice {
            message: ChatMessage,
        }

        let url = format!("{}/chat/completions", self.endpoint);
        debug!("Calling custom LLM API: {}", url);

        let request = OpenAIRequest {
            model: &self.model,
            messages,
            temperature,
            max_tokens,
        };

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        // Add Authorization header if API key is provided
        if let Some(key) = api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {key}"));
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .map_err(|e| SnapragError::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SnapragError::LlmError(format!(
                "Custom LLM API error ({status}): {error_text}"
            )));
        }

        let result: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| SnapragError::LlmError(format!("Failed to parse response: {e}")))?;

        result
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| SnapragError::LlmError("No response from custom LLM".to_string()))
    }

    /// Unified helper: Convert non-streaming response to streaming
    /// This provides a consistent interface while allowing future SSE implementation
    fn wrap_in_stream(response: String) -> StreamingResponse {
        use futures::stream;
        let stream = stream::once(async move { Ok(response) });
        StreamingResponse::new(Box::pin(stream))
    }
}
