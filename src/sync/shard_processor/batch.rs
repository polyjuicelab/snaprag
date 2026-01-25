use std::collections::HashMap;
use std::fmt::Write as _;

use tracing::warn;

use super::types::BatchedData;
use super::utils::sanitize_bytes;
use super::utils::sanitize_string;
use super::utils::sanitize_string_owned;
use crate::database::Database;
use crate::Result;

// PostgreSQL parameter limits for bulk inserts
const MAX_PARAMS: usize = 65000;

// Batch sizes for different entity types
const PROFILE_PARAMS_PER_ROW: usize = 5; // fid, field_name, field_value, timestamp, message_hash
const ONCHAIN_PARAMS_PER_ROW: usize = 9; // fid, event_type, chain_id, block_number, block_hash, block_timestamp, tx_hash, log_index, event_data
const USERNAME_PARAMS_PER_ROW: usize = 10; // fid, username, username_type, owner, signature, timestamp, message_hash, shard_id, block_height, transaction_fid
const FRAME_PARAMS_PER_ROW: usize = 13; // fid, url, button_index, cast_hash, cast_fid, input_text, state, transaction_id, timestamp, message_hash, shard_id, block_height, transaction_fid

const PROFILE_CHUNK_SIZE: usize = MAX_PARAMS / PROFILE_PARAMS_PER_ROW;
const ONCHAIN_CHUNK_SIZE: usize = MAX_PARAMS / ONCHAIN_PARAMS_PER_ROW;
const USERNAME_CHUNK_SIZE: usize = MAX_PARAMS / USERNAME_PARAMS_PER_ROW;
const FRAME_CHUNK_SIZE: usize = MAX_PARAMS / FRAME_PARAMS_PER_ROW;

/// Extract shard and block information from a vector of items with ShardBlockInfo
fn extract_shard_block_info<T, F>(items: &[T], extractor: F) -> Option<(u32, u64, u64)>
where
    F: Fn(&T) -> &crate::models::ShardBlockInfo,
{
    if items.is_empty() {
        return None;
    }

    let first_info = extractor(&items[0]);
    let mut shard_id = first_info.shard_id;
    let mut min_block = first_info.block_height;
    let mut max_block = first_info.block_height;

    for item in items {
        let info = extractor(item);
        shard_id = info.shard_id; // Assume all items in batch are from same shard
        min_block = min_block.min(info.block_height);
        max_block = max_block.max(info.block_height);
    }

    Some((shard_id, min_block, max_block))
}

/// Format shard and block info for logging
fn format_shard_block(shard_id: u32, min_block: u64, max_block: u64) -> String {
    if min_block == max_block {
        format!("shard {shard_id}, block {min_block}")
    } else {
        format!("shard {shard_id}, blocks {min_block}-{max_block}")
    }
}

/// Flush batched data to database
/// Public for testing, but re-exported through mod.rs
pub async fn flush_batched_data(database: &Database, batched: BatchedData) -> Result<()> {
    let start = std::time::Instant::now();
    tracing::trace!(
        "Flushing batch: {} FIDs, {} casts, {} links, {} reactions, {} verifications, {} profile updates, {} onchain events, {} username proofs, {} frame actions",
        batched.fids_to_ensure.len(),
        batched.casts.len(),
        batched.links.len(),
        batched.reactions.len(),
        batched.verifications.len(),
        batched.profile_updates.len(),
        batched.onchain_events.len(),
        batched.username_proofs.len(),
        batched.frame_actions.len()
    );

    // Start a transaction for the entire batch
    let mut tx = database.pool().begin().await?;

    // Batch insert FIDs to user_profile_changes (event-sourcing table)
    // 🚀 EVENT-SOURCING MODE: Each FID creates a synthetic "fid_created" event
    // Pure append-only, zero locks
    if !batched.fids_to_ensure.is_empty() {
        let now = chrono::Utc::now();

        const PARAMS_PER_ROW: usize = 5; // fid, field_name, field_value, timestamp, message_hash
        const MAX_PARAMS: usize = 65000;
        const CHUNK_SIZE: usize = MAX_PARAMS / PARAMS_PER_ROW;

        let mut fids: Vec<i64> = batched.fids_to_ensure.iter().copied().collect();
        fids.sort_unstable();

        for chunk in fids.chunks(CHUNK_SIZE) {
            let estimated_size = 150 + chunk.len() * 40;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO user_profile_changes (fid, field_name, field_value, timestamp, message_hash) VALUES ");

            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * PARAMS_PER_ROW;
                query.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5
                ));
            }
            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for fid in chunk {
                // Create synthetic message_hash for fid_created event
                let synthetic_hash = format!("fid_created_{fid}").as_bytes().to_vec();
                q = q
                    .bind(fid)
                    .bind("fid_created")
                    .bind::<Option<String>>(None) // No value for fid_created event
                    .bind(0i64)
                    .bind(synthetic_hash);
            }

            let result = q.execute(&mut *tx).await?;
            if result.rows_affected() > 0 {
                tracing::debug!("Created {} FID events", result.rows_affected());
            }
        }
    }

    // Batch insert casts (split into chunks to avoid parameter limit)
    if !batched.casts.is_empty() {
        tracing::trace!(
            "Batch inserting {} casts (before dedup)",
            batched.casts.len()
        );

        // 🚀 CRITICAL FIX: Deduplicate by message_hash to avoid "affect row a second time" error
        // Keep the latest version of each cast (by timestamp)
        let mut casts_map: HashMap<Vec<u8>, _> = HashMap::new();
        let original_count = batched.casts.len();
        for cast in &batched.casts {
            let hash = cast.3.clone(); // message_hash
            casts_map.insert(hash, cast.clone());
        }
        let deduped_casts: Vec<_> = casts_map.into_values().collect();
        let deduped_count = deduped_casts.len();
        if original_count != deduped_count {
            tracing::debug!(
                "Deduplicated casts: {} -> {} ({} duplicates removed)",
                original_count,
                deduped_count,
                original_count - deduped_count
            );
        }

        const PARAMS_PER_ROW: usize = 10; // Added shard_id and block_height
        const MAX_PARAMS: usize = 65000; // Keep below u16::MAX (65535)
        const CHUNK_SIZE: usize = MAX_PARAMS / PARAMS_PER_ROW; // ~6500 rows per chunk

        // Split casts into chunks
        for chunk in deduped_casts.chunks(CHUNK_SIZE) {
            // Build dynamic query
            // 🚀 Pre-allocate capacity
            let estimated_size = 150 + chunk.len() * 60;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO casts (fid, text, timestamp, message_hash, parent_hash, root_hash, embeds, mentions, shard_id, block_height) VALUES ");

            // 🚀 Direct string building
            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * PARAMS_PER_ROW;
                query.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9,
                    base + 10
                ));
            }

            // 🚀 CRITICAL FIX: Use DO NOTHING for re-sync performance
            // Casts are immutable - if message_hash exists, no need to update
            // This prevents 166M+ unnecessary updates on re-sync
            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                text,
                timestamp,
                message_hash,
                parent_hash,
                root_hash,
                embeds,
                mentions,
                shard_block_info,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(sanitize_string(text)) // Remove null bytes from text
                    .bind(timestamp)
                    .bind(message_hash)
                    .bind(parent_hash)
                    .bind(root_hash)
                    .bind(embeds)
                    .bind(mentions)
                    .bind(i32::try_from(shard_block_info.shard_id).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.block_height).unwrap_or(0));
            }

            q.execute(&mut *tx).await?;
        }
    }

    // Batch insert links (split into chunks to avoid parameter limit)
    if !batched.links.is_empty() {
        let shard_block_info =
            extract_shard_block_info(&batched.links, |(_, _, _, _, _, _, info)| info)
                .map(|(s, min, max)| format_shard_block(s, min, max))
                .unwrap_or_default();
        tracing::info!(
            "📎 Batch inserting {} links ({})",
            batched.links.len(),
            shard_block_info
        );

        const PARAMS_PER_ROW: usize = 8; // fid, target_fid, link_type, event_type, timestamp, message_hash, shard_id, block_height
        const MAX_PARAMS: usize = 65000;
        const CHUNK_SIZE: usize = MAX_PARAMS / PARAMS_PER_ROW;

        for chunk in batched.links.chunks(CHUNK_SIZE) {
            // 🚀 Pre-allocate
            let estimated_size = 150 + chunk.len() * 60;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO links (fid, target_fid, link_type, event_type, timestamp, message_hash, shard_id, block_height) VALUES ");

            // 🚀 Direct building
            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * PARAMS_PER_ROW;
                query.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8
                ));
            }

            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                target_fid,
                link_type,
                event_type,
                timestamp,
                message_hash,
                shard_block_info,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(target_fid)
                    .bind(link_type)
                    .bind(event_type)
                    .bind(timestamp)
                    .bind(message_hash)
                    .bind(i32::try_from(shard_block_info.shard_id).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.block_height).unwrap_or(0));
            }

            q.execute(&mut *tx).await?;
        }
    }

    // Batch insert reactions (split into chunks to avoid parameter limit)
    if !batched.reactions.is_empty() {
        let shard_block_info =
            extract_shard_block_info(&batched.reactions, |(_, _, _, _, _, _, _, info)| info)
                .map(|(s, min, max)| format_shard_block(s, min, max))
                .unwrap_or_default();
        tracing::info!(
            "❤️  Batch inserting {} reactions ({})",
            batched.reactions.len(),
            shard_block_info
        );

        const PARAMS_PER_ROW: usize = 10; // fid, target_cast_hash, target_fid, reaction_type, event_type, timestamp, message_hash, shard_id, block_height, transaction_fid
        const MAX_PARAMS: usize = 65000;
        const CHUNK_SIZE: usize = MAX_PARAMS / PARAMS_PER_ROW;

        for chunk in batched.reactions.chunks(CHUNK_SIZE) {
            // 🚀 Pre-allocate
            let estimated_size = 200 + chunk.len() * 75;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO reactions (fid, target_cast_hash, target_fid, reaction_type, event_type, timestamp, message_hash, shard_id, block_height, transaction_fid) VALUES ");

            // 🚀 Direct building
            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * PARAMS_PER_ROW;
                query.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9,
                    base + 10
                ));
            }

            // Only check message_hash (composite constraint will be removed in migration 007)
            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                target_cast_hash,
                target_fid,
                reaction_type,
                event_type,
                timestamp,
                message_hash,
                shard_block_info,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(target_cast_hash)
                    .bind(target_fid)
                    .bind(reaction_type)
                    .bind(event_type)
                    .bind(timestamp)
                    .bind(message_hash)
                    .bind(i32::try_from(shard_block_info.shard_id).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.block_height).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.transaction_fid).unwrap_or(0));
            }

            q.execute(&mut *tx).await?;
        }
    }

    // Batch insert verifications (split into chunks to avoid parameter limit)
    if !batched.verifications.is_empty() {
        let shard_block_info = extract_shard_block_info(
            &batched.verifications,
            |(_, _, _, _, _, _, _, _, _, info)| info,
        )
        .map(|(s, min, max)| format_shard_block(s, min, max))
        .unwrap_or_default();
        tracing::info!(
            "✅ Batch inserting {} verifications ({})",
            batched.verifications.len(),
            shard_block_info
        );

        const PARAMS_PER_ROW: usize = 12; // fid, address, claim_signature, block_hash, verification_type, chain_id, event_type, timestamp, message_hash, shard_id, block_height, transaction_fid
        const MAX_PARAMS: usize = 65000;
        const CHUNK_SIZE: usize = MAX_PARAMS / PARAMS_PER_ROW;

        for chunk in batched.verifications.chunks(CHUNK_SIZE) {
            // 🚀 Pre-allocate
            let estimated_size = 250 + chunk.len() * 85;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO verifications (fid, address, claim_signature, block_hash, verification_type, chain_id, event_type, timestamp, message_hash, shard_id, block_height, transaction_fid) VALUES ");

            // 🚀 Direct building
            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * PARAMS_PER_ROW;
                query.push_str(&format!(
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9,
                    base + 10,
                    base + 11,
                    base + 12
                ));
            }

            // Only check message_hash (composite constraint will be removed in migration 007)
            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                address,
                claim_signature,
                block_hash,
                verification_type,
                chain_id,
                event_type,
                timestamp,
                message_hash,
                shard_block_info,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(address)
                    .bind(claim_signature)
                    .bind(block_hash)
                    .bind(verification_type)
                    .bind(chain_id)
                    .bind(event_type)
                    .bind(timestamp)
                    .bind(message_hash)
                    .bind(i32::try_from(shard_block_info.shard_id).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.block_height).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.transaction_fid).unwrap_or(0));
            }

            q.execute(&mut *tx).await?;
        }
    }

    // ❌ Removed: user_activity_timeline table dropped
    // Activities tracking was removed for performance (356GB, WAL bottleneck)
    // All necessary data is already in specialized tables (casts, links, reactions, etc.)

    // 🚀 EVENT-SOURCING MODE: Insert individual field changes
    // Each update = one row in user_profile_changes table
    // Pure append-only, zero locks!
    if !batched.profile_updates.is_empty() {
        tracing::trace!(
            "Batch inserting {} profile field changes (before dedup)",
            batched.profile_updates.len()
        );

        // 🚀 CRITICAL: Deduplicate by message_hash in memory to avoid lock contention
        // Multiple workers may try to insert the same message_hash simultaneously
        let mut updates_map: HashMap<Vec<u8>, _> = HashMap::new();
        let original_count = batched.profile_updates.len();
        for update in batched.profile_updates {
            let hash = update.4.clone(); // message_hash
            updates_map.insert(hash, update);
        }
        let updates_list: Vec<_> = updates_map.into_values().collect();
        let deduped_count = updates_list.len();
        if original_count != deduped_count {
            tracing::debug!(
                "Deduplicated profile updates: {} -> {} ({} duplicates removed)",
                original_count,
                deduped_count,
                original_count - deduped_count
            );
        }

        for chunk in updates_list.chunks(PROFILE_CHUNK_SIZE) {
            let estimated_size = 200 + chunk.len() * 50;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO user_profile_changes (fid, field_name, field_value, timestamp, message_hash) VALUES ");

            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * PROFILE_PARAMS_PER_ROW;
                write!(
                    &mut query,
                    "(${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5
                )
                .expect("write! to String should not fail");
            }

            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (fid, field_name, value, timestamp, message_hash) in chunk {
                // Use the actual message_hash from Farcaster (passed from message_handlers)
                // This ensures deduplication works correctly across re-syncs
                q = q
                    .bind(fid)
                    .bind(field_name)
                    .bind(sanitize_string(value)) // Remove null bytes from profile field values
                    .bind(timestamp)
                    .bind(message_hash);
            }

            q.execute(&mut *tx).await?;
        }
    }

    // Batch insert onchain events (system messages)
    if !batched.onchain_events.is_empty() {
        // Extract block range from onchain events (they don't have shard_id)
        let block_info = if !batched.onchain_events.is_empty() {
            let first = &batched.onchain_events[0];
            let block_number = first.3 as u64; // block_number is at index 3
            let mut min_block = block_number;
            let mut max_block = block_number;
            for event in &batched.onchain_events {
                let block = event.3 as u64;
                min_block = min_block.min(block);
                max_block = max_block.max(block);
            }
            if min_block == max_block {
                format!("blocks {min_block}")
            } else {
                format!("blocks {min_block}-{max_block}")
            }
        } else {
            String::new()
        };
        tracing::info!(
            "⛓️  Batch inserting {} onchain events (before dedup) ({})",
            batched.onchain_events.len(),
            block_info
        );

        // 🚀 CRITICAL: Deduplicate by (transaction_hash, log_index) composite key
        // Multiple workers may process overlapping blocks containing same onchain events
        let mut events_map: HashMap<(Option<Vec<u8>>, Option<i32>), _> = HashMap::new();
        let original_count = batched.onchain_events.len();
        for event in batched.onchain_events {
            let key = (event.6.clone(), event.7); // (transaction_hash, log_index)
            events_map.insert(key, event);
        }
        let deduped_events: Vec<_> = events_map.into_values().collect();
        let deduped_count = deduped_events.len();
        if original_count != deduped_count {
            tracing::debug!(
                "Deduplicated onchain events: {} -> {} ({} duplicates removed)",
                original_count,
                deduped_count,
                original_count - deduped_count
            );
        }

        for chunk in deduped_events.chunks(ONCHAIN_CHUNK_SIZE) {
            let estimated_size = 250 + chunk.len() * 70;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO onchain_events (fid, event_type, chain_id, block_number, block_hash, block_timestamp, transaction_hash, log_index, event_data) VALUES ");

            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * ONCHAIN_PARAMS_PER_ROW;
                write!(
                    &mut query,
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9
                )
                .expect("write! to String should not fail");
            }

            query.push_str(" ON CONFLICT (transaction_hash, log_index) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                event_type,
                chain_id,
                block_number,
                block_hash,
                block_timestamp,
                tx_hash,
                log_index,
                event_data,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(event_type)
                    .bind(chain_id)
                    .bind(block_number)
                    .bind(block_hash)
                    .bind(block_timestamp)
                    .bind(tx_hash)
                    .bind(log_index)
                    .bind(event_data);
            }

            q.execute(&mut *tx).await?;
        }
    }

    // ✅ Removes now handled as INSERT events with event_type='remove' (pure event-sourcing)

    // Batch insert username proofs
    if !batched.username_proofs.is_empty() {
        let shard_block_info =
            extract_shard_block_info(&batched.username_proofs, |(_, _, _, _, _, _, _, info)| info)
                .map(|(s, min, max)| format_shard_block(s, min, max))
                .unwrap_or_default();
        tracing::info!(
            "👤 Batch inserting {} username proofs ({})",
            batched.username_proofs.len(),
            shard_block_info
        );

        for chunk in batched.username_proofs.chunks(USERNAME_CHUNK_SIZE) {
            let estimated_size = 350 + chunk.len() * 100;
            let mut query = String::with_capacity(estimated_size);
            // 🎯 Full schema with all tracking columns
            query.push_str("INSERT INTO username_proofs (fid, username, username_type, owner, signature, timestamp, message_hash, shard_id, block_height, transaction_fid) VALUES ");

            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * USERNAME_PARAMS_PER_ROW;
                write!(
                    &mut query,
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9,
                    base + 10
                )
                .expect("write! to String should not fail");
            }

            // 🚀 Pure INSERT mode - match server constraint
            query.push_str(" ON CONFLICT (fid, username_type) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                username,
                owner,
                signature,
                username_type,
                timestamp,
                message_hash,
                shard_block_info,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(sanitize_string_owned(&username)) // Remove null bytes from username
                    .bind(username_type)
                    .bind(owner) // owner is BYTEA, no conversion needed
                    .bind(signature)
                    .bind(timestamp)
                    .bind(message_hash)
                    .bind(i32::try_from(shard_block_info.shard_id).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.block_height).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.transaction_fid).unwrap_or(0));
            }

            q.execute(&mut *tx).await?;
        }
    }

    // Batch insert frame actions
    if !batched.frame_actions.is_empty() {
        let shard_block_info = extract_shard_block_info(
            &batched.frame_actions,
            |(_, _, _, _, _, _, _, _, _, _, info)| info,
        )
        .map(|(s, min, max)| format_shard_block(s, min, max))
        .unwrap_or_default();
        tracing::info!(
            "🖼️  Batch inserting {} frame actions ({})",
            batched.frame_actions.len(),
            shard_block_info
        );

        for chunk in batched.frame_actions.chunks(FRAME_CHUNK_SIZE) {
            let estimated_size = 400 + chunk.len() * 100;
            let mut query = String::with_capacity(estimated_size);
            query.push_str("INSERT INTO frame_actions (fid, url, button_index, cast_hash, cast_fid, input_text, state, transaction_id, timestamp, message_hash, shard_id, block_height, transaction_fid) VALUES ");

            for i in 0..chunk.len() {
                if i > 0 {
                    query.push_str(", ");
                }
                let base = i * FRAME_PARAMS_PER_ROW;
                write!(
                    &mut query,
                    "(${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4,
                    base + 5,
                    base + 6,
                    base + 7,
                    base + 8,
                    base + 9,
                    base + 10,
                    base + 11,
                    base + 12,
                    base + 13
                )
                .expect("write! to String should not fail");
            }

            query.push_str(" ON CONFLICT (message_hash) DO NOTHING");

            let mut q = sqlx::query(&query);
            for (
                fid,
                url,
                button_index,
                cast_hash,
                cast_fid,
                input_text,
                state,
                transaction_id,
                timestamp,
                message_hash,
                shard_block_info,
            ) in chunk
            {
                q = q
                    .bind(fid)
                    .bind(sanitize_string_owned(&url)) // Remove null bytes from URL
                    .bind(button_index)
                    .bind(cast_hash)
                    .bind(cast_fid)
                    .bind(sanitize_string(input_text)) // Remove null bytes from input_text
                    .bind(sanitize_bytes(state)) // Remove null bytes from state (Vec<u8>)
                    .bind(transaction_id)
                    .bind(timestamp)
                    .bind(message_hash)
                    .bind(i32::try_from(shard_block_info.shard_id).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.block_height).unwrap_or(0))
                    .bind(i64::try_from(shard_block_info.transaction_fid).unwrap_or(0));
            }

            q.execute(&mut *tx).await?;
        }
    }

    // Commit the transaction
    tx.commit().await?;

    let elapsed = start.elapsed();
    if elapsed.as_millis() > 1000 {
        warn!("Batch flush took {}ms (slow!)", elapsed.as_millis());
    } else {
        tracing::trace!("Batch flush completed in {}ms", elapsed.as_millis());
    }

    Ok(())
}
