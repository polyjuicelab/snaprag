//! API server module for serving read-only services via REST and MCP

pub mod auth;
pub mod cache;
pub mod handlers;
pub mod mcp;
pub mod mcp_server;
pub mod metrics;
#[cfg(feature = "payment")]
pub mod payment_middleware;
pub mod pricing;
pub mod redis_client;
pub mod routes;
pub mod server;
pub mod session;
pub mod types;

pub use mcp_server::serve_mcp;
pub use server::serve_api;
