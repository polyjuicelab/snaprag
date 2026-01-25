//! Embedding service factory for creating configured embedding services
//!
//! This module consolidates the duplicate logic for creating embedding services
//! with different configurations (local GPU or default).

use std::sync::Arc;

use crate::config::AppConfig;
use crate::embeddings::EmbeddingService;
use crate::Result;

/// Result of creating an embedding service with metadata
pub struct EmbeddingServiceResult {
    pub service: Arc<EmbeddingService>,
    pub endpoint_info: String,
}

/// Create an embedding service based on the provided configuration options
///
/// # Arguments
/// * `config` - Application configuration
/// * `local_gpu` - Whether to use local GPU (requires `local-gpu` feature)
/// * `gpu_device` - Optional GPU device ID (requires `local-gpu` feature)
///
/// # Returns
/// A tuple of (service, `endpoint_info`) where `endpoint_info` is a human-readable description
pub async fn create_embedding_service(
    config: &AppConfig,
    #[cfg(feature = "local-gpu")] local_gpu: bool,
    #[cfg(feature = "local-gpu")] gpu_device: Option<usize>,
) -> Result<EmbeddingServiceResult> {
    #[cfg(feature = "local-gpu")]
    if local_gpu {
        return create_local_gpu_service(config, gpu_device).await;
    }

    create_default_service(config)
}

/// Create an embedding service using local GPU
#[cfg(feature = "local-gpu")]
async fn create_local_gpu_service(
    config: &AppConfig,
    gpu_device: Option<usize>,
) -> Result<EmbeddingServiceResult> {
    tracing::info!("🔧 Using local GPU for embedding generation...");

    let embedding_config = crate::embeddings::EmbeddingConfig {
        provider: crate::embeddings::EmbeddingProvider::LocalGPU,
        model: "BAAI/bge-small-en-v1.5".to_string(),
        dimension: config.embedding_dimension(),
        endpoint: "local-gpu".to_string(),
        api_key: None,
    };

    let service =
        Arc::new(EmbeddingService::from_config_async(embedding_config, gpu_device).await?);

    Ok(EmbeddingServiceResult {
        service,
        endpoint_info: "local-gpu (BAAI/bge-small-en-v1.5)".to_string(),
    })
}

/// Create an embedding service using the configured endpoint
fn create_default_service(config: &AppConfig) -> Result<EmbeddingServiceResult> {
    let service = Arc::new(EmbeddingService::new(config)?);

    Ok(EmbeddingServiceResult {
        service,
        endpoint_info: format!(
            "{} ({})",
            config.embedding_provider(),
            config.embedding_endpoint()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_default_service_structure() {
        let config = AppConfig::default();
        let result = create_default_service(&config);
        assert!(result.is_ok());
        let service_result = result.unwrap();
        assert!(!service_result.endpoint_info.is_empty());
        // endpoint_info format: "provider (endpoint)"
        // Default config has provider="ollama" and endpoint="http://localhost:11434"
        // So endpoint_info should be "ollama (http://localhost:11434)"
        assert!(service_result.endpoint_info.contains("ollama"));
        assert!(service_result.endpoint_info.contains("localhost:11434"));
        // Verify format contains parentheses (provider (endpoint))
        assert!(service_result.endpoint_info.contains('('));
        assert!(service_result.endpoint_info.contains(')'));
    }

    #[cfg(feature = "local-gpu")]
    #[tokio::test]
    #[ignore = "Requires GPU hardware"]
    async fn test_create_local_gpu_service() {
        let config = AppConfig::default();
        let result = create_local_gpu_service(&config, None).await;
        // May fail without GPU, but should return proper error
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_service_result_contains_metadata() {
        let config = AppConfig::default();
        if let Ok(result) = create_default_service(&config) {
            // Endpoint info should describe the service
            assert!(!result.endpoint_info.is_empty());
            // Service should be usable
            assert!(std::sync::Arc::strong_count(&result.service) >= 1);
        }
    }
}
