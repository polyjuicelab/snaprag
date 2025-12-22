//! Script to check username_proofs for a specific FID
use sqlx::PgPool;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fid = env::args()
        .nth(1)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or_else(|| {
            eprintln!("Usage: cargo run --bin check_username_proofs <fid>");
            std::process::exit(1);
        });

    // Get database URL from environment or use default
    let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgresql://postgres:postgres@localhost:5432/snaprag".to_string()
    });

    let pool = PgPool::connect(&database_url).await?;

    println!("🔍 Checking username_proofs for FID: {}\n", fid);

    // Check username_proofs
    let proofs = sqlx::query!(
        r#"
        SELECT 
            fid,
            username,
            username_type,
            timestamp,
            created_at,
            shard_id,
            block_height
        FROM username_proofs
        WHERE fid = $1
        ORDER BY timestamp DESC
        "#,
        fid
    )
    .fetch_all(&pool)
    .await?;

    if proofs.is_empty() {
        println!("❌ No username_proofs found for FID {}", fid);
    } else {
        println!("✅ Found {} username_proof(s):\n", proofs.len());
        for proof in &proofs {
            let username_type = match proof.username_type {
                1 => "FNAME",
                2 => "ENS",
                _ => "UNKNOWN",
            };
            println!("  Username: {}", proof.username);
            println!("  Type: {} ({})", username_type, proof.username_type);
            println!("  Timestamp: {}", proof.timestamp);
            println!("  Created at: {:?}", proof.created_at);
            if let Some(shard_id) = proof.shard_id {
                println!("  Shard ID: {}", shard_id);
            }
            if let Some(block_height) = proof.block_height {
                println!("  Block height: {}", block_height);
            }
            println!();
        }
    }

    // Also check user_profiles
    let profile = sqlx::query!(
        r#"
        SELECT 
            fid,
            username,
            display_name
        FROM user_profiles
        WHERE fid = $1
        "#,
        fid
    )
    .fetch_optional(&pool)
    .await?;

    if let Some(profile) = profile {
        println!("📋 User Profile:");
        println!("  FID: {}", profile.fid);
        println!("  Username: {:?}", profile.username);
        println!("  Display Name: {:?}", profile.display_name);
    } else {
        println!("❌ No user_profile found for FID {}", fid);
    }

    Ok(())
}

