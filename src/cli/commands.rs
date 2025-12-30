//! CLI command definitions and argument parsing

use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;

#[derive(Parser)]
#[command(name = "snaprag")]
#[command(about = "SnapRAG CLI tool for database queries and data synchronization")]
#[command(version)]
pub struct Cli {
    /// Enable verbose debug logging (default: info level)
    #[arg(short, long)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize database schema and indexes
    Init {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Skip creating indexes (faster initialization, add indexes later)
        #[arg(long)]
        skip_indexes: bool,
    },
    /// List data from the database
    List {
        /// The type of data to list
        #[arg(value_enum)]
        data_type: DataType,
        /// Maximum number of records to return
        #[arg(short, long, default_value = "100")]
        limit: u32,
        /// Search term for filtering
        #[arg(short, long)]
        search: Option<String>,
        /// Sort by field
        #[arg(long)]
        sort_by: Option<String>,
        /// Sort order (asc/desc)
        #[arg(long, default_value = "desc")]
        sort_order: String,
        /// Filter by FID (for casts, profiles, etc.)
        #[arg(long)]
        fid: Option<i64>,
        /// Filter by FID range (min-max)
        #[arg(long)]
        fid_range: Option<String>,
        /// Filter by username
        #[arg(long)]
        username: Option<String>,
        /// Filter by display name
        #[arg(long)]
        display_name: Option<String>,
        /// Filter by bio content
        #[arg(long)]
        bio: Option<String>,
        /// Filter by location
        #[arg(long)]
        location: Option<String>,
        /// Filter by Twitter username
        #[arg(long)]
        twitter: Option<String>,
        /// Filter by GitHub username
        #[arg(long)]
        github: Option<String>,
        /// Show only profiles with username
        #[arg(long)]
        has_username: bool,
        /// Show only profiles with display name
        #[arg(long)]
        has_display_name: bool,
        /// Show only profiles with bio
        #[arg(long)]
        has_bio: bool,
    },
    /// Reset all synchronized data from the database and remove lock files
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Manage database indexes and autovacuum for bulk operations
    #[command(subcommand)]
    Index(IndexCommands),
    /// Fast sync mode management (indexes + `PostgreSQL` optimization)
    #[command(subcommand)]
    Fastsync(FastsyncCommands),
    /// Synchronization commands
    #[command(subcommand)]
    Sync(SyncCommands),
    /// Show statistics and analytics (fast overview by default, detailed with --detailed)
    Stats {
        /// Show detailed statistics instead of fast overview
        #[arg(short, long)]
        detailed: bool,
        /// Export statistics to JSON
        #[arg(short, long)]
        export: Option<String>,
    },
    /// Search profiles with advanced filters
    Search {
        /// Search term
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: u32,
        /// Search in specific fields (`username,display_name,bio,all`)
        #[arg(long, default_value = "all")]
        fields: String,
    },
    /// Show current configuration
    Config,
    /// Query user activity timeline by FID
    Activity {
        /// Farcaster ID to query
        fid: i64,
        /// Maximum number of activities to return
        #[arg(short, long, default_value = "50")]
        limit: i64,
        /// Skip first N activities
        #[arg(short, long, default_value = "0")]
        offset: i64,
        /// Filter by activity type (`cast_add`, `reaction_add`, `link_add`, etc.)
        #[arg(short = 't', long)]
        activity_type: Option<String>,
        /// Show detailed JSON data
        #[arg(short, long)]
        detailed: bool,
    },
    /// Cast commands
    #[command(subcommand)]
    Cast(CastCommands),
    /// RAG (Retrieval-Augmented Generation) commands
    #[command(subcommand)]
    Rag(RagCommands),
    /// Embeddings generation commands
    #[command(subcommand)]
    Embeddings(EmbeddingsCommands),
    /// Serve API commands
    #[command(subcommand)]
    Serve(ServeCommands),
    /// Fetch user data on-demand (lazy loading)
    #[command(subcommand)]
    Fetch(FetchCommands),
    /// Ask a question to a specific user (AI role-playing)
    Ask {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Question to ask (optional in --chat mode)
        #[arg(required_unless_present = "chat")]
        question: Option<String>,
        /// Enable interactive chat mode with conversation history
        #[arg(long)]
        chat: bool,
        /// Also fetch and embed casts if not already done
        #[arg(long)]
        fetch_casts: bool,
        /// Maximum number of relevant casts to use as context
        #[arg(long, default_value = "20")]
        context_limit: usize,
        /// LLM temperature (0.0 - 1.0)
        #[arg(long, default_value = "0.7")]
        temperature: f32,
        /// Show detailed sources and reasoning
        #[arg(short, long)]
        verbose: bool,
    },
    /// Analyze user's social graph and network
    Social {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Show detailed analysis
        #[arg(short, long)]
        verbose: bool,
    },
    /// Analyze user's MBTI personality type
    Mbti {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Use LLM for enhanced analysis (requires LLM service configured)
        #[arg(long)]
        llm: bool,
        /// Show detailed dimension scores and analysis
        #[arg(short, long)]
        verbose: bool,
        /// Export result to JSON file
        #[arg(short, long)]
        export: Option<String>,
    },
    /// MBTI-related commands
    #[command(subcommand)]
    MbtiCommands(MbtiCommands),
    /// Generate annual report for user(s)
    AnnualReport {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        /// If --csv is provided, this is ignored
        #[arg(short, long)]
        user: Option<String>,
        /// CSV file path containing FIDs (last column should be FID)
        #[arg(long)]
        csv: Option<String>,
        /// Year for the annual report
        #[arg(short, long)]
        year: u32,
        /// Output file path (for single user mode only)
        #[arg(short, long)]
        output: Option<String>,
        /// Output directory for CSV batch mode
        #[arg(long)]
        output_dir: Option<String>,
        /// Force regenerate even if cache exists (bypasses cache)
        #[arg(long)]
        force: bool,
    },
    /// User metrics and analytics commands
    #[command(subcommand)]
    User(UserCommands),
    /// Cache management commands
    #[command(subcommand)]
    Cache(CacheCommands),
    /// Authentication token management commands
    #[command(subcommand)]
    Auth(AuthCommands),
    /// Utility commands
    #[command(subcommand)]
    Utils(UtilsCommands),
    /// Task management commands
    #[command(subcommand)]
    Task(TaskCommands),
}

#[derive(Subcommand)]
pub enum FetchCommands {
    /// Fetch single user profile and optionally their casts
    User {
        /// FID to fetch
        fid: u64,
        /// Also fetch user's casts
        #[arg(long)]
        with_casts: bool,
        /// Maximum number of casts to fetch (0 = no limit, fetch all)
        #[arg(long, default_value = "0")]
        max_casts: usize,
        /// Generate embeddings for fetched casts
        #[arg(long)]
        generate_embeddings: bool,
    },
    /// Batch fetch multiple users
    Users {
        /// Comma-separated FIDs (e.g., "99,100,101")
        fids: String,
        /// Also fetch their casts
        #[arg(long)]
        with_casts: bool,
        /// Generate embeddings for fetched casts
        #[arg(long)]
        generate_embeddings: bool,
    },
    /// Preload popular users (top N by activity)
    Popular {
        /// Number of popular users to preload
        #[arg(short, long, default_value = "100")]
        limit: usize,
        /// Also fetch their casts
        #[arg(long)]
        with_casts: bool,
        /// Generate embeddings for fetched casts
        #[arg(long)]
        generate_embeddings: bool,
    },
}

#[derive(Subcommand)]
pub enum SyncCommands {
    /// Run all sync (historical + real-time)
    All,
    /// Start synchronization
    Start {
        /// Start block number (default: 0)
        #[arg(long)]
        from: Option<u64>,
        /// End block number (default: latest)
        #[arg(long)]
        to: Option<u64>,
        /// Shard IDs to sync (comma-separated, e.g., "1,2")
        #[arg(long)]
        shard: Option<String>,
        /// Number of parallel workers per shard (default: 1)
        /// Example: --workers 5 with 2 shards = 10 total workers
        #[arg(long)]
        workers: Option<u32>,
        /// Batch size for fetching blocks (default: from config)
        #[arg(long)]
        batch: Option<u32>,
        /// Sync interval in milliseconds (default: from config)
        #[arg(long)]
        interval: Option<u64>,
    },
    /// Test single block synchronization
    Test {
        /// Shard ID to test
        #[arg(long, default_value = "1")]
        shard: u32,
        /// Block number to test
        #[arg(long)]
        block: u64,
    },
    /// Run real-time sync only
    Realtime,
    /// Show sync status and statistics
    Status,
    /// Stop all running sync processes
    Stop {
        /// Force kill processes without graceful shutdown
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum IndexCommands {
    /// Disable non-essential indexes and autovacuum for bulk sync (faster writes)
    Unset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Re-enable indexes and autovacuum after bulk sync (slower but normal operation)
    Set {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Show current index and autovacuum status
    Status,
}

#[derive(Subcommand)]
pub enum FastsyncCommands {
    /// Enable fast sync mode (ULTRA TURBO + `PostgreSQL` optimization)
    Enable {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Disable fast sync mode (restore normal operation)
    Disable {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Show current fast sync status and performance metrics
    Status,
}

#[derive(Subcommand)]
pub enum CastCommands {
    /// Search casts by semantic similarity
    Search {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Minimum similarity threshold (0.0-1.0)
        #[arg(long, default_value = "0.5")]
        threshold: f32,
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },
    /// Get recent casts by FID
    Recent {
        /// FID to query
        fid: i64,
        /// Maximum number of casts
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },
    /// Show cast thread (conversation)
    Thread {
        /// Cast hash (hex)
        hash: String,
        /// Maximum depth
        #[arg(short, long, default_value = "10")]
        depth: usize,
    },
}

#[derive(Subcommand)]
pub enum RagCommands {
    /// Execute a RAG query (profiles)
    Query {
        /// The question to ask
        query: String,
        /// Maximum number of profiles to retrieve
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Retrieval method (semantic, keyword, hybrid, auto)
        #[arg(short, long, default_value = "auto")]
        method: String,
        /// LLM temperature (0.0 - 1.0)
        #[arg(long, default_value = "0.7")]
        temperature: f32,
        /// Maximum tokens for response
        #[arg(long, default_value = "2000")]
        max_tokens: usize,
        /// Show detailed sources
        #[arg(short, long)]
        verbose: bool,
    },
    /// RAG query on cast content
    QueryCasts {
        /// The question to ask
        query: String,
        /// Maximum number of casts to retrieve
        #[arg(short, long, default_value = "10")]
        limit: usize,
        /// Minimum similarity threshold (0.0-1.0)
        #[arg(long, default_value = "0.5")]
        threshold: f32,
        /// LLM temperature (0.0 - 1.0)
        #[arg(long, default_value = "0.7")]
        temperature: f32,
        /// Maximum tokens for response
        #[arg(long, default_value = "2000")]
        max_tokens: usize,
        /// Show detailed sources
        #[arg(short, long)]
        verbose: bool,
    },
    /// Search profiles without LLM generation
    Search {
        /// Search query
        query: String,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Search method (semantic, keyword, hybrid, auto)
        #[arg(short, long, default_value = "auto")]
        method: String,
    },
}

#[derive(Subcommand)]
pub enum EmbeddingsCommands {
    /// Backfill embeddings for user profiles or cast content
    Backfill {
        /// Type of data to backfill embeddings for
        #[arg(value_enum)]
        data_type: EmbeddingDataType,
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Process in batches of N items (profiles or casts)
        #[arg(short, long, default_value = "50")]
        batch_size: usize,
        /// Maximum number of items to process (casts only)
        #[arg(long)]
        limit: Option<usize>,
        /// Use local GPU for embedding generation
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        local_gpu: bool,
        /// Use multi-process parallel processing for maximum performance
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        multiprocess: bool,
        /// GPU device ID to use (0, 1, 2, etc.) - only applies with --local-gpu
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        gpu_device: Option<usize>,
    },
    /// Generate embeddings for a specific profile
    Generate {
        /// FID of the profile
        #[arg(long)]
        fid: i64,
        /// Show generated embedding details
        #[arg(short, long)]
        verbose: bool,
    },
    /// Test embedding generation
    Test {
        /// Text to generate embedding for
        text: String,
    },
    /// Test embedding generation for a specific cast (by message hash)
    TestCast {
        /// Message hash of the cast to test
        message_hash: String,
        /// Use local GPU for embedding generation
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        local_gpu: bool,
        /// GPU device ID to use (0, 1, 2, etc.) - only applies with --local-gpu
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        gpu_device: Option<usize>,
    },
    /// Show embedding statistics
    Stats,
    /// Reset all embeddings (remove vectorization)
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Cast-specific embedding operations
    Cast {
        #[command(subcommand)]
        action: CastEmbeddingAction,
    },
    /// Generate embeddings for cast content (alias for 'cast backfill')
    #[command(name = "backfill-casts")]
    BackfillCasts {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Process in batches of N casts
        #[arg(short, long, default_value = "1000")]
        batch_size: usize,
        /// Maximum number of casts to process
        #[arg(long)]
        limit: Option<usize>,
        /// Use local GPU for embedding generation
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        local_gpu: bool,
        /// Use multi-process parallel processing for maximum performance
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        multiprocess: bool,
        /// GPU device ID to use (0, 1, 2, etc.) - only applies with --local-gpu
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        gpu_device: Option<usize>,
    },
}

#[derive(Subcommand)]
pub enum CastEmbeddingAction {
    /// Backfill embeddings for cast content
    Backfill {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Process in batches of N casts
        #[arg(short, long, default_value = "1000")]
        batch_size: usize,
        /// Maximum number of casts to process
        #[arg(long)]
        limit: Option<usize>,
        /// Use local GPU for embedding generation
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        local_gpu: bool,
        /// Use multi-process parallel processing for maximum performance
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        multiprocess: bool,
        /// GPU device ID to use (0, 1, 2, etc.) - only applies with --local-gpu
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        gpu_device: Option<usize>,
    },
    /// Reset cast embeddings (remove all cast vectorization)
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Backfill cast embeddings with optional multi-vector support
    BackfillMultiVector {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Maximum number of casts to process
        #[arg(short, long)]
        limit: Option<usize>,
        /// Embedding endpoint to use (from config.toml endpoints list)
        #[arg(short, long)]
        endpoint: Option<String>,
        /// Use local GPU for embedding generation
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        local_gpu: bool,
        /// GPU device ID to use (0, 1, 2, etc.) - only applies with --local-gpu
        #[cfg(feature = "local-gpu")]
        #[arg(long)]
        gpu_device: Option<usize>,
        /// Enable multi-vector processing for long texts
        #[arg(long)]
        enable_multi_vector: bool,
        /// Chunking strategy: single, paragraph, sentence, importance, `sliding_window`
        #[arg(long, default_value = "importance")]
        strategy: String,
        /// Aggregation strategy: `first_chunk`, mean, `weighted_mean`, max
        #[arg(long, default_value = "weighted_mean")]
        aggregation: String,
        /// Minimum text length to use multi-vector (default: 500 chars)
        #[arg(long, default_value = "500")]
        min_length: usize,
    },
    /// Migrate existing embeddings to multi-vector format
    Migrate {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
        /// Minimum text length to migrate (default: 1000)
        #[arg(long, default_value = "1000")]
        min_length: usize,
        /// Chunking strategy: single, paragraph, sentence, importance, `sliding_window`
        #[arg(long, default_value = "importance")]
        strategy: String,
        /// Keep original embeddings in `cast_embeddings` table
        #[arg(long, default_value = "true")]
        keep_original: bool,
        /// Batch size for processing
        #[arg(long, default_value = "100")]
        batch_size: usize,
    },
    /// Analyze existing embeddings for migration planning
    Analyze,
}

#[derive(Subcommand)]
pub enum ServeCommands {
    /// Start RESTful API server
    Api {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind to
        #[arg(short, long, default_value = "3000")]
        port: u16,
        /// Enable CORS
        #[arg(long, action = clap::ArgAction::SetTrue)]
        cors: bool,
        /// Enable x402 payment
        #[cfg(feature = "payment")]
        #[arg(long, action = clap::ArgAction::SetTrue)]
        payment: bool,
        /// Address to receive payments (defaults to config or 0x0)
        #[cfg(feature = "payment")]
        #[arg(long)]
        payment_address: Option<String>,
        /// Use testnet (base-sepolia) or mainnet (base). If not specified, uses config default.
        #[cfg(feature = "payment")]
        #[arg(long)]
        testnet: Option<bool>,
    },
    /// Start MCP (Model Context Protocol) server
    Mcp {
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind to
        #[arg(short, long, default_value = "3001")]
        port: u16,
        /// Enable CORS
        #[arg(long, action = clap::ArgAction::SetTrue)]
        cors: bool,
        /// Enable x402 payment
        #[cfg(feature = "payment")]
        #[arg(long, action = clap::ArgAction::SetTrue)]
        payment: bool,
        /// Address to receive payments (defaults to config or 0x0)
        #[cfg(feature = "payment")]
        #[arg(long)]
        payment_address: Option<String>,
        /// Use testnet (base-sepolia) or mainnet (base). If not specified, uses config default.
        #[cfg(feature = "payment")]
        #[arg(long)]
        testnet: Option<bool>,
    },
    /// Start background worker for processing jobs
    Worker {
        /// Queue name(s) to process (comma-separated, e.g., "social,mbti,chat" or default: "social,mbti,chat,cast_stats,annual_report")
        /// Supported queues: social, mbti, chat, cast_stats, annual_report
        #[arg(long, default_value = "social,mbti,chat,cast_stats,annual_report")]
        queue: String,
        /// Number of concurrent workers
        #[arg(long, default_value = "1")]
        workers: usize,
        /// Clean up old/stuck jobs on startup
        #[arg(long, action = clap::ArgAction::SetTrue)]
        cleanup: bool,
    },
    /// Show worker status and active jobs
    Status {
        /// Queue name to filter by (optional)
        #[arg(long)]
        queue: Option<String>,
        /// Show detailed information for a specific job (format: type:fid, e.g., social:66)
        #[arg(long)]
        job: Option<String>,
    },
}

#[derive(ValueEnum, Clone)]
pub enum EmbeddingDataType {
    /// Backfill embeddings for user profiles
    User,
    /// Backfill embeddings for cast content
    Cast,
}

#[derive(Subcommand)]
pub enum CacheCommands {
    /// Delete all cache entries
    Clear {
        /// Skip confirmation prompt
        #[arg(short, long)]
        force: bool,
    },
    /// Delete cache for a specific FID
    Delete {
        /// FID to delete cache for
        fid: i64,
        /// Delete only profile cache
        #[arg(long)]
        profile_only: bool,
        /// Delete only social cache
        #[arg(long)]
        social_only: bool,
        /// Delete only MBTI cache
        #[arg(long)]
        mbti_only: bool,
    },
}

#[derive(ValueEnum, Clone)]
pub enum DataType {
    /// List FIDs (user IDs)
    Fid,
    /// List user profiles
    Profiles,
    /// List casts
    Casts,
    /// List follows
    Follows,
    /// List user data
    UserData,
}

#[derive(Subcommand)]
pub enum AuthCommands {
    /// Generate a new authentication token/secret pair
    Generate {
        /// Token name (identifier for this token)
        #[arg(short, long)]
        name: Option<String>,
    },
    /// List all configured authentication tokens
    List,
    /// Revoke (delete) an authentication token
    Revoke {
        /// Token name to revoke
        name: String,
    },
}

#[derive(Subcommand)]
pub enum UtilsCommands {
    /// Get top users by follower count
    TopUser {
        /// Number of top users to return
        limit: i64,
    },
}

#[derive(Subcommand)]
pub enum UserCommands {
    /// Get cast statistics for a user
    CastStats {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Start timestamp (Unix seconds, optional)
        #[arg(long)]
        start: Option<i64>,
        /// End timestamp (Unix seconds, optional)
        #[arg(long)]
        end: Option<i64>,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
        /// Force regenerate even if cache exists (bypasses cache)
        #[arg(long)]
        force: bool,
    },
    /// Get engagement metrics for a user
    Engagement {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Start timestamp (Unix seconds, optional)
        #[arg(long)]
        start: Option<i64>,
        /// End timestamp (Unix seconds, optional)
        #[arg(long)]
        end: Option<i64>,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get temporal activity metrics for a user
    TemporalActivity {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Start timestamp (Unix seconds, optional)
        #[arg(long)]
        start: Option<i64>,
        /// End timestamp (Unix seconds, optional)
        #[arg(long)]
        end: Option<i64>,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get content style analysis for a user
    ContentStyle {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Start timestamp (Unix seconds, optional)
        #[arg(long)]
        start: Option<i64>,
        /// End timestamp (Unix seconds, optional)
        #[arg(long)]
        end: Option<i64>,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get follower growth metrics for a user
    FollowerGrowth {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Start timestamp (Unix seconds, optional)
        #[arg(long)]
        start: Option<i64>,
        /// End timestamp (Unix seconds, optional)
        #[arg(long)]
        end: Option<i64>,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get domain/username status for a user
    Domains {
        /// FID or username of the user (e.g., "99" or "@jesse.base.eth")
        user: String,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get network statistics and averages
    NetworkStats {
        /// Start timestamp (Unix seconds, optional)
        #[arg(long)]
        start: Option<i64>,
        /// End timestamp (Unix seconds, optional)
        #[arg(long)]
        end: Option<i64>,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum TaskCommands {
    /// List all tasks (jobs) with their execution status
    List {
        /// Queue name to filter by (optional)
        #[arg(long)]
        queue: Option<String>,
        /// Show only tasks with specific status (pending, processing, completed, failed)
        #[arg(long)]
        status: Option<String>,
        /// Maximum number of tasks to show
        #[arg(short, long, default_value = "100")]
        limit: usize,
    },
    /// Stop a task by its job key (format: type:fid or type:fid:year for annual_report)
    Stop {
        /// Job key to stop (e.g., "social:66" or "annual_report:66:2024")
        job_key: String,
        /// Force stop even if task is processing
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum MbtiCommands {
    /// Batch analyze multiple users' MBTI types
    Batch {
        /// Comma-separated FIDs (e.g., "99,100,101")
        fids: String,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get MBTI statistics across all analyzed users
    Stats {
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Search users by MBTI type
    Search {
        /// MBTI type (e.g., "INTJ", "ENFP")
        mbti_type: String,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Get MBTI compatibility between two users
    Compatibility {
        /// First user FID or username
        user1: String,
        /// Second user FID or username
        user2: String,
        /// Output file path (optional, defaults to stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
}
