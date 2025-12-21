//! Binary to recover missing username_proofs from snapchain API
//!
//! Usage: cargo run --bin recover_username_proofs [--fid <fid>] [--batch]
//!
//! This script:
//! 1. Identifies FIDs that have user_profiles but missing username_proofs
//! 2. Calls snapchain API GetUserNameProofsByFid for each FID
//! 3. Inserts recovered username_proofs into database

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
        // Batch mode: find all FIDs missing username_proofs
        println!("📊 Batch mode: Finding FIDs missing username_proofs...\n");

        let missing_fids: Vec<i64> = sqlx::query_scalar::<_, i64>(
            r"
            SELECT DISTINCT up.fid
            FROM user_profiles up
            LEFT JOIN username_proofs un ON up.fid = un.fid
            WHERE un.fid IS NULL
              AND up.username IS NOT NULL
            LIMIT 1000
            ",
        )
        .fetch_all(database.pool())
        .await?;

        println!("Found {} FIDs missing username_proofs", missing_fids.len());
        if missing_fids.is_empty() {
            println!("No FIDs need recovery. Exiting.");
            return Ok(());
        }

        println!("Starting recovery...\n");
        println!("⚠️  This may take a while. Progress will be shown every 100 FIDs.\n");

        let mut success_count = 0;
        let mut error_count = 0;
        let mut skipped_count = 0;
        let total = missing_fids.len();

        for (idx, fid) in missing_fids.iter().enumerate() {
            if (idx + 1) % 100 == 0 || idx == 0 {
                println!(
                    "Progress: {}/{} ({} success, {} errors, {} skipped)",
                    idx + 1,
                    total,
                    success_count,
                    error_count,
                    skipped_count
                );
            }

            match recover_fid(&client, &database, *fid as u64).await {
                Ok(true) => success_count += 1,
                Ok(false) => skipped_count += 1, // Already exists or no proofs found
                Err(e) => {
                    error_count += 1;
                    if error_count <= 10 {
                        // Only show first 10 errors to avoid spam
                        eprintln!("  ⚠️  Error recovering FID {}: {}", fid, e);
                    } else if error_count == 11 {
                        eprintln!("  ... (suppressing further error messages)");
                    }
                }
            }

            // Rate limiting: small delay to avoid overwhelming the API
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        println!("\n📋 Recovery Summary:");
        println!("  Total FIDs processed: {}", total);
        println!("  Successfully recovered: {}", success_count);
        println!("  Skipped (already exists or no proofs): {}", skipped_count);
        println!("  Errors: {}", error_count);
        println!(
            "  Success rate: {:.1}%",
            if total > 0 {
                (success_count as f64 / total as f64) * 100.0
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
            .bind(fid as i64)
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
            eprintln!("  ❌ Failed to get username_proofs for FID {}: {}", fid, e);
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
                proof.fid as i64,
                username.clone(),
                username_type,
                proof.owner.clone(),
                proof.signature.clone(),
                proof.timestamp as i64,
            )
            .await
        {
            Ok(_) => {
                // Only print for first few FIDs or when verbose
                if fid <= 10 || fid.is_multiple_of(1000) {
                    println!("    ✅ Inserted: {} (type: {:?})", username, username_type);
                }
            }
            Err(e) => {
                // Only show errors for first few FIDs
                if fid <= 10 {
                    eprintln!("    ❌ Failed to insert {}: {}", username, e);
                }
            }
        }
    }

    Ok(true)
}
