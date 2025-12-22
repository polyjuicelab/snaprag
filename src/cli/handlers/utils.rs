//! Utility command handlers

use std::fs::File;
use std::io::Write;

use crate::Result;
use crate::SnapRag;

/// Handle top user command - export top users by follower count to CSV
///
/// # Errors
/// Returns an error if database query fails or file I/O operations fail
pub async fn handle_top_user_command(snaprag: &SnapRag, limit: i64) -> Result<()> {
    use tracing::info;

    info!("🔍 Finding top {limit} users by follower count...");

    let start_time = std::time::Instant::now();
    let top_users = snaprag.database().get_top_users_by_followers(limit).await?;
    let duration = start_time.elapsed();

    info!("✅ Found {} users in {:?}", top_users.len(), duration);

    // Generate CSV filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("top_{limit}_users_{timestamp}.csv");

    // Create CSV file
    let mut file = File::create(&filename)?;

    // Write CSV header
    writeln!(file, "Rank,followers,username,fid")?;

    // Write data rows
    for (rank, (fid, follower_count, username)) in top_users.iter().enumerate() {
        let rank_num = rank + 1;
        let username_str = username.as_deref().unwrap_or("");
        writeln!(file, "{rank_num},{follower_count},{username_str},{fid}")?;
    }

    info!("💾 Results exported to: {filename}");
    println!("✅ Exported {} users to {filename}", top_users.len());

    Ok(())
}
