//! Binary to recover missing `username_proofs` from snapchain API
//!
//! Usage: cargo run --bin `recover_username_proofs` [--fid <fid>] [--batch]
//!
//! This script:
//! 1. Identifies FIDs that have `user_profiles` but missing `username_proofs`
//! 2. Calls snapchain API `GetUserNameProofsByFid` for each FID
//! 3. Inserts recovered `username_proofs` into database
#![allow(
    clippy::items_after_statements,
    clippy::too_many_lines,
    clippy::significant_drop_tightening,
    clippy::uninlined_format_args
)]

use snaprag::database::Database;
use snaprag::models::ShardBlockInfo;
use snaprag::models::UsernameType;
use snaprag::sync::client::SnapchainClient;
use snaprag::AppConfig;
use snaprag::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔍 UsernameProof Recovery Tool");
    println!("================================\n");

    // Load config
    let config = AppConfig::load()?;
    let client = SnapchainClient::from_config(&config).await?;
    let database = Database::from_config(&config).await?;

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let fid_arg = args
        .iter()
        .position(|a| a == "--fid")
        .and_then(|i| args.get(i + 1));
    let batch_mode = args.contains(&"--batch".to_string());

    if let Some(fid_str) = fid_arg {
        // Single FID mode
        let fid: u64 = fid_str.parse().expect("Invalid FID");
        recover_fid(&client, &database, fid).await?;
    } else if batch_mode {
        // Batch mode: process FIDs in batches of 1000, gradually expanding to full dataset
        println!("📊 Batch mode: Processing FIDs in batches of 1000...\n");

        const BATCH_SIZE: i64 = 1000;
        let mut last_fid: Option<i64> = None;
        let mut batch_number = 0;
        let mut total_processed = 0;
        let mut success_count = 0;
        let mut error_count = 0;
        let mut skipped_count = 0;

        loop {
            batch_number += 1;
            println!(
                "🔄 Processing batch {} (starting from FID {:?})...",
                batch_number, last_fid
            );

            // Query next batch of missing FIDs
            let missing_fids: Vec<i64> = if let Some(last) = last_fid {
                sqlx::query_scalar::<_, i64>(
                    r"
                    SELECT DISTINCT up.fid
                    FROM user_profiles up
                    LEFT JOIN username_proofs un ON up.fid = un.fid
                    WHERE un.fid IS NULL
                      AND up.username IS NOT NULL
                      AND up.fid > $1
                    ORDER BY up.fid
                    LIMIT $2
                    ",
                )
                .bind(last)
                .bind(BATCH_SIZE)
                .fetch_all(database.pool())
                .await?
            } else {
                sqlx::query_scalar::<_, i64>(
                    r"
                    SELECT DISTINCT up.fid
                    FROM user_profiles up
                    LEFT JOIN username_proofs un ON up.fid = un.fid
                    WHERE un.fid IS NULL
                      AND up.username IS NOT NULL
                    ORDER BY up.fid
                    LIMIT $1
                    ",
                )
                .bind(BATCH_SIZE)
                .fetch_all(database.pool())
                .await?
            };

            if missing_fids.is_empty() {
                println!("✅ No more FIDs to process. All batches completed!\n");
                break;
            }

            println!(
                "  Found {} FIDs in this batch (FID range: {} - {})",
                missing_fids.len(),
                missing_fids.first().unwrap(),
                missing_fids.last().unwrap()
            );

            // Process this batch
            let mut batch_success = 0;
            let mut batch_error = 0;
            let mut batch_skipped = 0;

            for (idx, fid) in missing_fids.iter().enumerate() {
                if (idx + 1) % 100 == 0 {
                    println!(
                        "    Batch {} progress: {}/{} ({} success, {} errors, {} skipped)",
                        batch_number,
                        idx + 1,
                        missing_fids.len(),
                        batch_success,
                        batch_error,
                        batch_skipped
                    );
                }

                let fid_u64 = u64::try_from(*fid).unwrap_or(0);
                match recover_fid(&client, &database, fid_u64).await {
                    Ok(true) => {
                        success_count += 1;
                        batch_success += 1;
                    }
                    Ok(false) => {
                        skipped_count += 1;
                        batch_skipped += 1;
                    }
                    Err(e) => {
                        error_count += 1;
                        batch_error += 1;
                        if batch_error <= 5 {
                            eprintln!("    ⚠️  Error recovering FID {fid}: {e}");
                        }
                    }
                }

                // Rate limiting: small delay to avoid overwhelming the API
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }

            total_processed += missing_fids.len();
            last_fid = missing_fids.last().copied();

            println!(
                "  ✅ Batch {} completed: {} success, {} errors, {} skipped",
                batch_number, batch_success, batch_error, batch_skipped
            );
            println!(
                "  📊 Cumulative totals: {} processed, {} success, {} errors, {} skipped\n",
                total_processed, success_count, error_count, skipped_count
            );

            // If this batch had fewer than BATCH_SIZE FIDs, we've reached the end
            if missing_fids.len() < usize::try_from(BATCH_SIZE).unwrap_or(usize::MAX) {
                println!("✅ Reached end of missing FIDs. All batches completed!\n");
                break;
            }
        }

        println!("📋 Final Recovery Summary:");
        println!("  Total batches processed: {batch_number}");
        println!("  Total FIDs processed: {total_processed}");
        println!("  Successfully recovered: {success_count}");
        println!("  Skipped (already exists or no proofs): {skipped_count}");
        println!("  Errors: {error_count}");
        println!(
            "  Success rate: {:.1}%",
            if total_processed > 0 {
                let total_processed_f64 =
                    f64::from(u32::try_from(total_processed).unwrap_or(u32::MAX));
                (f64::from(success_count) / total_processed_f64) * 100.0
            } else {
                0.0
            }
        );
    } else {
        println!("Usage:");
        println!("  Single FID: cargo run --bin recover_username_proofs --fid <fid>");
        println!("  Batch mode: cargo run --bin recover_username_proofs --batch");
    }

    Ok(())
}

async fn recover_fid(client: &SnapchainClient, database: &Database, fid: u64) -> Result<bool> {
    // Check if we already have username_proofs for this FID
    let existing_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM username_proofs WHERE fid = $1")
            .bind(i64::try_from(fid).unwrap_or(i64::MAX))
            .fetch_one(database.pool())
            .await?;

    if existing_count > 0 {
        // Silently skip - don't print for every FID in batch mode
        return Ok(false);
    }

    // Get username_proofs from snapchain API
    let proofs = match client.get_username_proofs_by_fid(fid).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("  ❌ Failed to get username_proofs for FID {fid}: {e}");
            return Err(e);
        }
    };

    if proofs.is_empty() {
        // Silently skip - don't print for every FID in batch mode
        return Ok(false);
    }

    // Only print for first few or when verbose
    if fid <= 10 || fid.is_multiple_of(1000) {
        println!(
            "  ✅ FID {}: Found {} username_proof(s) in snapchain",
            fid,
            proofs.len()
        );
    }

    // Insert each proof into database
    for proof in &proofs {
        let username = String::from_utf8_lossy(&proof.name).to_string();
        let username_type = match proof.r#type {
            1 => UsernameType::Fname,
            2 => UsernameType::EnsL1,
            3 => UsernameType::Basename,
            _ => {
                eprintln!("    ⚠️  Unknown username_type: {}, skipping", proof.r#type);
                continue;
            }
        };

        // Use upsert to handle conflicts
        match database
            .upsert_username_proof(
                i64::try_from(proof.fid).unwrap_or(i64::MAX),
                username.clone(),
                username_type,
                proof.owner.clone(),
                proof.signature.clone(),
                i64::try_from(proof.timestamp).unwrap_or(i64::MAX),
            )
            .await
        {
            Ok(_) => {
                // Only print for first few FIDs or when verbose
                if fid <= 10 || fid.is_multiple_of(1000) {
                    println!("    ✅ Inserted: {username} (type: {username_type:?})");
                }
            }
            Err(e) => {
                // Only show errors for first few FIDs
                if fid <= 10 {
                    eprintln!("    ❌ Failed to insert {username}: {e}");
                }
            }
        }
    }

    Ok(true)
}
