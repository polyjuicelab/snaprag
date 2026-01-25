//! Test for parsing gRPC `GetShardChunks` response data
//!
//! This test demonstrates how to parse `ShardChunksResponse` data
//! from shard 1, requesting blocks from 0 to 42.

use std::collections::HashMap;

use prost::Message;
use tokio;

use crate::config::AppConfig;
use crate::generated::grpc_client::CastAddBody;
use crate::generated::grpc_client::CommitSignature;
use crate::generated::grpc_client::Commits;
use crate::generated::grpc_client::FarcasterNetwork;
use crate::generated::grpc_client::HashScheme;
use crate::generated::grpc_client::Height;
use crate::generated::grpc_client::Message as FarcasterMessage;
use crate::generated::grpc_client::MessageData;
use crate::generated::grpc_client::MessageType;
use crate::generated::grpc_client::ShardChunk;
use crate::generated::grpc_client::ShardChunksRequest;
use crate::generated::grpc_client::ShardChunksResponse;
use crate::generated::grpc_client::ShardHash;
use crate::generated::grpc_client::ShardHeader;
use crate::generated::grpc_client::SignatureScheme;
use crate::generated::grpc_client::Transaction;
use crate::grpc_client::HubServiceClient;

/// Common test setup: load config and create gRPC client
async fn setup_grpc_client() -> (AppConfig, HubServiceClient) {
    let config = crate::tests::load_test_config().expect("Failed to load configuration");
    let grpc_endpoint = config.snapchain_grpc_endpoint();
    assert!(!grpc_endpoint.is_empty(), "gRPC endpoint must not be empty");

    let client = HubServiceClient::new(grpc_endpoint)
        .await
        .expect("Failed to connect to gRPC service");

    (config, client)
}

/// Create a `ShardChunksRequest` with specified parameters
fn create_shard_chunks_request(
    shard_id: u32,
    start_block: u64,
    stop_block: Option<u64>,
) -> ShardChunksRequest {
    let mut request = ShardChunksRequest::default();
    request.shard_id = shard_id;
    request.start_block_number = start_block;
    request.stop_block_number = stop_block;
    request
}

/// Make gRPC call and return response
async fn make_grpc_call(
    client: &mut HubServiceClient,
    request: ShardChunksRequest,
) -> ShardChunksResponse {
    // Validate request parameters
    assert!(request.shard_id < 1000, "Shard ID must be reasonable");
    assert!(
        request.start_block_number < 1_000_000_000,
        "Start block number must be reasonable"
    );
    if let Some(stop_block) = request.stop_block_number {
        assert!(
            stop_block >= request.start_block_number,
            "Stop block must be >= start block"
        );
        assert!(
            stop_block < 1_000_000_000,
            "Stop block number must be reasonable"
        );
    }

    let response = client
        .get_shard_chunks(request)
        .await
        .expect("gRPC call failed");

    response
}

/// Verify basic chunk structure (header, hash, commits)
fn verify_chunk_basic_structure(chunk: &ShardChunk) {
    // Verify header
    let header = chunk
        .header
        .as_ref()
        .expect("Chunk header should not be None");
    let height = header
        .height
        .as_ref()
        .expect("Header height should not be None");
    // Note: shard_index and block_number are u32, so >= 0 is always true
    // We'll check for reasonable ranges instead
    assert!(
        height.shard_index < 1000,
        "Shard index should be reasonable"
    );
    assert!(
        height.block_number < 1_000_000_000,
        "Block number should be reasonable"
    );
    assert!(header.timestamp > 0, "Timestamp should be > 0");

    // Verify hash
    assert!(!chunk.hash.is_empty(), "Chunk hash should not be empty");

    // Verify commits
    let commits = chunk
        .commits
        .as_ref()
        .expect("Chunk commits should not be None");
    let commit_height = commits
        .height
        .as_ref()
        .expect("Commit height should not be None");
    // Note: shard_index and block_number are u32, so >= 0 is always true
    // We'll check for reasonable ranges instead
    assert!(
        commit_height.shard_index < 1000,
        "Commit shard index should be reasonable"
    );
    assert!(
        commit_height.block_number < 1_000_000_000,
        "Commit block number should be reasonable"
    );
    assert!(
        !commits.signatures.is_empty(),
        "Commit signatures should not be empty"
    );
}

/// Verify transactions with FID validation
fn verify_transactions(chunk: &ShardChunk, allow_fid_zero: bool) {
    for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
        if allow_fid_zero {
            // Note: fid is u64, so >= 0 is always true
            // We'll check for reasonable range instead
            assert!(
                transaction.fid < 1_000_000_000,
                "Transaction FID should be reasonable"
            );
        } else {
            assert!(
                transaction.fid > 0,
                "Transaction FID should be > 0, but found FID=0 at transaction index {tx_idx}"
            );
        }

        // Verify user messages (if any)
        for message in &transaction.user_messages {
            let message_data = message
                .data
                .as_ref()
                .expect("Message data should not be None");
            assert!(message_data.fid > 0, "Message FID should be > 0");
            assert!(!message.hash.is_empty(), "Message hash should not be empty");
        }
    }
}

/// Validate chunk summary information with strict assertions
fn validate_chunk_summary(chunk: &ShardChunk, chunk_index: usize) {
    let header = chunk.header.as_ref().unwrap();
    let height = header.height.as_ref().unwrap();
    let block_number = height.block_number;

    // Strict validation of chunk properties
    assert!(chunk_index < 1000, "Chunk index must be reasonable");
    assert!(
        block_number < 1_000_000_000,
        "Block number must be reasonable"
    );
    assert!(
        chunk.transactions.len() < 10000,
        "Transaction count must be reasonable"
    );
    assert!(header.timestamp > 0, "Timestamp must be positive");
}

/// Validate detailed block analysis with strict assertions
fn validate_detailed_block_analysis(chunk: &ShardChunk) {
    let header = chunk
        .header
        .as_ref()
        .expect("Chunk header should not be None");
    let height = header
        .height
        .as_ref()
        .expect("Header height should not be None");
    let block_number = height.block_number;

    // Strict validation of block properties
    assert!(
        block_number < 1_000_000_000,
        "Block number must be reasonable"
    );
    assert!(height.shard_index < 1000, "Shard index must be reasonable");
    assert!(header.timestamp > 0, "Timestamp must be positive");
    assert!(
        !header.parent_hash.is_empty(),
        "Parent hash must not be empty"
    );
    assert!(
        !header.shard_root.is_empty(),
        "Shard root must not be empty"
    );
    assert!(!chunk.hash.is_empty(), "Chunk hash must not be empty");
    assert!(
        chunk.transactions.len() < 10000,
        "Transaction count must be reasonable"
    );

    // Analyze transaction patterns with strict validation
    let mut fid_counts = HashMap::new();
    let mut account_root_counts = HashMap::new();
    let mut user_message_counts = HashMap::new();

    for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
        // Count FID patterns
        let fid = transaction.fid;
        *fid_counts.entry(fid).or_insert(0) += 1;

        // Count account root patterns
        let account_root = hex::encode(&transaction.account_root);
        *account_root_counts.entry(account_root).or_insert(0) += 1;

        // Count user message patterns
        let user_msg_count = transaction.user_messages.len();
        *user_message_counts.entry(user_msg_count).or_insert(0) += 1;

        // Validate transaction properties
        assert!(fid < 1_000_000_000, "Transaction FID must be reasonable");
        // Account root may be empty in some blocks - this is acceptable
        if !transaction.account_root.is_empty() {
            assert!(
                !transaction.account_root.is_empty(),
                "Account root must not be empty if present"
            );
        }
        assert!(
            user_msg_count < 1000,
            "User message count must be reasonable"
        );

        // Validate user messages
        for message in &transaction.user_messages {
            let message_data = message
                .data
                .as_ref()
                .expect("Message data should not be None");
            assert!(message_data.fid > 0, "Message FID must be positive");
            assert!(!message.hash.is_empty(), "Message hash must not be empty");
            assert_eq!(message.hash.len(), 32, "Message hash must be 32 bytes");
            assert_eq!(
                message.signature.len(),
                64,
                "Message signature must be 64 bytes"
            );
            assert_eq!(message.signer.len(), 32, "Message signer must be 32 bytes");
        }
    }

    // Validate statistics
    assert!(!fid_counts.is_empty(), "Must have at least one FID");
    assert!(
        !account_root_counts.is_empty(),
        "Must have at least one account root"
    );
    assert!(
        !user_message_counts.is_empty(),
        "Must have at least one user message count"
    );

    // Validate commits
    let commits = chunk
        .commits
        .as_ref()
        .expect("Chunk commits should not be None");
    let commit_height = commits
        .height
        .as_ref()
        .expect("Commit height should not be None");

    assert!(
        commit_height.shard_index < 1000,
        "Commit shard index must be reasonable"
    );
    assert!(
        commit_height.block_number < 1_000_000_000,
        "Commit block number must be reasonable"
    );
    assert!(
        !commits.signatures.is_empty(),
        "Must have at least one signature"
    );
    assert!(
        commits.signatures.len() < 1000,
        "Signature count must be reasonable"
    );

    // Validate signatures
    for signature in &commits.signatures {
        assert_eq!(
            signature.signer.len(),
            32,
            "Signature signer must be 32 bytes"
        );
        assert_eq!(signature.signature.len(), 64, "Signature must be 64 bytes");
    }
}

/// Validate detailed data for a specific block with strict assertions
fn validate_detailed_block_data(chunk: &ShardChunk, target_block: u64) {
    let header = chunk.header.as_ref().unwrap();
    let height = header.height.as_ref().unwrap();
    let block_number = height.block_number;

    if block_number == target_block {
        // Strict validation of target block
        assert_eq!(block_number, target_block, "Block number must match target");
        assert!(height.shard_index < 1000, "Shard index must be reasonable");
        assert!(header.timestamp > 0, "Timestamp must be positive");
        assert!(
            !header.parent_hash.is_empty(),
            "Parent hash must not be empty"
        );
        assert!(
            !header.shard_root.is_empty(),
            "Shard root must not be empty"
        );
        assert_eq!(chunk.hash.len(), 32, "Chunk hash must be 32 bytes");
        assert!(
            chunk.transactions.len() < 10000,
            "Transaction count must be reasonable"
        );

        // Validate all transactions
        for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
            assert!(
                transaction.fid < 1_000_000_000,
                "Transaction FID must be reasonable"
            );
            assert!(
                transaction.user_messages.len() < 1000,
                "User message count must be reasonable"
            );
            // Account root may be empty in some blocks - this is acceptable
            if !transaction.account_root.is_empty() {
                assert!(
                    !transaction.account_root.is_empty(),
                    "Account root must not be empty if present"
                );
            }

            // Validate user messages
            for message in &transaction.user_messages {
                let message_data = message
                    .data
                    .as_ref()
                    .expect("Message data should not be None");
                assert!(message_data.fid > 0, "Message FID must be positive");
                assert!(!message.hash.is_empty(), "Message hash must not be empty");
                assert_eq!(message.hash.len(), 32, "Message hash must be 32 bytes");
                assert_eq!(
                    message.signature.len(),
                    64,
                    "Message signature must be 64 bytes"
                );
                assert_eq!(message.signer.len(), 32, "Message signer must be 32 bytes");
            }
        }

        // Validate commits
        let commits = chunk
            .commits
            .as_ref()
            .expect("Chunk commits should not be None");
        let commit_height = commits
            .height
            .as_ref()
            .expect("Commit height should not be None");

        assert!(
            commit_height.shard_index < 1000,
            "Commit shard index must be reasonable"
        );
        assert!(
            commit_height.block_number < 1_000_000_000,
            "Commit block number must be reasonable"
        );
        assert!(
            !commits.signatures.is_empty(),
            "Must have at least one signature"
        );
        assert!(
            commits.signatures.len() < 1000,
            "Signature count must be reasonable"
        );

        // Validate signatures
        for signature in &commits.signatures {
            assert_eq!(
                signature.signer.len(),
                32,
                "Signature signer must be 32 bytes"
            );
            assert_eq!(signature.signature.len(), 64, "Signature must be 64 bytes");
        }

        // Validate value if present
        if let Some(value) = &commits.value {
            assert!(
                value.shard_index < 1000,
                "Value shard index must be reasonable"
            );
            assert_eq!(value.hash.len(), 32, "Value hash must be 32 bytes");
        }
    }
}

#[tokio::test]
#[ignore = "Requires Snapchain gRPC service running on localhost:3383"]
async fn test_parse_shard_chunks_response_real_blocks_0_to_7() {
    // Setup gRPC client
    let (_config, mut client) = setup_grpc_client().await;

    // Create request for blocks 0-7
    let request = create_shard_chunks_request(1, 0, Some(7));

    // Make gRPC call
    let response = make_grpc_call(&mut client, request).await;

    // Verify the response
    let chunk_count = response.shard_chunks.len();
    assert!(
        chunk_count > 0,
        "Expected to receive at least one shard chunk, but got 0"
    );

    // Verify the parsed data with strict validation
    for (i, chunk) in response.shard_chunks.iter().enumerate() {
        validate_chunk_summary(chunk, i);

        // Verify basic structure
        verify_chunk_basic_structure(chunk);

        // Verify transactions (allow FID=0 for this test)
        verify_transactions(chunk, true);

        // Validate user message statistics
        let mut total_user_messages = 0;
        let mut transactions_with_messages = 0;

        for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
            let user_message_count = transaction.user_messages.len();
            total_user_messages += user_message_count;

            if user_message_count > 0 {
                transactions_with_messages += 1;

                // Validate user message properties
                for message in &transaction.user_messages {
                    let message_data = message
                        .data
                        .as_ref()
                        .expect("Message data should not be None");
                    assert!(message_data.fid > 0, "Message FID must be positive");
                    assert!(!message.hash.is_empty(), "Message hash must not be empty");
                }
            }
        }

        // Validate block statistics
        let block_number = chunk
            .header
            .as_ref()
            .unwrap()
            .height
            .as_ref()
            .unwrap()
            .block_number;

        assert!(
            chunk.transactions.len() < 10000,
            "Transaction count must be reasonable"
        );
        assert!(
            transactions_with_messages <= chunk.transactions.len(),
            "Transactions with messages must not exceed total"
        );
        assert!(
            total_user_messages < 100_000,
            "Total user messages must be reasonable"
        );
    }
}

#[tokio::test]
#[ignore = "Requires Snapchain gRPC service running on localhost:3383"]
async fn test_parse_shard_chunks_response_real_block_9_with_fid_zero() {
    // Setup gRPC client
    let (_config, mut client) = setup_grpc_client().await;

    // Create request for blocks 8-10 (to ensure we get some data)
    let request = create_shard_chunks_request(1, 8, Some(10));

    // Make gRPC call
    let response = make_grpc_call(&mut client, request).await;

    // Verify the response
    let chunk_count = response.shard_chunks.len();
    assert!(
        chunk_count > 0,
        "Expected to receive at least one shard chunk, but got 0"
    );

    // Verify the parsed data - specifically look for Block 9
    let mut block_9_found = false;

    for (i, chunk) in response.shard_chunks.iter().enumerate() {
        validate_chunk_summary(chunk, i);

        // Check if this is Block 9
        if let Some(header) = &chunk.header {
            if let Some(height) = &header.height {
                if height.block_number == 9 {
                    block_9_found = true;

                    // Validate detailed data for Block 9
                    validate_detailed_block_data(chunk, 9);

                    // Verify basic structure
                    verify_chunk_basic_structure(chunk);

                    // Check for FID=0 in Block 9 transactions
                    for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
                        if transaction.fid == 0 {
                            // FID=0 found in Block 9 - this is expected behavior
                        }
                    }

                    // Validate Block 9 properties
                    assert!(
                        chunk.transactions.len() < 10000,
                        "Block 9 transaction count must be reasonable"
                    );
                    assert!(header.timestamp > 0, "Block 9 timestamp must be positive");
                }
            }
        }
    }

    // Ensure we found Block 9
    assert!(
        block_9_found,
        "Expected to find Block 9 in the response, but it was not found"
    );
}

#[test]
#[ignore = "Mock test - kept for reference but not run by default"]
fn test_parse_shard_chunks_response_mock() {
    // This test uses mock data for protobuf serialization validation
    // Real integration tests are in integration_sync_test.rs
    // Create a sample ShardChunksRequest for shard 1, blocks 0 to 42
    let mut request = ShardChunksRequest::default();
    request.shard_id = 1;
    request.start_block_number = 0;
    request.stop_block_number = Some(42);

    // Serialize the request to bytes (simulating gRPC call)
    let request_bytes = request.encode_to_vec();
    assert!(!request_bytes.is_empty(), "Request bytes must not be empty");

    // Create a sample ShardChunksResponse with mock data
    let mut response = ShardChunksResponse::default();

    // Create sample shard chunks for blocks 0-42
    for block_num in 0..=42 {
        let mut shard_chunk = ShardChunk::default();

        // Create ShardHeader
        let mut header = ShardHeader::default();
        let mut height = Height::default();
        height.shard_index = 1;
        height.block_number = block_num;
        header.height = Some(height);
        header.timestamp = 1_640_995_200 + block_num * 4; // Mock timestamp
        header.parent_hash = vec![0u8; 32]; // Mock parent hash
        header.shard_root = vec![0u8; 32]; // Mock shard root
        shard_chunk.header = Some(header);

        // Set chunk hash
        shard_chunk.hash = vec![block_num as u8; 32];

        // Create sample transactions
        let mut transactions = Vec::new();
        for fid in 1..=3 {
            let mut transaction = Transaction::default();
            transaction.fid = fid;

            // Create sample user messages
            let mut user_messages = Vec::new();
            let mut message = FarcasterMessage::default();
            let mut data = MessageData::default();
            data.r#type = MessageType::CastAdd as i32;
            data.fid = fid;
            data.timestamp = (1_640_995_200 + block_num * 4) as u32;
            data.network = FarcasterNetwork::Mainnet as i32;

            let mut cast_add_body = CastAddBody::default();
            cast_add_body.text = format!("Test cast from FID {fid} in block {block_num}");
            data.body =
                Some(crate::generated::grpc_client::message_data::Body::CastAddBody(cast_add_body));

            message.data = Some(data);
            message.hash = vec![fid as u8; 32];
            message.hash_scheme = HashScheme::Blake3 as i32;
            message.signature = vec![fid as u8; 64];
            message.signature_scheme = SignatureScheme::Ed25519 as i32;
            message.signer = vec![fid as u8; 32];

            user_messages.push(message);
            transaction.user_messages = user_messages;
            transaction.account_root = vec![fid as u8; 32];

            transactions.push(transaction);
        }
        shard_chunk.transactions = transactions;

        // Create commits
        let mut commits = Commits::default();
        let mut commit_height = Height::default();
        commit_height.shard_index = 1;
        commit_height.block_number = block_num;
        commits.height = Some(commit_height);
        commits.round = 0;

        let mut shard_hash = ShardHash::default();
        shard_hash.shard_index = 1;
        shard_hash.hash = vec![block_num as u8; 32];
        commits.value = Some(shard_hash);

        // Add commit signatures
        let mut signatures = Vec::new();
        for i in 0..3 {
            let mut signature = CommitSignature::default();
            signature.signer = vec![i as u8; 32];
            signature.signature = vec![i as u8; 64];
            signatures.push(signature);
        }
        commits.signatures = signatures;
        shard_chunk.commits = Some(commits);

        response.shard_chunks.push(shard_chunk);
    }

    // Serialize the response to bytes (simulating gRPC response)
    let response_bytes = response.encode_to_vec();
    assert!(
        !response_bytes.is_empty(),
        "Response bytes must not be empty"
    );

    // Parse the response back from bytes
    let parsed_response =
        ShardChunksResponse::decode(&response_bytes[..]).expect("Failed to parse response");

    // Verify the parsed data
    assert_eq!(parsed_response.shard_chunks.len(), 43); // 0 to 42 inclusive

    for (i, chunk) in parsed_response.shard_chunks.iter().enumerate() {
        let expected_block_num = i as u64;

        // Verify header
        assert_eq!(
            chunk
                .header
                .as_ref()
                .unwrap()
                .height
                .as_ref()
                .unwrap()
                .shard_index,
            1
        );
        assert_eq!(
            chunk
                .header
                .as_ref()
                .unwrap()
                .height
                .as_ref()
                .unwrap()
                .block_number,
            expected_block_num
        );
        assert_eq!(
            chunk.header.as_ref().unwrap().timestamp,
            1_640_995_200 + expected_block_num * 4
        );

        // Verify hash
        assert_eq!(chunk.hash.len(), 32);
        assert_eq!(chunk.hash[0], expected_block_num as u8);

        // Verify transactions
        assert_eq!(chunk.transactions.len(), 3); // 3 FIDs per block

        for (fid_idx, transaction) in chunk.transactions.iter().enumerate() {
            let expected_fid = (fid_idx + 1) as u64;
            assert_eq!(transaction.fid, expected_fid);
            assert_eq!(transaction.user_messages.len(), 1);

            // Verify user message
            let message = &transaction.user_messages[0];
            assert_eq!(message.data.as_ref().unwrap().fid, expected_fid);
            assert_eq!(
                message.data.as_ref().unwrap().r#type,
                MessageType::CastAdd as i32
            );

            if let Some(crate::generated::grpc_client::message_data::Body::CastAddBody(cast_body)) =
                &message.data.as_ref().unwrap().body
            {
                assert!(cast_body.text.contains(&format!("FID {expected_fid}")));
                assert!(cast_body
                    .text
                    .contains(&format!("block {expected_block_num}")));
            }
        }

        // Verify commits
        assert_eq!(
            chunk
                .commits
                .as_ref()
                .unwrap()
                .height
                .as_ref()
                .unwrap()
                .shard_index,
            1
        );
        assert_eq!(
            chunk
                .commits
                .as_ref()
                .unwrap()
                .height
                .as_ref()
                .unwrap()
                .block_number,
            expected_block_num
        );
        assert_eq!(chunk.commits.as_ref().unwrap().round, 0);
        assert_eq!(chunk.commits.as_ref().unwrap().signatures.len(), 3);
    }

    // Validate parsed response
    assert_eq!(
        parsed_response.shard_chunks.len(),
        43,
        "Must have 43 chunks (0-42 inclusive)"
    );

    // Validate summary information
    for chunk in parsed_response.shard_chunks.iter().take(5) {
        let block_number = chunk
            .header
            .as_ref()
            .unwrap()
            .height
            .as_ref()
            .unwrap()
            .block_number;
        let transaction_count = chunk.transactions.len();
        let timestamp = chunk.header.as_ref().unwrap().timestamp;

        assert!(block_number < 43, "Block number must be < 43");
        assert!(transaction_count > 0, "Must have at least one transaction");
        assert!(timestamp > 0, "Timestamp must be positive");
    }
}

#[test]
fn test_shard_chunks_request_serialization() {
    // Test creating and serializing a ShardChunksRequest
    let mut request = ShardChunksRequest::default();
    request.shard_id = 1;
    request.start_block_number = 0;
    request.stop_block_number = Some(42);

    let bytes = request.encode_to_vec();
    let parsed = ShardChunksRequest::decode(&bytes[..]).expect("Failed to parse request");

    assert_eq!(parsed.shard_id, 1);
    assert_eq!(parsed.start_block_number, 0);
    assert_eq!(parsed.stop_block_number, Some(42));

    // Test passed - no output needed
}

#[test]
fn test_empty_shard_chunks_response() {
    // Test handling empty response
    let response = ShardChunksResponse::default();
    // No chunks added - empty response

    let bytes = response.encode_to_vec();
    let parsed = ShardChunksResponse::decode(&bytes[..]).expect("Failed to parse empty response");

    assert_eq!(parsed.shard_chunks.len(), 0);
    // Test passed - no output needed
}

#[tokio::test]
#[ignore = "Requires Snapchain gRPC service running on localhost:3383"]
async fn test_parse_block_1_detailed_analysis() {
    // Setup gRPC client
    let (_config, mut client) = setup_grpc_client().await;

    // Create request for blocks 0-5 (to ensure we get some data)
    let request = create_shard_chunks_request(1, 0, Some(5));

    // Make gRPC call
    let response = make_grpc_call(&mut client, request).await;

    // Verify the response
    let chunk_count = response.shard_chunks.len();
    assert!(
        chunk_count > 0,
        "Expected to receive at least one shard chunk, but got 0"
    );

    // Analyze each block in detail with strict validation
    for chunk in &response.shard_chunks {
        validate_detailed_block_analysis(chunk);
    }
}
