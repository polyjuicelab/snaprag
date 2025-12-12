#![allow(clippy::cast_possible_truncation)] // Vec lengths and batch sizes won't exceed u32::MAX

use super::Database;
use crate::models::Cast;
use crate::models::CastQuery;
use crate::models::CastSearchResult;
use crate::models::CastSortBy;
use crate::models::CastStats;
use crate::models::SortOrder;
use crate::Result;

/// Cast thread structure
#[derive(Debug, Clone)]
pub struct CastThread {
    pub root: Option<Cast>,
    pub parents: Vec<Cast>,
    pub children: Vec<Cast>,
}

impl Database {
    /// List casts with filters
    pub async fn list_casts(&self, query: CastQuery) -> Result<Vec<Cast>> {
        let limit = query.limit.unwrap_or(100);
        let offset = query.offset.unwrap_or(0);

        // Use the dynamically built query with all filters
        // Note: We need to use query_as with dynamic SQL, which requires rebuilding
        // For complex filters, we'll use a pragmatic approach
        let casts = if query.fid.is_some()
            || query.text_search.is_some()
            || query.parent_hash.is_some()
            || query.root_hash.is_some()
            || query.start_timestamp.is_some()
            || query.end_timestamp.is_some()
        {
            // Complex query - use the specific filters we support
            let mut conditions = vec!["1=1".to_string()];
            let mut param_idx = 1;

            if let Some(_fid) = query.fid {
                let condition = format!("fid = ${param_idx}");
                conditions.push(condition);
                param_idx += 1;
            }

            if let Some(_text_search) = &query.text_search {
                let condition = format!("text ILIKE ${param_idx}");
                conditions.push(condition);
                param_idx += 1;
            }

            if let Some(_parent_hash) = &query.parent_hash {
                let condition = format!("parent_hash = ${param_idx}");
                conditions.push(condition);
                param_idx += 1;
            }

            if let Some(_start_timestamp) = query.start_timestamp {
                let condition = format!("timestamp >= ${param_idx}");
                conditions.push(condition);
                param_idx += 1;
            }

            if let Some(_end_timestamp) = query.end_timestamp {
                let condition = format!("timestamp <= ${param_idx}");
                conditions.push(condition);
                // param_idx would be incremented here if we had more conditions
            }

            let where_clause = conditions.join(" AND ");
            let order_by = match query.sort_by {
                Some(CastSortBy::Timestamp) => "timestamp",
                Some(CastSortBy::Fid) => "fid",
                _ => "timestamp",
            };
            let order_dir = match query.sort_order {
                Some(SortOrder::Asc) => "ASC",
                _ => "DESC",
            };

            let sql = format!(
                "SELECT * FROM casts WHERE {where_clause} ORDER BY {order_by} {order_dir} LIMIT {limit} OFFSET {offset}"
            );

            let mut q = sqlx::query_as::<_, Cast>(&sql);

            if let Some(fid) = query.fid {
                q = q.bind(fid);
            }
            if let Some(text_search) = &query.text_search {
                q = q.bind(format!("%{text_search}%"));
            }
            if let Some(parent_hash) = &query.parent_hash {
                q = q.bind(parent_hash);
            }
            if let Some(start_timestamp) = query.start_timestamp {
                q = q.bind(start_timestamp);
            }
            if let Some(end_timestamp) = query.end_timestamp {
                q = q.bind(end_timestamp);
            }

            q.fetch_all(&self.pool).await?
        } else {
            // Simple query - just sort and paginate
            sqlx::query_as::<_, Cast>(
                "SELECT * FROM casts ORDER BY timestamp DESC LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(casts)
    }

    /// Get casts by FID
    pub async fn get_casts_by_fid(
        &self,
        fid: i64,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Cast>> {
        let offset = offset.unwrap_or(0);

        // If no limit specified, fetch all casts (use a very large number)
        // This is more efficient than dynamic SQL construction
        let limit = limit.unwrap_or(1_000_000);

        let casts = sqlx::query_as::<_, Cast>(
            "SELECT * FROM casts WHERE fid = $1 ORDER BY timestamp DESC LIMIT $2 OFFSET $3",
        )
        .bind(fid)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(casts)
    }

    /// Get casts by FID within a time range
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    pub async fn get_casts_by_fid_and_time_range(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Vec<Cast>> {
        let casts = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, Cast>(
                "SELECT * FROM casts WHERE fid = $1 AND timestamp >= $2 AND timestamp <= $3 AND text IS NOT NULL AND text != '' ORDER BY timestamp DESC",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
            .fetch_all(&self.pool)
            .await?
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, Cast>(
                "SELECT * FROM casts WHERE fid = $1 AND timestamp >= $2 AND text IS NOT NULL AND text != '' ORDER BY timestamp DESC",
            )
            .bind(fid)
            .bind(start)
            .fetch_all(&self.pool)
            .await?
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, Cast>(
                "SELECT * FROM casts WHERE fid = $1 AND timestamp <= $2 AND text IS NOT NULL AND text != '' ORDER BY timestamp DESC",
            )
            .bind(fid)
            .bind(end)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Cast>(
                "SELECT * FROM casts WHERE fid = $1 AND text IS NOT NULL AND text != '' ORDER BY timestamp DESC",
            )
            .bind(fid)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(casts)
    }

    /// Count casts without embeddings (optimized for large datasets)
    pub async fn count_casts_without_embeddings(&self) -> Result<i64> {
        // For large datasets, it's much faster to calculate:
        // total_casts - existing_embeddings = missing_embeddings
        // This avoids the expensive NOT IN subquery on 200M+ rows

        let total_casts = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts")
            .fetch_one(&self.pool)
            .await?;

        let existing_embeddings =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cast_embeddings")
                .fetch_one(&self.pool)
                .await?;

        let missing = total_casts - existing_embeddings;

        tracing::debug!(
            "Count calculation: {} total casts - {} existing embeddings = {} missing",
            total_casts,
            existing_embeddings,
            missing
        );

        Ok(missing)
    }

    /// Get casts without embeddings (optimized for large datasets)
    pub async fn get_casts_without_embeddings(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Cast>> {
        let casts = sqlx::query_as::<_, Cast>(
            r"
            SELECT c.* 
            FROM casts c
            WHERE c.message_hash NOT IN (
                SELECT message_hash FROM cast_embeddings
            )
            AND c.text IS NOT NULL 
            AND length(c.text) > 0
            ORDER BY c.timestamp DESC
            LIMIT $1 OFFSET $2
            ",
        )
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .bind(i64::try_from(offset).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;

        Ok(casts)
    }

    /// Check which message hashes from a list don't have embeddings
    /// Returns a `HashSet` of message hashes that need embeddings
    pub async fn get_missing_embeddings(
        &self,
        message_hashes: &[Vec<u8>],
    ) -> Result<std::collections::HashSet<Vec<u8>>> {
        if message_hashes.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        // Get all hashes that already have embeddings
        let existing = sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT message_hash FROM cast_embeddings WHERE message_hash = ANY($1)",
        )
        .bind(message_hashes)
        .fetch_all(&self.pool)
        .await?;

        let existing_set: std::collections::HashSet<Vec<u8>> = existing.into_iter().collect();

        // Return hashes that are NOT in the existing set
        let missing: std::collections::HashSet<Vec<u8>> = message_hashes
            .iter()
            .filter(|hash| !existing_set.contains(*hash))
            .cloned()
            .collect();

        Ok(missing)
    }

    /// Store cast embedding (single vector - backward compatibility)
    pub async fn store_cast_embedding(
        &self,
        message_hash: &[u8],
        fid: i64,
        text: &str,
        embedding: &[f32],
    ) -> Result<()> {
        sqlx::query(
            r"
            INSERT INTO cast_embeddings (message_hash, fid, text, embedding)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (message_hash) 
            DO UPDATE SET 
                embedding = EXCLUDED.embedding,
                updated_at = NOW()
            ",
        )
        .bind(message_hash)
        .bind(fid)
        .bind(text)
        .bind(embedding)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Store chunked cast embeddings
    pub async fn store_cast_embedding_chunks(
        &self,
        message_hash: &[u8],
        fid: i64,
        chunks: &[(usize, String, Vec<f32>, String)], // (chunk_index, chunk_text, embedding, strategy)
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // First, clear existing chunks for this message_hash
        sqlx::query("DELETE FROM cast_embedding_chunks WHERE message_hash = $1")
            .bind(message_hash)
            .execute(&mut *tx)
            .await?;

        // Insert new chunks
        for (chunk_index, chunk_text, embedding, strategy) in chunks {
            sqlx::query(
                r"
                INSERT INTO cast_embedding_chunks 
                (message_hash, fid, chunk_index, chunk_text, chunk_strategy, embedding, chunk_length)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                ",
            )
            .bind(message_hash)
            .bind(fid)
            .bind(i32::try_from(*chunk_index).unwrap_or(i32::MAX))
            .bind(chunk_text)
            .bind(strategy)
            .bind(embedding)
            .bind(i32::try_from(chunk_text.len()).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// Store aggregated cast embedding
    pub async fn store_cast_embedding_aggregated(
        &self,
        message_hash: &[u8],
        fid: i64,
        text: &str,
        embedding: &[f32],
        aggregation_strategy: &str,
        chunk_count: usize,
        total_text_length: usize,
    ) -> Result<()> {
        sqlx::query(
            r"
            INSERT INTO cast_embedding_aggregated 
            (message_hash, fid, text, embedding, aggregation_strategy, chunk_count, total_text_length)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (message_hash) 
            DO UPDATE SET 
                embedding = EXCLUDED.embedding,
                aggregation_strategy = EXCLUDED.aggregation_strategy,
                chunk_count = EXCLUDED.chunk_count,
                total_text_length = EXCLUDED.total_text_length,
                updated_at = NOW()
            ",
        )
        .bind(message_hash)
        .bind(fid)
        .bind(text)
        .bind(embedding)
        .bind(aggregation_strategy)
        .bind(i32::try_from(chunk_count).unwrap_or(i32::MAX))
        .bind(i32::try_from(total_text_length).unwrap_or(i32::MAX))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Semantic search for casts (lightweight version without engagement metrics)
    /// Now searches both single-vector and multi-vector tables for comprehensive results
    ///
    /// # Panics
    /// Never panics - unwrap is only called when HashMap key is known to exist
    pub async fn semantic_search_casts_simple(
        &self,
        query_embedding: Vec<f32>,
        limit: i64,
        threshold: Option<f32>,
    ) -> Result<Vec<CastSearchResult>> {
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
            similarity: f64,
            chunk_index: Option<i32>,
            chunk_text: Option<String>,
            chunk_strategy: Option<String>,
        }

        // Search both single-vector and multi-vector tables
        let raw_results = sqlx::query_as::<_, RawResult>(
            r"
            (
                -- Search single-vector embeddings (original table)
                SELECT 
                    ce.message_hash,
                    ce.fid,
                    ce.text,
                    c.timestamp,
                    c.parent_hash,
                    c.embeds,
                    c.mentions,
                    1 - (ce.embedding <=> $1::vector) as similarity,
                    NULL::integer as chunk_index,
                    NULL::text as chunk_text,
                    'single'::text as chunk_strategy
                FROM cast_embeddings ce
                INNER JOIN casts c ON ce.message_hash = c.message_hash
                WHERE 1 - (ce.embedding <=> $1::vector) > $2
            )
            UNION ALL
            (
                -- Search multi-vector chunks
                SELECT 
                    cec.message_hash,
                    cec.fid,
                    c.text,
                    c.timestamp,
                    c.parent_hash,
                    c.embeds,
                    c.mentions,
                    1 - (cec.embedding <=> $1::vector) as similarity,
                    cec.chunk_index,
                    cec.chunk_text,
                    cec.chunk_strategy
                FROM cast_embedding_chunks cec
                INNER JOIN casts c ON cec.message_hash = c.message_hash
                WHERE 1 - (cec.embedding <=> $1::vector) > $2
            )
            UNION ALL
            (
                -- Search aggregated multi-vector embeddings
                SELECT 
                    cea.message_hash,
                    cea.fid,
                    cea.text,
                    c.timestamp,
                    c.parent_hash,
                    c.embeds,
                    c.mentions,
                    1 - (cea.embedding <=> $1::vector) as similarity,
                    NULL::integer as chunk_index,
                    NULL::text as chunk_text,
                    cea.aggregation_strategy as chunk_strategy
                FROM cast_embedding_aggregated cea
                INNER JOIN casts c ON cea.message_hash = c.message_hash
                WHERE 1 - (cea.embedding <=> $1::vector) > $2
            )
            ORDER BY similarity DESC
            LIMIT $3
            ",
        )
        .bind(&query_embedding)
        .bind(threshold_val)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        // Deduplicate results by message_hash, keeping the highest similarity score
        let mut deduplicated: std::collections::HashMap<Vec<u8>, CastSearchResult> =
            std::collections::HashMap::new();

        for r in raw_results {
            let message_hash = r.message_hash.clone();
            let similarity = r.similarity as f32;

            // Only keep if this is a new message_hash or has higher similarity
            if !deduplicated.contains_key(&message_hash)
                || deduplicated.get(&message_hash).unwrap().similarity < similarity
            {
                deduplicated.insert(
                    message_hash,
                    CastSearchResult {
                        message_hash: r.message_hash,
                        fid: r.fid,
                        text: r.text,
                        timestamp: r.timestamp,
                        parent_hash: r.parent_hash,
                        embeds: r.embeds,
                        mentions: r.mentions,
                        similarity,
                        reply_count: None,
                        reaction_count: None,
                        chunk_index: r.chunk_index,
                        chunk_text: r.chunk_text,
                        chunk_strategy: r.chunk_strategy,
                    },
                );
            }
        }

        // Convert back to Vec and sort by similarity
        let mut results: Vec<CastSearchResult> = deduplicated.into_values().collect();
        results.sort_by(|a, b| {
            // Use total_cmp for f32 which provides a total ordering (handles NaN)
            b.similarity.total_cmp(&a.similarity)
        });
        results.truncate(usize::try_from(limit).unwrap_or(usize::MAX));

        Ok(results)
    }

    /// Semantic search for casts with engagement metrics
    pub async fn semantic_search_casts(
        &self,
        query_embedding: Vec<f32>,
        limit: i64,
        threshold: Option<f32>,
    ) -> Result<Vec<CastSearchResult>> {
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
            similarity: f64, // PostgreSQL returns FLOAT8 (f64) from distance operator
            reply_count: Option<i64>,
            reaction_count: Option<i64>,
        }

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
                1 - (ce.embedding <=> $1::vector) as similarity,
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
            WHERE 1 - (ce.embedding <=> $1::vector) > $2
            ORDER BY ce.embedding <=> $1::vector
            LIMIT $3
            ",
        )
        .bind(&query_embedding)
        .bind(threshold_val)
        .bind(limit)
        .fetch_all(&self.pool)
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
                similarity: r.similarity as f32, // Convert f64 to f32
                reply_count: Some(r.reply_count.unwrap_or(0)),
                reaction_count: Some(r.reaction_count.unwrap_or(0)),
                chunk_index: None,
                chunk_text: None,
                chunk_strategy: None,
            })
            .collect();

        Ok(results)
    }

    /// Get cast statistics (replies, reactions, etc.)
    pub async fn get_cast_stats(&self, message_hash: &[u8]) -> Result<CastStats> {
        let stats = sqlx::query_as::<_, CastStats>(
            r"
            SELECT 
                $1 as message_hash,
                (SELECT COUNT(*) FROM casts WHERE parent_hash = $1) as reply_count,
                (SELECT COUNT(*) FROM (
                    SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                    ) as rn
                    FROM reactions
                    WHERE target_cast_hash = $1
                ) r WHERE r.rn = 1 AND r.event_type = 'add') as reaction_count,
                (SELECT COUNT(DISTINCT fid) FROM (
                    SELECT fid, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                    ) as rn
                    FROM reactions
                    WHERE target_cast_hash = $1
                ) r WHERE r.rn = 1 AND r.event_type = 'add') as unique_reactors
            ",
        )
        .bind(message_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(stats)
    }

    /// Get cast by message hash
    pub async fn get_cast_by_hash(&self, message_hash: Vec<u8>) -> Result<Option<Cast>> {
        let cast = sqlx::query_as::<_, Cast>("SELECT * FROM casts WHERE message_hash = $1")
            .bind(message_hash)
            .fetch_optional(&self.pool)
            .await?;

        Ok(cast)
    }

    /// Multi-vector semantic search for casts (searches across all chunks)
    pub async fn semantic_search_casts_multi_vector(
        &self,
        query_embedding: Vec<f32>,
        limit: i64,
        threshold: Option<f32>,
        search_strategy: Option<&str>, // "chunks", "aggregated", "both"
    ) -> Result<Vec<CastSearchResult>> {
        let threshold_val = threshold.unwrap_or(0.0);
        let strategy = search_strategy.unwrap_or("both");

        #[derive(sqlx::FromRow)]
        struct RawResult {
            message_hash: Vec<u8>,
            fid: i64,
            text: String,
            timestamp: i64,
            parent_hash: Option<Vec<u8>>,
            embeds: Option<serde_json::Value>,
            mentions: Option<serde_json::Value>,
            similarity: f64,
            chunk_index: Option<i32>,
            chunk_text: Option<String>,
            chunk_strategy: Option<String>,
        }

        let query = match strategy {
            "chunks" => {
                r"
                SELECT 
                    cec.message_hash,
                    cec.fid,
                    c.text,
                    c.timestamp,
                    c.parent_hash,
                    c.embeds,
                    c.mentions,
                    1 - (cec.embedding <=> $1::vector) as similarity,
                    cec.chunk_index,
                    cec.chunk_text,
                    cec.chunk_strategy
                FROM cast_embedding_chunks cec
                INNER JOIN casts c ON cec.message_hash = c.message_hash
                WHERE 1 - (cec.embedding <=> $1::vector) > $2
                ORDER BY cec.embedding <=> $1::vector
                LIMIT $3
            "
            }
            "aggregated" => {
                r"
                SELECT 
                    cea.message_hash,
                    cea.fid,
                    cea.text,
                    c.timestamp,
                    c.parent_hash,
                    c.embeds,
                    c.mentions,
                    1 - (cea.embedding <=> $1::vector) as similarity,
                    NULL::integer as chunk_index,
                    NULL::text as chunk_text,
                    cea.aggregation_strategy as chunk_strategy
                FROM cast_embedding_aggregated cea
                INNER JOIN casts c ON cea.message_hash = c.message_hash
                WHERE 1 - (cea.embedding <=> $1::vector) > $2
                ORDER BY cea.embedding <=> $1::vector
                LIMIT $3
            "
            }
            "both" => {
                r"
                (
                    SELECT 
                        cec.message_hash,
                        cec.fid,
                        c.text,
                        c.timestamp,
                        c.parent_hash,
                        c.embeds,
                        c.mentions,
                        1 - (cec.embedding <=> $1::vector) as similarity,
                        cec.chunk_index,
                        cec.chunk_text,
                        cec.chunk_strategy
                    FROM cast_embedding_chunks cec
                    INNER JOIN casts c ON cec.message_hash = c.message_hash
                    WHERE 1 - (cec.embedding <=> $1::vector) > $2
                )
                UNION ALL
                (
                    SELECT 
                        cea.message_hash,
                        cea.fid,
                        cea.text,
                        c.timestamp,
                        c.parent_hash,
                        c.embeds,
                        c.mentions,
                        1 - (cea.embedding <=> $1::vector) as similarity,
                        NULL::integer as chunk_index,
                        NULL::text as chunk_text,
                        cea.aggregation_strategy as chunk_strategy
                    FROM cast_embedding_aggregated cea
                    INNER JOIN casts c ON cea.message_hash = c.message_hash
                    WHERE 1 - (cea.embedding <=> $1::vector) > $2
                )
                ORDER BY similarity DESC
                LIMIT $3
            "
            }
            _ => {
                return Err(crate::SnapRagError::Custom(
                    "Invalid search strategy".to_string(),
                ))
            }
        };

        let raw_results = sqlx::query_as::<_, RawResult>(query)
            .bind(&query_embedding)
            .bind(threshold_val)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let results = raw_results
            .into_iter()
            .map(|raw| CastSearchResult {
                message_hash: raw.message_hash,
                fid: raw.fid,
                text: raw.text,
                timestamp: raw.timestamp,
                parent_hash: raw.parent_hash,
                embeds: raw.embeds,
                mentions: raw.mentions,
                similarity: raw.similarity as f32,
                reply_count: None,
                reaction_count: None,
                chunk_index: raw.chunk_index,
                chunk_text: raw.chunk_text,
                chunk_strategy: raw.chunk_strategy,
            })
            .collect();

        Ok(results)
    }

    /// Get cast replies (children)
    pub async fn get_cast_replies(
        &self,
        parent_hash: Vec<u8>,
        limit: Option<i64>,
    ) -> Result<Vec<Cast>> {
        let casts = sqlx::query_as::<_, Cast>(
            "SELECT * FROM casts WHERE parent_hash = $1 ORDER BY timestamp ASC LIMIT $2",
        )
        .bind(parent_hash)
        .bind(limit.unwrap_or(100))
        .fetch_all(&self.pool)
        .await?;

        Ok(casts)
    }

    /// Get cast thread (recursive parents and children)
    ///
    /// # Panics
    /// Never panics - unwrap is only called after checking Option is Some
    pub async fn get_cast_thread(
        &self,
        message_hash: Vec<u8>,
        max_depth: usize,
    ) -> Result<CastThread> {
        let mut thread = CastThread {
            root: None,
            parents: Vec::new(),
            children: Vec::new(),
        };

        // Get the target cast
        let cast = self.get_cast_by_hash(message_hash.clone()).await?;
        if cast.is_none() {
            return Ok(thread);
        }

        let current_cast = cast.unwrap();
        thread.root = Some(current_cast.clone());

        // Traverse up to find parents
        let mut current_parent = current_cast.parent_hash.clone();
        let mut depth = 0;
        while let Some(parent_hash) = current_parent {
            if depth >= max_depth {
                break;
            }

            if let Some(parent) = self.get_cast_by_hash(parent_hash.clone()).await? {
                thread.parents.push(parent.clone());
                current_parent = parent.parent_hash.clone();
                depth += 1;
            } else {
                break;
            }
        }

        // Reverse parents so root is first
        thread.parents.reverse();

        // Get direct replies
        let replies = self.get_cast_replies(message_hash, Some(50)).await?;
        thread.children = replies;

        Ok(thread)
    }
}
