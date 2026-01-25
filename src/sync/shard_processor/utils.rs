use std::collections::HashSet;

use super::types::BatchedData;
use crate::models::ShardBlockInfo;
use crate::Result;

/// Sanitize a string by removing null bytes (0x00)
/// PostgreSQL's UTF-8 encoding doesn't allow null bytes in text fields
/// This function removes all null bytes from the string
pub fn sanitize_string(s: &Option<String>) -> Option<String> {
    s.as_ref().map(|text| text.replace('\0', ""))
}

/// Sanitize a String by removing null bytes
pub fn sanitize_string_owned(s: &str) -> String {
    s.replace('\0', "")
}

/// Sanitize a string reference by removing null bytes
pub fn sanitize_string_ref(s: &str) -> String {
    s.replace('\0', "")
}

/// Sanitize a Vec<u8> by removing null bytes (for state field in frame_actions)
/// This converts the Vec<u8> to a String, removes null bytes, and converts back
pub fn sanitize_bytes(s: &Option<Vec<u8>>) -> Option<Vec<u8>> {
    s.as_ref().and_then(|bytes| {
        String::from_utf8(bytes.clone())
            .ok()
            .map(|text| text.replace('\0', "").into_bytes())
    })
}

/// Create activity data for batch insertion
pub(super) fn create_activity(
    fid: i64,
    activity_type: String,
    activity_data: Option<serde_json::Value>,
    timestamp: i64,
    message_hash: Option<Vec<u8>>,
    shard_block_info: &ShardBlockInfo,
) -> (
    i64,
    String,
    Option<serde_json::Value>,
    i64,
    Option<Vec<u8>>,
    Option<i32>,
    Option<i64>,
) {
    (
        fid,
        activity_type,
        activity_data,
        timestamp,
        message_hash,
        Some(i32::try_from(shard_block_info.shard_id.min(i32::MAX as u32)).unwrap_or(i32::MAX)),
        Some(i64::try_from(shard_block_info.block_height.min(i64::MAX as u64)).unwrap_or(i64::MAX)),
    )
}

/// Batch verify FIDs are registered
pub(super) async fn batch_verify_fids(
    database: &crate::database::Database,
    registered_fids: &std::sync::Mutex<std::collections::HashSet<i64>>,
    fids: &HashSet<i64>,
) -> Result<()> {
    if fids.is_empty() {
        return Ok(());
    }

    let fid_vec: Vec<i64> = fids.iter().copied().collect();

    // 🚀 Single query to check all FIDs at once
    let verified_fids = sqlx::query_scalar::<_, i64>(
        r"
        SELECT DISTINCT fid 
        FROM onchain_events 
        WHERE fid = ANY($1) AND event_type = 3
        ",
    )
    .bind(&fid_vec)
    .fetch_all(database.pool())
    .await?;

    // Update cache with verified FIDs
    {
        if let Ok(mut cache) = registered_fids.lock() {
            for fid in verified_fids {
                cache.insert(fid);
            }
        }
    }

    Ok(())
}
