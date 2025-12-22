/// Annual report aggregator API handlers
///
/// This module provides a comprehensive annual report endpoint that combines
/// data from all other endpoints into a single response.
use std::collections::HashMap;

use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::DateTime;
use chrono::Utc;
use tracing::debug;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::text_analysis::analyze_text;
use crate::text_analysis::count_word_frequencies;
use crate::text_analysis::extract_nouns;
use crate::text_analysis::extract_verbs;
use crate::utils::emoji::count_emoji_frequencies;

/// Get annual report for a user
///
/// Combines data from engagement, temporal activity, content style, and follower growth
/// endpoints into a comprehensive annual report.
///
/// # Errors
///
/// Returns `StatusCode::BAD_REQUEST` if the year format is invalid.
/// Returns `StatusCode::NOT_FOUND` if the user profile is not found.
/// Returns `StatusCode::INTERNAL_SERVER_ERROR` if database queries fail.
#[allow(clippy::too_many_lines)]
pub async fn get_annual_report(
    State(state): State<AppState>,
    Path((fid, year)): Path<(i64, u32)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!("GET /api/users/{}/annual-report/{}", fid, year);

    // Calculate time range for the year
    let year_start = DateTime::parse_from_rfc3339(&format!("{year}-01-01T00:00:00Z"))
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .timestamp();
    let year_end = DateTime::parse_from_rfc3339(&format!("{year}-12-31T23:59:59Z"))
        .map_err(|_| StatusCode::BAD_REQUEST)?
        .timestamp();
    debug!(
        "Calculating time range for year {}: start={}, end={}",
        year, year_start, year_end
    );

    // Convert to Farcaster timestamps
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let start_farcaster = crate::unix_to_farcaster_timestamp(year_start as u64) as i64;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let end_farcaster = crate::unix_to_farcaster_timestamp(year_end as u64) as i64;

    // Get user profile
    let profile = match state.database.get_user_profile(fid).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Error fetching profile for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let registered_at = state
        .database
        .get_registration_timestamp(fid)
        .await
        .unwrap_or(None);

    // Get engagement metrics
    let reactions_received = state
        .database
        .get_reactions_received(fid, Some(start_farcaster), Some(end_farcaster), 1)
        .await
        .unwrap_or(0);
    let recasts_received = state
        .database
        .get_recasts_received(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    let replies_received = state
        .database
        .get_replies_received(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    let most_popular_cast = state
        .database
        .get_most_popular_cast(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .ok()
        .flatten();
    let top_reactors = state
        .database
        .get_top_interactive_users(fid, Some(start_farcaster), Some(end_farcaster), 3)
        .await
        .unwrap_or_default();

    // Get temporal activity
    let hourly_dist = state
        .database
        .get_hourly_distribution(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();
    let monthly_dist = state
        .database
        .get_monthly_distribution(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();
    let first_cast = state
        .database
        .get_first_cast_in_range(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .ok()
        .flatten();
    let last_cast = state
        .database
        .get_last_cast_in_range(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .ok()
        .flatten();

    // Get content style
    let casts_text = state
        .database
        .get_casts_text_for_analysis(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();
    let frames_used = state
        .database
        .get_frame_usage_count(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    debug!(
        "Frame usage for FID {} in year {}: {} unique frames (timestamp range: {} to {})",
        fid, year, frames_used, start_farcaster, end_farcaster
    );

    // Get follower growth using database (reverted from API to avoid pagination issues)
    let current_followers = state
        .database
        .get_current_follower_count(fid)
        .await
        .unwrap_or(0);
    let current_following = state
        .database
        .get_current_following_count(fid)
        .await
        .unwrap_or(0);
    let followers_at_start = state
        .database
        .get_follower_count_at_timestamp(fid, start_farcaster)
        .await
        .unwrap_or(0);
    let following_at_start = state
        .database
        .get_following_count_at_timestamp(fid, start_farcaster)
        .await
        .unwrap_or(0);
    let monthly_snapshots = state
        .database
        .get_monthly_follower_snapshots(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();

    // Calculate days since registration
    let days_since_registration = registered_at.map_or(0, |reg_ts| {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
        let reg_unix = crate::farcaster_to_unix_timestamp(reg_ts as u64) as i64;
        let reg_dt = DateTime::<Utc>::from_timestamp(reg_unix, 0).unwrap_or_else(Utc::now);
        let now = Utc::now();
        (now - reg_dt).num_days()
    });

    // Calculate total casts in year
    #[allow(clippy::cast_possible_wrap)]
    let total_casts_in_year = casts_text.len() as i64;
    debug!(
        "Casts in year {} for FID {}: {}",
        year, fid, total_casts_in_year
    );

    // Get total casts count (all time, not just this year)
    let total_casts_all_time = state.database.count_casts_by_fid(fid).await.unwrap_or(0);
    debug!(
        "Total casts (all time) for FID {}: {}",
        fid, total_casts_all_time
    );

    // Calculate emoji frequencies
    let mut emoji_freq: HashMap<String, usize> = HashMap::new();
    for text in &casts_text {
        let freqs = count_emoji_frequencies(text);
        for (emoji, count) in freqs {
            *emoji_freq.entry(emoji).or_insert(0) += count;
        }
    }
    #[allow(clippy::cast_possible_wrap)]
    let mut top_emojis: Vec<_> = emoji_freq.into_iter().map(|(e, c)| (e, c as i64)).collect();
    top_emojis.sort_by(|a, b| b.1.cmp(&a.1));
    top_emojis.truncate(10); // Show top 10 emojis
    debug!("Top emojis count: {}", top_emojis.len());

    // Calculate word frequencies
    let mut word_freq_by_lang: HashMap<String, HashMap<String, usize>> = HashMap::new();

    for text in &casts_text {
        if text.trim().is_empty() {
            continue;
        }

        let (tagged_words, lang_info) = analyze_text(text);
        let lang_code = &lang_info.lang_code;

        // Extract nouns and verbs
        let nouns = extract_nouns(&tagged_words);
        let verbs = extract_verbs(&tagged_words);

        // Combine nouns and verbs for word frequency analysis
        let mut words = nouns;
        words.extend(verbs);

        // Count word frequencies by language
        let word_freq = count_word_frequencies(&words, lang_code);
        let lang_word_freq = word_freq_by_lang.entry(lang_code.clone()).or_default();
        for (word, count) in word_freq {
            *lang_word_freq.entry(word).or_insert(0) += count;
        }
    }

    // Aggregate top words across all languages
    let mut all_word_freq: HashMap<String, usize> = HashMap::new();
    for freq_map in word_freq_by_lang.values() {
        for (word, count) in freq_map {
            *all_word_freq.entry(word.clone()).or_insert(0) += count;
        }
    }

    // Get top words (excluding stop words)
    #[allow(clippy::cast_possible_wrap)]
    let mut top_words: Vec<_> = all_word_freq
        .into_iter()
        .map(|(word, count)| {
            serde_json::json!({
                "word": word,
                "count": count as i64,
            })
        })
        .collect();
    top_words.sort_by(|a, b| {
        let count_a = a
            .get("count")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let count_b = b
            .get("count")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        count_b.cmp(&count_a)
    });
    top_words.truncate(20); // Show top 20 words
    debug!("Top words count: {}", top_words.len());

    // Find most active hour and month
    let most_active_hour = hourly_dist.iter().max_by_key(|h| h.count).map(|h| h.hour);
    let most_active_month = monthly_dist
        .iter()
        .max_by_key(|m| m.count)
        .map(|m| m.month.clone());

    // Build comprehensive response
    let response = serde_json::json!({
        "year": year,
        "user": {
            "fid": fid,
            "username": profile.username,
            "display_name": profile.display_name,
            "pfp_url": profile.pfp_url,
            "registered_at": registered_at,
            "days_since_registration": days_since_registration,
        },
        "activity": {
            "total_casts": total_casts_all_time,
            "total_casts_in_year": total_casts_in_year,
            "first_cast": first_cast.map(|c| serde_json::json!({
                "message_hash": hex::encode(c.message_hash),
                "text": c.text,
                "timestamp": c.timestamp,
            })),
            "last_cast": last_cast.map(|c| serde_json::json!({
                "message_hash": hex::encode(c.message_hash),
                "text": c.text,
                "timestamp": c.timestamp,
            })),
            "hourly_distribution": hourly_dist,
            "monthly_distribution": monthly_dist,
            "most_active_hour": most_active_hour,
            "most_active_month": most_active_month,
        },
        "engagement": {
            "reactions_received": reactions_received,
            "recasts_received": recasts_received,
            "replies_received": replies_received,
            "total_engagement": reactions_received + recasts_received + replies_received,
            "most_popular_cast": most_popular_cast.map(|c| serde_json::json!({
                "message_hash": hex::encode(c.message_hash),
                "text": c.text,
                "reactions": c.reactions,
                "recasts": c.recasts,
                "replies": c.replies,
                "timestamp": c.timestamp,
            })),
            "top_reactors": top_reactors.into_iter().map(|u| serde_json::json!({
                "fid": u.fid,
                "username": u.username,
                "display_name": u.display_name,
                "interaction_count": u.interaction_count,
            })).collect::<Vec<_>>(),
        },
        "content_style": {
            "top_emojis": top_emojis.into_iter().map(|(e, c)| serde_json::json!({
                "emoji": e,
                "count": c,
            })).collect::<Vec<_>>(),
            "top_words": top_words,
            "frames_used": frames_used,
        },
        "social_growth": {
            "current_followers": current_followers,
            "current_following": current_following,
            "followers_at_start": followers_at_start,
            "following_at_start": following_at_start,
            "net_growth": current_followers.saturating_sub(followers_at_start),
            "following_net_growth": current_following.saturating_sub(following_at_start),
            "monthly_snapshots": monthly_snapshots.into_iter().map(|s| serde_json::json!({
                "month": s.month,
                "followers": s.followers,
                "following": s.following,
            })).collect::<Vec<_>>(),
        },
    });

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/users/{}/annual-report/{} - {}ms - 200",
        fid,
        year,
        duration.as_millis()
    );

    Ok(Json(ApiResponse::success(response)))
}
