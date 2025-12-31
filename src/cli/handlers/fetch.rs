//! Lazy loading (on-demand fetch) handlers

use std::sync::Arc;

// Import hex for message hash encoding
use hex;

use crate::cli::output::print_error;
use crate::cli::output::print_info;
use crate::cli::output::print_success;
use crate::cli::output::truncate_str;
use crate::database::Database;
use crate::AppConfig;
use crate::Result;
use crate::SnapRag;

pub async fn handle_fetch_user(
    config: &AppConfig,
    fid: u64,
    with_casts: bool,
    max_casts: usize,
    generate_embeddings: bool,
) -> Result<()> {
    use crate::sync::client::SnapchainClient;
    use crate::sync::lazy_loader::LazyLoader;

    let start_time = std::time::Instant::now();

    print_info(&format!("🔄 Fetching user {fid} on demand..."));

    // Create lazy loader
    tracing::debug!("⏱️  Connecting to database...");
    let db_start = std::time::Instant::now();
    let database = Arc::new(Database::from_config(config).await?);
    tracing::debug!("   Database connected in {:?}", db_start.elapsed());

    tracing::debug!("⏱️  Connecting to Snapchain...");
    let client_start = std::time::Instant::now();
    let snapchain_client = Arc::new(SnapchainClient::from_config(config).await?);
    tracing::debug!("   Snapchain connected in {:?}", client_start.elapsed());

    let lazy_loader = LazyLoader::new(database.clone(), snapchain_client);

    // Always fetch from Snapchain and update database
    let profile = lazy_loader
        .fetch_user_profile_force(fid)
        .await
        .map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to fetch user {fid} from Snapchain: {e}"))
        })?;

    println!("\n✅ Profile loaded successfully:");
    println!("   FID: {}", profile.fid);
    if let Some(username) = &profile.username {
        println!("   Username: @{username}");
    }
    if let Some(display_name) = &profile.display_name {
        println!("   Display Name: {display_name}");
    }
    if let Some(bio) = &profile.bio {
        println!("   Bio: {}", truncate_str(bio, 100));
    }

    // Fetch casts if requested
    if with_casts {
        print_info(&format!("🔄 Fetching casts for FID {fid}..."));
        let limit = if max_casts > 0 {
            Some(max_casts)
        } else {
            None // No limit
        };
        let casts = lazy_loader
            .get_user_casts_smart_with_limit(fid as i64, limit)
            .await?;
        println!("   ✅ Loaded {} casts", casts.len());

        if !casts.is_empty() {
            println!("\n📝 Recent casts:");
            for (idx, cast) in casts.iter().take(5).enumerate() {
                if let Some(text) = &cast.text {
                    println!("   {}. {}", idx + 1, truncate_str(text, 80));
                }
            }
            if casts.len() > 5 {
                println!("   ... and {} more", casts.len() - 5);
            }

            // Generate embeddings if requested
            if generate_embeddings {
                // First, check which casts don't have embeddings yet
                print_info("🔍 Checking for existing embeddings...");

                // Collect message hashes from casts with text
                let message_hashes: Vec<Vec<u8>> = casts
                    .iter()
                    .filter(|c| c.text.as_ref().is_some_and(|t| !t.trim().is_empty()))
                    .map(|c| c.message_hash.clone())
                    .collect();

                // Efficiently check which ones are missing embeddings
                let missing_hashes = database.get_missing_embeddings(&message_hashes).await?;

                let casts_without_embeddings: Vec<_> = casts
                    .iter()
                    .filter(|cast| missing_hashes.contains(&cast.message_hash))
                    .cloned()
                    .collect();

                let existing_count = message_hashes.len() - casts_without_embeddings.len();
                println!("   ✅ {existing_count} already have embeddings");

                if casts_without_embeddings.is_empty() {
                    println!("   ℹ️  All casts already have embeddings. Skipping generation.");
                } else {
                    print_info(&format!(
                        "🔮 Generating embeddings for {} casts...",
                        casts_without_embeddings.len()
                    ));

                    let embedding_service =
                        Arc::new(crate::embeddings::EmbeddingService::new(config)?);

                    let mut success = 0;
                    let mut skipped = 0;
                    let mut failed = 0;
                    let total = casts_without_embeddings.len();

                    for (idx, cast) in casts_without_embeddings.iter().enumerate() {
                        // Skip casts without text
                        let Some(ref text) = cast.text else {
                            skipped += 1;
                            continue;
                        };
                        if text.trim().is_empty() {
                            skipped += 1;
                            continue;
                        }

                        // Generate embedding
                        match embedding_service.generate(text).await {
                            Ok(embedding) => {
                                // Store in database
                                match database
                                    .store_cast_embedding(
                                        &cast.message_hash,
                                        cast.fid,
                                        text,
                                        &embedding,
                                    )
                                    .await
                                {
                                    Ok(()) => {
                                        success += 1;
                                    }
                                    Err(e) => {
                                        tracing::warn!("Failed to store embedding: {}", e);
                                        failed += 1;
                                    }
                                }
                            }
                            Err(e) => {
                                let hash_str = hex::encode(&cast.message_hash);
                                tracing::error!(
                                    "Failed to generate embedding for cast {}: {}",
                                    hash_str,
                                    e
                                );
                                failed += 1;
                            }
                        }

                        // Update progress bar
                        let processed = idx + 1;
                        let percentage = (processed as f64 / total as f64 * 100.0) as u32;
                        let bar_width = 40;
                        let filled = (processed as f64 / total as f64 * bar_width as f64) as usize;
                        let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);

                        print!(
                            "\r   Progress: [{bar}] {percentage}% ({processed}/{total}) - ✅ {success} ⏭ {skipped} ❌ {failed}"
                        );
                        std::io::Write::flush(&mut std::io::stdout()).ok();
                    }

                    println!();
                    println!(
                        "   ✅ Embeddings: {success} success, {skipped} skipped, {failed} failed"
                    );
                }
            }
        }
    }

    tracing::debug!("⏱️  Total time: {:?}", start_time.elapsed());
    print_success(&format!("✅ Successfully fetched FID {fid}"));
    Ok(())
}

/// Handle fetch users (batch) command
pub async fn handle_fetch_users(
    config: &AppConfig,
    fids_str: String,
    with_casts: bool,
    generate_embeddings: bool,
) -> Result<()> {
    use crate::sync::client::SnapchainClient;
    use crate::sync::lazy_loader::LazyLoader;

    // Parse FIDs
    let fids: Vec<u64> = fids_str
        .split(',')
        .filter_map(|s| s.trim().parse::<u64>().ok())
        .collect();

    if fids.is_empty() {
        print_error("No valid FIDs provided. Use format: 99,100,101");
        return Ok(());
    }

    print_info(&format!(
        "🔄 Batch fetching {} users on demand...",
        fids.len()
    ));

    // Create lazy loader
    let database = Arc::new(Database::from_config(config).await?);
    let snapchain_client = Arc::new(SnapchainClient::from_config(config).await?);
    let lazy_loader = LazyLoader::new(database.clone(), snapchain_client);

    let mut success_count = 0;
    let mut fail_count = 0;
    let mut total_casts = 0;

    for (idx, fid) in fids.iter().enumerate() {
        print_info(&format!(
            "[{}/{}] Fetching FID {}...",
            idx + 1,
            fids.len(),
            fid
        ));

        // Use smart queries that check database first
        let profile_result = lazy_loader.get_user_profile_smart(*fid as i64).await;
        let casts_result = if with_casts {
            lazy_loader.get_user_casts_smart(*fid as i64).await
        } else {
            Ok(Vec::new())
        };

        let result = match (profile_result, casts_result) {
            (Ok(Some(profile)), Ok(casts)) => Ok((profile, casts)),
            (Ok(None), _) => Err(crate::SnapRagError::Custom(format!("User {fid} not found"))),
            (Err(e), _) | (_, Err(e)) => Err(e),
        };

        match result {
            Ok((profile, casts)) => {
                success_count += 1;
                total_casts += casts.len();
                println!(
                    "   ✅ @{} loaded{}",
                    profile.username.as_deref().unwrap_or("unknown"),
                    if with_casts {
                        format!(" with {} casts", casts.len())
                    } else {
                        String::new()
                    }
                );
            }
            Err(e) => {
                fail_count += 1;
                println!("   ❌ Failed: {e}");
            }
        }

        // Small delay to avoid overwhelming the server
        if idx < fids.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    println!("\n📊 Batch fetch complete:");
    println!("   ✅ Success: {success_count}");
    if fail_count > 0 {
        println!("   ❌ Failed: {fail_count}");
    }
    if with_casts {
        println!("   📝 Total casts: {total_casts}");
    }

    Ok(())
}

/// Handle fetch popular users command
pub async fn handle_fetch_popular(
    config: &AppConfig,
    limit: usize,
    with_casts: bool,
    generate_embeddings: bool,
) -> Result<()> {
    use crate::sync::client::SnapchainClient;
    use crate::sync::lazy_loader::LazyLoader;

    print_info(&format!("🔄 Fetching top {limit} popular users..."));

    // Create lazy loader
    let database = Arc::new(Database::from_config(config).await?);
    let snapchain_client = Arc::new(SnapchainClient::from_config(config).await?);
    let lazy_loader = LazyLoader::new(database.clone(), snapchain_client);

    // Get popular FIDs from all tables (casts, links, reactions)
    let popular_fids = sqlx::query_scalar::<_, i64>(
        r"
        WITH all_activity AS (
            SELECT fid FROM casts
            UNION ALL
            SELECT fid FROM links WHERE event_type = 'add'
            UNION ALL
            SELECT fid FROM reactions WHERE event_type = 'add'
        )
        SELECT fid, COUNT(*) as activity_count
        FROM all_activity
        GROUP BY fid
        ORDER BY activity_count DESC
        LIMIT $1
        ",
    )
    .bind(limit as i64)
    .fetch_all(database.pool())
    .await?;

    println!("   Found {} popular FIDs\n", popular_fids.len());

    let mut success_count = 0;
    let mut total_casts = 0;

    for (idx, fid) in popular_fids.iter().enumerate() {
        print_info(&format!(
            "[{}/{}] Fetching FID {}...",
            idx + 1,
            popular_fids.len(),
            fid
        ));

        // Use smart queries that check database first
        let profile_result = lazy_loader.get_user_profile_smart(*fid).await;
        let casts_result = if with_casts {
            lazy_loader.get_user_casts_smart(*fid).await
        } else {
            Ok(Vec::new())
        };

        let result = match (profile_result, casts_result) {
            (Ok(Some(profile)), Ok(casts)) => Ok((profile, casts)),
            (Ok(None), _) => Err(crate::SnapRagError::Custom(format!("User {fid} not found"))),
            (Err(e), _) | (_, Err(e)) => Err(e),
        };

        match result {
            Ok((profile, casts)) => {
                success_count += 1;
                total_casts += casts.len();
                println!(
                    "   ✅ @{} loaded{}",
                    profile.username.as_deref().unwrap_or("unknown"),
                    if with_casts {
                        format!(" with {} casts", casts.len())
                    } else {
                        String::new()
                    }
                );
            }
            Err(e) => {
                println!("   ⚠️  Skipped: {e}");
            }
        }

        // Delay between requests
        if idx < popular_fids.len() - 1 {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        }
    }

    println!("\n📊 Preload complete:");
    println!("   ✅ Loaded: {}/{}", success_count, popular_fids.len());
    if with_casts {
        println!("   📝 Total casts: {total_casts}");
    }

    print_success("✅ Popular users preloaded!");
    Ok(())
}
