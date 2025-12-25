//! MBTI personality analysis command handler

use std::sync::Arc;

use tracing::debug;
use tracing::info;

use crate::api::cache::CacheResult;
use crate::api::cache::CacheService;
use crate::cli::commands::MbtiCommands;
use crate::cli::output::print_info;
use crate::cli::output::print_success;
use crate::cli::output::print_warning;
use crate::config::MbtiMethod;
use crate::database::Database;
use crate::llm::LlmService;
use crate::personality::MbtiAnalyzer;
use crate::social_graph::SocialGraphAnalyzer;
use crate::sync::client::SnapchainClient;
use crate::sync::lazy_loader::LazyLoader;
use crate::AppConfig;
use crate::Result;

/// Handle MBTI personality analysis command
pub async fn handle_mbti_analysis(
    config: &AppConfig,
    user_identifier: String,
    use_llm: bool,
    verbose: bool,
    export_path: Option<String>,
) -> Result<()> {
    // Initialize services
    let database = Arc::new(Database::from_config(config).await?);
    let snapchain_client = Arc::new(SnapchainClient::from_config(config).await?);
    let lazy_loader = LazyLoader::new(database.clone(), snapchain_client.clone());

    // Parse user identifier
    let fid = parse_user_identifier(&user_identifier, &database).await?;

    // Get user profile
    let profile = lazy_loader
        .get_user_profile_smart(fid as i64)
        .await?
        .ok_or_else(|| crate::SnapRagError::Custom(format!("User {fid} not found")))?;

    let username = profile
        .username
        .as_ref()
        .map_or_else(|| format!("FID {fid}"), |u| format!("@{u}"));
    let display_name = profile.display_name.as_deref().unwrap_or("Unknown");

    // Determine analysis method
    let method = config.mbti.method;
    let method_name = match method {
        MbtiMethod::RuleBased => "Rule-based",
        MbtiMethod::MachineLearning => "Machine Learning (BERT + MLP)",
        MbtiMethod::Ensemble => "Ensemble (Rule-based + ML)",
    };

    print_info(&format!(
        "🧠 Analyzing MBTI personality for {display_name} ({username})..."
    ));
    print_info(&format!("📊 Method: {method_name}"));
    println!();

    // Get social profile for context (optional, used by rule-based and ensemble)
    let social_profile =
        if verbose || matches!(method, MbtiMethod::RuleBased | MbtiMethod::Ensemble) {
            let social_analyzer =
                SocialGraphAnalyzer::with_snapchain(database.clone(), snapchain_client);
            match social_analyzer.analyze_user(fid as i64).await {
                Ok(profile) => Some(profile),
                Err(e) => {
                    print_warning(&format!("Could not load social profile: {e}"));
                    None
                }
            }
        } else {
            None
        };

    // Perform analysis based on configured method
    let mbti_profile = match method {
        MbtiMethod::RuleBased => {
            // Rule-based analysis
            let analyzer = if use_llm || config.mbti.use_llm {
                match LlmService::new(config) {
                    Ok(llm_service) => {
                        print_info("🤖 Using LLM for enhanced analysis...");
                        MbtiAnalyzer::with_llm(database.clone(), Arc::new(llm_service))
                    }
                    Err(e) => {
                        print_warning(&format!(
                            "⚠️  LLM service not available: {e}. Using rule-based analysis."
                        ));
                        MbtiAnalyzer::new(database.clone())
                    }
                }
            } else {
                MbtiAnalyzer::new(database.clone())
            };

            analyzer
                .analyze_mbti(fid as i64, social_profile.as_ref())
                .await?
        }
        #[cfg(feature = "ml-mbti")]
        MbtiMethod::MachineLearning => {
            // ML-based analysis
            print_info("🤖 Using machine learning model (BERT + Multi-Task MLP)...");
            let ml_predictor = crate::personality_ml::MlMbtiPredictor::new(database.clone())?;
            ml_predictor.predict_mbti(fid as i64).await?
        }
        #[cfg(not(feature = "ml-mbti"))]
        MbtiMethod::MachineLearning => {
            print_warning("⚠️  ML-MBTI feature not enabled. Falling back to rule-based analysis.");
            print_warning("   To enable: cargo build --features ml-mbti");
            let analyzer = MbtiAnalyzer::new(database.clone());
            analyzer
                .analyze_mbti(fid as i64, social_profile.as_ref())
                .await?
        }
        #[cfg(feature = "ml-mbti")]
        MbtiMethod::Ensemble => {
            // Ensemble analysis (both methods)
            print_info("🔄 Using ensemble method (combining rule-based + ML)...");
            let ensemble = crate::personality_ml::EnsembleMbtiPredictor::new(database.clone())?;
            ensemble
                .predict_ensemble(fid as i64, social_profile.as_ref())
                .await?
        }
        #[cfg(not(feature = "ml-mbti"))]
        MbtiMethod::Ensemble => {
            print_warning(
                "⚠️  ML-MBTI feature not enabled for ensemble. Falling back to rule-based analysis.",
            );
            print_warning("   To enable: cargo build --features ml-mbti");
            let analyzer = MbtiAnalyzer::new(database.clone());
            analyzer
                .analyze_mbti(fid as i64, social_profile.as_ref())
                .await?
        }
    };

    // Display results - compact format
    let confidence_indicator = if mbti_profile.confidence >= 0.8 {
        "🟢"
    } else if mbti_profile.confidence >= 0.6 {
        "🟡"
    } else if mbti_profile.confidence >= 0.4 {
        "🟠"
    } else {
        "🔴"
    };

    println!("┌──────────────────────────────────────────────────────────┐");
    println!(
        "│  MBTI: {}  {}  Confidence: {:.0}%                      │",
        mbti_profile.mbti_type,
        confidence_indicator,
        mbti_profile.confidence * 100.0
    );
    println!("└──────────────────────────────────────────────────────────┘");
    println!();

    // Type description with traits
    let type_desc = get_type_description(&mbti_profile.mbti_type);
    println!("  {}", type_desc.lines().next().unwrap_or(""));
    println!();

    // Compact traits display
    println!("  💎 Traits: {}", mbti_profile.traits.join(" • "));
    println!();

    // Compact dimension scores
    print_compact_dimension("E/I", mbti_profile.dimensions.ei_score);
    print_compact_dimension("S/N", mbti_profile.dimensions.sn_score);
    print_compact_dimension("T/F", mbti_profile.dimensions.tf_score);
    print_compact_dimension("J/P", mbti_profile.dimensions.jp_score);
    println!();

    // Key insight from analysis (first paragraph only, unless verbose)
    if verbose {
        println!("  📊 Analysis:");
        for line in mbti_profile.analysis.lines() {
            if line.trim().is_empty() {
                println!();
            } else {
                let wrapped = textwrap::wrap(line.trim(), 58);
                for wrapped_line in wrapped {
                    println!("     {wrapped_line}");
                }
            }
        }
    } else {
        // Show only first paragraph for concise output
        let first_para: String = mbti_profile
            .analysis
            .lines()
            .take_while(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" ");

        if !first_para.is_empty() {
            println!("  📊 Key Insight:");
            let wrapped = textwrap::wrap(first_para.trim(), 58);
            for wrapped_line in wrapped {
                println!("     {wrapped_line}");
            }
        }
    }
    println!();

    // Verbose mode: show detailed dimension breakdown with full visualization
    if verbose {
        println!("  ┌─────────────────────────────────────────────────────────┐");
        println!("  │  DETAILED DIMENSION BREAKDOWN                           │");
        println!("  └─────────────────────────────────────────────────────────┘");
        println!();

        print_dimension(
            "E/I",
            "Extraversion ↔ Introversion",
            mbti_profile.dimensions.ei_score,
            mbti_profile.dimensions.ei_confidence,
        );
        print_dimension(
            "S/N",
            "Sensing ↔ Intuition",
            mbti_profile.dimensions.sn_score,
            mbti_profile.dimensions.sn_confidence,
        );
        print_dimension(
            "T/F",
            "Thinking ↔ Feeling",
            mbti_profile.dimensions.tf_score,
            mbti_profile.dimensions.tf_confidence,
        );
        print_dimension(
            "J/P",
            "Judging ↔ Perceiving",
            mbti_profile.dimensions.jp_score,
            mbti_profile.dimensions.jp_confidence,
        );
    }

    // Export to JSON if requested
    if let Some(export_path) = export_path {
        let json = serde_json::to_string_pretty(&mbti_profile)?;
        std::fs::write(&export_path, json)?;
        print_success(&format!("✅ Exported analysis to: {export_path}"));
    }

    Ok(())
}

/// Handle MBTI commands (batch, stats, search, compatibility)
pub async fn handle_mbti_command(config: &AppConfig, command: &MbtiCommands) -> Result<()> {
    match command {
        MbtiCommands::Batch { fids, output } => {
            handle_mbti_batch(config, fids.clone(), output.clone()).await?;
        }
        MbtiCommands::Stats { output } => {
            handle_mbti_stats(config, output.clone()).await?;
        }
        MbtiCommands::Search { mbti_type, output } => {
            handle_mbti_search(config, mbti_type.clone(), output.clone()).await?;
        }
        MbtiCommands::Compatibility {
            user1,
            user2,
            output,
        } => {
            handle_mbti_compatibility(config, user1.clone(), user2.clone(), output.clone()).await?;
        }
    }
    Ok(())
}

/// Handle MBTI batch analysis
async fn handle_mbti_batch(
    config: &AppConfig,
    fids_str: String,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);
    let cache_service = create_cache_service(config).ok();

    // Parse FIDs
    let fids: Vec<i64> = fids_str
        .split(',')
        .map(|s| s.trim().parse::<i64>())
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|_| crate::SnapRagError::Custom("Invalid FID format".to_string()))?;

    print_info(&format!(
        "📊 Batch analyzing MBTI for {} users...",
        fids.len()
    ));

    let method = config.mbti.method;
    let mut results = Vec::new();

    for fid in fids {
        // Check cache first
        if let Some(cache) = &cache_service {
            match cache.get_mbti(fid).await {
                Ok(CacheResult::Fresh(cached_mbti)) => {
                    results.push(crate::api::handlers::mbti::MbtiResult {
                        fid,
                        mbti_profile: Some(cached_mbti),
                        error: None,
                    });
                    continue;
                }
                Ok(CacheResult::Stale(_)) | Ok(CacheResult::Updating(_)) => {
                    info!("Cache expired for FID {fid}, regenerating...");
                }
                Ok(CacheResult::Miss) => {
                    debug!("Cache miss for FID {fid}");
                }
                Err(_) => {}
            }
        }

        // Analyze MBTI
        let analyzer = MbtiAnalyzer::new(Arc::clone(&database));
        let social_profile = if matches!(method, MbtiMethod::RuleBased | MbtiMethod::Ensemble) {
            let social_analyzer = SocialGraphAnalyzer::new(Arc::clone(&database));
            social_analyzer.analyze_user(fid).await.ok()
        } else {
            None
        };

        match analyzer.analyze_mbti(fid, social_profile.as_ref()).await {
            Ok(mbti_profile) => {
                // Cache result
                if let Some(cache) = &cache_service {
                    let _ = cache.set_mbti(fid, &mbti_profile).await;
                }
                results.push(crate::api::handlers::mbti::MbtiResult {
                    fid,
                    mbti_profile: Some(mbti_profile),
                    error: None,
                });
            }
            Err(e) => {
                results.push(crate::api::handlers::mbti::MbtiResult {
                    fid,
                    mbti_profile: None,
                    error: Some(format!("{e}")),
                });
            }
        }
    }

    let response = serde_json::json!(results);
    write_json_output(&response, output.as_ref())
}

/// Handle MBTI stats
async fn handle_mbti_stats(config: &AppConfig, output: Option<String>) -> Result<()> {
    // This would require storing MBTI results in database
    // For now, return placeholder
    let response = serde_json::json!({
        "total_analyzed": 0,
        "type_distribution": {},
        "average_confidence": 0.0,
        "most_common_type": None::<String>,
    });

    write_json_output(&response, output.as_ref())
}

/// Handle MBTI search
async fn handle_mbti_search(
    config: &AppConfig,
    mbti_type: String,
    output: Option<String>,
) -> Result<()> {
    // Validate MBTI type
    let mbti_upper = mbti_type.to_uppercase();
    if mbti_upper.len() != 4 {
        return Err(crate::SnapRagError::Custom(
            "Invalid MBTI type format. Expected 4 letters (e.g., INTJ, ENFP)".to_string(),
        ));
    }

    // This would require a database table to store MBTI results
    // For now, return empty results
    let response = serde_json::json!([]);
    write_json_output(&response, output.as_ref())
}

/// Handle MBTI compatibility
async fn handle_mbti_compatibility(
    config: &AppConfig,
    user1: String,
    user2: String,
    output: Option<String>,
) -> Result<()> {
    let database = Arc::new(Database::from_config(config).await?);
    let cache_service = create_cache_service(config).ok();

    let fid1 = parse_user_identifier(&user1, &database).await?;
    let fid2 = parse_user_identifier(&user2, &database).await?;

    print_info(&format!(
        "📊 Analyzing MBTI compatibility between FID {fid1} and FID {fid2}..."
    ));

    let method = config.mbti.method;

    // Get MBTI profiles (with cache support)
    let mbti1 = get_mbti_profile_cached(&database, &cache_service, fid1 as i64, method).await?;
    let mbti2 = get_mbti_profile_cached(&database, &cache_service, fid2 as i64, method).await?;

    // Use API handler to get compatibility (it has access to the private function)
    // For now, we'll create a simplified compatibility response
    // In a real implementation, we'd need to make calculate_mbti_compatibility public or use the API endpoint
    let response = serde_json::json!({
        "fid1": fid1,
        "fid2": fid2,
        "mbti_type1": mbti1.mbti_type,
        "mbti_type2": mbti2.mbti_type,
        "note": "Full compatibility analysis requires API endpoint access. Use /api/mbti/compatibility/:fid1/:fid2 for detailed analysis.",
    });

    write_json_output(&response, output.as_ref())
}

/// Get MBTI profile with cache support
async fn get_mbti_profile_cached(
    database: &Arc<Database>,
    cache_service: &Option<CacheService>,
    fid: i64,
    method: MbtiMethod,
) -> Result<crate::personality::MbtiProfile> {
    // Check cache first
    if let Some(cache) = cache_service {
        match cache.get_mbti(fid).await {
            Ok(CacheResult::Fresh(cached_mbti)) => {
                return Ok(cached_mbti);
            }
            Ok(CacheResult::Stale(_)) | Ok(CacheResult::Updating(_)) => {
                info!("Cache expired for FID {fid}, regenerating...");
            }
            Ok(CacheResult::Miss) => {
                debug!("Cache miss for FID {fid}");
            }
            Err(_) => {}
        }
    }

    // Analyze MBTI
    let analyzer = MbtiAnalyzer::new(Arc::clone(database));
    let social_profile = if matches!(method, MbtiMethod::RuleBased | MbtiMethod::Ensemble) {
        let social_analyzer = SocialGraphAnalyzer::new(Arc::clone(database));
        social_analyzer.analyze_user(fid).await.ok()
    } else {
        None
    };

    let mbti_profile = analyzer.analyze_mbti(fid, social_profile.as_ref()).await?;

    // Cache result
    if let Some(cache) = cache_service {
        let _ = cache.set_mbti(fid, &mbti_profile).await;
    }

    Ok(mbti_profile)
}

/// Create cache service from config
fn create_cache_service(config: &AppConfig) -> Result<crate::api::cache::CacheService> {
    use std::time::Duration;

    use crate::api::cache::CacheConfig;
    use crate::api::redis_client::RedisClient;

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
        annual_report_ttl: Duration::from_secs(86400),
        stale_threshold: Duration::from_secs(redis_cfg.stale_threshold_secs),
        enable_stats: config.cache.enable_stats,
    };

    Ok(crate::api::cache::CacheService::with_config(
        redis,
        cache_config,
    ))
}

/// Write JSON output to file or stdout
fn write_json_output(data: &serde_json::Value, output: Option<&String>) -> Result<()> {
    use std::fs::File;
    use std::io::Write;

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

/// Print compact dimension score (for non-verbose mode)
fn print_compact_dimension(code: &str, score: f32) {
    let letters: Vec<&str> = code.split('/').collect();
    let left = letters[0];
    let right = letters[1];

    // Determine which side is dominant
    let (dominant, percentage) = if score < 0.5 {
        (left, score.mul_add(-2.0, 1.0) * 100.0)
    } else {
        (right, score.mul_add(2.0, -1.0) * 100.0)
    };

    // Create compact visual bar (20 chars total)
    let bar_length = ((score * 20.0) as usize).min(20);
    let left_bar = "─".repeat(10 - bar_length.min(10));
    let right_bar = "─".repeat(bar_length.saturating_sub(10));

    println!("  {code}  {left}{left_bar}●{right_bar}{right}  {dominant} {percentage:.0}%");
}

/// Print dimension score with visual representation (for verbose mode)
fn print_dimension(code: &str, description: &str, score: f32, confidence: f32) {
    let letters: Vec<&str> = code.split('/').collect();
    let left = letters[0];
    let right = letters[1];

    // Determine which side is dominant
    let (dominant, percentage) = if score < 0.5 {
        (left, score.mul_add(-2.0, 1.0) * 100.0)
    } else {
        (right, score.mul_add(2.0, -1.0) * 100.0)
    };

    // Create visual bar
    let bar_length = ((score * 40.0) as usize).min(40);
    let left_bar = "─".repeat(20 - bar_length.min(20));
    let right_bar = "─".repeat(bar_length.saturating_sub(20));
    let marker = "●";

    println!("  {code}: {description}");
    println!(
        "      {left} {left_bar}{marker}{right_bar} {right}  →  {dominant} ({percentage:.0}%)"
    );
    println!(
        "      Confidence: {:.0}% {}",
        confidence * 100.0,
        confidence_bar(confidence)
    );
    println!();
}

/// Generate confidence bar
fn confidence_bar(confidence: f32) -> String {
    let filled = ((confidence * 10.0) as usize).min(10);
    let empty = 10 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Get type description for MBTI types
fn get_type_description(mbti_type: &str) -> &'static str {
    match mbti_type {
        "INTJ" => {
            "The Architect: Strategic, independent thinkers with a plan.\n\
             Natural leaders in innovation and problem-solving."
        }
        "INTP" => {
            "The Logician: Innovative inventors with unquenchable thirst.\n\
             for knowledge. Love theoretical and abstract thinking."
        }
        "ENTJ" => {
            "The Commander: Bold, imaginative, and strong-willed leaders.\n\
             Always find a way or make one."
        }
        "ENTP" => {
            "The Debater: Smart and curious thinkers who cannot resist\n\
             an intellectual challenge."
        }
        "INFJ" => {
            "The Advocate: Quiet and mystical, yet very inspiring and\n\
             idealistic. Deep thinkers with strong principles."
        }
        "INFP" => {
            "The Mediator: Poetic, kind, and altruistic. Always eager\n\
             to help a good cause and find meaning."
        }
        "ENFJ" => {
            "The Protagonist: Charismatic and inspiring leaders who\n\
             captivate their audience."
        }
        "ENFP" => {
            "The Campaigner: Enthusiastic, creative, and sociable free\n\
             spirits who always find a reason to smile."
        }
        "ISTJ" => {
            "The Logistician: Practical and fact-minded individuals.\n\
             Reliability cannot be doubted."
        }
        "ISFJ" => {
            "The Defender: Very dedicated and warm protectors, always\n\
             ready to defend loved ones."
        }
        "ESTJ" => {
            "The Executive: Excellent administrators, managing things\n\
             and people effectively."
        }
        "ESFJ" => {
            "The Consul: Caring, social, and popular. Always eager to\n\
             help and bring people together."
        }
        "ISTP" => {
            "The Virtuoso: Bold and practical experimenters, masters\n\
             of all kinds of tools."
        }
        "ISFP" => {
            "The Adventurer: Flexible and charming artists, always\n\
             ready to explore and experience."
        }
        "ESTP" => {
            "The Entrepreneur: Smart, energetic, and perceptive. Living\n\
             on the edge and loving it."
        }
        "ESFP" => {
            "The Entertainer: Spontaneous, energetic, and enthusiastic\n\
             entertainers. Life is never boring around them."
        }
        _ => "Unknown type: Analysis based on behavioral patterns.",
    }
}
