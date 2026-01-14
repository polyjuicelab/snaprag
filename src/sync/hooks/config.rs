//! Hook configuration structures

use serde::Deserialize;
use serde::Serialize;

/// Hub event types that can be hooked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum EventType {
    /// Merge message event (CastAdd, ReactionAdd, etc.)
    #[value(name = "MERGE_MESSAGE")]
    MergeMessage,
    /// Prune message event
    #[value(name = "PRUNE_MESSAGE")]
    PruneMessage,
    /// Revoke message event
    #[value(name = "REVOKE_MESSAGE")]
    RevokeMessage,
    /// Merge username proof event
    #[value(name = "MERGE_USERNAME_PROOF")]
    MergeUsernameProof,
    /// Merge on-chain event
    #[value(name = "MERGE_ON_CHAIN_EVENT")]
    MergeOnChainEvent,
    /// Merge failure event
    #[value(name = "MERGE_FAILURE")]
    MergeFailure,
    /// Block confirmed event
    #[value(name = "BLOCK_CONFIRMED")]
    BlockConfirmed,
    /// On-chain event types (use with OnChainEventType)
    #[value(name = "ON_CHAIN")]
    OnChain,
}

/// On-chain event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
pub enum OnChainEventType {
    /// Signer event
    #[value(name = "SIGNER")]
    Signer,
    /// Signer migrated event
    #[value(name = "SIGNER_MIGRATED")]
    SignerMigrated,
    /// ID register event
    #[value(name = "ID_REGISTER")]
    IdRegister,
    /// Storage rent event
    #[value(name = "STORAGE_RENT")]
    StorageRent,
    /// Tier purchase event
    #[value(name = "TIER_PURCHASE")]
    TierPurchase,
}

/// Hook configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    /// Event type to match
    pub event_type: EventType,
    /// On-chain event type (only used when event_type is OnChain)
    pub onchain_event_type: Option<OnChainEventType>,
    /// Webhook URL to call when event matches
    pub webhook_url: String,
    /// Optional regex filter for text matching
    pub regex_filter: Option<String>,
    /// Optional FID filter
    pub fid_filter: Option<i64>,
    /// Optional target FID filter (for LinkAdd/Remove, ReactionAdd/Remove)
    pub target_fid_filter: Option<i64>,
}

impl HookConfig {
    /// Create a new hook configuration
    pub fn new(
        event_type: EventType,
        webhook_url: String,
        regex_filter: Option<String>,
        fid_filter: Option<i64>,
        target_fid_filter: Option<i64>,
        onchain_event_type: Option<OnChainEventType>,
    ) -> Self {
        Self {
            event_type,
            onchain_event_type,
            webhook_url,
            regex_filter,
            fid_filter,
            target_fid_filter,
        }
    }
}
