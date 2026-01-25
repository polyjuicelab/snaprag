/// Domain/username status API handlers
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;
use tracing::warn;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::DomainStatusResponse;
use crate::models::UsernameType;

/// Get domain/username status for a user
///
/// # Errors
/// Returns an error if database queries fail
pub async fn get_domains(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!("GET /api/users/{}/domains", fid);

    // Get all username proofs for the user - use API (no pagination, typically only a few proofs per user)
    let proofs = if let Some(lazy_loader) = &state.lazy_loader {
        let fid_u64 = u64::try_from(fid).map_err(|_| StatusCode::BAD_REQUEST)?;
        let client = lazy_loader.client();
        match client.get_username_proofs_by_fid(fid_u64).await {
            Ok(api_proofs) => {
                // Convert API proofs to database format
                let converted: Result<Vec<_>, StatusCode> = api_proofs
                    .into_iter()
                    .map(|proof| {
                        let proof_fid = i64::try_from(proof.fid)
                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                        let timestamp = i64::try_from(proof.timestamp)
                            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
                        Ok(crate::models::UsernameProof {
                            id: uuid::Uuid::new_v4(),
                            fid: proof_fid,
                            username: String::from_utf8_lossy(&proof.name).to_string(),
                            owner: proof.owner,
                            signature: proof.signature,
                            timestamp,
                            username_type: match proof.r#type {
                                1 => 1_i16, // Fname
                                2 => 2_i16, // EnsL1
                                3 => 3_i16, // Basename
                                _ => 0_i16, // None
                            },
                            message_hash: vec![0u8; 32], // API doesn't provide message_hash
                            created_at: chrono::Utc::now(),
                            shard_id: None,
                            block_height: None,
                            transaction_fid: None,
                        })
                    })
                    .collect();
                converted?
            }
            Err(e) => {
                warn!("Failed to get username proofs from API for FID {}: {}, falling back to database", fid, e);
                // Fallback to database
                match state.database.get_user_username_proofs(fid).await {
                    Ok(p) => p,
                    Err(db_err) => {
                        error!(
                            "Error fetching username proofs from database for FID {}: {}",
                            fid, db_err
                        );
                        return Err(StatusCode::INTERNAL_SERVER_ERROR);
                    }
                }
            }
        }
    } else {
        // Fallback to database if lazy_loader is not available
        match state.database.get_user_username_proofs(fid).await {
            Ok(p) => p,
            Err(e) => {
                error!("Error fetching username proofs for FID {}: {}", fid, e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
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
