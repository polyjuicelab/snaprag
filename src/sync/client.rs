//! Snapchain gRPC client for synchronization

use std::collections::HashMap;

use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use serde_json::Map;
use serde_json::Value;
use tonic::transport::Channel;

use crate::generated::blocks::ShardChunk;
use crate::generated::grpc_client::hub_service_client::HubServiceClient;
use crate::generated::grpc_client::message_data;
use crate::generated::grpc_client::{
    self as grpc_proto,
};
use crate::generated::hub_event::HubEvent;
use crate::generated::request_response::FidsRequest;
use crate::generated::request_response::FidsResponse;
use crate::generated::request_response::GetInfoRequest;
use crate::generated::request_response::GetInfoResponse;
use crate::generated::request_response::ShardChunksRequest;
use crate::generated::request_response::ShardChunksResponse;
use crate::Result;

/// Legacy proto types for backward compatibility
pub mod proto {
    use serde::Deserialize;
    use serde::Serialize;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Block {
        pub header: Option<BlockHeader>,
        pub hash: Vec<u8>,
        pub transactions: Vec<Transaction>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BlockHeader {
        pub height: Option<Height>,
        pub timestamp: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Height {
        pub shard_index: u32,
        pub block_number: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Transaction {
        pub fid: u64,
        pub user_messages: Vec<Message>,
        pub system_messages: Vec<ValidatorMessage>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Message {
        pub data: Option<MessageData>,
        pub hash: Vec<u8>,
        pub signature: Vec<u8>,
        pub signer: Vec<u8>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MessageData {
        pub r#type: i32, // MessageType enum
        pub fid: u64,
        pub timestamp: u32,
        pub network: i32,                    // FarcasterNetwork enum
        pub body: Option<serde_json::Value>, // Oneof body
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ValidatorMessage {
        pub on_chain_event: Option<OnChainEvent>,
        pub fname_transfer: Option<FnameTransfer>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct OnChainEvent {
        pub r#type: i32,
        pub chain_id: u32,
        pub block_number: u64,
        pub block_hash: Vec<u8>,
        pub block_timestamp: u64, // Unix timestamp
        pub transaction_hash: Vec<u8>,
        pub log_index: u32,
        pub fid: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FnameTransfer {
        pub id: u64,
        pub from_fid: u64,
        pub to_fid: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ShardChunk {
        pub header: Option<ShardHeader>,
        pub hash: Vec<u8>,
        pub transactions: Vec<Transaction>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ShardHeader {
        pub height: Option<Height>,
        pub timestamp: u64,
        pub parent_hash: Vec<u8>,
        pub shard_root: Vec<u8>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HubEvent {
        pub id: u64,
        pub r#type: i32, // HubEventType enum
        pub block_number: u64,
        pub block_hash: Vec<u8>,
        pub block_timestamp: u64,
        pub transaction_hash: Vec<u8>,
        pub log_index: u32,
        pub fid: u64,
        pub message: Option<Message>,
    }

    // Request/Response types
    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    pub struct ShardChunksRequest {
        pub shard_id: u32,
        pub start_block_number: u64,
        pub stop_block_number: Option<u64>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ShardChunksResponse {
        pub shard_chunks: Vec<ShardChunk>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetInfoRequest {}

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct GetInfoResponse {
        pub version: String,
        pub db_stats: Option<DbStats>,
        pub peer_id: String,
        pub num_shards: u32,
        pub shard_infos: Vec<ShardInfo>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct DbStats {
        pub num_messages: u64,
        pub num_fid_registrations: u64,
        pub approx_size: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ShardInfo {
        pub shard_id: u32,
        pub max_height: u64,
        pub num_messages: u64,
        pub num_fid_registrations: u64,
        pub approx_size: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FidsResponse {
        pub fids: Vec<u64>,
        pub next_page_token: Option<String>,
    }
}

/// Snapchain gRPC client
#[derive(Clone)]
pub struct SnapchainClient {
    client: Client,
    base_url: String, // HTTP endpoint for REST API
    grpc_client: HubServiceClient<Channel>,
}

/// Snapchain protobuf client for serialization/deserialization
#[derive(Debug, Clone)]
pub struct SnapchainProtobufClient {
    // This client is for protobuf serialization/deserialization only
    // No network communication - just data processing
}

impl SnapchainClient {
    /// Create a new Snapchain client with separate HTTP and gRPC endpoints
    pub async fn new(http_endpoint: &str, grpc_endpoint: &str) -> Result<Self> {
        let client = Client::new();
        let base_url = http_endpoint.trim_end_matches('/').to_string();

        // Create gRPC client with increased message size limits for large batch requests
        let grpc_url = if grpc_endpoint.starts_with("http://") {
            grpc_endpoint.to_string()
        } else {
            format!("http://{grpc_endpoint}")
        };

        tracing::debug!("Creating Snapchain client:");
        tracing::debug!("  HTTP endpoint: {}", base_url);
        tracing::debug!("  gRPC endpoint: {}", grpc_url);

        // Use the generated gRPC client
        let mut grpc_client =
            crate::generated::grpc_client::hub_service_client::HubServiceClient::connect(grpc_url)
                .await
                .map_err(|e| {
                    crate::SnapRagError::Custom(format!("Failed to connect to gRPC endpoint: {e}"))
                })?;

        // Set large message size limits for batch processing after client creation
        // Default is 4MB, we increase to 256MB to support batch_size up to 50
        const MAX_MESSAGE_SIZE: usize = 256 * 1024 * 1024; // 256MB
        grpc_client = grpc_client
            .max_decoding_message_size(MAX_MESSAGE_SIZE)
            .max_encoding_message_size(MAX_MESSAGE_SIZE);

        Ok(Self {
            client,
            base_url,
            grpc_client,
        })
    }

    /// Create a new Snapchain client from `AppConfig`
    pub async fn from_config(config: &crate::AppConfig) -> Result<Self> {
        Self::new(
            config.snapchain_http_endpoint(),
            config.snapchain_grpc_endpoint(),
        )
        .await
    }

    /// Get node information
    pub async fn get_info(&self) -> Result<proto::GetInfoResponse> {
        // Use gRPC client instead of HTTP client
        let mut grpc_client = self.grpc_client.clone();
        let request = tonic::Request::new(crate::generated::grpc_client::GetInfoRequest::default());

        let response = grpc_client.get_info(request).await.map_err(|e| {
            crate::errors::SnapRagError::Custom(format!("gRPC get_info call failed: {e}"))
        })?;

        let grpc_response = response.into_inner();

        Ok(proto::GetInfoResponse {
            version: grpc_response.version,
            db_stats: Some(proto::DbStats {
                num_messages: grpc_response
                    .db_stats
                    .as_ref()
                    .map_or(0, |s| s.num_messages),
                num_fid_registrations: grpc_response
                    .db_stats
                    .as_ref()
                    .map_or(0, |s| s.num_fid_registrations),
                approx_size: grpc_response.db_stats.as_ref().map_or(0, |s| s.approx_size),
            }),
            peer_id: grpc_response.peer_id,
            num_shards: grpc_response.num_shards,
            shard_infos: grpc_response
                .shard_infos
                .into_iter()
                .map(|s| proto::ShardInfo {
                    shard_id: s.shard_id,
                    max_height: s.max_height,
                    num_messages: s.num_messages,
                    num_fid_registrations: s.num_fid_registrations,
                    approx_size: s.approx_size,
                })
                .collect(),
        })
    }

    /// Get shard chunks for a shard
    pub async fn get_shard_chunks(
        &self,
        request: proto::ShardChunksRequest,
    ) -> Result<proto::ShardChunksResponse> {
        // Convert our internal proto request to gRPC client request
        let grpc_request = crate::generated::grpc_client::ShardChunksRequest {
            shard_id: request.shard_id,
            start_block_number: request.start_block_number,
            stop_block_number: request.stop_block_number,
        };

        // Make the gRPC call using tonic::Request wrapper
        let mut grpc_client = self.grpc_client.clone();
        let tonic_request = tonic::Request::new(grpc_request);
        let response = grpc_client
            .get_shard_chunks(tonic_request)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("gRPC call failed: {e}")))?;

        let grpc_response = response.into_inner();

        // Convert generated protobuf response to our internal proto response
        let mut proto_response = proto::ShardChunksResponse {
            shard_chunks: vec![],
        };

        // Convert each shard chunk
        for grpc_chunk in grpc_response.shard_chunks {
            let mut proto_chunk = proto::ShardChunk {
                header: None,
                hash: grpc_chunk.hash.clone(),
                transactions: vec![],
            };

            // Convert header if present
            if let Some(grpc_header) = grpc_chunk.header {
                let mut proto_header = proto::ShardHeader {
                    height: None,
                    timestamp: grpc_header.timestamp,
                    parent_hash: grpc_header.parent_hash.clone(),
                    shard_root: grpc_header.shard_root.clone(),
                };

                // Convert height if present
                if let Some(grpc_height) = grpc_header.height {
                    proto_header.height = Some(proto::Height {
                        shard_index: grpc_height.shard_index,
                        block_number: grpc_height.block_number,
                    });
                }

                proto_chunk.header = Some(proto_header);
            }

            // Convert transactions
            for grpc_tx in grpc_chunk.transactions {
                let mut proto_tx = proto::Transaction {
                    fid: grpc_tx.fid,
                    user_messages: vec![],
                    system_messages: vec![],
                };

                // Convert user messages
                for grpc_msg in grpc_tx.user_messages {
                    let mut proto_msg = proto::Message {
                        data: None,
                        hash: grpc_msg.hash.clone(),
                        signature: grpc_msg.signature.clone(),
                        signer: grpc_msg.signer.clone(),
                    };

                    // Convert message data if present
                    if let Some(grpc_data) = grpc_msg.data {
                        let body_json = grpc_data.body.as_ref().map(convert_grpc_message_body);

                        proto_msg.data = Some(proto::MessageData {
                            r#type: grpc_data.r#type,
                            fid: grpc_data.fid,
                            timestamp: grpc_data.timestamp,
                            network: grpc_data.network,
                            body: body_json,
                        });
                    }

                    proto_tx.user_messages.push(proto_msg);
                }

                // Convert system messages (on-chain events, fname transfers, etc.)
                for grpc_sys_msg in grpc_tx.system_messages {
                    let mut proto_sys_msg = proto::ValidatorMessage {
                        on_chain_event: None,
                        fname_transfer: None,
                    };

                    // Convert on-chain event if present
                    if let Some(grpc_event) = grpc_sys_msg.on_chain_event {
                        proto_sys_msg.on_chain_event = Some(proto::OnChainEvent {
                            r#type: grpc_event.r#type,
                            chain_id: grpc_event.chain_id,
                            block_number: u64::from(grpc_event.block_number),
                            block_hash: grpc_event.block_hash.clone(),
                            block_timestamp: grpc_event.block_timestamp, // Unix timestamp from chain
                            transaction_hash: grpc_event.transaction_hash.clone(),
                            log_index: grpc_event.log_index,
                            fid: grpc_event.fid,
                        });
                    }

                    // Convert fname transfer if present
                    if let Some(grpc_fname) = grpc_sys_msg.fname_transfer {
                        proto_sys_msg.fname_transfer = Some(proto::FnameTransfer {
                            id: grpc_fname.id,
                            from_fid: grpc_fname.from_fid,
                            to_fid: 0, // Note: protobuf may not have to_fid field
                        });
                    }

                    proto_tx.system_messages.push(proto_sys_msg);
                }

                proto_chunk.transactions.push(proto_tx);
            }

            proto_response.shard_chunks.push(proto_chunk);
        }

        Ok(proto_response)
    }

    /// Get links by FID (who this user follows) using gRPC
    pub async fn get_links_by_fid(
        &self,
        fid: u64,
        link_type: &str,
        page_size: Option<u32>,
        next_page_token: Option<&str>,
    ) -> Result<LinksByTargetFidResponse> {
        let mut grpc_client = self.grpc_client.clone();

        let mut request = crate::generated::grpc_client::LinksByFidRequest {
            fid,
            link_type: Some(link_type.to_string()),
            ..Default::default()
        };
        if let Some(size) = page_size {
            request.page_size = Some(size);
        }
        if let Some(token) = next_page_token {
            request.page_token = Some(token.as_bytes().to_vec());
        }

        tracing::debug!("Calling gRPC get_links_by_fid for FID {}", fid);
        let response = grpc_client.get_links_by_fid(request).await.map_err(|e| {
            crate::errors::SnapRagError::Custom(format!("gRPC get_links_by_fid failed: {e}"))
        })?;

        let grpc_response = response.into_inner();
        tracing::debug!(
            "gRPC returned {} messages for FID {}",
            grpc_response.messages.len(),
            fid
        );

        // Convert gRPC response to our HTTP-style format for consistency
        let mut http_messages = Vec::new();
        for msg in &grpc_response.messages {
            if let Some(data) = &msg.data {
                let mut body = std::collections::HashMap::new();

                // Extract link_body from the proto message body oneof
                if let Some(ref body_oneof) = &data.body {
                    use crate::generated::grpc_client::link_body::Target;
                    use crate::generated::grpc_client::message_data::Body;

                    if let Body::LinkBody(ref link_body) = body_oneof {
                        let mut link_body_json = serde_json::Map::new();

                        // Extract target_fid from the target oneof
                        if let Some(Target::TargetFid(target_fid)) = &link_body.target {
                            link_body_json
                                .insert("target_fid".to_string(), serde_json::json!(target_fid));
                        }

                        link_body_json
                            .insert("type".to_string(), serde_json::json!(&link_body.r#type));
                        body.insert(
                            "link_body".to_string(),
                            serde_json::Value::Object(link_body_json),
                        );
                    }
                }

                let farcaster_msg = FarcasterMessage {
                    data: Some(FarcasterMessageData {
                        message_type: format!("{}", data.r#type),
                        fid: data.fid,
                        timestamp: u64::from(data.timestamp),
                        network: format!("{}", data.network),
                        body,
                    }),
                    hash: hex::encode(&msg.hash),
                    hash_scheme: msg.hash_scheme.to_string(),
                    signature: hex::encode(&msg.signature),
                    signature_scheme: msg.signature_scheme.to_string(),
                    signer: hex::encode(&msg.signer),
                };

                http_messages.push(farcaster_msg);
            }
        }

        Ok(LinksByTargetFidResponse {
            messages: http_messages,
            next_page_token: grpc_response
                .next_page_token
                .as_ref()
                .map(|t| String::from_utf8_lossy(t).to_string()),
        })
    }

    /// Get links by target FID (who follows this user)
    pub async fn get_links_by_target_fid(
        &self,
        target_fid: u64,
        link_type: &str,
        page_size: Option<u32>,
        next_page_token: Option<&str>,
    ) -> Result<LinksByTargetFidResponse> {
        let mut url = format!(
            "{}/v1/linksByTargetFid?target_fid={}&link_type={}",
            self.base_url, target_fid, link_type
        );

        if let Some(size) = page_size {
            url.push_str(&format!("&pageSize={size}"));
        }

        if let Some(token) = next_page_token {
            url.push_str(&format!("&nextPageToken={token}"));
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(crate::errors::SnapRagError::Custom(format!(
                "Failed to get links by target FID: HTTP {}",
                response.status()
            )));
        }

        let links_response: LinksByTargetFidResponse = response.json().await?;
        Ok(links_response)
    }

    /// Get casts by FID
    pub async fn get_casts_by_fid(
        &self,
        fid: u64,
        page_size: Option<u32>,
        next_page_token: Option<&str>,
    ) -> Result<CastsByFidResponse> {
        let mut url = format!("{}/v1/castsByFid?fid={}", self.base_url, fid);

        if let Some(size) = page_size {
            url.push_str(&format!("&pageSize={size}"));
        }

        if let Some(token) = next_page_token {
            url.push_str(&format!("&nextPageToken={token}"));
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(crate::errors::SnapRagError::Custom(format!(
                "Failed to get casts by FID: HTTP {}",
                response.status()
            )));
        }

        let casts_response: CastsByFidResponse = response.json().await?;
        Ok(casts_response)
    }

    /// Get user data by FID
    pub async fn get_user_data_by_fid(
        &self,
        fid: u64,
        user_data_type: Option<&str>,
    ) -> Result<UserDataByFidResponse> {
        let mut url = format!("{}/v1/userDataByFid?fid={}", self.base_url, fid);

        if let Some(data_type) = user_data_type {
            url.push_str(&format!("&user_data_type={data_type}"));
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(crate::errors::SnapRagError::Custom(format!(
                "Failed to get user data by FID: HTTP {}",
                response.status()
            )));
        }

        let user_data_response: UserDataByFidResponse = response.json().await?;
        Ok(user_data_response)
    }

    /// Get all FIDs for a specific shard
    pub async fn get_fids_by_shard(
        &self,
        shard_id: u32,
        page_size: Option<u32>,
        page_token: Option<&str>,
    ) -> Result<proto::FidsResponse> {
        let mut url = format!("{}/v1/fids", self.base_url);

        // Add query parameters
        let mut params = vec![format!("shard_id={}", shard_id)];
        if let Some(size) = page_size {
            params.push(format!("pageSize={size}"));
        }
        if let Some(token) = page_token {
            params.push(format!("pageToken={token}"));
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(crate::errors::SnapRagError::Custom(format!(
                "Failed to get FIDs for shard {}: HTTP {}",
                shard_id,
                response.status()
            )));
        }

        let fids_response: proto::FidsResponse = response.json().await?;
        Ok(fids_response)
    }

    /// Get all FIDs across all shards (for comprehensive sync)
    pub async fn get_all_fids(&self) -> Result<Vec<u64>> {
        let mut all_fids = std::collections::HashSet::new();

        // Get node info to determine number of shards
        let info = self.get_info().await?;

        // Collect FIDs from user shards (skip shard 0 which is the block shard)
        for shard_id in 1..info.num_shards {
            let mut page_token: Option<String> = None;

            loop {
                let response = self
                    .get_fids_by_shard(shard_id, Some(1000), page_token.as_deref())
                    .await?;

                // Add FIDs to our set
                for fid in response.fids {
                    all_fids.insert(fid);
                }

                // Check if there are more pages
                page_token.clone_from(&response.next_page_token);
                if page_token.is_none() {
                    break;
                }
            }
        }

        let mut fids_vec: Vec<u64> = all_fids.into_iter().collect();
        fids_vec.sort_unstable();

        tracing::info!(
            "Discovered {} unique FIDs across {} shards",
            fids_vec.len(),
            info.num_shards
        );
        Ok(fids_vec)
    }

    /// Get username proofs by FID using gRPC
    ///
    /// # Errors
    /// Returns an error if the gRPC call fails
    pub async fn get_username_proofs_by_fid(
        &self,
        fid: u64,
    ) -> Result<Vec<crate::generated::grpc_client::UserNameProof>> {
        let mut grpc_client = self.grpc_client.clone();

        let request = crate::generated::grpc_client::FidRequest {
            fid,
            ..Default::default()
        };

        let response = grpc_client
            .get_user_name_proofs_by_fid(request)
            .await
            .map_err(|e| {
                crate::errors::SnapRagError::Custom(format!(
                    "gRPC get_user_name_proofs_by_fid failed: {e}"
                ))
            })?;

        let grpc_response = response.into_inner();
        Ok(grpc_response.proofs)
    }

    /// Get ID registry onchain event by FID using gRPC
    ///
    /// Returns the registration event for a user (event_type = 3).
    ///
    /// # Errors
    /// Returns an error if the gRPC call fails
    pub async fn get_id_registry_onchain_event(
        &self,
        fid: u64,
    ) -> Result<Option<crate::generated::grpc_client::OnChainEvent>> {
        let mut grpc_client = self.grpc_client.clone();

        let request = crate::generated::grpc_client::FidRequest {
            fid,
            ..Default::default()
        };

        let response = grpc_client
            .get_id_registry_on_chain_event(request)
            .await
            .map_err(|e| {
                crate::errors::SnapRagError::Custom(format!(
                    "gRPC get_id_registry_on_chain_event failed: {e}"
                ))
            })?;

        let grpc_response = response.into_inner();
        // Convert empty event to None
        if grpc_response.fid == 0 {
            Ok(None)
        } else {
            Ok(Some(grpc_response))
        }
    }
}

fn convert_grpc_message_body(body: &message_data::Body) -> Value {
    match body {
        message_data::Body::CastAddBody(cast_add) => {
            let mut root = Map::new();
            root.insert("cast_add_body".to_string(), cast_add_body_to_json(cast_add));
            Value::Object(root)
        }
        message_data::Body::CastRemoveBody(cast_remove) => {
            let mut root = Map::new();
            root.insert(
                "cast_remove_body".to_string(),
                cast_remove_body_to_json(cast_remove),
            );
            Value::Object(root)
        }
        message_data::Body::ReactionBody(reaction) => {
            let mut root = Map::new();
            root.insert("reaction_body".to_string(), reaction_body_to_json(reaction));
            Value::Object(root)
        }
        message_data::Body::UserDataBody(user_data) => {
            let mut root = Map::new();
            root.insert(
                "user_data_body".to_string(),
                user_data_body_to_json(user_data),
            );
            Value::Object(root)
        }
        message_data::Body::LinkBody(link_body) => {
            let mut root = Map::new();
            root.insert("link_body".to_string(), link_body_to_json(link_body));
            Value::Object(root)
        }
        message_data::Body::VerificationAddAddressBody(verification) => {
            let mut root = Map::new();
            root.insert(
                "verification_add_eth_address_body".to_string(),
                verification_add_body_to_json(verification),
            );
            Value::Object(root)
        }
        _ => Value::Null,
    }
}

fn cast_add_body_to_json(cast_add: &grpc_proto::CastAddBody) -> Value {
    let mut obj = Map::new();
    obj.insert("text".to_string(), json!(cast_add.text));

    if !cast_add.mentions.is_empty() {
        obj.insert("mentions".to_string(), json!(cast_add.mentions));
    }

    if !cast_add.mentions_positions.is_empty() {
        obj.insert(
            "mentions_positions".to_string(),
            json!(cast_add.mentions_positions),
        );
    }

    if !cast_add.embeds_deprecated.is_empty() {
        obj.insert(
            "embeds_deprecated".to_string(),
            json!(cast_add.embeds_deprecated.clone()),
        );
    }

    if !cast_add.embeds.is_empty() {
        let embeds: Vec<Value> = cast_add.embeds.iter().map(embed_to_json).collect();
        obj.insert("embeds".to_string(), Value::Array(embeds));
    }

    obj.insert("type".to_string(), json!(cast_add.r#type));

    if let Some(parent) = cast_add.parent.as_ref() {
        let parent_value = match parent {
            grpc_proto::cast_add_body::Parent::ParentCastId(cast_id) => {
                let mut parent_map = Map::new();
                parent_map.insert("parent_cast_id".to_string(), cast_id_to_json(cast_id));
                Value::Object(parent_map)
            }
            grpc_proto::cast_add_body::Parent::ParentUrl(url) => {
                let mut parent_map = Map::new();
                parent_map.insert("parent_url".to_string(), json!(url));
                Value::Object(parent_map)
            }
        };
        obj.insert("parent".to_string(), parent_value);
    }

    Value::Object(obj)
}

fn cast_remove_body_to_json(cast_remove: &grpc_proto::CastRemoveBody) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "target_hash".to_string(),
        json!(hex::encode(&cast_remove.target_hash)),
    );
    Value::Object(obj)
}

fn reaction_body_to_json(reaction: &grpc_proto::ReactionBody) -> Value {
    let mut obj = Map::new();
    obj.insert("type".to_string(), json!(reaction.r#type));

    // 🚀 FIX: Use "target_cast_id" key to match parsing code expectations
    if let Some(target) = reaction.target.as_ref() {
        match target {
            grpc_proto::reaction_body::Target::TargetCastId(cast_id) => {
                // Insert directly as target_cast_id (not nested under "target")
                obj.insert("target_cast_id".to_string(), cast_id_to_json(cast_id));
            }
            grpc_proto::reaction_body::Target::TargetUrl(url) => {
                obj.insert("target_url".to_string(), json!(url));
            }
        }
    }

    Value::Object(obj)
}

fn user_data_body_to_json(user_data: &grpc_proto::UserDataBody) -> Value {
    let mut obj = Map::new();
    obj.insert("type".to_string(), json!(user_data.r#type));
    obj.insert("value".to_string(), json!(user_data.value.clone()));
    Value::Object(obj)
}

fn link_body_to_json(link_body: &grpc_proto::LinkBody) -> Value {
    use crate::generated::grpc_client::link_body::Target;

    let mut obj = Map::new();
    obj.insert("type".to_string(), json!(link_body.r#type.clone()));

    // Extract target_fid from the target oneof
    if let Some(target) = &link_body.target {
        match target {
            Target::TargetFid(target_fid) => {
                obj.insert("target_fid".to_string(), json!(target_fid));
            }
        }
    }

    Value::Object(obj)
}

fn verification_add_body_to_json(verification: &grpc_proto::VerificationAddAddressBody) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "address".to_string(),
        json!(hex::encode(&verification.address)),
    );
    obj.insert(
        "claim_signature".to_string(),
        json!(hex::encode(&verification.claim_signature)),
    );
    obj.insert(
        "block_hash".to_string(),
        json!(hex::encode(&verification.block_hash)),
    );
    obj.insert(
        "verification_type".to_string(),
        json!(verification.verification_type),
    );
    obj.insert("chain_id".to_string(), json!(verification.chain_id));
    obj.insert("protocol".to_string(), json!(verification.protocol));
    Value::Object(obj)
}

fn embed_to_json(embed: &grpc_proto::Embed) -> Value {
    match embed.embed.as_ref() {
        Some(grpc_proto::embed::Embed::Url(url)) => {
            let mut obj = Map::new();
            obj.insert("url".to_string(), json!(url));
            Value::Object(obj)
        }
        Some(grpc_proto::embed::Embed::CastId(cast_id)) => {
            let mut obj = Map::new();
            obj.insert("cast_id".to_string(), cast_id_to_json(cast_id));
            Value::Object(obj)
        }
        None => Value::Null,
    }
}

fn cast_id_to_json(cast_id: &grpc_proto::CastId) -> Value {
    json!({
        "fid": cast_id.fid,
        "hash": hex::encode(&cast_id.hash),
    })
}

// Response types for the HTTP API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfoResponse {
    pub version: Option<String>,
    #[serde(rename = "dbStats")]
    pub db_stats: Option<DbStatsResponse>,
    #[serde(rename = "peer_id")]
    pub peer_id: Option<String>,
    #[serde(rename = "numShards")]
    pub num_shards: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbStatsResponse {
    #[serde(rename = "numMessages")]
    pub num_messages: u64,
    #[serde(rename = "numFidRegistrations")]
    pub num_fid_registrations: u64,
    #[serde(rename = "approxSize")]
    pub approx_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinksByTargetFidResponse {
    pub messages: Vec<FarcasterMessage>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastsByFidResponse {
    pub messages: Vec<FarcasterMessage>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDataByFidResponse {
    pub messages: Vec<FarcasterMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarcasterMessage {
    pub data: Option<FarcasterMessageData>,
    pub hash: String,
    #[serde(rename = "hashScheme")]
    pub hash_scheme: String,
    pub signature: String,
    #[serde(rename = "signatureScheme")]
    pub signature_scheme: String,
    pub signer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FarcasterMessageData {
    #[serde(rename = "type")]
    pub message_type: String,
    pub fid: u64,
    pub timestamp: u64,
    pub network: String,
    #[serde(flatten)]
    pub body: HashMap<String, serde_json::Value>,
}

impl Default for SnapchainProtobufClient {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapchainProtobufClient {
    /// Create a new protobuf client
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Create a new protobuf client from `AppConfig`
    #[must_use]
    pub const fn from_config(_config: &crate::AppConfig) -> Self {
        Self::new()
    }

    /// Serialize data to protobuf format
    pub fn serialize<T: protobuf::Message>(&self, message: &T) -> Result<Vec<u8>> {
        message
            .write_to_bytes()
            .map_err(|e| crate::SnapRagError::Custom(format!("Failed to serialize protobuf: {e}")))
    }

    /// Deserialize data from protobuf format
    pub fn deserialize<T: protobuf::Message>(&self, data: &[u8]) -> Result<T> {
        T::parse_from_bytes(data).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to deserialize protobuf: {e}"))
        })
    }
}
