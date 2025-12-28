//! User metrics command handlers (engagement, temporal activity, content style, etc.)

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use chrono::DateTime;
use chrono::Utc;
use tracing::debug;
use tracing::info;

use crate::api::cache::CacheConfig;
use crate::api::cache::CacheResult;
use crate::api::cache::CacheService;
use crate::api::redis_client::RedisClient;
use crate::api::types::CastStatsResponse;
use crate::cli::commands::UserCommands;
use crate::cli::output::print_error;
use crate::cli::output::print_info;
use crate::database::Database;
use crate::AppConfig;
use crate::Result;

/// Handle user metrics commands
pub async fn handle_user_command(config: &AppConfig, command: &UserCommands) -> Result<()> {
    match command {
        UserCommands::CastStats {
            user,
            start,
            end,
            output,
            force,
        } => {
            handle_cast_stats(config, user.clone(), *start, *end, output.clone(), *force).await?;
        }
        UserCommands::Engagement {
            user,
            start,
            end,
            output,
        } => {
            handle_engagement(config, user.clone(), *start, *end, output.clone()).await?;
        }
        UserCommands::TemporalActivity {
            user,
            start,
            end,
            output,
        } => {
            handle_temporal_activity(config, user.clone(), *start, *end, output.clone()).await?;
        }
        UserCommands::ContentStyle {
            user,
            start,
            end,
            output,
        } => {
            handle_content_style(config, user.clone(), *start, *end, output.clone()).await?;
        }
        UserCommands::FollowerGrowth {
            user,
            start,
            end,
            output,
        } => {
            handle_follower_growth(config, user.clone(), *start, *end, output.clone()).await?;
        }
        UserCommands::Domains { user, output } => {
            handle_domains(config, user.clone(), output.clone()).await?;
        }
        UserCommands::NetworkStats { start, end, output } => {
            handle_network_stats(config, *start, *end, output.clone()).await?;
        }
    }
    Ok(())
}

/// Create cache service from config (optional, returns None if Redis not configured)
fn create_cache_service(config: &AppConfig) -> Result<CacheService> {
    let redis_cfg = config
        .redis
        .as_ref()
        .ok_or_else(|| crate::SnapRagError::Custom("Redis not configured".to_string()))?;

    let redis = Arc::new(RedisClient::connect(redis_cfg)?);
    let cache_config = CacheConfig {
        profile_ttl: Duration::from_secs(config.cache.profile_ttl_secs),
        social_ttl: Duration::from_secs(config.cache.social_ttl_secs),
        mbti_ttl: Duration::from_secs(7200), // 2 hours for MBTI
        cast_stats_ttl: Duration::from_secs(config.cache.cast_stats_ttl_secs),
        annual_report_ttl: Duration::from_secs(0), // Never expire (permanent)
        stale_threshold: Duration::from_secs(redis_cfg.stale_threshold_secs),
        enable_stats: config.cache.enable_stats,
    };

    Ok(CacheService::with_config(redis, cache_config))
}

/// Parse user identifier (FID or username)
async fn parse_user_identifier(identifier: &str, database: &Database) -> Result<u64> {
    let trimmed = identifier.trim();

    if trimmed.starts_with('@') {
        let username = trimmed.trim_start_matches('@');
        let profile = database
            .get_user_profile_by_username(username)
            .await?
            .ok_or_else(|| {
                crate::SnapRagError::Custom(format!("Username @{username} not found"))
            })?;
        #[allow(clippy::cast_sign_loss)] // FID is guaranteed positive in database
        Ok(profile.fid as u64)
    } else {
        trimmed.parse::<u64>().map_err(|_| {
            crate::SnapRagError::Custom(format!(
                "Invalid user identifier '{identifier}'. Use FID or @username"
            ))
        })
    }
}

/// Normalize timestamps to day boundaries for caching (same as API)
fn normalize_timestamps_for_cache(
    start: Option<i64>,
    end: Option<i64>,
) -> (Option<i64>, Option<i64>, Option<String>) {
    match (start, end) {
        (Some(start_ts), Some(end_ts)) => {
            let start_dt = DateTime::<Utc>::from_timestamp(start_ts, 0)
                .unwrap_or_else(Utc::now)
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap();
            let end_dt = DateTime::<Utc>::from_timestamp(end_ts, 0)
                .unwrap_or_else(Utc::now)
                .date_naive()
                .and_hms_opt(23, 59, 59)
                .unwrap();

            let normalized_start = start_dt.and_utc().timestamp();
            let normalized_end = end_dt.and_utc().timestamp();
            let date_range_key = Some(format!(
                "{}:{}",
                start_dt.format("%Y-%m-%d"),
                end_dt.format("%Y-%m-%d")
            ));

            (Some(normalized_start), Some(normalized_end), date_range_key)
        }
        (Some(start_ts), None) => {
            let start_dt = DateTime::<Utc>::from_timestamp(start_ts, 0)
                .unwrap_or_else(Utc::now)
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .unwrap();

            let normalized_start = start_dt.and_utc().timestamp();
            let date_range_key = Some(format!("{}:", start_dt.format("%Y-%m-%d")));

            (Some(normalized_start), None, date_range_key)
        }
        (None, Some(end_ts)) => {
            let end_dt = DateTime::<Utc>::from_timestamp(end_ts, 0)
                .unwrap_or_else(Utc::now)
                .date_naive()
                .and_hms_opt(23, 59, 59)
                .unwrap();

            let normalized_end = end_dt.and_utc().timestamp();
            let date_range_key = Some(format!(":{}", end_dt.format("%Y-%m-%d")));

            (None, Some(normalized_end), date_range_key)
        }
        (None, None) => (None, None, None),
    }
}

/// Convert Unix timestamp to Farcaster timestamp
#[allow(clippy::cast_sign_loss)] // Timestamps are always positive
fn unix_to_farcaster_ts(unix_ts: i64) -> i64 {
    crate::unix_to_farcaster_timestamp(unix_ts as u64) as i64
}

/// Write JSON response to file or stdout
fn write_json_output(data: &serde_json::Value, output: Option<&String>) -> Result<()> {
    let json_str = serde_json::to_string_pretty(data)?;

    if let Some(path) = output {
        let mut file = File::create(path)?;
        file.write_all(json_str.as_bytes())?;
        print_info(&format!("✅ Results saved to: {path}"));
    } else {
        println!("{json_str}");
    }

    Ok(())
}

/// Handle cast stats command
async fn handle_cast_stats(
    config: &AppConfig,
    user: String,
    start: Option<i64>,
    end: Option<i64>,
    output: Option<String>,
    force: bool,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);
    let cache_service = create_cache_service(config).ok();

    let fid = parse_user_identifier(&user, &database).await?;
    print_info(&format!("📊 Getting cast statistics for FID {fid}..."));

    // Normalize timestamps for caching (same as API)
    let (normalized_start, normalized_end, date_range_key) =
        normalize_timestamps_for_cache(start, end);

    let start_farcaster = normalized_start.map(unix_to_farcaster_ts);
    let end_farcaster = normalized_end.map(unix_to_farcaster_ts);

    // Check cache first if available and not forcing
    if !force {
        if let Some(cache) = &cache_service {
            match cache
                .get_cast_stats_with_range(fid as i64, date_range_key.as_deref())
                .await
            {
                Ok(CacheResult::Fresh(cached_stats)) => {
                    let stats_data = serde_json::to_value(&cached_stats)?;
                    print_info("📦 Using cached cast statistics (fresh)");
                    return write_json_output(&stats_data, output.as_ref());
                }
                Ok(CacheResult::Stale(_) | CacheResult::Updating(_)) => {
                    info!("📦 Cache expired, regenerating cast statistics...");
                }
                Ok(CacheResult::Miss) => {
                    debug!("📭 Cache miss, generating cast statistics...");
                }
                Err(e) => {
                    debug!("Cache lookup error: {e}, continuing...");
                }
            }
        }
    } else {
        info!("🔄 Force mode: bypassing cache");
    }

    // Query casts from database
    let casts = database
        .get_casts_by_fid_and_time_range(fid as i64, start_farcaster, end_farcaster)
        .await?;

    if casts.is_empty() {
        return Err(crate::SnapRagError::Custom(format!(
            "No casts found for FID {fid}"
        )));
    }

    // Compute statistics using the same function as API
    let response = crate::api::handlers::cast_stats::compute_cast_statistics(&casts);

    // Cache the response if cache service is available
    if let Some(cache) = &cache_service {
        if let Err(e) = cache
            .set_cast_stats_with_range(fid as i64, date_range_key.as_deref(), &response)
            .await
        {
            debug!("Failed to cache cast stats: {e}");
        } else {
            debug!("Cached cast statistics for FID {fid}");
        }
    }

    let stats_data = serde_json::to_value(&response)?;
    write_json_output(&stats_data, output.as_ref())
}

/// Handle engagement command
async fn handle_engagement(
    config: &AppConfig,
    user: String,
    start: Option<i64>,
    end: Option<i64>,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    let fid = parse_user_identifier(&user, &database).await?;
    print_info(&format!("📊 Getting engagement metrics for FID {fid}..."));

    let start_farcaster = start.map(unix_to_farcaster_ts);
    let end_farcaster = end.map(unix_to_farcaster_ts);

    let reactions_received = database
        .get_reactions_received(fid as i64, start_farcaster, end_farcaster, 1)
        .await
        .unwrap_or(0);
    let recasts_received = database
        .get_recasts_received(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or(0);
    let replies_received = database
        .get_replies_received(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or(0);

    let most_popular_cast = database
        .get_most_popular_cast(fid as i64, start_farcaster, end_farcaster)
        .await
        .ok()
        .flatten();

    let top_reactors = database
        .get_top_interactive_users(fid as i64, start_farcaster, end_farcaster, 10)
        .await
        .unwrap_or_default();

    let response = serde_json::json!({
        "fid": fid,
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
            "pfp_url": u.pfp_url,
            "interaction_count": u.interaction_count,
        })).collect::<Vec<_>>(),
    });

    write_json_output(&response, output.as_ref())
}

/// Handle temporal activity command
async fn handle_temporal_activity(
    config: &AppConfig,
    user: String,
    start: Option<i64>,
    end: Option<i64>,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    let fid = parse_user_identifier(&user, &database).await?;
    print_info(&format!("📊 Getting temporal activity for FID {fid}..."));

    let start_farcaster = start.map(unix_to_farcaster_ts);
    let end_farcaster = end.map(unix_to_farcaster_ts);

    let hourly_dist = database
        .get_hourly_distribution(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or_default();
    let monthly_dist = database
        .get_monthly_distribution(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or_default();
    let first_cast = database
        .get_first_cast_in_range(fid as i64, start_farcaster, end_farcaster)
        .await
        .ok()
        .flatten();
    let last_cast = database
        .get_last_cast_in_range(fid as i64, start_farcaster, end_farcaster)
        .await
        .ok()
        .flatten();

    let response = serde_json::json!({
        "fid": fid,
        "hourly_distribution": hourly_dist,
        "monthly_distribution": monthly_dist,
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
    });

    write_json_output(&response, output.as_ref())
}

/// Handle content style command
async fn handle_content_style(
    config: &AppConfig,
    user: String,
    start: Option<i64>,
    end: Option<i64>,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    let fid = parse_user_identifier(&user, &database).await?;
    print_info(&format!("📊 Getting content style for FID {fid}..."));

    let start_farcaster = start.map(unix_to_farcaster_ts);
    let end_farcaster = end.map(unix_to_farcaster_ts);

    let casts_text = database
        .get_casts_text_for_analysis(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or_default();
    let frames_used = database
        .get_frame_usage_count(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or(0);

    // Calculate emoji frequencies
    use std::collections::HashMap;

    use crate::utils::emoji::count_emoji_frequencies;

    let mut emoji_freq: HashMap<String, usize> = HashMap::new();
    for text in &casts_text {
        let freqs = count_emoji_frequencies(text);
        for (emoji, count) in freqs {
            if !emoji.is_empty() {
                *emoji_freq.entry(emoji).or_insert(0) += count;
            }
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    let mut top_emojis: Vec<_> = emoji_freq
        .into_iter()
        .filter(|(e, _)| !e.is_empty())
        .map(|(e, c)| {
            serde_json::json!({
                "emoji": e,
                "count": c as i64,
            })
        })
        .collect();
    top_emojis.sort_by(|a, b| {
        let count_a = a.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        let count_b = b.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        count_b.cmp(&count_a)
    });
    top_emojis.truncate(10);

    // Calculate word frequencies (simplified)
    use crate::text_analysis::analyze_text;
    use crate::text_analysis::count_word_frequencies;
    use crate::text_analysis::extract_nouns;
    use crate::text_analysis::extract_verbs;

    let mut word_freq_by_lang: HashMap<String, HashMap<String, usize>> = HashMap::new();

    for text in &casts_text {
        if text.trim().is_empty() {
            continue;
        }

        let (tagged_words, lang_info) = analyze_text(text);
        let lang_code = &lang_info.lang_code;

        let nouns = extract_nouns(&tagged_words);
        let verbs = extract_verbs(&tagged_words);

        let mut words = nouns;
        words.extend(verbs);

        let word_freq = count_word_frequencies(&words, lang_code);
        let lang_word_freq = word_freq_by_lang.entry(lang_code.clone()).or_default();
        for (word, count) in word_freq {
            *lang_word_freq.entry(word).or_insert(0) += count;
        }
    }

    let mut all_word_freq: HashMap<String, usize> = HashMap::new();
    for freq_map in word_freq_by_lang.values() {
        for (word, count) in freq_map {
            *all_word_freq.entry(word.clone()).or_insert(0) += count;
        }
    }

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
        let count_a = a.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        let count_b = b.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        count_b.cmp(&count_a)
    });
    top_words.truncate(20);

    let response = serde_json::json!({
        "fid": fid,
        "top_emojis": top_emojis,
        "top_words": top_words,
        "frames_used": frames_used,
    });

    write_json_output(&response, output.as_ref())
}

/// Handle follower growth command
async fn handle_follower_growth(
    config: &AppConfig,
    user: String,
    start: Option<i64>,
    end: Option<i64>,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    let fid = parse_user_identifier(&user, &database).await?;
    print_info(&format!("📊 Getting follower growth for FID {fid}..."));

    let start_farcaster = start.map(unix_to_farcaster_ts);

    let current_followers = database
        .get_current_follower_count(fid as i64)
        .await
        .unwrap_or(0);
    let current_following = database
        .get_current_following_count(fid as i64)
        .await
        .unwrap_or(0);
    let followers_at_start = if let Some(ts) = start_farcaster {
        database
            .get_follower_count_at_timestamp(fid as i64, ts)
            .await
            .unwrap_or(0)
    } else {
        0
    };
    let following_at_start = if let Some(ts) = start_farcaster {
        database
            .get_following_count_at_timestamp(fid as i64, ts)
            .await
            .unwrap_or(0)
    } else {
        0
    };

    let end_farcaster = end.map(unix_to_farcaster_ts);
    let monthly_snapshots = database
        .get_monthly_follower_snapshots(fid as i64, start_farcaster, end_farcaster)
        .await
        .unwrap_or_default();

    let response = serde_json::json!({
        "fid": fid,
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
    });

    write_json_output(&response, output.as_ref())
}

/// Handle domains command
async fn handle_domains(config: &AppConfig, user: String, output: Option<String>) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    let fid = parse_user_identifier(&user, &database).await?;
    print_info(&format!("📊 Getting domain status for FID {fid}..."));

    let proofs = database.get_user_username_proofs(fid as i64).await?;

    use crate::models::UsernameType;
    let mut has_ens = false;
    let mut ens_name = None;
    let mut has_farcaster_name = false;
    let mut farcaster_name = None;
    let mut username_type = None;

    for proof in proofs {
        let username_type_enum = UsernameType::from(i32::from(proof.username_type));
        match username_type_enum {
            UsernameType::EnsL1 => {
                has_ens = true;
                ens_name = Some(proof.username.clone());
                if username_type.is_none() {
                    username_type = Some("ens".to_string());
                }
            }
            UsernameType::Fname => {
                has_farcaster_name = true;
                farcaster_name = Some(proof.username.clone());
                if username_type.is_none() {
                    username_type = Some("fname".to_string());
                }
            }
            _ => {}
        }
    }

    let response = serde_json::json!({
        "fid": fid,
        "has_ens": has_ens,
        "ens_name": ens_name,
        "has_farcaster_name": has_farcaster_name,
        "farcaster_name": farcaster_name,
        "username_type": username_type,
    });

    write_json_output(&response, output.as_ref())
}

/// Handle network stats command
async fn handle_network_stats(
    config: &AppConfig,
    start: Option<i64>,
    end: Option<i64>,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    print_info("📊 Getting network statistics...");

    let start_farcaster = start.map(unix_to_farcaster_ts);
    let end_farcaster = end.map(unix_to_farcaster_ts);

    let averages = database
        .get_network_averages(start_farcaster, end_farcaster)
        .await?;

    let cast_percentiles = database
        .get_cast_percentiles(start_farcaster, end_farcaster)
        .await
        .unwrap_or(crate::api::types::Percentiles {
            p50: 0,
            p75: 0,
            p90: 0,
        });

    let response = serde_json::json!({
        "avg_casts_per_user": averages.avg_casts_per_user,
        "avg_reactions_per_user": averages.avg_reactions_per_user,
        "avg_followers_per_user": averages.avg_followers_per_user,
        "total_active_users": averages.total_active_users,
        "percentiles": {
            "casts": cast_percentiles,
            "reactions": {
                "p50": 0,
                "p75": 0,
                "p90": 0,
            },
        },
    });

    write_json_output(&response, output.as_ref())
}
