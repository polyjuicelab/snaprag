//! Cast retrieval module for semantic search

use std::sync::Arc;

use tracing::debug;

use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::errors::Result;
use crate::models::CastSearchResult;

/// Retriever for cast content
pub struct CastRetriever {
    database: Arc<Database>,
    embedding_service: Arc<EmbeddingService>,
}

impl CastRetriever {
    /// Create a new cast retriever
    #[must_use]
    pub const fn new(database: Arc<Database>, embedding_service: Arc<EmbeddingService>) -> Self {
        Self {
            database,
            embedding_service,
        }
    }

    /// Semantic search for casts
    ///
    /// # Errors
    /// - Embedding generation errors (API failures, preprocessing errors)
    /// - Database query errors (connection failures, vector search errors)
    /// - Invalid threshold value (not in range 0.0-1.0)
    pub async fn semantic_search(
        &self,
        query: &str,
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<CastSearchResult>> {
        debug!("Performing cast semantic search: {}", query);

        // Generate query embedding
        let query_embedding = self.embedding_service.generate(query).await?;

        // Search in database
        #[allow(clippy::cast_possible_wrap)] // limit is user-specified, typically small values
        let results = self
            .database
            .semantic_search_casts(query_embedding, limit as i64, threshold)
            .await?;

        debug!("Found {} matching casts", results.len());
        Ok(results)
    }

    /// Search casts by FID
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    pub async fn search_by_fid(&self, fid: i64, limit: usize) -> Result<Vec<crate::models::Cast>> {
        debug!("Searching casts for FID {}", fid);

        #[allow(clippy::cast_possible_wrap)] // limit is user-specified, typically small values
        let casts = self
            .database
            .get_casts_by_fid(fid, Some(limit as i64), Some(0))
            .await?;

        Ok(casts)
    }

    /// Get cast thread
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    /// - Invalid message hash (malformed hash)
    pub async fn get_thread(
        &self,
        message_hash: Vec<u8>,
        max_depth: usize,
    ) -> Result<crate::database::CastThread> {
        debug!("Retrieving cast thread");

        let thread = self
            .database
            .get_cast_thread(message_hash, max_depth)
            .await?;

        Ok(thread)
    }

    /// Search recent casts across all users
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    /// - Invalid limit or offset values
    pub async fn search_recent(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<crate::models::Cast>> {
        debug!(
            "Searching recent casts (limit: {}, offset: {})",
            limit, offset
        );

        // Query recent casts with text content
        #[allow(clippy::cast_possible_wrap)] // limit/offset are user-specified, typically small values
        let casts = sqlx::query_as::<_, crate::models::Cast>(
            "SELECT * FROM casts WHERE text IS NOT NULL AND text != '' ORDER BY timestamp DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit as i64)
        .bind(offset as i64)
        .fetch_all(self.database.pool())
        .await?;

        Ok(casts)
    }

    /// Search casts with time range filter
    pub async fn search_by_time_range(
        &self,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
        limit: usize,
    ) -> Result<Vec<crate::models::Cast>> {
        debug!("Searching casts by time range");

        let casts = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, crate::models::Cast>(
                "SELECT * FROM casts WHERE text IS NOT NULL AND timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp DESC LIMIT $3",
            )
            .bind(start)
            .bind(end)
            .bind(i64::try_from(limit).unwrap_or(i64::MAX))
            .fetch_all(self.database.pool())
            .await?
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, crate::models::Cast>(
                "SELECT * FROM casts WHERE text IS NOT NULL AND timestamp >= $1 ORDER BY timestamp DESC LIMIT $2",
            )
            .bind(start)
            .bind(i64::try_from(limit).unwrap_or(i64::MAX))
            .fetch_all(self.database.pool())
            .await?
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, crate::models::Cast>(
                "SELECT * FROM casts WHERE text IS NOT NULL AND timestamp <= $1 ORDER BY timestamp DESC LIMIT $2",
            )
            .bind(end)
            .bind(i64::try_from(limit).unwrap_or(i64::MAX))
            .fetch_all(self.database.pool())
            .await?
        } else {
            self.search_recent(limit, 0).await?
        };

        Ok(casts)
    }

    /// Semantic search with FID filter and engagement metrics
    pub async fn semantic_search_by_fid(
        &self,
        query: &str,
        fid: i64,
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<CastSearchResult>> {
        debug!("Performing cast semantic search for FID {}", fid);

        // Generate query embedding
        let query_embedding = self.embedding_service.generate(query).await?;
        let threshold_val = threshold.unwrap_or(0.0);

        #[derive(sqlx::FromRow)]
        struct RawResult {
            message_hash: Vec<u8>,
            fid: i64,
            text: String,
            timestamp: i64,
            parent_hash: Option<Vec<u8>>,
            embeds: Option<serde_json::Value>,
            mentions: Option<serde_json::Value>,
            similarity: f32,
            reply_count: Option<i64>,
            reaction_count: Option<i64>,
        }

        // Search with FID filter and engagement metrics
        let raw_results = sqlx::query_as::<_, RawResult>(
            r"
            SELECT 
                ce.message_hash,
                ce.fid,
                ce.text,
                c.timestamp,
                c.parent_hash,
                c.embeds,
                c.mentions,
                1 - (ce.embedding <=> $1) as similarity,
                (SELECT COUNT(*) FROM casts WHERE parent_hash = ce.message_hash) as reply_count,
                (SELECT COUNT(*) FROM (
                    SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                    ) as rn
                    FROM reactions
                    WHERE target_cast_hash = ce.message_hash
                ) r WHERE r.rn = 1 AND r.event_type = 'add') as reaction_count
            FROM cast_embeddings ce
            INNER JOIN casts c ON ce.message_hash = c.message_hash
            WHERE ce.fid = $2 AND 1 - (ce.embedding <=> $1) > $3
            ORDER BY ce.embedding <=> $1
            LIMIT $4
            ",
        )
        .bind(&query_embedding)
        .bind(fid)
        .bind(threshold_val)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(self.database.pool())
        .await?;

        let results = raw_results
            .into_iter()
            .map(|r| CastSearchResult {
                message_hash: r.message_hash,
                fid: r.fid,
                text: r.text,
                timestamp: r.timestamp,
                parent_hash: r.parent_hash,
                embeds: r.embeds,
                mentions: r.mentions,
                similarity: r.similarity,
                reply_count: Some(r.reply_count.unwrap_or(0)),
                reaction_count: Some(r.reaction_count.unwrap_or(0)),
                chunk_index: None,
                chunk_text: None,
                chunk_strategy: None,
            })
            .collect();

        Ok(results)
    }

    /// Keyword search for casts with engagement metrics
    pub async fn keyword_search(&self, query: &str, limit: usize) -> Result<Vec<CastSearchResult>> {
        debug!("Performing cast keyword search: {}", query);

        #[derive(sqlx::FromRow)]
        struct RawResult {
            message_hash: Vec<u8>,
            fid: i64,
            text: String,
            timestamp: i64,
            parent_hash: Option<Vec<u8>>,
            embeds: Option<serde_json::Value>,
            mentions: Option<serde_json::Value>,
            similarity: f32,
            reply_count: Option<i64>,
            reaction_count: Option<i64>,
        }

        let raw_results = sqlx::query_as::<_, RawResult>(
            r"
            SELECT 
                c.message_hash,
                c.fid,
                COALESCE(c.text, '') as text,
                c.timestamp,
                c.parent_hash,
                c.embeds,
                c.mentions,
                0.8 as similarity,
                (SELECT COUNT(*) FROM casts WHERE parent_hash = c.message_hash) as reply_count,
                (SELECT COUNT(*) FROM (
                    SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                    ) as rn
                    FROM reactions
                    WHERE target_cast_hash = c.message_hash
                ) r WHERE r.rn = 1 AND r.event_type = 'add') as reaction_count
            FROM casts c
            WHERE c.text ILIKE $1
            ORDER BY c.timestamp DESC
            LIMIT $2
            ",
        )
        .bind(format!("%{query}%"))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(self.database.pool())
        .await?;

        let results = raw_results
            .into_iter()
            .map(|r| CastSearchResult {
                message_hash: r.message_hash,
                fid: r.fid,
                text: r.text,
                timestamp: r.timestamp,
                parent_hash: r.parent_hash,
                embeds: r.embeds,
                mentions: r.mentions,
                similarity: r.similarity,
                reply_count: Some(r.reply_count.unwrap_or(0)),
                reaction_count: Some(r.reaction_count.unwrap_or(0)),
                chunk_index: None,
                chunk_text: None,
                chunk_strategy: None,
            })
            .collect();

        Ok(results)
    }

    /// Hybrid search combining semantic and keyword
    pub async fn hybrid_search(
        &self,
        query: &str,
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<CastSearchResult>> {
        debug!("Performing cast hybrid search: {}", query);

        // Run both searches in parallel
        let semantic_results = self.semantic_search(query, limit, threshold);
        let keyword_results = self.keyword_search(query, limit);

        let (semantic, keyword) = tokio::try_join!(semantic_results, keyword_results)?;

        // Merge and deduplicate results using RRF
        let merged = Self::merge_results(semantic, keyword);

        Ok(merged.into_iter().take(limit).collect())
    }

    /// Merge and rerank results from multiple sources
    fn merge_results(
        semantic: Vec<CastSearchResult>,
        keyword: Vec<CastSearchResult>,
    ) -> Vec<CastSearchResult> {
        use std::collections::HashMap;

        let mut scores: HashMap<Vec<u8>, (f32, CastSearchResult)> = HashMap::new();
        let k = 60.0; // RRF constant

        // Add semantic results
        for (rank, result) in semantic.into_iter().enumerate() {
            let rrf_score = 1.0 / (k + rank as f32 + 1.0);
            scores.insert(result.message_hash.clone(), (rrf_score, result));
        }

        // Add/merge keyword results
        for (rank, result) in keyword.into_iter().enumerate() {
            let rrf_score = 1.0 / (k + rank as f32 + 1.0);
            scores
                .entry(result.message_hash.clone())
                .and_modify(|(score, _)| *score += rrf_score)
                .or_insert((rrf_score, result));
        }

        // Sort by combined score
        let mut merged: Vec<_> = scores
            .into_iter()
            .map(|(_, (score, mut result))| {
                result.similarity = score; // Use RRF score
                result
            })
            .collect();

        merged.sort_by(|a, b| {
            // Use total_cmp for f32 which provides a total ordering (handles NaN)
            b.similarity.total_cmp(&a.similarity)
        });

        merged
    }
}
