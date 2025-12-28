//! HTTP server implementation

use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::api::auth::auth_middleware;
use crate::api::auth::AuthState;
use crate::api::cache::CacheService;
use crate::api::handlers::AppState;
use crate::api::mcp;
use crate::api::metrics;
#[cfg(feature = "payment")]
use crate::api::payment_middleware::smart_payment_middleware;
#[cfg(feature = "payment")]
use crate::api::payment_middleware::PaymentMiddlewareState;
use crate::api::routes;
use crate::config::AppConfig;
use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::llm::LlmService;
use crate::Result;

/// Access log middleware to log all HTTP requests
async fn access_log_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    let start = Instant::now();

    // Use info! without target to ensure it shows up
    tracing::info!("→ {} {}", method, uri);

    let response = next.run(request).await;
    let duration = start.elapsed();

    tracing::info!("← {} {}ms", response.status(), duration.as_millis());

    response
}

/// Start the API server (RESTful API only)
pub async fn serve_api(
    config: &AppConfig,
    host: String,
    port: u16,
    enable_cors: bool,
    #[cfg(feature = "payment")] payment_enabled: bool,
    #[cfg(feature = "payment")] payment_address: Option<String>,
    #[cfg(feature = "payment")] testnet: bool,
) -> Result<()> {
    info!("🚀 Starting SnapRAG API server...");

    // Initialize Prometheus metrics
    match metrics::init_metrics() {
        Ok(_) => {
            info!("✅ Prometheus metrics initialized");
            info!("📊 Metrics endpoint available at: http://{host}:{port}/api/metrics");
        }
        Err(e) => {
            tracing::warn!("⚠️  Failed to initialize metrics: {}", e);
        }
    }

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

    // Initialize cache service with Redis
    info!("🔧 Initializing cache service...");
    info!("  Cache enabled: {}", config.cache.enabled);
    info!("  Profile TTL: {} seconds", config.cache.profile_ttl_secs);
    info!("  Social TTL: {} seconds", config.cache.social_ttl_secs);
    info!("  Annual report TTL: never expire (permanent)");
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
                annual_report_ttl: std::time::Duration::from_secs(0), // Never expire (permanent)
                stale_threshold: Duration::from_secs(redis_cfg.stale_threshold_secs),
                enable_stats: config.cache.enable_stats,
            },
        ))
    } else {
        return Err(crate::SnapRagError::Custom(
            "Redis configuration is required for cache service".to_string(),
        ));
    };

    if config.cache.enabled {
        info!("✅ Cache service initialized (enabled) with Redis");
        info!("  Profile TTL: {} seconds", config.cache.profile_ttl_secs);
        info!("  Social TTL: {} seconds", config.cache.social_ttl_secs);
    } else {
        info!("⚠️ Cache service disabled in configuration");
    }

    let state = AppState {
        config: Arc::new(config.clone()),
        database,
        embedding_service,
        llm_service,
        lazy_loader,
        session_manager,
        cache_service,
    };

    // Initialize authentication state
    let auth_state = AuthState::new(&config.auth);
    if auth_state.is_enabled() {
        info!("🔐 Authentication enabled");
        info!("  Configured tokens: {}", config.auth.tokens.len());
    } else {
        info!("💡 Authentication disabled");
    }

    // Build API routes
    let api_router = routes::api_routes(state.clone());
    let mcp_router = mcp::mcp_routes(state.clone());

    // Apply authentication middleware if enabled
    let api_router = if auth_state.is_enabled() {
        api_router.layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
    } else {
        api_router
    };

    let mcp_router = if auth_state.is_enabled() {
        mcp_router.layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ))
    } else {
        mcp_router
    };

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
        let base_url = format!("http://{host}:{port}/api");
        info!("🔗 Payment base URL: {}", base_url);

        // Create payment middleware state
        let payment_state = PaymentMiddlewareState::new(
            payment_addr,
            testnet,
            base_url,
            config.x402.facilitator_url.clone(),
            config.x402.rpc_url.clone(),
        );

        // Apply payment middleware to API routes
        let protected_api = api_router.layer(axum::middleware::from_fn_with_state(
            payment_state.clone(),
            smart_payment_middleware,
        ));

        // MCP routes also protected
        let protected_mcp = mcp_router.layer(axum::middleware::from_fn_with_state(
            payment_state,
            smart_payment_middleware,
        ));

        app = Router::new()
            .nest("/api", protected_api)
            .nest("/mcp", protected_mcp);

        info!("🔒 Payment middleware applied to /api and /mcp routes");
    } else {
        app = Router::new()
            .nest("/api", api_router)
            .nest("/mcp", mcp_router);
        info!("💡 Payment disabled - all endpoints are free");
    }

    #[cfg(not(feature = "payment"))]
    {
        app = Router::new()
            .nest("/api", api_router)
            .nest("/mcp", mcp_router);
        info!("💡 Payment feature not compiled - all endpoints are free");
    }

    // Cache-server mode removed - using Redis-based cache with worker pattern

    // Add middleware layers with detailed request/response logging
    // Use custom access log middleware for reliable logging
    app = app
        .layer(axum::middleware::from_fn(access_log_middleware))
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

    // Add CORS if enabled
    if enable_cors {
        info!("✅ CORS enabled");
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
        app = app.layer(cors);
    }

    // Start server
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("🌐 API server listening on http://{}", addr);
    info!("📋 RESTful API available at http://{}/api", addr);
    info!("🔌 MCP service available at http://{}/mcp", addr);
    info!("");

    #[cfg(feature = "payment")]
    if payment_enabled {
        info!("💰 Payment Information:");
        info!("  Free:     /api/health, /api/stats");
        info!("  $0.001:   /api/profiles");
        info!("  $0.01:    /api/search/*");
        info!("  $0.1:     /api/rag/query");
        info!("");
    }

    info!("Available endpoints:");
    info!("  GET  /api/health         - Health check");
    info!("  GET  /api/profiles       - List profiles");
    info!("  GET  /api/profiles/:fid  - Get profile by FID");
    info!("  POST /api/search/profiles - Search profiles");
    info!("  POST /api/search/casts   - Search casts");
    info!("  POST /api/rag/query      - RAG query");
    info!("  GET  /api/stats          - Statistics");

    axum::serve(listener, app).await?;

    Ok(())
}
