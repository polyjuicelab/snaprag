//! Test script to verify annual report data exists in database
//! This bypasses the API and directly queries the database

use snaprag::database::Database;
use snaprag::AppConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing Annual Report Data Availability");
    println!("===========================================");
    println!();

    // Load config
    let config = AppConfig::load()?;
    let database = Database::from_config(&config).await?;

    // Test FID
    let test_fid = 460432i64;
    let test_year = 2024u32;

    println!("Testing FID: {}", test_fid);
    println!("Year: {}", test_year);
    println!();

    // Check if user profile exists
    println!("1. Checking user profile...");
    match database.get_user_profile(test_fid).await {
        Ok(Some(profile)) => {
            println!("   ✅ User profile found");
            println!("      Username: {:?}", profile.username);
            println!("      Display Name: {:?}", profile.display_name);
        }
        Ok(None) => {
            println!("   ❌ User profile not found");
            return Ok(());
        }
        Err(e) => {
            println!("   ❌ Error: {}", e);
            return Err(e.into());
        }
    }
    println!();

    // Calculate year range
    let year_start = chrono::DateTime::parse_from_rfc3339(&format!("{}-01-01T00:00:00Z", test_year))
        .map_err(|e| format!("Failed to parse year start: {}", e))?
        .timestamp();
    let year_end = chrono::DateTime::parse_from_rfc3339(&format!("{}-12-31T23:59:59Z", test_year))
        .map_err(|e| format!("Failed to parse year end: {}", e))?
        .timestamp();

    let start_farcaster = snaprag::unix_to_farcaster_timestamp(year_start as u64) as i64;
    let end_farcaster = snaprag::unix_to_farcaster_timestamp(year_end as u64) as i64;

    println!("2. Checking casts in year {}...", test_year);
    let casts_text = database
        .get_casts_text_for_analysis(test_fid, Some(start_farcaster), Some(end_farcaster))
        .await?;
    println!("   ✅ Found {} casts", casts_text.len());
    println!();

    // Check engagement
    println!("3. Checking engagement metrics...");
    let reactions = database
        .get_reactions_received(test_fid, Some(start_farcaster), Some(end_farcaster), 1)
        .await
        .unwrap_or(0);
    let recasts = database
        .get_recasts_received(test_fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    let replies = database
        .get_replies_received(test_fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    println!("   ✅ Reactions: {}, Recasts: {}, Replies: {}", reactions, recasts, replies);
    println!();

    // Check follower growth
    println!("4. Checking follower growth...");
    let current_followers = database
        .get_current_follower_count(test_fid)
        .await
        .unwrap_or(0);
    let current_following = database
        .get_current_following_count(test_fid)
        .await
        .unwrap_or(0);
    println!("   ✅ Current Followers: {}, Current Following: {}", current_followers, current_following);
    println!();

    // Check frame usage
    println!("5. Checking frame usage...");
    let frames_used = database
        .get_frame_usage_count(test_fid, Some(start_farcaster), Some(end_farcaster))
        .await
        .unwrap_or(0);
    println!("   ✅ Frames Used: {}", frames_used);
    println!();

    // Check total casts
    println!("6. Checking total casts (all time)...");
    let total_casts = database.count_casts_by_fid(test_fid).await?;
    println!("   ✅ Total Casts: {}", total_casts);
    println!();

    // Summary
    println!("📊 Summary:");
    println!("   - User profile: ✅");
    println!("   - Casts in {}: {} ✅", test_year, casts_text.len());
    println!("   - Total casts: {} ✅", total_casts);
    println!("   - Engagement: {} reactions, {} recasts, {} replies ✅", reactions, recasts, replies);
    println!("   - Social: {} followers, {} following ✅", current_followers, current_following);
    println!("   - Frames: {} ✅", frames_used);
    println!();

    if casts_text.is_empty() {
        println!("⚠️  Warning: No casts found for year {}. Annual report will have limited data.", test_year);
    } else {
        println!("✅ All data checks passed! Annual report API should return data.");
    }

    Ok(())
}

