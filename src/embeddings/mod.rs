//! Vector embeddings generation and management
//!
//! This module provides high-performance embedding generation with support for multiple providers
//! and backends, including `OpenAI`, Ollama, and local GPU acceleration.
//!
//! # Providers
//!
//! - **`OpenAI`**: Cloud-based (text-embedding-ada-002, text-embedding-3-small)
//! - **Ollama**: Local LLM server with various models
//! - **Local GPU**: Direct GPU acceleration with Candle (optional feature)
//! - **Custom**: Any OpenAI-compatible API endpoint
//!
//! # Features
//!
//! - **Multi-vector embeddings**: Chunk long texts for better retrieval
//! - **Batch processing**: Efficient bulk embedding generation
//! - **Parallel tasks**: Concurrent requests for high throughput
//! - **Backfill**: Generate embeddings for existing data
//! - **Migration**: Safely migrate between embedding models
//!
//! # Basic Usage
//!
//! ## Single Embedding
//!
//! ```rust,no_run
//! use snaprag::embeddings::EmbeddingService;
//! use snaprag::config::AppConfig;
//!
//! # async fn example() -> snaprag::Result<()> {
//! let config = AppConfig::load()?;
//! let service = EmbeddingService::new(&config)?;
//!     
//! let embedding = service.generate("Hello, world!").await?;
//! println!("Generated embedding with {} dimensions", embedding.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Batch Generation
//!
//! ```rust,no_run
//! use snaprag::embeddings::EmbeddingService;
//!
//! # async fn example(service: &EmbeddingService) -> snaprag::Result<()> {
//! let texts = vec!["First text", "Second text", "Third text"];
//! let embeddings = service.generate_batch(texts).await?;
//! println!("Generated {} embeddings", embeddings.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Backfill Existing Data
//!
//! ```rust,no_run
//! use snaprag::embeddings::{backfill_embeddings, EmbeddingService};
//! use snaprag::Database;
//! use std::sync::Arc;
//!
//! # async fn example(database: Arc<Database>, service: Arc<EmbeddingService>) -> snaprag::Result<()> {
//! // Backfill profile embeddings
//! let stats = backfill_embeddings(
//!     database,
//!     service
//! ).await?;
//!
//! println!("✅ Updated: {}, ⏭️ Skipped: {}", stats.updated, stats.skipped);
//! # Ok(())
//! # }
//! ```
//!
//! # Configuration
//!
//! Configure multiple endpoints in `config.toml`:
//!
//! ```toml
//! [[embeddings.endpoints]]
//! name = "ollama"
//! endpoint = "http://localhost:11434"
//! model = "bge-small-en-v1.5"
//! provider = "ollama"
//!
//! [[embeddings.endpoints]]
//! name = "openai"
//! endpoint = "https://api.openai.com/v1"
//! api_key = "sk-..."
//! model = "text-embedding-3-small"
//! provider = "openai"
//! ```
//!
//! # Performance Tuning
//!
//! ```toml
//! [embeddings]
//! batch_size = 500          # Items per batch
//! parallel_tasks = 100      # Concurrent requests
//! cpu_threads = 0           # Auto-detect CPU cores
//! ```
//!
//! # Text Preprocessing
//!
//! For long or complex texts, use preprocessing utilities:
//!
//! ```rust,no_run
//! use snaprag::embeddings::{preprocess_text_for_embedding, EmbeddingService};
//!
//! # async fn example(service: &EmbeddingService) -> snaprag::Result<()> {
//! // Preprocess text before generating embedding
//! let text = "Very long text with special characters and formatting...";
//! let processed = preprocess_text_for_embedding(text)?;
//!
//! // Generate embedding for processed text
//! let embedding = service.generate(&processed).await?;
//! println!("Generated embedding: {} dimensions", embedding.len());
//! # Ok(())
//! # }
//! ```

pub mod backfill;
pub mod cast_backfill;
pub mod client;
pub mod generator;
pub mod local_gpu;
pub mod migration;
pub mod multi_vector;
#[cfg(feature = "local-gpu")]
pub mod multiprocess;
pub mod service_factory;
pub mod text_preprocessing;

pub use backfill::backfill_embeddings;
pub use cast_backfill::backfill_cast_embeddings;
#[cfg(feature = "local-gpu")]
pub use cast_backfill::backfill_cast_embeddings_multiprocess;
// pub use cast_backfill::backfill_cast_embeddings_optimized;
pub use cast_backfill::CastBackfillStats;
pub use client::EmbeddingClient;
pub use client::EmbeddingProvider;
pub use generator::EmbeddingService;
pub use migration::analyze_existing_embeddings;
pub use migration::migrate_existing_embeddings;
pub use migration::MigrationAnalysis;
pub use migration::MigrationOptions;
pub use migration::MigrationStats;
pub use multi_vector::AggregationStrategy;
pub use multi_vector::ChunkMetadata;
pub use multi_vector::ChunkStrategy;
pub use multi_vector::ChunkedEmbeddingResult;
pub use multi_vector::MultiVectorEmbeddingService;
#[cfg(feature = "local-gpu")]
pub use multiprocess::MultiProcessConfig;
#[cfg(feature = "local-gpu")]
pub use multiprocess::MultiProcessEmbeddingGenerator;
#[cfg(feature = "local-gpu")]
pub use multiprocess::MultiProcessStats;
pub use service_factory::create_embedding_service;
pub use service_factory::EmbeddingServiceResult;
pub use text_preprocessing::generate_text_chunks;
pub use text_preprocessing::preprocess_text_for_embedding;
pub use text_preprocessing::validate_text_for_embedding;

use crate::errors::Result;

/// Default embedding dimension for `OpenAI` text-embedding-ada-002
pub const DEFAULT_EMBEDDING_DIM: usize = 1536;

/// Maximum batch size for embedding generation
pub const MAX_BATCH_SIZE: usize = 1000;

/// Configuration for embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProvider,
    pub model: String,
    pub dimension: usize,
    pub endpoint: String,
    pub api_key: Option<String>,
}

impl EmbeddingConfig {
    /// Create from app config (uses embeddings section, falls back to LLM endpoint)
    #[must_use]
    pub fn from_app_config(config: &crate::config::AppConfig) -> Self {
        // Try embeddings config first, fall back to LLM config for backward compatibility
        if config.embedding_endpoint().is_empty() {
            // Fallback: use LLM endpoint for backward compatibility
            let provider = if config.llm_key() == "ollama" {
                EmbeddingProvider::Ollama
            } else if config.llm_endpoint().contains("api.openai.com") {
                EmbeddingProvider::OpenAI
            } else {
                EmbeddingProvider::Ollama
            };

            Self {
                provider,
                model: config.embedding_model().to_string(),
                dimension: config.embedding_dimension(),
                endpoint: config.llm_endpoint().to_string(),
                api_key: if provider == EmbeddingProvider::OpenAI {
                    Some(config.llm_key().to_string())
                } else {
                    None
                },
            }
        } else {
            Self::from_embeddings_config(config)
        }
    }

    /// Create from embeddings configuration section
    #[must_use]
    pub fn from_embeddings_config(config: &crate::config::AppConfig) -> Self {
        let provider = match config.embedding_provider().to_lowercase().as_str() {
            "openai" => EmbeddingProvider::OpenAI,
            "ollama" => EmbeddingProvider::Ollama,
            #[cfg(feature = "local-gpu")]
            "local_gpu" => EmbeddingProvider::LocalGPU,
            _ => {
                // Auto-detect based on endpoint
                if config.embedding_endpoint().contains("api.openai.com") {
                    EmbeddingProvider::OpenAI
                } else {
                    EmbeddingProvider::Ollama
                }
            }
        };

        Self {
            provider,
            model: config.embedding_model().to_string(),
            dimension: config.embedding_dimension(),
            endpoint: config.embedding_endpoint().to_string(),
            api_key: config
                .embedding_api_key()
                .map(std::string::ToString::to_string),
        }
    }
}
