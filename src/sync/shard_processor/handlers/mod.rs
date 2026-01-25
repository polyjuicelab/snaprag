/// Message handlers module - routes messages to appropriate type-specific handlers
///
/// This module is organized by message type for better maintainability:
/// - reaction.rs: ReactionAdd/Remove handlers
/// - link.rs: LinkAdd/Remove handlers
/// - verification.rs: VerificationAdd/Remove handlers (ETH + Solana)
/// - `user_data.rs`: `UserDataAdd` handler (13 field types)
/// - username.rs: `UsernameProof` handler
/// - frame.rs: `FrameAction` handler
/// - system.rs: System message (`OnChainEvent`) handler
use std::sync::Arc;

use super::cast_handlers::collect_cast_add;
use super::types::BatchedData;
use crate::models::ShardBlockInfo;
use crate::sync::client::proto::Message as FarcasterMessage;
use crate::sync::client::proto::Transaction;
use crate::sync::hooks::EventData;
use crate::sync::hooks::EventType;
use crate::sync::hooks::HookManager;
use crate::Result;

mod frame;
mod link;
mod reaction;
mod system;
mod user_data;
mod username;
mod verification;

pub(super) use system::process_system_message;

/// Collect transaction data and route to appropriate handlers
pub(super) async fn collect_transaction_data(
    transaction: &Transaction,
    shard_id: u32,
    block_number: u64,
    timestamp: u64,
    tx_index: usize,
    batched: &mut BatchedData,
    hook_manager: Option<&Arc<HookManager>>,
) -> Result<()> {
    let fid = transaction.fid;

    // Create shard block info for tracking
    // Note: For system transactions (fid=0), we use 0 as transaction_fid
    let shard_block_info = ShardBlockInfo::new(shard_id, block_number, fid, timestamp);

    // Process user messages (only in user transactions, fid > 0)
    if fid > 0 {
        for (msg_idx, message) in transaction.user_messages.iter().enumerate() {
            collect_message_data(message, &shard_block_info, msg_idx, batched, hook_manager)
                .await?;
        }
    }

    // Process system messages (can appear in both user and system transactions)
    // System transactions (fid=0) contain batch OP chain events like id_register
    for system_msg in &transaction.system_messages {
        process_system_message(system_msg, &shard_block_info, batched, hook_manager).await?;
    }

    Ok(())
}

/// Collect message data and route to appropriate type-specific handlers
pub(super) async fn collect_message_data(
    message: &FarcasterMessage,
    shard_block_info: &ShardBlockInfo,
    msg_index: usize,
    batched: &mut BatchedData,
    hook_manager: Option<&Arc<HookManager>>,
) -> Result<()> {
    let data = message
        .data
        .as_ref()
        .ok_or_else(|| crate::SnapRagError::Custom("Missing message data".to_string()))?;

    let message_type = data.r#type;
    let fid = data.fid as i64;
    let timestamp = i64::from(data.timestamp);
    let message_hash = message.hash.clone();

    // Ensure FID will be created for ALL message types
    batched.fids_to_ensure.insert(fid);

    match message_type {
        1 => {
            // CastAdd - delegate to cast_handlers
            collect_cast_add(data, &message_hash, shard_block_info, batched).await?;

            // Trigger hook for MERGE_MESSAGE (CastAdd)
            if let Some(hm) = hook_manager {
                let text = data
                    .body
                    .as_ref()
                    .and_then(|b| b.get("cast_add_body"))
                    .and_then(|c| c.get("text"))
                    .and_then(|t| t.as_str())
                    .map(|s| s.to_string());

                let event_data = EventData {
                    event_type: EventType::MergeMessage,
                    fid,
                    target_fid: None,
                    timestamp,
                    data: serde_json::json!({
                        "message_type": message_type,
                        "message_hash": hex::encode(&message_hash),
                        "text": text
                    }),
                    text,
                };
                hm.check_and_trigger(&event_data).await;
            }
        }
        2 => {
            // CastRemove - no action needed (soft delete handled in casts table)
        }
        3 => {
            // ReactionAdd
            if let Some(body) = &data.body {
                reaction::handle_reaction_add(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );

                // Trigger hook for MERGE_MESSAGE (ReactionAdd)
                if let Some(hm) = hook_manager {
                    let target_fid = body
                        .get("reaction_body")
                        .and_then(|rb| rb.get("target_cast_id"))
                        .and_then(|tc| tc.get("fid"))
                        .and_then(|f| f.as_i64());

                    let event_data = EventData {
                        event_type: EventType::MergeMessage,
                        fid,
                        target_fid,
                        timestamp,
                        data: serde_json::json!({
                            "message_type": message_type,
                            "message_hash": hex::encode(&message_hash),
                            "reaction_body": body.get("reaction_body")
                        }),
                        text: None,
                    };
                    hm.check_and_trigger(&event_data).await;
                }
            }
        }
        4 => {
            // ReactionRemove
            if let Some(body) = &data.body {
                reaction::handle_reaction_remove(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );

                // Trigger hook for MERGE_MESSAGE (ReactionRemove)
                if let Some(hm) = hook_manager {
                    let target_fid = body
                        .get("reaction_body")
                        .and_then(|rb| rb.get("target_cast_id"))
                        .and_then(|tc| tc.get("fid"))
                        .and_then(|f| f.as_i64());

                    let event_data = EventData {
                        event_type: EventType::MergeMessage,
                        fid,
                        target_fid,
                        timestamp,
                        data: serde_json::json!({
                            "message_type": message_type,
                            "message_hash": hex::encode(&message_hash),
                            "reaction_body": body.get("reaction_body")
                        }),
                        text: None,
                    };
                    hm.check_and_trigger(&event_data).await;
                }
            }
        }
        5 => {
            // LinkAdd
            if let Some(body) = &data.body {
                link::handle_link_add(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );

                // Trigger hook for MERGE_MESSAGE (LinkAdd)
                if let Some(hm) = hook_manager {
                    let target_fid = body
                        .get("link_body")
                        .and_then(|lb| lb.get("target_fid"))
                        .and_then(|f| f.as_i64());

                    let event_data = EventData {
                        event_type: EventType::MergeMessage,
                        fid,
                        target_fid,
                        timestamp,
                        data: serde_json::json!({
                            "message_type": message_type,
                            "message_hash": hex::encode(&message_hash),
                            "link_body": body.get("link_body")
                        }),
                        text: None,
                    };
                    hm.check_and_trigger(&event_data).await;
                }
            }
        }
        6 => {
            // LinkRemove
            if let Some(body) = &data.body {
                link::handle_link_remove(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );

                // Trigger hook for MERGE_MESSAGE (LinkRemove)
                if let Some(hm) = hook_manager {
                    let target_fid = body
                        .get("link_body")
                        .and_then(|lb| lb.get("target_fid"))
                        .and_then(|f| f.as_i64());

                    let event_data = EventData {
                        event_type: EventType::MergeMessage,
                        fid,
                        target_fid,
                        timestamp,
                        data: serde_json::json!({
                            "message_type": message_type,
                            "message_hash": hex::encode(&message_hash),
                            "link_body": body.get("link_body")
                        }),
                        text: None,
                    };
                    hm.check_and_trigger(&event_data).await;
                }
            }
        }
        7 => {
            // VerificationAdd (ETH or Solana)
            if let Some(body) = &data.body {
                verification::handle_verification_add(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );
            }
        }
        8 => {
            // VerificationRemove
            if let Some(body) = &data.body {
                verification::handle_verification_remove(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );
            }
        }
        11 => {
            // UserDataAdd
            if let Some(body) = &data.body {
                user_data::handle_user_data_add(body, fid, timestamp, &message_hash, batched);
            }
        }
        12 => {
            // UsernameProof
            if let Some(body) = &data.body {
                username::handle_username_proof(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );

                // Trigger hook for MERGE_USERNAME_PROOF
                if let Some(hm) = hook_manager {
                    let username = body
                        .get("username_proof_body")
                        .and_then(|upb| upb.get("username"))
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string());

                    let event_data = EventData {
                        event_type: EventType::MergeUsernameProof,
                        fid,
                        target_fid: None,
                        timestamp,
                        data: serde_json::json!({
                            "message_type": message_type,
                            "message_hash": hex::encode(&message_hash),
                            "username_proof_body": body.get("username_proof_body")
                        }),
                        text: username,
                    };
                    hm.check_and_trigger(&event_data).await;
                }
            }
        }
        13 => {
            // FrameAction - NOTE: May be deprecated/abandoned by Farcaster protocol
            // Recent block analysis (1000 blocks) showed 0 Type 13 messages
            // Code kept for backward compatibility with historical data
            if let Some(body) = &data.body {
                frame::handle_frame_action(
                    body,
                    fid,
                    timestamp,
                    &message_hash,
                    shard_block_info,
                    batched,
                );
            }
        }
        14 | 15 => {
            // LINK_COMPACT_STATE (14) and LEND_STORAGE (15) - low priority, log only
            tracing::debug!(
                "Received message type {} for FID {} - not stored",
                message_type,
                fid
            );
        }
        _ => {
            tracing::debug!("Unknown message type {} for FID {}", message_type, fid);
        }
    }

    Ok(())
}

// REMOVED: message_handlers_test.rs was outdated and deleted
// Tests are now in src/tests/message_types_test.rs
