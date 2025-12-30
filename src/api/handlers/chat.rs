/// Chat-related API handlers
use std::fmt::Write;

use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;
use uuid::Uuid;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::ChatMessageRequest;
use crate::api::types::ChatMessageResponse;
use crate::api::types::CreateChatRequest;
use crate::api::types::CreateChatResponse;
use crate::api::types::GetSessionRequest;
use crate::api::types::SessionInfoResponse;

/// Parse user identifier (FID or username) and return FID
async fn parse_user_identifier(
    identifier: &str,
    database: &crate::database::Database,
) -> crate::Result<u64> {
    let trimmed = identifier.trim();

    // Check if it starts with @ (username)
    if trimmed.starts_with('@') {
        // Remove @ and query by username
        let username = trimmed.trim_start_matches('@');

        // Query database for username
        let profile = database
            .get_user_profile_by_username(username)
            .await?
            .ok_or_else(|| {
                crate::SnapRagError::Custom(format!("Username @{username} not found in database"))
            })?;

        #[allow(clippy::cast_sign_loss)] // FID is guaranteed to be positive in database
        Ok(profile.fid as u64)
    } else {
        // Try to parse as FID number
        trimmed.parse::<u64>().map_err(|_| {
            crate::SnapRagError::Custom(format!(
                "Invalid user identifier '{identifier}'. Use FID (e.g., '99') or username (e.g., '@jesse.base.eth')"
            ))
        })
    }
}

/// Build chat context for LLM
pub fn build_chat_context(
    profile: &crate::models::UserProfile,
    casts: &[crate::models::CastSearchResult],
    session: &crate::api::session::ChatSession,
    message: &str,
) -> String {
    let mut context = String::new();

    write!(
        context,
        "You are role-playing as {}, a Farcaster user",
        profile.display_name.as_deref().unwrap_or("Unknown")
    )
    .unwrap();

    if let Some(username) = &profile.username {
        write!(context, " (username: @{username})").unwrap();
    }

    write!(context, ". Your FID is {}.\n\n", profile.fid).unwrap();

    if let Some(bio) = &profile.bio {
        write!(context, "Your bio: {bio}\n\n").unwrap();
    }

    // Add writing style analysis and examples
    if !casts.is_empty() {
        let avg_length: usize =
            casts.iter().map(|c| c.text.len()).sum::<usize>() / casts.len().max(1);

        context.push_str("\n═══════════════════════════════════════════════════════\n");
        context.push_str("🎭 YOUR WRITING STYLE - STUDY THESE EXAMPLES CAREFULLY\n");
        context.push_str("═══════════════════════════════════════════════════════\n\n");

        context.push_str("These are YOUR actual posts. This is HOW YOU WRITE:\n\n");
        for (idx, result) in casts.iter().take(15).enumerate() {
            writeln!(context, "{}. \"{}\"", idx + 1, result.text).unwrap();
        }

        context.push_str("\n─────────────────────────────────────────────────────\n");
        context.push_str("📊 STYLE ANALYSIS\n");
        context.push_str("─────────────────────────────────────────────────────\n");
        write!(context, "Average length: {avg_length} characters\n\n").unwrap();

        context.push_str("🎯 CRITICAL RULES:\n\n");

        if avg_length < 50 {
            context.push_str(
                "⚠️ ULTRA-SHORT: Response MUST be under 50 characters. 1 sentence max.\n",
            );
        } else if avg_length < 100 {
            context.push_str("⚠️ CONCISE: Keep under 100 chars. 1-2 short sentences only.\n");
        } else if avg_length < 200 {
            context.push_str("📝 MODERATE: 100-200 chars. 2-3 sentences max.\n");
        } else {
            context.push_str("📚 DETAILED: 200-300 chars. Thoughtful explanations okay.\n");
        }

        context.push_str("\n1. MATCH LENGTH shown in examples\n");
        context.push_str("2. USE SAME vocabulary and phrases\n");
        context.push_str("3. COPY tone (casual/professional/technical)\n");
        context.push_str("4. EMOJIS: If examples have them, USE THEM. If not, DON'T.\n");
        context.push_str("5. MATCH punctuation (!,?, etc.)\n");
        context.push_str("6. KEEP slang if present (lol, fr, ngl, etc.)\n\n");

        context.push_str("⚡ Ask: \"Does this sound EXACTLY like my examples?\"\n\n");
        context.push_str("═══════════════════════════════════════════════════════\n\n");
    }

    // Add conversation history if available
    if !session.conversation_history.is_empty() {
        context.push_str("Previous conversation:\n\n");
        for message in &session.conversation_history {
            writeln!(context, "{}: {}", message.role, message.content).unwrap();
        }
        context.push('\n');
    }

    context.push_str("═══ THE QUESTION ═══\n\n");
    write!(context, "User: {message}\n\n").unwrap();
    context.push_str("You (RESPOND IN YOUR EXACT STYLE):");

    context
}

/// Create chat session
///
/// # Errors
///
/// Returns `StatusCode::BAD_REQUEST` if user identifier is invalid.
/// Returns `StatusCode::NOT_FOUND` if user is not found.
/// Returns `StatusCode::INTERNAL_SERVER_ERROR` if database query fails.
pub async fn create_chat_session(
    State(state): State<AppState>,
    Json(req): Json<CreateChatRequest>,
) -> Result<Json<ApiResponse<CreateChatResponse>>, StatusCode> {
    info!("POST /api/chat/create - user: {}", req.user);

    // Parse user identifier (FID or username)
    let fid = match parse_user_identifier(&req.user, &state.database).await {
        Ok(fid) => fid,
        Err(e) => {
            error!("Failed to parse user identifier: {}", e);
            return Ok(Json(ApiResponse::error(format!(
                "Invalid user identifier: {e}"
            ))));
        }
    };

    // Get user profile
    #[allow(clippy::cast_possible_wrap)] // FID from session is guaranteed to fit in i64
    let profile = match state.database.get_user_profile(fid as i64).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Ok(Json(ApiResponse::error(format!("User {fid} not found"))));
        }
        Err(e) => {
            error!("Database error: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Count user's casts - optimized query
    #[allow(clippy::cast_possible_wrap)] // FID from session is guaranteed to fit in i64
    #[allow(clippy::cast_sign_loss)] // COUNT result is always non-negative
    let casts_count = usize::try_from(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts WHERE fid = $1")
            .bind(fid as i64)
            .fetch_one(state.database.pool())
            .await
            .unwrap_or(0),
    )
    .unwrap_or(0);

    // Create session
    #[allow(clippy::cast_possible_wrap)] // FID from session is guaranteed to fit in i64
    let session = match state
        .session_manager
        .create_session(
            fid as i64,
            profile.username.clone(),
            profile.display_name.clone(),
            req.context_limit,
            req.temperature,
        )
        .await
    {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to create session: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    info!(
        "Created chat session: {} for FID {}",
        session.session_id, fid
    );

    #[allow(clippy::cast_possible_wrap)] // FID is guaranteed to be positive and fit in i64
    Ok(Json(ApiResponse::success(CreateChatResponse {
        session_id: session.session_id,
        fid: fid as i64,
        username: profile.username,
        display_name: profile.display_name,
        bio: profile.bio,
        total_casts: casts_count,
    })))
}

/// Send a message in a chat session
///
/// # Errors
///
/// Returns `StatusCode::BAD_REQUEST` if session is invalid or expired.
/// Returns `StatusCode::NOT_FOUND` if user is not found.
/// Returns `StatusCode::INTERNAL_SERVER_ERROR` if database query or LLM call fails.
///
/// # Panics
/// Panics if the system time is before UNIX_EPOCH (1970-01-01), which is impossible on modern systems
#[allow(clippy::too_many_lines)] // Complex function requires many lines
pub async fn send_chat_message(
    State(state): State<AppState>,
    Json(req): Json<ChatMessageRequest>,
) -> Result<Json<ApiResponse<ChatMessageResponse>>, StatusCode> {
    info!("POST /api/chat/message - session: {}", req.session_id);

    // Verify session exists first
    let session = match state.session_manager.get_session(&req.session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(Json(ApiResponse::error("Session not found or expired")));
        }
        Err(e) => {
            error!("Failed to get session: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Check if Redis is available for queue mode
    if let Some(redis_cfg) = &state.config.redis {
        if let Ok(redis_client) = crate::api::redis_client::RedisClient::connect(redis_cfg) {
            // Create job for async processing
            let message_id = Uuid::new_v4().to_string();
            let job_key = format!("chat:{}:{}", req.session_id, message_id);
            let job_data = serde_json::json!({
                "session_id": req.session_id,
                "message": req.message,
                "type": "chat"
            })
            .to_string();

            if let Ok(Some(_job_id)) = redis_client.push_job("chat", &job_key, &job_data).await {
                info!(
                    "📤 Created chat job: {} for session: {}",
                    job_key, req.session_id
                );
                return Ok(Json(ApiResponse::success(ChatMessageResponse {
                    session_id: req.session_id,
                    message: "Processing... Please check back later or poll for result".to_string(),
                    relevant_casts_count: 0,
                    conversation_length: session.conversation_history.len(),
                })));
            }
        }
    }

    // Fallback to synchronous processing if Redis is not available or job creation failed
    info!("Processing chat message synchronously (queue mode unavailable)");
    let mut session = session;

    // Get user profile
    let profile = match state.database.get_user_profile(session.fid).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Ok(Json(ApiResponse::error(format!(
                "User {} not found",
                session.fid
            ))));
        }
        Err(e) => {
            error!("Database error: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Generate query embedding
    let query_embedding = match state.embedding_service.generate(&req.message).await {
        Ok(emb) => emb,
        Err(e) => {
            error!("Embedding generation failed: {}", e);
            return Ok(Json(ApiResponse::error("Failed to process question")));
        }
    };

    // Search for relevant casts
    let search_limit = (session.context_limit * 5).max(100);
    #[allow(clippy::cast_possible_wrap)] // Limit is guaranteed to be positive and reasonable
    let search_results = match state
        .database
        .semantic_search_casts_simple(query_embedding, search_limit as i64, Some(0.3))
        .await
    {
        Ok(results) => results,
        Err(e) => {
            error!("Vector search failed: {}", e);
            return Ok(Json(ApiResponse::error("Failed to search context")));
        }
    };

    // Filter to this user and prioritize substantial content
    let mut user_casts: Vec<_> = search_results
        .into_iter()
        .filter(|r| r.fid == session.fid)
        .collect();

    // Calculate current timestamp
    #[allow(clippy::cast_possible_wrap)] // Unix timestamp will fit in i64 until year 292277026596
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Sort by: relevance * substance * recency
    // Use sort_by with total_cmp to ensure total ordering (handles NaN, inf, etc.)
    user_casts.sort_by(|a, b| {
        // Helper function to calculate score safely, handling all edge cases
        fn calculate_score(similarity: f32, text_len: usize, recency: f32) -> f32 {
            // Ensure similarity is valid (not NaN, not negative, not inf)
            let sim = if !similarity.is_finite() || similarity < 0.0 {
                0.0
            } else {
                similarity.min(1.0) // Clamp to [0, 1]
            };

            // Ensure text length is at least 1 to avoid ln(0) = -inf
            let len = text_len.clamp(1, 10000); // Clamp to reasonable range
            #[allow(clippy::cast_precision_loss)] // Precision loss acceptable for log calculation
            let substance = (len as f32).ln().clamp(1.0, 10.0); // Clamp ln result

            // Ensure recency is valid and clamped
            let rec = if !recency.is_finite() || recency < 0.0 {
                0.5
            } else {
                recency.min(1.0) // Clamp to [0, 1]
            };

            // Calculate score and ensure it's finite
            let score = sim * substance * rec;

            // Return 0.0 for any invalid scores
            if !score.is_finite() || score < 0.0 {
                0.0
            } else {
                score
            }
        }

        // Recency factor: newer posts (< 30 days) = 1.0, older (> 1 year) = 0.5
        #[allow(clippy::cast_precision_loss)]
        // Precision loss acceptable for time difference calculation
        let age_days_a = ((now - a.timestamp) as f32) / 86400.0;
        #[allow(clippy::cast_precision_loss)]
        // Precision loss acceptable for time difference calculation
        let age_days_b = ((now - b.timestamp) as f32) / 86400.0;
        let recency_a = (1.0 - (age_days_a / 365.0).min(0.5)).max(0.5);
        let recency_b = (1.0 - (age_days_b / 365.0).min(0.5)).max(0.5);

        // Calculate scores
        let score_a = calculate_score(a.similarity, a.text.len(), recency_a);
        let score_b = calculate_score(b.similarity, b.text.len(), recency_b);

        // Use total_cmp for f32 which provides a total ordering (handles NaN)
        // Reverse order: higher scores first (score_b.total_cmp(&score_a))
        score_b.total_cmp(&score_a)
    });
    user_casts.truncate(session.context_limit);

    // Build context
    let context = build_chat_context(&profile, &user_casts, &session, &req.message);

    // Generate response
    let Some(llm_service) = &state.llm_service else {
        error!("LLM service not configured");
        return Ok(Json(ApiResponse::error(
            "LLM service not configured".to_string(),
        )));
    };

    let response_text = match llm_service
        .generate_with_params(&context, session.temperature, 2000)
        .await
    {
        Ok(text) => text,
        Err(e) => {
            error!("LLM generation failed: {}", e);
            return Ok(Json(ApiResponse::error("Failed to generate response")));
        }
    };

    // Add to conversation history
    session.add_message("user", req.message.clone());
    session.add_message("assistant", response_text.clone());

    // Update session
    if let Err(e) = state.session_manager.update_session(session.clone()).await {
        error!("Failed to update session: {}", e);
        // Continue anyway, response already generated
    }

    Ok(Json(ApiResponse::success(ChatMessageResponse {
        session_id: session.session_id,
        message: response_text,
        relevant_casts_count: user_casts.len(),
        conversation_length: session.conversation_history.len(),
    })))
}

/// Get session information
///
/// # Errors
///
/// Returns `StatusCode::NOT_FOUND` if session is not found.
pub async fn get_chat_session(
    State(state): State<AppState>,
    Query(params): Query<GetSessionRequest>,
) -> Result<Json<ApiResponse<SessionInfoResponse>>, StatusCode> {
    info!("GET /api/chat/session?session_id={}", params.session_id);

    match state.session_manager.get_session(&params.session_id).await {
        Ok(Some(session)) => Ok(Json(ApiResponse::success(SessionInfoResponse {
            session_id: session.session_id,
            fid: session.fid,
            username: session.username.clone(),
            display_name: session.display_name.clone(),
            conversation_history: session.conversation_history.clone(),
            created_at: session.created_at,
            last_activity: session.last_activity,
        }))),
        Ok(None) => Ok(Json(ApiResponse::error("Session not found or expired"))),
        Err(e) => {
            error!("Failed to get session: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Delete chat session
///
/// # Errors
///
/// Returns `StatusCode::NOT_FOUND` if session is not found.
pub async fn delete_chat_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("DELETE /api/chat/session/{}", session_id);

    match state.session_manager.delete_session(&session_id).await {
        Ok(_) => Ok(Json(ApiResponse::success("Session deleted".to_string()))),
        Err(e) => {
            error!("Failed to delete session: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
