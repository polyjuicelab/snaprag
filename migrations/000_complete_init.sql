-- Complete database initialization for SnapRAG
-- This script creates all tables with the exact structure from production
-- Run with: snaprag init --force
--
-- NOTE: This script is IDEMPOTENT and safe to run multiple times.
-- It uses CREATE TABLE IF NOT EXISTS to avoid dropping data.
-- To drop existing data, use: snaprag reset --force

-- Enable pgvector extension (requires superuser)
-- If this fails, run on DB server: sudo -u postgres psql -d snaprag -c 'CREATE EXTENSION IF NOT EXISTS vector;'
CREATE EXTENSION IF NOT EXISTS vector;

-- Enable pg_trgm extension for trigram text search (requires superuser)
-- If this fails, run on DB server: sudo -u postgres psql -d snaprag -c 'CREATE EXTENSION IF NOT EXISTS pg_trgm;'
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ==============================================================================
-- 1. USER PROFILES (Event-Sourcing Architecture)
-- ==============================================================================

-- Event-sourcing table: stores individual field changes
CREATE TABLE IF NOT EXISTS user_profile_changes (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    fid bigint NOT NULL,
    field_name varchar(50) NOT NULL,
    field_value text,
    timestamp bigint NOT NULL,
    message_hash bytea NOT NULL UNIQUE,
    shard_id integer,
    block_height bigint,
    transaction_fid bigint,
    created_at timestamp with time zone DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_profile_changes_fid_field_ts 
    ON user_profile_changes(fid, field_name, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_profile_changes_message_hash 
    ON user_profile_changes(message_hash);
-- Optimized index for username lookups (used by get_user_profile_by_username)
CREATE INDEX IF NOT EXISTS idx_profile_changes_username_value 
    ON user_profile_changes(field_value, fid, timestamp DESC) 
    WHERE field_name = 'username';

-- Reconstructed view: merges field changes into complete profiles
CREATE OR REPLACE VIEW user_profiles AS
WITH latest_changes AS (
    SELECT DISTINCT ON (fid, field_name)
        fid,
        field_name,
        field_value,
        timestamp
    FROM user_profile_changes
    ORDER BY fid, field_name, timestamp DESC
)
SELECT 
    gen_random_uuid() as id,
    fid,
    MAX(CASE WHEN field_name = 'username' THEN field_value END) as username,
    MAX(CASE WHEN field_name = 'display_name' THEN field_value END) as display_name,
    MAX(CASE WHEN field_name = 'bio' THEN field_value END) as bio,
    MAX(CASE WHEN field_name = 'pfp_url' THEN field_value END) as pfp_url,
    MAX(CASE WHEN field_name = 'banner_url' THEN field_value END) as banner_url,
    MAX(CASE WHEN field_name = 'location' THEN field_value END) as location,
    MAX(CASE WHEN field_name = 'website_url' THEN field_value END) as website_url,
    MAX(CASE WHEN field_name = 'twitter_username' THEN field_value END) as twitter_username,
    MAX(CASE WHEN field_name = 'github_username' THEN field_value END) as github_username,
    MAX(CASE WHEN field_name = 'primary_address_ethereum' THEN field_value END) as primary_address_ethereum,
    MAX(CASE WHEN field_name = 'primary_address_solana' THEN field_value END) as primary_address_solana,
    NULL::varchar(255) as profile_token,
    NULL::vector(384) as profile_embedding,
    NULL::vector(384) as bio_embedding,
    NULL::vector(384) as interests_embedding,
    MAX(timestamp) as last_updated_timestamp,
    NOW() as last_updated_at,
    NULL::integer as shard_id,
    NULL::bigint as block_height,
    NULL::bigint as transaction_fid
FROM latest_changes
GROUP BY fid;

-- Profile Embeddings (separate table for UPDATE support)
CREATE TABLE IF NOT EXISTS profile_embeddings (
    fid bigint PRIMARY KEY,
    profile_embedding vector(384),
    bio_embedding vector(384),
    interests_embedding vector(384),
    updated_at timestamp with time zone DEFAULT now()
);

-- Enhanced view with embeddings
CREATE OR REPLACE VIEW user_profiles_with_embeddings AS
SELECT 
    p.*,
    COALESCE(e.profile_embedding, NULL::vector(384)) as profile_embedding_vec,
    COALESCE(e.bio_embedding, NULL::vector(384)) as bio_embedding_vec,
    COALESCE(e.interests_embedding, NULL::vector(384)) as interests_embedding_vec
FROM user_profiles p
LEFT JOIN profile_embeddings e ON p.fid = e.fid;

-- ==============================================================================
-- 2. USER DATA CHANGES
-- ==============================================================================

CREATE TABLE IF NOT EXISTS user_data_changes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    data_type SMALLINT NOT NULL,
    old_value TEXT,
    new_value TEXT NOT NULL,
    change_timestamp BIGINT NOT NULL,
    message_hash BYTEA NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_activities (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    activity_type VARCHAR(50) NOT NULL,
    activity_data JSONB,
    timestamp BIGINT NOT NULL,
    message_hash BYTEA,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Note: user_activity_timeline table removed for performance optimization

-- ==============================================================================
-- 3. CASTS AND LINKS
-- ==============================================================================

CREATE TABLE IF NOT EXISTS casts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    text TEXT,
    timestamp BIGINT NOT NULL,
    message_hash BYTEA UNIQUE NOT NULL,
    parent_hash BYTEA,
    root_hash BYTEA,
    embeds JSONB,
    mentions JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT
);

-- Add tracking columns if they don't exist
ALTER TABLE casts ADD COLUMN IF NOT EXISTS shard_id INTEGER;
ALTER TABLE casts ADD COLUMN IF NOT EXISTS block_height BIGINT;
ALTER TABLE casts ADD COLUMN IF NOT EXISTS transaction_fid BIGINT;

CREATE TABLE IF NOT EXISTS links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    target_fid BIGINT NOT NULL,
    link_type VARCHAR(50) NOT NULL DEFAULT 'follow',
    event_type VARCHAR(10) NOT NULL DEFAULT 'add',  -- 'add' or 'remove'
    timestamp BIGINT NOT NULL,
    message_hash BYTEA UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT
);

-- Add tracking columns if they don't exist
ALTER TABLE links ADD COLUMN IF NOT EXISTS shard_id INTEGER;
ALTER TABLE links ADD COLUMN IF NOT EXISTS block_height BIGINT;
ALTER TABLE links ADD COLUMN IF NOT EXISTS transaction_fid BIGINT;

-- Indexes for efficient window function queries
CREATE INDEX IF NOT EXISTS idx_links_latest ON links(fid, target_fid, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_links_event_type ON links(event_type);
CREATE INDEX IF NOT EXISTS idx_links_fid_type ON links(fid, link_type);

CREATE TABLE IF NOT EXISTS user_data (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    data_type SMALLINT NOT NULL,
    value TEXT NOT NULL,
    timestamp BIGINT NOT NULL,
    message_hash BYTEA UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT
);

-- Add tracking columns if they don't exist
ALTER TABLE user_data ADD COLUMN IF NOT EXISTS shard_id INTEGER;
ALTER TABLE user_data ADD COLUMN IF NOT EXISTS block_height BIGINT;
ALTER TABLE user_data ADD COLUMN IF NOT EXISTS transaction_fid BIGINT;

-- ==============================================================================
-- 4. REACTIONS AND VERIFICATIONS
-- ==============================================================================

CREATE TABLE IF NOT EXISTS reactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    target_cast_hash BYTEA NOT NULL,
    target_fid BIGINT,
    reaction_type SMALLINT NOT NULL,  -- 1=like, 2=recast
    event_type VARCHAR(10) NOT NULL DEFAULT 'add',  -- 'add' or 'remove'
    timestamp BIGINT NOT NULL,
    message_hash BYTEA UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT
);

CREATE INDEX IF NOT EXISTS idx_reactions_fid ON reactions(fid);
CREATE INDEX IF NOT EXISTS idx_reactions_target_cast ON reactions(target_cast_hash);
CREATE INDEX IF NOT EXISTS idx_reactions_target_fid ON reactions(target_fid);
CREATE INDEX IF NOT EXISTS idx_reactions_type ON reactions(reaction_type);
CREATE INDEX IF NOT EXISTS idx_reactions_timestamp ON reactions(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_reactions_latest ON reactions(fid, target_cast_hash, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_reactions_event_type ON reactions(event_type);

CREATE TABLE IF NOT EXISTS verifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    address BYTEA NOT NULL,
    claim_signature BYTEA,
    block_hash BYTEA,
    verification_type SMALLINT DEFAULT 0,
    chain_id INTEGER,
    event_type VARCHAR(10) NOT NULL DEFAULT 'add',  -- 'add' or 'remove'
    timestamp BIGINT NOT NULL,
    message_hash BYTEA UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT
);

CREATE INDEX IF NOT EXISTS idx_verifications_fid ON verifications(fid);
CREATE INDEX IF NOT EXISTS idx_verifications_address ON verifications(address);
CREATE INDEX IF NOT EXISTS idx_verifications_timestamp ON verifications(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_verifications_latest ON verifications(fid, address, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_verifications_event_type ON verifications(event_type);

CREATE TABLE IF NOT EXISTS username_proofs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    username TEXT NOT NULL,
    owner BYTEA NOT NULL,
    signature BYTEA NOT NULL,
    timestamp BIGINT NOT NULL,
    username_type SMALLINT NOT NULL,  -- 1=FNAME, 2=ENS
    message_hash BYTEA UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT,
    UNIQUE(fid, username_type)  -- One username per FID per type (FNAME vs ENS)
);

CREATE INDEX IF NOT EXISTS idx_username_proofs_fid ON username_proofs(fid);
CREATE INDEX IF NOT EXISTS idx_username_proofs_username ON username_proofs(username);
CREATE INDEX IF NOT EXISTS idx_username_proofs_timestamp ON username_proofs(timestamp DESC);

CREATE TABLE IF NOT EXISTS frame_actions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    url TEXT NOT NULL,
    button_index INTEGER,
    cast_hash BYTEA,
    cast_fid BIGINT,
    input_text TEXT,
    state BYTEA,
    transaction_id BYTEA,
    timestamp BIGINT NOT NULL,
    message_hash BYTEA UNIQUE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    shard_id INTEGER,
    block_height BIGINT,
    transaction_fid BIGINT
);

CREATE INDEX IF NOT EXISTS idx_frame_actions_fid ON frame_actions(fid);
CREATE INDEX IF NOT EXISTS idx_frame_actions_url ON frame_actions(url);
CREATE INDEX IF NOT EXISTS idx_frame_actions_cast_hash ON frame_actions(cast_hash);
CREATE INDEX IF NOT EXISTS idx_frame_actions_timestamp ON frame_actions(timestamp DESC);

-- ==============================================================================
-- 5. EMBEDDINGS
-- ==============================================================================

CREATE TABLE IF NOT EXISTS cast_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_hash BYTEA UNIQUE NOT NULL,
    fid BIGINT NOT NULL,
    text TEXT NOT NULL,
    embedding VECTOR(384),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Create trigger function for updated_at
CREATE OR REPLACE FUNCTION update_cast_embeddings_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger
DROP TRIGGER IF EXISTS trigger_cast_embeddings_updated_at ON cast_embeddings;
CREATE TRIGGER trigger_cast_embeddings_updated_at
    BEFORE UPDATE ON cast_embeddings
    FOR EACH ROW
    EXECUTE FUNCTION update_cast_embeddings_updated_at();

-- Multi-vector embedding storage tables
CREATE TABLE IF NOT EXISTS cast_embedding_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_hash BYTEA NOT NULL,
    fid BIGINT NOT NULL,
    chunk_index INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,
    chunk_strategy VARCHAR(50) NOT NULL, -- 'paragraph', 'sentence', 'importance', 'sliding_window'
    embedding VECTOR(384) NOT NULL,
    chunk_length INTEGER NOT NULL,
    is_aggregated BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(message_hash, chunk_index)
);

-- Aggregated embeddings table (for backward compatibility)
CREATE TABLE IF NOT EXISTS cast_embedding_aggregated (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_hash BYTEA UNIQUE NOT NULL,
    fid BIGINT NOT NULL,
    text TEXT NOT NULL,
    embedding VECTOR(384) NOT NULL,
    aggregation_strategy VARCHAR(50) NOT NULL, -- 'mean', 'max', 'weighted_mean', 'first_chunk'
    chunk_count INTEGER NOT NULL,
    total_text_length INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- ==============================================================================
-- 5. ONCHAIN EVENTS (System Messages)
-- ==============================================================================

CREATE TABLE IF NOT EXISTS onchain_events (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    fid bigint NOT NULL,
    event_type integer NOT NULL,  -- OnChainEventType: 1=signer, 3=id_register, 4=storage_rent
    chain_id integer NOT NULL,
    block_number integer NOT NULL,
    block_hash bytea,
    block_timestamp bigint NOT NULL,
    transaction_hash bytea,
    log_index integer,
    event_data jsonb,  -- Store the full event body as JSON
    shard_id integer,
    shard_block_height bigint,
    created_at timestamp with time zone DEFAULT now(),
    UNIQUE(transaction_hash, log_index)  -- Prevent duplicates
);

CREATE INDEX IF NOT EXISTS idx_onchain_events_fid 
    ON onchain_events(fid);
CREATE INDEX IF NOT EXISTS idx_onchain_events_type 
    ON onchain_events(event_type);
CREATE INDEX IF NOT EXISTS idx_onchain_events_block 
    ON onchain_events(block_number DESC);

-- ==============================================================================
-- 6. SYNC TRACKING
-- ==============================================================================

CREATE TABLE IF NOT EXISTS sync_progress (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    shard_id INTEGER UNIQUE NOT NULL,
    last_processed_height BIGINT DEFAULT 0,
    status VARCHAR(20) DEFAULT 'idle',
    error_message TEXT,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS sync_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    shard_id INTEGER UNIQUE NOT NULL,
    total_messages BIGINT DEFAULT 0,
    total_blocks BIGINT DEFAULT 0,
    last_updated TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS processed_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_hash BYTEA UNIQUE NOT NULL,
    shard_id INTEGER NOT NULL,
    block_height BIGINT NOT NULL,
    transaction_fid BIGINT NOT NULL,
    message_type VARCHAR(50) NOT NULL,
    fid BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    content_hash BYTEA,
    processed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- ==============================================================================
-- 6. OTHER TABLES
-- ==============================================================================

CREATE TABLE IF NOT EXISTS user_profile_trends (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    fid BIGINT NOT NULL,
    trend_period VARCHAR(20) NOT NULL,
    trend_date DATE NOT NULL,
    profile_changes_count INTEGER DEFAULT 0,
    bio_changes_count INTEGER DEFAULT 0,
    username_changes_count INTEGER DEFAULT 0,
    activity_score FLOAT DEFAULT 0.0,
    engagement_score FLOAT DEFAULT 0.0,
    profile_embedding VECTOR(384),
    bio_embedding VECTOR(384),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(fid, trend_period, trend_date)
);

-- ==============================================================================
-- 7. ESSENTIAL INDEXES ONLY (for write performance)
-- ==============================================================================

-- Note: user_profiles is a VIEW, indexes are on underlying tables

-- casts (essential only)
CREATE INDEX IF NOT EXISTS idx_casts_fid ON casts(fid);

-- processed_messages (for sync tracking)
CREATE INDEX IF NOT EXISTS idx_processed_shard_height 
ON processed_messages(shard_id, block_height DESC);

CREATE INDEX IF NOT EXISTS idx_processed_messages_hash 
ON processed_messages(message_hash);

-- sync_progress
CREATE INDEX IF NOT EXISTS idx_sync_progress_shard_id ON sync_progress(shard_id);

-- cast_embeddings
CREATE INDEX IF NOT EXISTS idx_cast_embeddings_message_hash ON cast_embeddings(message_hash);
CREATE INDEX IF NOT EXISTS idx_cast_embeddings_fid ON cast_embeddings(fid);

-- Multi-vector embedding indexes
CREATE INDEX IF NOT EXISTS idx_cast_embedding_chunks_message_hash ON cast_embedding_chunks(message_hash);
CREATE INDEX IF NOT EXISTS idx_cast_embedding_chunks_fid ON cast_embedding_chunks(fid);
CREATE INDEX IF NOT EXISTS idx_cast_embedding_chunks_strategy ON cast_embedding_chunks(chunk_strategy);

CREATE INDEX IF NOT EXISTS idx_cast_embedding_aggregated_message_hash ON cast_embedding_aggregated(message_hash);
CREATE INDEX IF NOT EXISTS idx_cast_embedding_aggregated_fid ON cast_embedding_aggregated(fid);

-- Vector similarity search indexes for multi-vector tables
CREATE INDEX IF NOT EXISTS idx_cast_embedding_chunks_embedding_cosine 
ON cast_embedding_chunks USING ivfflat (embedding vector_cosine_ops) 
WITH (lists = 100);

CREATE INDEX IF NOT EXISTS idx_cast_embedding_aggregated_embedding_cosine 
ON cast_embedding_aggregated USING ivfflat (embedding vector_cosine_ops) 
WITH (lists = 100);

-- Optimize queries for cast embeddings backfill
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_text_hash 
ON casts(message_hash) 
WHERE text IS NOT NULL AND length(text) > 0;

-- Additional indexes for better performance on large datasets
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_message_hash_desc 
ON casts(message_hash DESC) 
WHERE text IS NOT NULL AND length(text) > 0;

-- Optimize cast_embeddings lookups
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embeddings_message_hash_btree 
ON cast_embeddings USING btree(message_hash);

-- Composite index for text filtering and message_hash ordering
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_text_hash_desc_composite 
ON casts(message_hash DESC, fid, timestamp) 
WHERE text IS NOT NULL AND length(text) > 0;

-- ==============================================================================
-- 10. PG_TRGM TEXT SEARCH INDEXES
-- ==============================================================================

-- GIN indexes using pg_trgm for fast text search on casts
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_text_trgm 
ON casts USING gin(text gin_trgm_ops) 
WHERE text IS NOT NULL AND length(text) > 0;

-- GIN indexes for cast embeddings text search
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embeddings_text_trgm 
ON cast_embeddings USING gin(text gin_trgm_ops) 
WHERE text IS NOT NULL AND length(text) > 0;

-- GIN indexes for cast embedding chunks text search
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_chunks_text_trgm 
ON cast_embedding_chunks USING gin(chunk_text gin_trgm_ops) 
WHERE chunk_text IS NOT NULL AND length(chunk_text) > 0;

-- GIN indexes for user profile fields text search
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_profile_changes_value_trgm 
ON user_profile_changes USING gin(field_value gin_trgm_ops) 
WHERE field_value IS NOT NULL AND length(field_value) > 0;

-- GIN indexes for username proofs text search
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_username_proofs_username_trgm 
ON username_proofs USING gin(username gin_trgm_ops) 
WHERE username IS NOT NULL AND length(username) > 0;

-- ==============================================================================
-- 8. FOREIGN KEY (if needed)
-- ==============================================================================

-- Add FK constraint for cast_embeddings (optional, can be slow on HDD)
-- ALTER TABLE cast_embeddings 
-- ADD CONSTRAINT fk_cast_embeddings_message_hash 
-- FOREIGN KEY (message_hash) REFERENCES casts(message_hash) ON DELETE CASCADE;

-- ==============================================================================
-- 9. TABLE OPTIMIZATION
-- ==============================================================================

-- Disable autovacuum for high-write tables during bulk sync (better performance)
-- To re-enable after sync: ALTER TABLE table_name SET (autovacuum_enabled = true);
-- Or run: psql -f scripts/enable_autovacuum.sql

ALTER TABLE casts SET (
    autovacuum_enabled = false
);

ALTER TABLE processed_messages SET (
    autovacuum_enabled = false
);

ALTER TABLE user_profile_changes SET (
    autovacuum_enabled = false
);

ALTER TABLE links SET (
    autovacuum_enabled = false
);

ALTER TABLE reactions SET (
    autovacuum_enabled = false
);

ALTER TABLE verifications SET (
    autovacuum_enabled = false
);

ALTER TABLE username_proofs SET (
    autovacuum_enabled = false
);

ALTER TABLE frame_actions SET (
    autovacuum_enabled = false
);

-- Update statistics
ANALYZE;

SELECT 'SnapRAG database initialized successfully!' as status;


