//! Annual report command handler

use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
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
use crate::cli::output::print_error;
use crate::cli::output::print_info;
use crate::cli::output::print_warning;
use crate::database::Database;
use crate::text_analysis::analyze_text;
use crate::text_analysis::count_word_frequencies;
use crate::text_analysis::extract_nouns;
use crate::text_analysis::extract_verbs;
use crate::utils::emoji::count_emoji_frequencies;
use crate::AppConfig;
use crate::Result;

/// Handle annual report command for a single user
pub async fn handle_annual_report_user(
    config: &AppConfig,
    user_identifier: String,
    year: u32,
    output: Option<String>,
    force: bool,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    // Initialize cache service if Redis is configured
    let cache_service = create_cache_service(config).ok();

    // Parse user identifier
    let fid = parse_user_identifier(&user_identifier, &database).await?;

    print_info(&format!(
        "📊 Generating annual report for FID {fid} year {year}..."
    ));

    let report =
        generate_annual_report(&database, cache_service.as_ref(), fid as i64, year, force).await?;

    // Output the report
    let output_path = output.unwrap_or_else(|| {
        format!(
            "annual_report_{}_{}_{}.json",
            fid,
            year,
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        )
    });

    let json_str = serde_json::to_string_pretty(&report)?;
    let mut file = File::create(&output_path)?;
    file.write_all(json_str.as_bytes())?;

    print_info(&format!("✅ Annual report saved to: {output_path}"));
    println!();

    // Print summary
    print_summary(&report);

    Ok(())
}

/// Handle annual report command for CSV batch processing
pub async fn handle_annual_report_csv(
    config: &AppConfig,
    csv_path: String,
    year: u32,
    output_dir: Option<String>,
    force: bool,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);

    // Initialize cache service if Redis is configured
    let cache_service = create_cache_service(config).ok();

    print_info(&format!("📂 Reading CSV file: {csv_path}"));
    let fids = read_fids_from_csv(&csv_path)?;
    print_info(&format!("✅ Found {} FIDs in CSV", fids.len())); // clippy: fids.len() needs format! for numeric formatting
    println!();

    let output_dir = output_dir.unwrap_or_else(|| "annual_reports".to_string());
    std::fs::create_dir_all(&output_dir)?;

    let mut success_count = 0;
    let mut error_count = 0;
    let mut errors = Vec::new();

    for (index, fid) in fids.iter().enumerate() {
        let progress = index + 1;
        let total = fids.len();
        print_info(&format!("[{progress}/{total}] Processing FID {fid}..."));

        match generate_annual_report(&database, cache_service.as_ref(), *fid, year, force).await {
            Ok(report) => {
                let output_path = format!("{output_dir}/annual_report_{fid}_{year}.json");
                let json_str = serde_json::to_string_pretty(&report)?;
                let mut file = File::create(&output_path)?;
                file.write_all(json_str.as_bytes())?;
                success_count += 1;
            }
            Err(e) => {
                error_count += 1;
                let error_msg = format!("FID {fid}: {e}");
                errors.push(error_msg.clone());
                print_error(&error_msg);
            }
        }
    }

    println!();
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  BATCH PROCESSING SUMMARY                                     ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!("  Total FIDs:      {}", fids.len());
    println!("  ✅ Successful:   {success_count}");
    println!("  ❌ Errors:       {error_count}");
    println!("  Output directory: {output_dir}");
    println!();

    if !errors.is_empty() {
        print_warning("Errors encountered:");
        for error in &errors {
            println!("  - {error}");
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
        annual_report_ttl: Duration::from_secs(86400), // 1 day default
        stale_threshold: Duration::from_secs(redis_cfg.stale_threshold_secs),
        enable_stats: config.cache.enable_stats,
    };

    Ok(CacheService::with_config(redis, cache_config))
}

/// Generate annual report for a specific FID and year
///
/// If force is true, bypasses cache and regenerates the report.
/// If force is false and cache is fresh, returns cached data.
/// If cache is stale/updating, generates new report and updates cache (CLI purpose: build cache).
#[allow(clippy::too_many_lines)] // Complex report generation requires many lines
async fn generate_annual_report(
    database: &Database,
    cache_service: Option<&CacheService>,
    fid: i64,
    year: u32,
    force: bool,
) -> Result<serde_json::Value> {
    let start_time = std::time::Instant::now();

    // Check cache first if available and not forcing
    if !force {
        if let Some(cache) = cache_service {
            match cache.get_annual_report(fid, year).await {
                Ok(CacheResult::Fresh(cached_report)) => {
                    let duration = start_time.elapsed();
                    info!(
                        "📦 Annual report cache hit (fresh) for FID {} year {} - {}ms",
                        fid,
                        year,
                        duration.as_millis()
                    );
                    return Ok(cached_report);
                }
                Ok(CacheResult::Stale(_)) => {
                    info!(
                        "📦 Annual report cache hit (stale) for FID {} year {}, regenerating...",
                        fid, year
                    );
                    // Continue to generate new report and update cache
                }
                Ok(CacheResult::Updating(_)) => {
                    info!(
                        "🔄 Annual report cache expired for FID {} year {}, regenerating...",
                        fid, year
                    );
                    // Continue to generate new report and update cache
                }
                Ok(CacheResult::Miss) => {
                    debug!("📭 Annual report cache miss for FID {} year {}", fid, year);
                }
                Err(e) => {
                    debug!(
                        "Cache lookup error for annual report FID {} year {}: {}",
                        fid, year, e
                    );
                    // Continue with database query on cache error
                }
            }
        }
    } else {
        info!(
            "🔄 Force mode: bypassing cache for FID {} year {}",
            fid, year
        );
    }

    // Calculate time range for the year
    let year_start = DateTime::parse_from_rfc3339(&format!("{year}-01-01T00:00:00Z"))
        .map_err(|_| crate::SnapRagError::Custom("Invalid year format".to_string()))?
        .timestamp();
    let year_end = DateTime::parse_from_rfc3339(&format!("{year}-12-31T23:59:59Z"))
        .map_err(|_| crate::SnapRagError::Custom("Invalid year format".to_string()))?
        .timestamp();

    // Convert to Farcaster timestamps
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let start_farcaster = crate::unix_to_farcaster_timestamp(year_start as u64) as i64;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
    let end_farcaster = crate::unix_to_farcaster_timestamp(year_end as u64) as i64;

    debug!(
        "Calculating time range for year {}: start={}, end={}",
        year, year_start, year_end
    );

    // Get user profile
    let profile = database
        .get_user_profile(fid)
        .await?
        .ok_or_else(|| crate::SnapRagError::Custom(format!("User {fid} not found")))?;

    let registered_at = database
        .get_registration_timestamp(fid)
        .await
        .unwrap_or(None);

    // Get engagement metrics
    let reactions_received = database
        .get_reactions_received(fid, Some(start_farcaster), Some(end_farcaster), 1)
        .await
        .unwrap_or(0);
    let recasts_received = database
        .get_recasts_received(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    let replies_received = database
        .get_replies_received(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    let most_popular_cast = database
        .get_most_popular_cast(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .ok()
        .flatten();
    let top_reactors = database
        .get_top_interactive_users(fid, Some(start_farcaster), Some(end_farcaster), 10)
        .await
        .unwrap_or_default();

    // Get temporal activity
    let hourly_dist = database
        .get_hourly_distribution(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();
    let monthly_dist = database
        .get_monthly_distribution(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();
    let first_cast = database
        .get_first_cast_in_range(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .ok()
        .flatten();

    // Get content style
    let casts_text = database
        .get_casts_text_for_analysis(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();

    // Get follower growth
    let current_followers = database.get_current_follower_count(fid).await.unwrap_or(0);
    let followers_at_start = database
        .get_follower_count_at_timestamp(fid, start_farcaster)
        .await
        .unwrap_or(0);
    let monthly_snapshots = database
        .get_monthly_follower_snapshots(fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or_default();

    // Calculate days since registration
    let days_since_registration = registered_at.map_or(0, |reg_ts| {
        let reg_dt = DateTime::<Utc>::from_timestamp(reg_ts, 0).unwrap_or_else(Utc::now);
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

    // Calculate emoji frequencies
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
        .map(|(e, c)| (e, c as i64))
        .collect();
    top_emojis.sort_by(|a, b| b.1.cmp(&a.1));
    top_emojis.truncate(10);

    // Calculate word frequencies
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

    // Aggregate top words across all languages
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
    top_words.truncate(20);

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
            "total_casts_in_year": total_casts_in_year,
            "first_cast": first_cast.map(|c| serde_json::json!({
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
                "pfp_url": u.pfp_url,
                "interaction_count": u.interaction_count,
            })).collect::<Vec<_>>(),
        },
        "content_style": {
            "top_emojis": top_emojis.into_iter().map(|(e, c)| serde_json::json!({
                "emoji": e,
                "count": c,
            })).collect::<Vec<_>>(),
            "top_words": top_words,
        },
        "social_growth": {
            "current_followers": current_followers,
            "followers_at_start": followers_at_start,
            "net_growth": current_followers.saturating_sub(followers_at_start),
            "monthly_snapshots": monthly_snapshots.into_iter().map(|s| serde_json::json!({
                "month": s.month,
                "followers": s.followers,
            })).collect::<Vec<_>>(),
        },
    });

    // Cache the response if cache service is available
    if let Some(cache) = cache_service {
        if let Err(e) = cache.set_annual_report(fid, year, &response).await {
            debug!(
                "Failed to cache annual report for FID {} year {}: {}",
                fid, year, e
            );
        } else {
            debug!("Cached annual report for FID {} year {}", fid, year);
        }
    }

    let duration = start_time.elapsed();
    info!(
        "Generated annual report for FID {} year {} in {:?}",
        fid, year, duration
    );

    Ok(response)
}

/// Read FIDs from CSV file (last column is FID)
fn read_fids_from_csv(csv_path: &str) -> Result<Vec<i64>> {
    let file = File::open(csv_path)?;
    let reader = BufReader::new(file);
    let mut fids = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Split by comma and get the last field
        let fields: Vec<&str> = trimmed.split(',').collect();
        if fields.is_empty() {
            continue;
        }

        let fid_str = fields.last().unwrap().trim();
        match fid_str.parse::<i64>() {
            Ok(fid) => fids.push(fid),
            Err(e) => {
                return Err(crate::SnapRagError::Custom(format!(
                    "Invalid FID on line {}: {} ({})",
                    line_num + 1,
                    fid_str,
                    e
                )));
            }
        }
    }

    Ok(fids)
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

/// Print summary of annual report
fn print_summary(report: &serde_json::Value) {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  ANNUAL REPORT SUMMARY                                        ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    if let Some(user) = report.get("user") {
        if let Some(username) = user.get("username") {
            println!("  User: @{}", username.as_str().unwrap_or(""));
        }
        if let Some(fid) = user.get("fid") {
            println!("  FID: {}", fid.as_i64().unwrap_or(0));
        }
        println!();
    }

    if let Some(activity) = report.get("activity") {
        if let Some(total) = activity.get("total_casts_in_year") {
            println!("  Total Casts (year): {}", total.as_i64().unwrap_or(0));
        }
        println!();
    }

    if let Some(engagement) = report.get("engagement") {
        if let Some(total) = engagement.get("total_engagement") {
            println!("  Total Engagement: {}", total.as_i64().unwrap_or(0));
        }
        if let Some(reactions) = engagement.get("reactions_received") {
            println!("  Reactions Received: {}", reactions.as_i64().unwrap_or(0));
        }
        if let Some(recasts) = engagement.get("recasts_received") {
            println!("  Recasts Received: {}", recasts.as_i64().unwrap_or(0));
        }
        if let Some(replies) = engagement.get("replies_received") {
            println!("  Replies Received: {}", replies.as_i64().unwrap_or(0));
        }
        println!();
    }

    if let Some(social) = report.get("social_growth") {
        if let Some(current) = social.get("current_followers") {
            println!("  Current Followers: {}", current.as_i64().unwrap_or(0));
        }
        if let Some(net) = social.get("net_growth") {
            println!("  Net Growth: {}", net.as_i64().unwrap_or(0));
        }
    }
}
