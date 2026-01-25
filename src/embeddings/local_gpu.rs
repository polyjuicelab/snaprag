//! Local GPU embedding client for BAAI/bge-small-en-v1.5
//!
//! This module provides local GPU-accelerated embedding generation using the
//! BAAI/bge-small-en-v1.5 model from `HuggingFace`.
//!
//! This module is only available when the `local-gpu` feature is enabled.

use std::path::PathBuf;
use std::sync::Arc;

#[cfg(feature = "local-gpu")]
use candle_core::DType;
#[cfg(feature = "local-gpu")]
use candle_core::Device;
#[cfg(feature = "local-gpu")]
use candle_core::IndexOp;
#[cfg(feature = "local-gpu")]
use candle_core::Module;
#[cfg(feature = "local-gpu")]
use candle_core::Tensor;
#[cfg(feature = "local-gpu")]
use candle_nn::Dropout;
#[cfg(feature = "local-gpu")]
use candle_nn::Embedding;
#[cfg(feature = "local-gpu")]
use candle_nn::LayerNorm;
#[cfg(feature = "local-gpu")]
use candle_nn::Linear;
#[cfg(feature = "local-gpu")]
use candle_nn::VarBuilder;
#[cfg(feature = "local-gpu")]
use candle_transformers::models::bert::BertModel;
#[cfg(feature = "local-gpu")]
use candle_transformers::models::bert::Config as BertConfig;
#[cfg(feature = "local-gpu")]
use hf_hub::api::tokio::Api;
#[cfg(feature = "local-gpu")]
use rayon::prelude::*; // Parallel processing support
#[cfg(feature = "local-gpu")]
use tokenizers::Tokenizer;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::errors::Result;
use crate::errors::SnapragError;

/// Local GPU client for BAAI/bge-small-en-v1.5 embeddings
#[cfg(feature = "local-gpu")]
pub struct LocalGPUClient {
    tokenizer: Tokenizer,
    device: Device,
    model_path: PathBuf,
    embedding_dim: usize,
    model: BertModel,
}

#[cfg(feature = "local-gpu")]
impl LocalGPUClient {
    /// Create a new local GPU client with default 384 dimensions (BGE small model)
    pub async fn new(model_name: &str) -> Result<Self> {
        Self::new_with_dimension(model_name, 384, None).await
    }

    /// Create a new local GPU client with specified embedding dimension and GPU device
    /// BGE small model supports 384 dimensions
    pub async fn new_with_dimension(
        model_name: &str,
        embedding_dim: usize,
        gpu_device_id: Option<usize>,
    ) -> Result<Self> {
        info!(
            "Initializing local GPU client for model: {} with dimension: {}",
            model_name, embedding_dim
        );

        // Validate embedding dimension for BGE small model
        if embedding_dim != 384 {
            return Err(SnapragError::EmbeddingError(format!(
                "BGE small model only supports 384 dimensions, got: {}",
                embedding_dim
            )));
        }

        // Determine device (CUDA > Metal > CPU) with optional GPU selection
        let device = Self::get_device(gpu_device_id)?;
        info!("Using device: {:?}", device);

        // Download model if needed
        let model_path = Self::download_model(model_name).await?;

        // Load tokenizer
        let tokenizer = Self::load_tokenizer(&model_path)?;

        // Load BERT model
        let model = Self::load_bert_model(&model_path, &device)?;

        Ok(Self {
            tokenizer,
            device,
            model_path,
            embedding_dim,
            model,
        })
    }

    /// Get device with optional GPU selection
    fn get_device(gpu_device_id: Option<usize>) -> Result<Device> {
        match gpu_device_id {
            Some(device_id) => {
                // User specified a GPU device ID
                #[cfg(target_os = "macos")]
                {
                    info!("Attempting to use Metal device {}", device_id);
                    match Device::new_metal(device_id) {
                        Ok(device) => {
                            info!("Successfully using Metal device {}", device_id);
                            Ok(device)
                        }
                        Err(e) => {
                            warn!("Failed to use Metal device {}: {}. Falling back to best available device.", device_id, e);
                            Self::get_best_device()
                        }
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    info!("Attempting to use CUDA device {}", device_id);
                    match Device::cuda_if_available(device_id) {
                        Ok(device) => {
                            info!("Successfully using CUDA device {}", device_id);
                            Ok(device)
                        }
                        Err(e) => {
                            warn!("Failed to use CUDA device {}: {}. Falling back to best available device.", device_id, e);
                            Self::get_best_device()
                        }
                    }
                }
            }
            None => {
                // No specific device requested, use best available
                Self::get_best_device()
            }
        }
    }

    /// Get the best available device (CUDA > Metal > CPU)
    fn get_best_device() -> Result<Device> {
        match Device::cuda_if_available(0) {
            Ok(device) => {
                info!("Using CUDA device 0");
                Ok(device)
            }
            Err(e) => {
                warn!("CUDA device 0 not available: {}. Trying Metal...", e);
                match Device::new_metal(0) {
                    Ok(device) => {
                        info!("Using Metal device");
                        Ok(device)
                    }
                    Err(e) => {
                        warn!("Metal device not available: {}. Falling back to CPU.", e);
                        info!("Using CPU device");
                        Ok(Device::Cpu)
                    }
                }
            }
        }
    }

    /// Download model from HuggingFace if not already cached
    async fn download_model(model_name: &str) -> Result<PathBuf> {
        info!("Initializing HuggingFace API...");

        let api = Api::new().map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to initialize HuggingFace API: {}", e))
        })?;

        info!("Creating model repository reference for: {}", model_name);
        let repo = api.model(model_name.to_string());

        // Use a stable cache location
        let cache_root = std::env::var("SNAPRAG_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".cache").join("snaprag")
            });

        let model_dir = cache_root
            .join("hf")
            .join("models")
            .join(model_name.replace("/", "--"))
            .join("main");

        std::fs::create_dir_all(&model_dir).map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to create model directory: {}", e))
        })?;

        // Download model files
        let model_path = model_dir.join("model.safetensors");

        if !model_path.exists() {
            info!("Downloading model.safetensors...");
            let api_model_path = repo.get("model.safetensors").await.map_err(|e| {
                SnapragError::EmbeddingError(format!("Failed to download model.safetensors: {}", e))
            })?;

            std::fs::copy(&api_model_path, &model_path).map_err(|e| {
                SnapragError::EmbeddingError(format!("Failed to copy model.safetensors: {}", e))
            })?;
        }

        // Download config.json
        let config_path = model_dir.join("config.json");
        if !config_path.exists() {
            info!("Downloading config.json...");
            match repo.get("config.json").await {
                Ok(api_config_path) => {
                    std::fs::copy(&api_config_path, &config_path).map_err(|e| {
                        SnapragError::EmbeddingError(format!("Failed to copy config.json: {}", e))
                    })?;
                    info!("Successfully downloaded config.json");
                }
                Err(e) => {
                    warn!(
                        "Failed to download config.json via API: {}. Trying direct download.",
                        e
                    );
                    // Try direct download
                    let url = format!(
                        "https://huggingface.co/{}/resolve/main/config.json",
                        model_name
                    );
                    info!("Trying direct download from: {}", url);

                    match reqwest::get(&url).await {
                        Ok(response) => {
                            if response.status().is_success() {
                                let content = response.bytes().await.map_err(|e| {
                                    SnapragError::EmbeddingError(format!(
                                        "Failed to read config response: {}",
                                        e
                                    ))
                                })?;

                                std::fs::write(&config_path, content).map_err(|e| {
                                    SnapragError::EmbeddingError(format!(
                                        "Failed to write config.json: {}",
                                        e
                                    ))
                                })?;

                                info!("Successfully downloaded config.json via direct URL");
                            } else {
                                return Err(SnapragError::EmbeddingError(format!(
                                    "Failed to download config.json: {}",
                                    response.status()
                                )));
                            }
                        }
                        Err(e) => {
                            return Err(SnapragError::EmbeddingError(format!(
                                "Failed to download config.json: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        // Download tokenizer files
        let tokenizer_files = ["tokenizer.json", "tokenizer_config.json", "vocab.txt"];
        for tokenizer_file in &tokenizer_files {
            let tokenizer_path = model_dir.join(tokenizer_file);
            if !tokenizer_path.exists() {
                info!("Downloading {}...", tokenizer_file);
                match repo.get(tokenizer_file).await {
                    Ok(api_tokenizer_path) => {
                        let _ = std::fs::copy(&api_tokenizer_path, &tokenizer_path).map_err(|e| {
                            warn!("Failed to copy {}: {}", tokenizer_file, e);
                        });
                        info!("Successfully downloaded {}", tokenizer_file);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to download {} via API: {}. Trying direct download.",
                            tokenizer_file, e
                        );
                        // Try direct download
                        let url = format!(
                            "https://huggingface.co/{}/resolve/main/{}",
                            model_name, tokenizer_file
                        );
                        info!("Trying direct download from: {}", url);

                        match reqwest::get(&url).await {
                            Ok(response) => {
                                if response.status().is_success() {
                                    let content = response.bytes().await.map_err(|e| {
                                        warn!("Failed to read {} response: {}", tokenizer_file, e);
                                    });
                                    if let Ok(content) = content {
                                        if let Err(e) = std::fs::write(&tokenizer_path, content) {
                                            warn!("Failed to write {}: {}", tokenizer_file, e);
                                        } else {
                                            info!(
                                                "Successfully downloaded {} via direct URL",
                                                tokenizer_file
                                            );
                                        }
                                    }
                                } else {
                                    warn!(
                                        "Direct download failed for {}: {}",
                                        tokenizer_file,
                                        response.status()
                                    );
                                }
                            }
                            Err(e) => {
                                warn!("Direct download failed for {}: {}", tokenizer_file, e);
                            }
                        }
                    }
                }
            }
        }

        info!("Model downloaded successfully");
        Ok(model_dir)
    }

    /// Load tokenizer from model path
    fn load_tokenizer(model_path: &PathBuf) -> Result<Tokenizer> {
        let tokenizer_files = ["tokenizer.json", "tokenizer_config.json", "vocab.txt"];

        for tokenizer_file in &tokenizer_files {
            let tokenizer_path = model_path.join(tokenizer_file);
            if tokenizer_path.exists() {
                info!("Loading tokenizer from: {:?}", tokenizer_path);
                return Tokenizer::from_file(&tokenizer_path).map_err(|e| {
                    SnapragError::EmbeddingError(format!(
                        "Failed to load tokenizer from {:?}: {}",
                        tokenizer_path, e
                    ))
                });
            }
        }

        Err(SnapragError::EmbeddingError(format!(
            "No tokenizer files found in {:?}. Expected one of: {:?}",
            model_path, tokenizer_files
        )))
    }

    /// Load BERT model from downloaded files
    fn load_bert_model(model_path: &PathBuf, device: &Device) -> Result<BertModel> {
        info!("Loading BERT model from: {:?}", model_path);

        // Load config
        let config_path = model_path.join("config.json");
        let config_content = std::fs::read_to_string(&config_path)
            .map_err(|e| SnapragError::EmbeddingError(format!("Failed to read config: {}", e)))?;

        let config: BertConfig = serde_json::from_str(&config_content)
            .map_err(|e| SnapragError::EmbeddingError(format!("Failed to parse config: {}", e)))?;

        // Load model weights
        let model_file = model_path.join("model.safetensors");
        let vb =
            unsafe { VarBuilder::from_mmaped_safetensors(&[&model_file], DType::F32, device)? };

        // Create model
        let model = BertModel::load(vb, &config).map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to load BERT model: {}", e))
        })?;

        info!("BERT model loaded successfully");
        Ok(model)
    }

    /// Generate embedding for a single text using BGE model
    pub async fn generate(&self, text: &str) -> Result<Vec<f32>> {
        debug!("Generating embedding for text: {}", text);

        // Preprocess text to handle newlines and invalid characters
        let processed_text = crate::embeddings::preprocess_text_for_embedding(text)?;

        // Tokenize input with error handling
        let encoding = self
            .tokenizer
            .encode(processed_text.to_string(), true)
            .map_err(|e| SnapragError::EmbeddingError(format!("Tokenization failed: {}", e)))?;

        // Validate tokenization results
        let input_ids = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        if input_ids.is_empty() {
            return Err(SnapragError::EmbeddingError(
                "No tokens generated".to_string(),
            ));
        }

        if input_ids.len() > 512 {
            return Err(SnapragError::EmbeddingError(
                "Too many tokens (max 512)".to_string(),
            ));
        }

        let input_ids_tensor = Tensor::new(input_ids, &self.device).map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to create input_ids tensor: {}", e))
        })?;
        let attention_mask_tensor = Tensor::new(attention_mask, &self.device).map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to create attention_mask tensor: {}", e))
        })?;

        // Add batch dimension
        let input_ids = input_ids_tensor.unsqueeze(0).map_err(|e| {
            SnapragError::EmbeddingError(format!(
                "Failed to add batch dimension to input_ids: {}",
                e
            ))
        })?;
        let attention_mask = attention_mask_tensor.unsqueeze(0).map_err(|e| {
            SnapragError::EmbeddingError(format!(
                "Failed to add batch dimension to attention_mask: {}",
                e
            ))
        })?;

        // Use the real BERT model for embedding computation with error handling
        let embeddings = self
            .compute_embeddings_real(&input_ids, &attention_mask)
            .map_err(|e| {
                SnapragError::EmbeddingError(format!("BERT forward pass failed: {}", e))
            })?;

        // Mean pooling
        let pooled = self
            .mean_pooling(&embeddings, &attention_mask)
            .map_err(|e| SnapragError::EmbeddingError(format!("Mean pooling failed: {}", e)))?;

        // Normalize - squeeze to remove batch dimension
        let pooled_squeezed = pooled.squeeze(0).map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to squeeze pooled tensor: {}", e))
        })?;
        let normalized = self
            .normalize(&pooled_squeezed)
            .map_err(|e| SnapragError::EmbeddingError(format!("Normalization failed: {}", e)))?;

        // Convert to Vec<f32>
        let embedding_vec: Vec<f32> = normalized.to_vec1().map_err(|e| {
            SnapragError::EmbeddingError(format!("Failed to convert tensor to vector: {}", e))
        })?;

        debug!(
            "Generated embedding with {} dimensions",
            embedding_vec.len()
        );
        Ok(embedding_vec)
    }

    /// Generate embeddings for multiple texts in batch
    pub async fn generate_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        debug!("Generating embeddings for {} texts", texts.len());

        let mut embeddings = Vec::with_capacity(texts.len());

        // Process in batches optimized for Tesla V100 32GB
        const BATCH_SIZE: usize = 1024; // Aggressive batch size to utilize V100's 32GB memory

        for chunk in texts.chunks(BATCH_SIZE) {
            let chunk_embeddings = self.generate_batch_chunk(chunk).await?;
            embeddings.extend(chunk_embeddings);
        }

        debug!("Generated {} embeddings", embeddings.len());
        Ok(embeddings)
    }

    /// Generate embeddings for a chunk of texts with dynamic batch sizing
    async fn generate_batch_chunk(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Dynamic batch sizing based on text length to maximize GPU memory usage
        let avg_text_len = texts.iter().map(|t| t.len()).sum::<usize>() / texts.len();
        let dynamic_batch_size = if avg_text_len < 100 {
            2048 // Short texts: can handle larger batches
        } else if avg_text_len < 500 {
            1024 // Medium texts: moderate batches
        } else {
            512 // Long texts: smaller batches to avoid OOM
        };

        info!(
            "Processing {} texts with dynamic batch size: {} (avg text length: {})",
            texts.len(),
            dynamic_batch_size,
            avg_text_len
        );

        let mut embeddings = Vec::with_capacity(texts.len());

        // Process in dynamic batches
        for chunk in texts.chunks(dynamic_batch_size) {
            let chunk_embeddings = self.process_batch_chunk(chunk).await?;
            embeddings.extend(chunk_embeddings);
        }

        Ok(embeddings)
    }

    /// Process a single batch chunk with CPU parallelization
    async fn process_batch_chunk(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // CPU parallel tokenization using rayon for maximum CPU utilization
        let encodings: Result<Vec<_>> = texts
            .par_iter() // Parallel iteration using rayon
            .map(|text| {
                self.tokenizer.encode(text.to_string(), true).map_err(|e| {
                    SnapragError::EmbeddingError(format!("Tokenization failed: {}", e))
                })
            })
            .collect();
        let encodings = encodings?;

        // Find max length for padding (parallel)
        let max_len = encodings.par_iter().map(|e| e.len()).max().unwrap_or(0);

        // Parallel tensor construction
        let batch_size = texts.len();
        let mut input_ids = Vec::with_capacity(batch_size * max_len);
        let mut attention_mask = Vec::with_capacity(batch_size * max_len);

        // Use parallel processing for tensor construction
        let tensor_data: Vec<(Vec<u32>, Vec<u32>)> = encodings
            .par_iter()
            .map(|encoding| {
                let ids = encoding.get_ids();
                let mask = encoding.get_attention_mask();

                let mut padded_ids = Vec::with_capacity(max_len);
                let mut padded_mask = Vec::with_capacity(max_len);

                for i in 0..max_len {
                    padded_ids.push(if i < ids.len() { ids[i] } else { 0 });
                    padded_mask.push(if i < mask.len() { mask[i] } else { 0 });
                }

                (padded_ids, padded_mask)
            })
            .collect();

        // Flatten the parallel results
        for (ids, mask) in tensor_data {
            input_ids.extend(ids);
            attention_mask.extend(mask);
        }

        let input_ids =
            Tensor::new(input_ids.as_slice(), &self.device)?.reshape((batch_size, max_len))?;
        let attention_mask =
            Tensor::new(attention_mask.as_slice(), &self.device)?.reshape((batch_size, max_len))?;

        // GPU inference (single operation)
        let embeddings = self.compute_embeddings_real(&input_ids, &attention_mask)?;

        // Batch mean pooling for the entire batch at once
        let pooled = self.mean_pooling_batch(&embeddings, &attention_mask)?;

        // Batch normalization for the entire batch at once
        let normalized = self.normalize_batch(&pooled)?;

        // Convert to Vec<Vec<f32>>
        let results: Result<Vec<Vec<f32>>> = (0..batch_size)
            .into_par_iter()
            .map(|i| {
                let embedding_vec: Vec<f32> = normalized.i(i)?.to_vec1()?;
                Ok(embedding_vec)
            })
            .collect();

        Ok(results?)
    }

    /// Real embedding computation using BERT model
    fn compute_embeddings_real(
        &self,
        input_ids: &Tensor,
        attention_mask: &Tensor,
    ) -> Result<Tensor> {
        // Forward pass through BERT model
        // BERT forward takes: input_ids, attention_mask, token_type_ids (optional)
        let hidden_states = self.model.forward(input_ids, attention_mask, None)?;

        // Return the last hidden states (shape: [batch_size, seq_len, hidden_size])
        Ok(hidden_states)
    }

    /// Mean pooling of embeddings
    fn mean_pooling(&self, embeddings: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        // Convert attention_mask to f32 to match embeddings dtype
        let mask_f32 = attention_mask.to_dtype(DType::F32)?;
        let mask_expanded = mask_f32
            .unsqueeze(attention_mask.dims().len())?
            .expand(embeddings.shape())?;
        let sum_embeddings = (embeddings * &mask_expanded)?.sum(1)?;
        let sum_mask = mask_expanded.sum(1)?.clamp(1e-9, f32::MAX)?;
        Ok(sum_embeddings.broadcast_div(&sum_mask)?)
    }

    /// Batch mean pooling for multiple embeddings at once
    fn mean_pooling_batch(&self, embeddings: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        // Convert attention_mask to f32 to match embeddings dtype
        let mask_f32 = attention_mask.to_dtype(DType::F32)?;
        let mask_expanded = mask_f32
            .unsqueeze(attention_mask.dims().len())?
            .expand(embeddings.shape())?;
        let sum_embeddings = (embeddings * &mask_expanded)?.sum(1)?;
        let sum_mask = mask_expanded.sum(1)?.clamp(1e-9, f32::MAX)?;
        Ok(sum_embeddings.broadcast_div(&sum_mask)?)
    }

    /// Normalize embeddings to unit length
    fn normalize(&self, embeddings: &Tensor) -> Result<Tensor> {
        let norm = embeddings.sqr()?.sum_all()?.sqrt()?;
        Ok(embeddings.broadcast_div(&norm)?)
    }

    /// Batch normalize embeddings to unit length
    fn normalize_batch(&self, embeddings: &Tensor) -> Result<Tensor> {
        // Compute norms for each embedding in the batch
        let norms = embeddings.sqr()?.sum(1)?.sqrt()?;
        // Broadcast divide each embedding by its norm
        Ok(embeddings.broadcast_div(&norms)?)
    }
}

#[cfg(feature = "local-gpu")]
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "Requires GPU and model download"]
    async fn test_bge_embedding() {
        let client = LocalGPUClient::new("BAAI/bge-small-en-v1.5").await.unwrap();
        let embedding = client.generate("What is machine learning?").await.unwrap();
        assert_eq!(
            embedding.len(),
            384,
            "Expected dimension 384 but got {}",
            embedding.len()
        );
    }
}

// Stub implementation when local-gpu feature is disabled
#[cfg(not(feature = "local-gpu"))]
pub struct LocalGPUClient;

#[cfg(not(feature = "local-gpu"))]
impl LocalGPUClient {
    pub fn new(_model_name: &str) -> Result<Self> {
        Err(SnapragError::ConfigError(
            "Local GPU support not compiled. Enable 'local-gpu' feature to use local GPU embeddings.".to_string()
        ))
    }

    pub fn generate(&self, _text: &str) -> Result<Vec<f32>> {
        Err(SnapragError::ConfigError(
            "Local GPU support not compiled. Enable 'local-gpu' feature to use local GPU embeddings.".to_string()
        ))
    }

    pub fn generate_batch(&self, _texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        Err(SnapragError::ConfigError(
            "Local GPU support not compiled. Enable 'local-gpu' feature to use local GPU embeddings.".to_string()
        ))
    }
}
