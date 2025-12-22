//! CLI command handlers
//!
//! Note: Type conversions (usize/u64 -> i64) in CLI handlers are safe because:
//! - User input values (limits, FIDs, etc.) are reasonable and won't exceed i64::MAX
//! - Farcaster FIDs and block heights never reach i64::MAX in practice

#![allow(clippy::cast_possible_wrap)] // Safe for CLI user inputs and Farcaster values module
//!
//! This module is organized by functional domains:
//! - init: Database initialization and reset
//! - data: Data querying (list, search, activity)
//! - cast: Cast operations (search, recent, thread)
//! - rag: RAG queries
//! - embeddings: Embedding generation and backfill
//! - fetch: Lazy loading (on-demand fetching)
//! - sync: Synchronization commands
//! - serve: API server
//! - info: Information display (stats, dashboard, config)
//! - ask: AI role-playing as a specific user
//! - index: Database index and autovacuum management

pub mod ask;
pub mod ask_handler;
pub mod auth;
pub mod cache;
pub mod cast;
pub mod data;
pub mod embeddings;
pub mod fastsync;
pub mod fetch;
pub mod index;
pub mod info;
pub mod init;
pub mod mbti;
pub mod rag;
pub mod serve;
pub mod social;
pub mod sync;
pub mod utils;

// Re-export all public handlers
pub use ask::*;
pub use ask_handler::*;
pub use auth::*;
pub use cache::*;
pub use cast::*;
pub use data::*;
pub use embeddings::*;
pub use fastsync::*;
pub use fetch::*;
pub use index::*;
pub use info::*;
pub use init::*;
pub use mbti::*;
pub use rag::*;
pub use serve::*;
pub use social::*;
pub use sync::*;
pub use utils::*;
