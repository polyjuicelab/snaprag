-- Annual Report API Performance Indexes
-- These indexes optimize queries for the annual report endpoints
-- Run with: psql -d snaprag -f migrations/001_annual_report_indexes.sql

-- ==============================================================================
-- 1. ENGAGEMENT METRICS INDEXES
-- ==============================================================================

-- Optimize reactions received queries (JOIN reactions + casts)
-- Query pattern: JOIN reactions r ON r.target_cast_hash = c.message_hash WHERE c.fid = X
-- This composite index helps with the JOIN and filtering
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_target_cast_timestamp_type 
ON reactions(target_cast_hash, timestamp, reaction_type, event_type)
WHERE event_type = 'add';

-- Optimize casts queries by fid and timestamp range
-- Used by: engagement, temporal, content_style queries
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_fid_timestamp 
ON casts(fid, timestamp DESC)
WHERE text IS NOT NULL AND text != '';

-- ==============================================================================
-- 2. TEMPORAL ACTIVITY INDEXES
-- ==============================================================================

-- The idx_casts_fid_timestamp index above also covers temporal queries
-- Additional index for parent_hash lookups (replies received)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_parent_hash_timestamp 
ON casts(parent_hash, timestamp DESC)
WHERE parent_hash IS NOT NULL;

-- ==============================================================================
-- 3. FOLLOWER GROWTH INDEXES
-- ==============================================================================

-- Optimize follower count queries by target_fid
-- Query pattern: WHERE target_fid = X AND link_type = 'follow' AND timestamp <= Y
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_links_target_fid_type_timestamp 
ON links(target_fid, link_type, timestamp DESC)
WHERE link_type = 'follow';

-- Optimize following count queries by fid
-- Query pattern: WHERE fid = X AND link_type = 'follow' AND timestamp <= Y
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_links_fid_type_timestamp 
ON links(fid, link_type, timestamp DESC)
WHERE link_type = 'follow';

-- ==============================================================================
-- 4. FRAME ACTIONS INDEXES
-- ==============================================================================

-- Optimize frame usage count queries
-- Query pattern: WHERE fid = X AND timestamp >= Y AND timestamp <= Z
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_frame_actions_fid_timestamp 
ON frame_actions(fid, timestamp DESC);

-- ==============================================================================
-- 5. NETWORK STATS INDEXES (Optional - for aggregation queries)
-- ==============================================================================

-- These are expensive aggregation queries, indexes help but may not be critical
-- The existing idx_casts_fid_timestamp should help with user-level aggregations

-- ==============================================================================
-- INDEX USAGE NOTES
-- ==============================================================================
-- 
-- These indexes are designed to optimize:
-- 1. Engagement metrics: reactions received on user's casts
-- 2. Temporal analysis: casts by fid and time range
-- 3. Follower growth: links by target_fid/fid and time range
-- 4. Content style: casts text retrieval by fid and time range
--
-- All indexes use CONCURRENTLY to avoid locking during creation.
-- Partial indexes (WHERE clauses) reduce index size and improve performance.
--
-- Monitor index usage with:
-- SELECT schemaname, tablename, indexname, idx_scan, idx_tup_read, idx_tup_fetch
-- FROM pg_stat_user_indexes
-- WHERE indexname LIKE 'idx_%annual%' OR indexname LIKE 'idx_%engagement%'
-- ORDER BY idx_scan DESC;

