use super::super::types::BatchedData;
/// `FrameAction` message handler
///
/// NOTE: FrameAction (MessageType 13) may be deprecated/abandoned by Farcaster protocol.
/// Recent analysis of 1000+ blocks showed 0 Type 13 messages, while other message types
/// (LinkAdd, ReactionAdd, CastAdd, etc.) were abundant. This handler is kept for
/// backward compatibility with historical data.
use crate::models::ShardBlockInfo;
use crate::Result;

/// Handle `FrameAction` message (type 13)
///
/// # Note
/// This message type may be deprecated. Recent block analysis found no Type 13 messages
/// in 1000+ recent blocks across multiple shards.
pub(super) fn handle_frame_action(
    body: &serde_json::Value,
    fid: i64,
    timestamp: i64,
    message_hash: &[u8],
    shard_block_info: &ShardBlockInfo,
    batched: &mut BatchedData,
) {
    if let Some(frame_action_body) = body.get("frame_action_body") {
        let url = frame_action_body
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let button_index = frame_action_body
            .get("button_index")
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| i32::try_from(v).ok());

        let (cast_hash, cast_fid) = if let Some(cast_id) = frame_action_body.get("cast_id") {
            let hash = cast_id
                .get("hash")
                .and_then(|v| v.as_str())
                .and_then(|h| hex::decode(h).ok());
            let fid = cast_id.get("fid").and_then(serde_json::Value::as_i64);
            (hash, fid)
        } else {
            (None, None)
        };

        let input_text = frame_action_body
            .get("input_text")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        let state = frame_action_body
            .get("state")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok());

        let transaction_id = frame_action_body
            .get("transaction_id")
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok());

        batched.frame_actions.push((
            fid,
            url.clone(),
            button_index,
            cast_hash,
            cast_fid,
            input_text,
            state,
            transaction_id,
            timestamp,
            message_hash.to_vec(),
            shard_block_info.clone(),
        ));

        tracing::debug!(
            "Collected frame action: FID {} -> {} (button: {:?})",
            fid,
            url,
            button_index
        );
    }
}
