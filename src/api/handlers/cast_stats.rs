use std::collections::HashMap;

/// Cast statistics API handlers
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::DateTime;
use chrono::NaiveDate;
use chrono::Utc;
use tracing::error;
use tracing::info;

use super::job_helpers::*;
use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::CastStatsRequest;
use crate::api::types::CastStatsResponse;
use crate::api::types::DateDistributionEntry;
use crate::api::types::DateRange;
use crate::api::types::WordFrequency;
use crate::text_analysis::analyze_text;
use crate::text_analysis::count_word_frequencies;
use crate::text_analysis::extract_nouns;
use crate::text_analysis::extract_verbs;

/// Get cast statistics for a user
pub async fn get_cast_stats(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
    Query(params): Query<CastStatsRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();

    // Normalize timestamps to day boundaries for caching
    // Start timestamp: round down to 00:00:00 of the day
    // End timestamp: round up to 23:59:59 of the day
    // This ensures same-day queries share the same cache
    let (normalized_start, normalized_end, date_range_key) =
        match (params.start_timestamp, params.end_timestamp) {
            (Some(start), Some(end)) => {
                // Both timestamps provided - normalize both to day boundaries
                let start_dt = DateTime::<Utc>::from_timestamp(start, 0)
                    .unwrap_or_else(Utc::now)
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                let end_dt = DateTime::<Utc>::from_timestamp(end, 0)
                    .unwrap_or_else(Utc::now)
                    .date_naive()
                    .and_hms_opt(23, 59, 59)
                    .unwrap();

                let normalized_start = start_dt.and_utc().timestamp();
                let normalized_end = end_dt.and_utc().timestamp();

                // Create cache key based on date range (YYYY-MM-DD format)
                let start_date = start_dt.format("%Y-%m-%d").to_string();
                let end_date = end_dt.format("%Y-%m-%d").to_string();
                let date_range_key = Some(format!("{}:{}", start_date, end_date));

                (Some(normalized_start), Some(normalized_end), date_range_key)
            }
            (Some(start), None) => {
                // Only start timestamp - normalize to 00:00:00 of that day
                let start_dt = DateTime::<Utc>::from_timestamp(start, 0)
                    .unwrap_or_else(Utc::now)
                    .date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();

                let normalized_start = start_dt.and_utc().timestamp();
                let start_date = start_dt.format("%Y-%m-%d").to_string();
                let date_range_key = Some(format!("{}:", start_date));

                (Some(normalized_start), None, date_range_key)
            }
            (None, Some(end)) => {
                // Only end timestamp - normalize to 23:59:59 of that day
                let end_dt = DateTime::<Utc>::from_timestamp(end, 0)
                    .unwrap_or_else(Utc::now)
                    .date_naive()
                    .and_hms_opt(23, 59, 59)
                    .unwrap();

                let normalized_end = end_dt.and_utc().timestamp();
                let end_date = end_dt.format("%Y-%m-%d").to_string();
                let date_range_key = Some(format!(":{}", end_date));

                (None, Some(normalized_end), date_range_key)
            }
            (None, None) => {
                // No timestamps - use all data, no date range key
                (None, None, None)
            }
        };

    info!(
        "GET /api/casts/stats/{} (original: start={:?}, end={:?}, normalized: start={:?}, end={:?})",
        fid, params.start_timestamp, params.end_timestamp, normalized_start, normalized_end
    );

    // Use cache if we have normalized timestamps (same day range queries share cache)
    let use_cache = true; // Now we can cache time-range queries too

    if use_cache {
        // Create cache key with date range if specified
        let cache_key_suffix = if let Some(ref date_range) = date_range_key {
            format!("{}:{}", fid, date_range)
        } else {
            fid.to_string()
        };
        let job_key = format!("cast_stats:{}", cache_key_suffix);

        // Check cache first (with date range support)
        match state
            .cache_service
            .get_cast_stats_with_range(fid, date_range_key.as_deref())
            .await
        {
            Ok(crate::api::cache::CacheResult::Fresh(cached_stats)) => {
                let duration = start_time.elapsed();
                info!(
                    "📦 Cast stats cache hit (fresh) for FID {} - {}ms",
                    fid,
                    duration.as_millis()
                );
                let stats_data = serde_json::to_value(&cached_stats).unwrap_or_else(|_| {
                    serde_json::json!({
                        "error": "Failed to serialize cached cast stats"
                    })
                });
                return Ok(Json(ApiResponse::success(stats_data)));
            }
            Ok(crate::api::cache::CacheResult::Stale(cached_stats)) => {
                // Stale cache - return stale data and trigger background update
                info!(
                    "📦 Cast stats cache hit (stale) for FID {}, triggering background update",
                    fid
                );

                // Trigger background update if Redis is available
                if let Some(redis_cfg) = &state.config.redis {
                    if let Ok(redis_client) =
                        crate::api::redis_client::RedisClient::connect(redis_cfg)
                    {
                        let job_data =
                            serde_json::json!({"fid": fid, "type": "cast_stats"}).to_string();
                        if let Ok(Some(_)) = redis_client
                            .push_job("cast_stats", &job_key, &job_data)
                            .await
                        {
                            info!("🔄 Triggered background update for FID {}", fid);
                        }
                    }
                }

                let duration = start_time.elapsed();
                let stats_data = serde_json::to_value(&cached_stats).unwrap_or_else(|_| {
                    serde_json::json!({
                        "error": "Failed to serialize cached cast stats"
                    })
                });
                return Ok(Json(ApiResponse::success(stats_data)));
            }
            Ok(crate::api::cache::CacheResult::Updating(cached_stats)) => {
                // Cache expired - return updating status with old data
                info!(
                    "🔄 Cast stats cache expired (updating) for FID {}, returning old data with updating status",
                    fid
                );

                // Trigger background update if Redis is available
                if let Some(redis_cfg) = &state.config.redis {
                    if let Ok(redis_client) =
                        crate::api::redis_client::RedisClient::connect(redis_cfg)
                    {
                        let job_data =
                            serde_json::json!({"fid": fid, "type": "cast_stats"}).to_string();
                        if let Ok(Some(_)) = redis_client
                            .push_job("cast_stats", &job_key, &job_data)
                            .await
                        {
                            info!("🔄 Triggered background update for FID {}", fid);
                        }
                    }
                }

                let duration = start_time.elapsed();
                let stats_data = serde_json::to_value(&cached_stats).unwrap_or_else(|_| {
                    serde_json::json!({
                        "error": "Failed to serialize cached cast stats"
                    })
                });
                // Return with updating status
                return Ok(Json(ApiResponse::success(serde_json::json!({
                    "status": "updating",
                    "data": stats_data,
                    "message": "Cast statistics are being updated in the background. Please refresh to get the latest data."
                }))));
            }
            Ok(crate::api::cache::CacheResult::Miss) => {
                tracing::debug!(
                    "No cache hit for cast stats FID {}, checking job status",
                    fid
                );
            }
            Err(e) => {
                error!("Cache error for FID {}: {}", fid, e);
                // Continue to process
            }
        }

        // Cache miss - for date range queries, process synchronously
        // For full stats (no date range), use background job
        if date_range_key.is_some() {
            // Date range query - process synchronously for immediate response
            info!(
                "Cache miss for date range query, processing synchronously for FID {}",
                fid
            );
            // Fall through to synchronous processing below
        } else {
            // No date range - check if job is already processing or create new one
            let job_config = JobConfig {
                job_type: "cast_stats",
                job_key: job_key.clone(),
                fid,
            };
            match check_or_create_job(&state, &job_config).await {
                JobResult::AlreadyExists(status) => {
                    let message = match status.as_str() {
                        "pending" => "Statistics are queued, please check back later",
                        "processing" => "Statistics in progress, please check back later",
                        "completed" => {
                            // Job completed but cache not updated yet - try to get result from status
                            if let Some(redis_cfg) = &state.config.redis {
                                if let Ok(redis_client) =
                                    crate::api::redis_client::RedisClient::connect(redis_cfg)
                                {
                                    if let Ok(Some((_, Some(result_json)))) =
                                        redis_client.get_job_status(&job_key).await
                                    {
                                        // Try to parse the result and return it
                                        if let Ok(stats_data) =
                                            serde_json::from_str::<serde_json::Value>(&result_json)
                                        {
                                            return Ok(Json(ApiResponse::success(stats_data)));
                                        }
                                    }
                                }
                            }
                            "Statistics completed, refreshing cache..."
                        }
                        "failed" => "Statistics failed, please try again",
                        _ => "Statistics in progress, please check back later",
                    };
                    return Ok(create_job_status_response(&job_key, &status, message));
                }
                JobResult::Created => {
                    return Ok(create_job_status_response(
                        &job_key,
                        "pending",
                        "Statistics started, please check back later",
                    ));
                }
                JobResult::Failed => {
                    // Fall through to synchronous processing
                    error!("Redis not available, falling back to synchronous processing");
                }
            }
        }
    }

    // Fallback: process synchronously if Redis is not available or time range is specified
    // Convert normalized Unix timestamps to Farcaster timestamps for database query
    // API receives Unix timestamps, but database stores Farcaster timestamps
    let start_farcaster = normalized_start.map(|unix_ts| {
        // Timestamps are always positive
        #[allow(clippy::cast_sign_loss)]
        let farcaster_ts = crate::unix_to_farcaster_timestamp(unix_ts as u64);
        farcaster_ts as i64
    });
    let end_farcaster = normalized_end.map(|unix_ts| {
        // Timestamps are always positive
        #[allow(clippy::cast_sign_loss)]
        let farcaster_ts = crate::unix_to_farcaster_timestamp(unix_ts as u64);
        farcaster_ts as i64
    });

    // Query casts from database
    let casts = match state
        .database
        .get_casts_by_fid_and_time_range(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            error!("Error fetching casts for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if casts.is_empty() {
        let duration = start_time.elapsed();
        info!(
            "❌ GET /api/casts/stats/{} - {}ms - 404 (no casts)",
            fid,
            duration.as_millis()
        );
        return Err(StatusCode::NOT_FOUND);
    }

    // Compute statistics
    let response = compute_cast_statistics(&casts);

    // Cache the response (with date range key if specified)
    if use_cache {
        tracing::debug!(
            "Caching cast stats response for FID {} (range: {:?})",
            fid,
            date_range_key
        );
        if let Err(e) = state
            .cache_service
            .set_cast_stats_with_range(fid, date_range_key.as_deref(), &response)
            .await
        {
            error!("Failed to cache cast stats: {}", e);
        }
    }

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/casts/stats/{} - {}ms - 200 ({} casts)",
        fid,
        duration.as_millis(),
        casts.len()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize cast stats response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}

/// Compute cast statistics from a list of casts
/// This function is used both by the API handler and the worker
pub fn compute_cast_statistics(casts: &[crate::models::Cast]) -> CastStatsResponse {
    // Calculate date distribution (GitHub-style daily counts)
    let date_distribution = calculate_date_distribution(casts);

    // Analyze text content for nouns and verbs
    let mut language_counts: HashMap<String, i64> = HashMap::new();

    // Count word frequencies by language
    let mut noun_freq_by_lang: HashMap<String, HashMap<String, usize>> = HashMap::new();
    let mut verb_freq_by_lang: HashMap<String, HashMap<String, usize>> = HashMap::new();

    // Re-analyze to get language-specific frequencies
    for cast in casts {
        if let Some(text) = &cast.text {
            if text.trim().is_empty() {
                continue;
            }

            let (tagged_words, lang_info) = analyze_text(text);
            let lang_code = &lang_info.lang_code;

            // Count language usage
            *language_counts.entry(lang_code.clone()).or_insert(0) += 1;

            let nouns: Vec<String> = extract_nouns(&tagged_words);
            let verbs: Vec<String> = extract_verbs(&tagged_words);

            let noun_freq = noun_freq_by_lang.entry(lang_code.clone()).or_default();
            let verb_freq = verb_freq_by_lang.entry(lang_code.clone()).or_default();

            let noun_counts = count_word_frequencies(&nouns, lang_code);
            let verb_counts = count_word_frequencies(&verbs, lang_code);

            for (word, count) in noun_counts {
                *noun_freq.entry(word).or_insert(0) += count;
            }

            for (word, count) in verb_counts {
                *verb_freq.entry(word).or_insert(0) += count;
            }
        }
    }

    // Aggregate top nouns and verbs across all languages
    let mut top_nouns: Vec<WordFrequency> = Vec::new();
    let mut top_verbs: Vec<WordFrequency> = Vec::new();

    for (lang_code, freq_map) in noun_freq_by_lang {
        for (word, count) in freq_map {
            top_nouns.push(WordFrequency {
                word,
                count: count as i64,
                language: lang_code.clone(),
            });
        }
    }

    for (lang_code, freq_map) in verb_freq_by_lang {
        for (word, count) in freq_map {
            top_verbs.push(WordFrequency {
                word,
                count: count as i64,
                language: lang_code.clone(),
            });
        }
    }

    // Sort by count (descending) and take top 50
    top_nouns.sort_by(|a, b| b.count.cmp(&a.count));
    top_nouns.truncate(50);

    top_verbs.sort_by(|a, b| b.count.cmp(&a.count));
    top_verbs.truncate(50);

    // Determine date range
    let timestamps: Vec<i64> = casts.iter().map(|c| c.timestamp).collect();
    let min_ts = timestamps.iter().min().copied().unwrap_or(0);
    let max_ts = timestamps.iter().max().copied().unwrap_or(0);

    // Convert Farcaster timestamps to Unix timestamps
    // Farcaster timestamps are seconds since Farcaster epoch (Jan 1, 2021)
    #[allow(clippy::cast_sign_loss)] // Timestamps are always positive
    let start_unix = crate::farcaster_to_unix_timestamp(min_ts as u64) as i64;
    #[allow(clippy::cast_sign_loss)] // Timestamps are always positive
    let end_unix = crate::farcaster_to_unix_timestamp(max_ts as u64) as i64;

    let start_dt = DateTime::<Utc>::from_timestamp(start_unix, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    let end_dt = DateTime::<Utc>::from_timestamp(end_unix, 0)
        .unwrap_or_else(Utc::now)
        .to_rfc3339();

    CastStatsResponse {
        total_casts: casts.len() as i64,
        date_range: DateRange {
            start: start_dt,
            end: end_dt,
        },
        date_distribution,
        language_distribution: language_counts,
        top_nouns,
        top_verbs,
    }
}

/// Calculate daily date distribution (GitHub-style)
fn calculate_date_distribution(casts: &[crate::models::Cast]) -> Vec<DateDistributionEntry> {
    use std::collections::HashMap;

    // Group casts by date (YYYY-MM-DD)
    let mut date_counts: HashMap<String, i64> = HashMap::new();

    for cast in casts {
        // Convert Farcaster timestamp to Unix timestamp
        #[allow(clippy::cast_sign_loss)] // Timestamps are always positive
        let unix_ts = crate::farcaster_to_unix_timestamp(cast.timestamp as u64) as i64;

        // Convert to date
        if let Some(dt) = DateTime::<Utc>::from_timestamp(unix_ts, 0) {
            let date_str = dt.format("%Y-%m-%d").to_string();
            *date_counts.entry(date_str).or_insert(0) += 1;
        }
    }

    // Convert to sorted vector
    let mut entries: Vec<DateDistributionEntry> = date_counts
        .into_iter()
        .map(|(date, count)| DateDistributionEntry { date, count })
        .collect();

    // Sort by date
    entries.sort_by(|a, b| a.date.cmp(&b.date));

    entries
}
