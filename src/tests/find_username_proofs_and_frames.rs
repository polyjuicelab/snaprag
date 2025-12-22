//! Test script to find real UsernameProof and FrameAction examples from recent blocks
//!
//! This script:
//! 1. Gets the latest block heights from all shards
//! 2. Fetches the most recent blocks (trunks)
//! 3. Searches for MessageType 12 (UsernameProof) and MessageType 13 (FrameAction)
//! 4. Extracts example data and creates test fixtures

use snaprag::sync::client::SnapchainClient;
use snaprag::AppConfig;
use snaprag::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 Searching for UsernameProof and FrameAction examples...");
    println!("===========================================================\n");

    // Load config
    let config = AppConfig::load()?;
    let client = SnapchainClient::from_config(&config).await?;

    // Get info to find latest block heights
    let info = client.get_info().await?;
    println!("📊 Hub Info:");
    println!("  Version: {}", info.version);
    println!("  Number of shards: {}", info.num_shards);
    println!();

    // Search in recent blocks from each shard
    let mut username_proof_examples = Vec::new();
    let mut frame_action_examples = Vec::new();
    let blocks_to_check = 200; // Check last 200 blocks from each shard

    for shard_info in &info.shard_infos {
        let shard_id = shard_info.shard_id;
        let max_height = shard_info.max_height;

        if max_height == 0 {
            continue;
        }

        // Calculate start block (go back N blocks from max)
        let start_block = max_height.saturating_sub(blocks_to_check);
        let stop_block = max_height;

        println!("🔎 Checking shard {}: blocks {} to {} (max: {})", shard_id, start_block, stop_block, max_height);

        // Fetch chunks
        let request = snaprag::sync::client::proto::ShardChunksRequest {
            shard_id,
            start_block_number: start_block,
            stop_block_number: Some(stop_block),
        };

        match client.get_shard_chunks(request).await {
            Ok(response) => {
                for chunk in &response.shard_chunks {
                    let block_num = chunk.header.as_ref().and_then(|h| h.height.as_ref().map(|h| h.block_number));
                    
                    for transaction in &chunk.transactions {
                        for message in &transaction.user_messages {
                            if let Some(data) = &message.data {
                                let message_type = data.r#type;

                                // Check for UsernameProof (type 12)
                                if message_type == 12 {
                                    if let Some(body) = &data.body {
                                        if let Some(username_proof_body) = body.get("username_proof_body") {
                                            println!("  ✅ Found UsernameProof in shard {} block {:?} (FID: {})", 
                                                shard_id,
                                                block_num,
                                                data.fid
                                            );
                                            
                                            username_proof_examples.push(serde_json::json!({
                                                "shard_id": shard_id,
                                                "block_number": block_num,
                                                "fid": data.fid,
                                                "timestamp": data.timestamp,
                                                "message_hash": hex::encode(&message.hash),
                                                "body": username_proof_body,
                                            }));

                                            // Stop after finding a few examples
                                            if username_proof_examples.len() >= 5 {
                                                break;
                                            }
                                        }
                                    }
                                }

                                // Check for FrameAction (type 13)
                                if message_type == 13 {
                                    if let Some(body) = &data.body {
                                        if let Some(frame_action_body) = body.get("frame_action_body") {
                                            println!("  ✅ Found FrameAction in shard {} block {:?} (FID: {})", 
                                                shard_id,
                                                block_num,
                                                data.fid
                                            );
                                            
                                            frame_action_examples.push(serde_json::json!({
                                                "shard_id": shard_id,
                                                "block_number": block_num,
                                                "fid": data.fid,
                                                "timestamp": data.timestamp,
                                                "message_hash": hex::encode(&message.hash),
                                                "body": frame_action_body,
                                            }));

                                            // Stop after finding a few examples
                                            if frame_action_examples.len() >= 5 {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Break if we found enough examples
                        if username_proof_examples.len() >= 5 && frame_action_examples.len() >= 5 {
                            break;
                        }
                    }

                    // Break if we found enough examples
                    if username_proof_examples.len() >= 5 && frame_action_examples.len() >= 5 {
                        break;
                    }
                }
            }
            Err(e) => {
                println!("  ⚠️  Error fetching chunks for shard {}: {}", shard_id, e);
            }
        }

        // Break if we found enough examples
        if username_proof_examples.len() >= 5 && frame_action_examples.len() >= 5 {
            break;
        }
    }

    println!("\n📋 Results:");
    println!("  UsernameProof examples found: {}", username_proof_examples.len());
    println!("  FrameAction examples found: {}", frame_action_examples.len());
    println!();

    // Save examples to files
    if !username_proof_examples.is_empty() {
        let filename = "tests/fixtures/username_proof_examples.json";
        std::fs::create_dir_all("tests/fixtures").ok();
        std::fs::write(filename, serde_json::to_string_pretty(&username_proof_examples)?)?;
        println!("✅ Saved {} UsernameProof examples to {}", username_proof_examples.len(), filename);
    }

    if !frame_action_examples.is_empty() {
        let filename = "tests/fixtures/frame_action_examples.json";
        std::fs::create_dir_all("tests/fixtures").ok();
        std::fs::write(filename, serde_json::to_string_pretty(&frame_action_examples)?)?;
        println!("✅ Saved {} FrameAction examples to {}", frame_action_examples.len(), filename);
    }

    Ok(())
}
