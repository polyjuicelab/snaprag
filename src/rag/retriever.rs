//! Retrieval module for semantic and hybrid search

use std::sync::Arc;

use tracing::debug;

use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::errors::Result;
use crate::models::UserProfile;
use crate::rag::MatchType;
use crate::rag::RetrievalMethod;
use crate::rag::SearchResult;

/// Retriever for semantic and hybrid search
pub struct Retriever {
    database: Arc<Database>,
    embedding_service: Arc<EmbeddingService>,
}

impl Retriever {
    /// Create a new retriever
    #[must_use]
    pub const fn new(database: Arc<Database>, embedding_service: Arc<EmbeddingService>) -> Self {
        Self {
            database,
            embedding_service,
        }
    }

    /// Semantic search using vector embeddings
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
    ) -> Result<Vec<SearchResult>> {
        debug!("Performing semantic search: {}", query);

        // Generate query embedding
        let query_embedding = self.embedding_service.generate(query).await?;

        // Search in database
        #[allow(clippy::cast_possible_wrap)] // Limit is guaranteed to be positive and reasonable
        let profiles = self
            .database
            .semantic_search_profiles(query_embedding, limit as i64, threshold)
            .await?;

        // Convert to search results with scores
        let results = profiles
            .into_iter()
            .enumerate()
            .map(|(idx, profile)| SearchResult {
                profile,
                score: 1.0 - (idx as f32 / limit as f32), // Decreasing score based on rank
                match_type: MatchType::Semantic,
            })
            .collect();

        Ok(results)
    }

    /// Keyword search using text matching
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    /// - Invalid search patterns (malformed queries)
    pub async fn keyword_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        debug!("Performing keyword search: {}", query);

        let profiles = self
            .database
            .list_user_profiles(crate::models::UserProfileQuery {
                fid: None,
                username: None,
                display_name: None,
                bio: None,
                location: None,
                twitter_username: None,
                github_username: None,
                #[allow(clippy::cast_possible_wrap)] // Limit is guaranteed to be positive and reasonable
                limit: Some(limit as i64),
                offset: None,
                start_timestamp: None,
                end_timestamp: None,
                sort_by: None,
                sort_order: None,
                search_term: Some(query.to_string()),
            })
            .await?;

        let results = profiles
            .into_iter()
            .enumerate()
            .map(|(idx, profile)| SearchResult {
                profile,
                score: 1.0 - (idx as f32 / limit as f32),
                match_type: MatchType::Keyword,
            })
            .collect();

        Ok(results)
    }

    /// Hybrid search combining semantic and keyword matching
    pub async fn hybrid_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        debug!("Performing hybrid search: {}", query);

        // Generate query embedding
        let query_embedding = self.embedding_service.generate(query).await?;

        // Perform hybrid search
        #[allow(clippy::cast_possible_wrap)] // Limit is guaranteed to be positive and reasonable
        let profiles = self
            .database
            .hybrid_search_profiles(Some(query_embedding), Some(query.to_string()), limit as i64)
            .await?;

        let results = profiles
            .into_iter()
            .enumerate()
            .map(|(idx, profile)| SearchResult {
                profile,
                score: 1.0 - (idx as f32 / limit as f32),
                match_type: MatchType::Hybrid,
            })
            .collect();

        Ok(results)
    }

    /// Search with automatic method selection based on query characteristics
    pub async fn auto_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        // Analyze query to select optimal search method
        let method = Self::analyze_query(query);

        match method {
            RetrievalMethod::Semantic => self.semantic_search(query, limit, None).await,
            RetrievalMethod::Keyword => self.keyword_search(query, limit).await,
            RetrievalMethod::Hybrid => self.hybrid_search(query, limit).await,
            RetrievalMethod::Auto => self.hybrid_search(query, limit).await, // Fallback
        }
    }

    /// Analyze query to determine optimal retrieval method
    fn analyze_query(query: &str) -> RetrievalMethod {
        let query_lower = query.to_lowercase();

        // Check for specific patterns
        let has_quotes = query.contains('"') || query.contains('\'');
        let has_username = query.contains('@');
        let has_exact_match = has_quotes || has_username;

        // Check query characteristics
        let words: Vec<&str> = query.split_whitespace().collect();
        let word_count = words.len();

        // Decision logic
        if has_exact_match {
            // Exact matches work better with keyword search
            RetrievalMethod::Keyword
        } else if word_count <= 2 {
            // Short queries benefit from semantic expansion
            RetrievalMethod::Semantic
        } else if query_lower.contains("like") || query_lower.contains("similar") {
            // Similarity queries are semantic by nature
            RetrievalMethod::Semantic
        } else {
            // Default to hybrid for balanced results
            RetrievalMethod::Hybrid
        }
    }
}

/// Rerank search results using various strategies
pub struct Reranker;

impl Reranker {
    /// Reciprocal Rank Fusion (RRF) for combining multiple result sets
    #[must_use]
    pub fn reciprocal_rank_fusion(
        results_sets: Vec<Vec<SearchResult>>,
        k: f32,
    ) -> Vec<SearchResult> {
        use std::collections::HashMap;

        let mut scores: HashMap<i64, (f32, UserProfile, MatchType)> = HashMap::new();

        for results in results_sets {
            for (rank, result) in results.into_iter().enumerate() {
                let rrf_score = 1.0 / (k + rank as f32 + 1.0);
                let entry = scores.entry(result.profile.fid).or_insert((
                    0.0,
                    result.profile.clone(),
                    result.match_type,
                ));
                entry.0 += rrf_score;
            }
        }

        let mut final_results: Vec<_> = scores
            .into_iter()
            .map(|(_, (score, profile, match_type))| SearchResult {
                profile,
                score,
                match_type,
            })
            .collect();

        // Sort by score descending, using total_cmp for total ordering
        final_results.sort_by(|a, b| {
            // Use total_cmp for f32 which provides a total ordering (handles NaN)
            b.score.total_cmp(&a.score)
        });
        final_results
    }

    /// Simple score-based reranking
    #[must_use]
    pub fn rerank_by_score(mut results: Vec<SearchResult>) -> Vec<SearchResult> {
        // Sort by score descending, using total_cmp for total ordering
        results.sort_by(|a, b| {
            // Use total_cmp for f32 which provides a total ordering (handles NaN)
            b.score.total_cmp(&a.score)
        });
        results
    }
}
