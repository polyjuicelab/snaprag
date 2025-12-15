use thiserror::Error;

#[derive(Error, Debug)]
pub enum SnapRagError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("User not found: fid {0}")]
    UserNotFound(u64),

    #[error("Profile snapshot not found: fid {0}, timestamp {1}")]
    ProfileSnapshotNotFound(u64, i64),

    #[error("Invalid user data type: {0}")]
    InvalidUserDataType(i32),

    #[error("Invalid username type: {0}")]
    InvalidUsernameType(i32),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("TOML parsing error: {0}")]
    TomlParsing(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSerialization(#[from] toml::ser::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid URI: {0}")]
    InvalidUri(#[from] http::uri::InvalidUri),

    // Removed tonic dependency - only using protobuf for serialization
    #[error("HTTP request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("Custom error: {0}")]
    Custom(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("HTTP error: {0}")]
    HttpError(String),

    #[error("Embedding generation error: {0}")]
    EmbeddingError(String),

    #[error("LLM error: {0}")]
    LlmError(String),
}

impl From<&str> for SnapRagError {
    fn from(msg: &str) -> Self {
        Self::Custom(msg.to_string())
    }
}

impl From<String> for SnapRagError {
    fn from(msg: String) -> Self {
        Self::Custom(msg)
    }
}

#[cfg(feature = "local-gpu")]
impl From<candle_core::Error> for SnapRagError {
    fn from(err: candle_core::Error) -> Self {
        SnapRagError::ConfigError(format!("Candle error: {}", err))
    }
}

#[cfg(feature = "local-gpu")]
impl From<hf_hub::api::tokio::ApiError> for SnapRagError {
    fn from(err: hf_hub::api::tokio::ApiError) -> Self {
        SnapRagError::ConfigError(format!("HuggingFace Hub error: {}", err))
    }
}

pub type Result<T> = std::result::Result<T, SnapRagError>;

// Re-export for convenience
pub use SnapRagError as SnapragError;
