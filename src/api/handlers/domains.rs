/// Domain/username status API handlers
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::DomainStatusResponse;
use crate::models::UsernameType;

/// Get domain/username status for a user
pub async fn get_domains(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!("GET /api/users/{}/domains", fid);

    // Get all username proofs for the user
    let proofs = match state.database.get_user_username_proofs(fid).await {
        Ok(p) => p,
        Err(e) => {
            error!("Error fetching username proofs for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Find ENS and Farcaster names
    let mut has_ens = false;
    let mut ens_name = None;
    let mut has_farcaster_name = false;
    let mut farcaster_name = None;
    let mut username_type = None;

    for proof in proofs {
        let username_type_enum = UsernameType::from(i32::from(proof.username_type));
        match username_type_enum {
            UsernameType::EnsL1 => {
                has_ens = true;
                ens_name = Some(proof.username.clone());
                if username_type.is_none() {
                    username_type = Some("ens".to_string());
                }
            }
            UsernameType::Fname => {
                has_farcaster_name = true;
                farcaster_name = Some(proof.username.clone());
                if username_type.is_none() {
                    username_type = Some("fname".to_string());
                }
            }
            _ => {}
        }
    }

    let response = DomainStatusResponse {
        has_ens,
        ens_name,
        has_farcaster_name,
        farcaster_name,
        username_type,
    };

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/users/{}/domains - {}ms - 200",
        fid,
        duration.as_millis()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize domain status response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}
