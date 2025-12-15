//! Cache management command handlers

use std::io::Write;

use crate::cli::commands::CacheCommands;
use crate::errors::Result;
use crate::SnapRag;

/// Handle cache management commands
pub async fn handle_cache_command(snaprag: &SnapRag, command: &CacheCommands) -> Result<()> {
    match command {
        CacheCommands::Clear { force } => handle_cache_clear(snaprag, *force).await,
        CacheCommands::Delete {
            fid,
            profile_only,
            social_only,
            mbti_only,
        } => handle_cache_delete(snaprag, *fid, *profile_only, *social_only, *mbti_only).await,
    }
}

/// Clear all cache entries
async fn handle_cache_clear(snaprag: &SnapRag, force: bool) -> Result<()> {
    tracing::info!("Preparing to clear all cache entries...");

    if !force {
        println!("\n⚠️  This will delete ALL cache entries:");
        println!("  - All profile caches");
        println!("  - All social analysis caches");
        println!("  - All MBTI analysis caches");
        println!("  - All cache proxy entries");
        println!("\n⚠️  This action cannot be undone!\n");

        print!("Continue? [y/N] ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("❌ Aborted");
            return Ok(());
        }
    }

    // Get cache service from SnapRag
    let cache_service = snaprag
        .cache_service
        .as_ref()
        .ok_or_else(|| crate::SnapRagError::Custom("Cache service not initialized".to_string()))?;

    println!("🗑️  Clearing all cache entries...");
    let deleted_count = cache_service.invalidate_all().await?;

    println!("✅ Successfully deleted {deleted_count} cache entries");
    Ok(())
}

/// Delete cache for a specific FID
async fn handle_cache_delete(
    snaprag: &SnapRag,
    fid: i64,
    profile_only: bool,
    social_only: bool,
    mbti_only: bool,
) -> Result<()> {
    // Get cache service from SnapRag
    let cache_service = snaprag
        .cache_service
        .as_ref()
        .ok_or_else(|| crate::SnapRagError::Custom("Cache service not initialized".to_string()))?;

    println!("🗑️  Deleting cache for FID {fid}...");

    if profile_only {
        cache_service.invalidate_profile(fid).await?;
        println!("✅ Deleted profile cache for FID {fid}");
    } else if social_only {
        cache_service.invalidate_social(fid).await?;
        println!("✅ Deleted social cache for FID {fid}");
    } else if mbti_only {
        cache_service.invalidate_mbti(fid).await?;
        println!("✅ Deleted MBTI cache for FID {fid}");
    } else {
        // Delete all caches for this FID
        cache_service.invalidate_user(fid).await?;
        println!("✅ Deleted all caches for FID {fid}");
    }

    Ok(())
}
