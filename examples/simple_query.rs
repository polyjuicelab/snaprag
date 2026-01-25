//! Simple example: Query user profiles
//!
//! Run with: cargo run --example `simple_query`

use snaprag::AppConfig;
use snaprag::SnapRag;

#[allow(clippy::significant_drop_tightening)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = AppConfig::load()?;

    // Create SnapRAG instance
    let snaprag = SnapRag::new(&config).await?;

    println!("🔍 Searching for developers...\n");

    // Search for developers
    let profiles = snaprag.search_profiles("developer").await?;

    println!("Found {} profiles:", profiles.len());
    for (i, profile) in profiles.iter().take(5).enumerate() {
        println!(
            "  {}. @{} - {}",
            i + 1,
            profile.username.as_deref().unwrap_or("unknown"),
            profile.display_name.as_deref().unwrap_or("N/A")
        );

        if let Some(bio) = &profile.bio {
            let bio_preview = if bio.len() > 100 {
                format!("{}...", &bio[..100])
            } else {
                bio.clone()
            };
            println!("     {bio_preview}");
        }
        println!();
    }

    Ok(())
}
