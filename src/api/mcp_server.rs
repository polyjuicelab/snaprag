//! MCP (Model Context Protocol) server implementation

use std::sync::Arc;

use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::api::cache::CacheService;
use crate::api::handlers::AppState;
use crate::api::mcp;
#[cfg(feature = "payment")]
use crate::api::payment_middleware::smart_payment_middleware;
#[cfg(feature = "payment")]
use crate::api::payment_middleware::PaymentMiddlewareState;
use crate::config::AppConfig;
use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::llm::LlmService;
use crate::Result;

/// Start the MCP server
pub async fn serve_mcp(
    config: &AppConfig,
    host: String,
    port: u16,
    enable_cors: bool,
    #[cfg(feature = "payment")] payment_enabled: bool,
    #[cfg(feature = "payment")] payment_address: Option<String>,
    #[cfg(feature = "payment")] testnet: bool,
) -> Result<()> {
    info!("🚀 Starting SnapRAG MCP server...");

    // Initialize services
    let database = Arc::new(Database::from_config(config).await?);
    info!("✅ Database service initialized");

    let embedding_service = Arc::new(EmbeddingService::new(config)?);
    info!("✅ Embedding service initialized");

    let llm_service = match LlmService::new(config) {
        Ok(service) => {
            info!("✅ LLM service initialized");
            Some(Arc::new(service))
        }
        Err(e) => {
            info!("⚠️  LLM service initialization failed: {}", e);
            info!("   RAG and MBTI analysis features will be unavailable");
            None
        }
    };

    // Initialize lazy loader for on-demand fetching
    let snapchain_client = Arc::new(crate::sync::SnapchainClient::from_config(config).await?);
    let lazy_loader = Some(Arc::new(crate::sync::LazyLoader::new(
        database.clone(),
        snapchain_client,
    )));

    // Create session manager (1 hour timeout)
    let session_manager = Arc::new(crate::api::session::SessionManager::new(3600));
    info!("Session manager initialized (timeout: 1 hour)");

    // Initialize Redis client if configured
    let redis_client = if let Some(redis_cfg) = &config.redis {
        info!("🔧 Initializing Redis client...");
        match crate::api::redis_client::RedisClient::connect(redis_cfg) {
            Ok(client) => {
                info!("✅ Redis client initialized");
                Some(Arc::new(client))
            }
            Err(e) => {
                tracing::error!("❌ Failed to connect to Redis: {}", e);
                None
            }
        }
    } else {
        info!("⚠️ Redis not configured, skipping Redis client initialization");
        None
    };

    // Initialize cache service
    info!("🔧 Initializing cache service...");
    info!("  Cache enabled: {}", config.cache.enabled);
    info!("  Profile TTL: {} seconds", config.cache.profile_ttl_secs);
    info!("  Social TTL: {} seconds", config.cache.social_ttl_secs);
    info!("  Enable stats: {}", config.cache.enable_stats);

    let cache_service = if let Some(redis_cfg) = &config.redis {
        let redis = Arc::new(crate::api::redis_client::RedisClient::connect(redis_cfg)?);
        Arc::new(CacheService::with_config(
            redis,
            crate::api::cache::CacheConfig {
                profile_ttl: std::time::Duration::from_secs(config.cache.profile_ttl_secs),
                social_ttl: std::time::Duration::from_secs(config.cache.social_ttl_secs),
                mbti_ttl: std::time::Duration::from_secs(7200), // 2 hours for MBTI
                cast_stats_ttl: std::time::Duration::from_secs(config.cache.cast_stats_ttl_secs),
                stale_threshold: std::time::Duration::from_secs(redis_cfg.stale_threshold_secs),
                enable_stats: config.cache.enable_stats,
            },
        ))
    } else {
        info!("⚠️ Cache service disabled (Redis not configured)");
        // Create a dummy cache service that will always return misses
        // This is needed for the AppState, but won't actually cache anything
        let dummy_redis = Arc::new(
            crate::api::redis_client::RedisClient::connect(&crate::config::RedisConfig {
                url: "redis://127.0.0.1:6379".to_string(),
                namespace: "snaprag:".to_string(),
                default_ttl_secs: 2_592_000,
                stale_threshold_secs: 300,
                refresh_channel: "snaprag.refresh".to_string(),
            })
            .unwrap_or_else(|_| {
                panic!("Failed to create dummy Redis client - this should not happen")
            }),
        );
        Arc::new(CacheService::with_config(
            dummy_redis,
            crate::api::cache::CacheConfig::default(),
        ))
    };

    // Create application state
    let state = AppState {
        database,
        embedding_service,
        llm_service,
        lazy_loader,
        session_manager,
        cache_service,
        config: Arc::new(config.clone()),
    };

    // Build MCP routes
    let mcp_router = mcp::mcp_routes(state.clone());

    // Combine routes with optional payment middleware
    let mut app;

    #[cfg(feature = "payment")]
    if payment_enabled {
        let payment_addr = payment_address.ok_or_else(|| {
            crate::SnapRagError::Custom("Payment address required when payment is enabled".into())
        })?;

        info!("💰 Payment enabled");
        info!("📍 Payment address: {}", payment_addr);
        info!(
            "🌐 Network: {}",
            if testnet {
                "base-sepolia (testnet)"
            } else {
                "base (mainnet)"
            }
        );
        info!("🔍 Facilitator URL: {}", config.x402.facilitator_url);
        if let Some(rpc) = &config.x402.rpc_url {
            info!("⛓️  RPC URL: {}", rpc);
        }

        // Create base URL for payment requirements
        let base_url = format!("http://{host}:{port}/mcp");
        info!("🔗 Payment base URL: {}", base_url);

        // Create payment middleware state
        let payment_state = PaymentMiddlewareState::new(
            payment_addr,
            testnet,
            base_url,
            config.x402.facilitator_url.clone(),
            config.x402.rpc_url.clone(),
        );

        // Apply payment middleware to MCP routes
        let protected_mcp = mcp_router.layer(axum::middleware::from_fn_with_state(
            payment_state,
            smart_payment_middleware,
        ));

        app = Router::new().nest("/mcp", protected_mcp);

        info!("🔒 Payment middleware applied to /mcp routes");
    } else {
        app = Router::new().nest("/mcp", mcp_router);
        info!("💡 Payment disabled - all endpoints are free");
    }

    #[cfg(not(feature = "payment"))]
    {
        app = Router::new().nest("/mcp", mcp_router);
        info!("💡 Payment feature not compiled - all endpoints are free");
    }

    // Add middleware
    if enable_cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
        info!("🌐 CORS enabled");
    }

    app = app
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                tracing::debug_span!(
                    "http_request",
                    method = %request.method(),
                    uri = %request.uri(),
                )
            }),
        )
        .layer(CompressionLayer::new());

    // Bind and serve
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("✅ MCP server listening on {}", addr);
    info!("");

    info!("📋 Available endpoints:");
    info!("  GET  /mcp/               - MCP server info");
    info!("  GET  /mcp/resources      - List MCP resources");
    info!("  GET  /mcp/tools          - List MCP tools");
    info!("  POST /mcp/tools/call     - Call MCP tool");
    info!("");

    axum::serve(listener, app).await?;

    Ok(())
}
