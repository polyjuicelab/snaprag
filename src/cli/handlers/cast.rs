//! Cast-related command handlers

use crate::cli::output::print_error;
use crate::cli::output::print_info;
use crate::cli::output::print_warning;
use crate::cli::output::truncate_str;
use crate::AppConfig;
use crate::Result;
use crate::SnapRag;

/// Handle cast search command
///
/// # Panics
/// Never panics - unwrap is only called after checking Option is Some
pub async fn handle_cast_search(
    snaprag: &SnapRag,
    query: String,
    limit: usize,
    threshold: f32,
    detailed: bool,
) -> Result<()> {
    use crate::embeddings::EmbeddingService;

    print_info(&format!("🔍 Searching casts: \"{query}\""));

    // Check if we have any cast embeddings
    let embed_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cast_embeddings")
        .fetch_one(snaprag.database().pool())
        .await?;

    if embed_count == 0 {
        print_warning("⚠️  No cast embeddings found. Please run:");
        println!("   snaprag embeddings backfill-casts");
        return Ok(());
    }

    // Generate query embedding (create new service instance)
    let config = AppConfig::load()?;
    let embedding_service = EmbeddingService::new(&config)?;
    let query_embedding = embedding_service.generate(&query).await?;

    // Search casts
    let results = snaprag
        .database()
        .semantic_search_casts(query_embedding, limit as i64, Some(threshold))
        .await?;

    if results.is_empty() {
        print_warning(&format!(
            "No casts found matching '{query}' (threshold: {threshold:.2})"
        ));
        return Ok(());
    }

    println!("\n📝 Found {} matching casts:\n", results.len());
    println!("{}", "─".repeat(100));

    for (idx, result) in results.iter().enumerate() {
        // Get author profile
        let author = snaprag.database().get_user_profile(result.fid).await?;
        let author_display = if let Some(profile) = author {
            if let Some(username) = profile.username {
                format!("@{username}")
            } else if let Some(display_name) = profile.display_name {
                display_name
            } else {
                format!("FID {}", result.fid)
            }
        } else {
            format!("FID {}", result.fid)
        };

        // Format timestamp
        let timestamp_str = chrono::DateTime::from_timestamp(result.timestamp, 0).map_or_else(
            || "Unknown".to_string(),
            |dt| dt.format("%Y-%m-%d %H:%M").to_string(),
        );

        println!(
            "{}. {} | {} | Similarity: {:.2}%",
            idx + 1,
            author_display,
            timestamp_str,
            result.similarity * 100.0
        );

        // Show cast text (truncate if needed)
        let display_text = if !detailed && result.text.chars().count() > 200 {
            truncate_str(&result.text, 200)
        } else {
            result.text.clone()
        };
        println!("   {display_text}");

        if detailed {
            println!("   Hash: {}", hex::encode(&result.message_hash));
            if let Some(parent_hash) = &result.parent_hash {
                println!("   (Reply to: {})", hex::encode(parent_hash));
            }
        }
        println!();
    }

    println!("{}", "─".repeat(100));
    println!("💡 Tip: Use --threshold to adjust sensitivity, --detailed for full info");

    Ok(())
}

/// Handle cast recent command
///
/// # Panics
/// Never panics - unwrap is only called after checking Option is Some
pub async fn handle_cast_recent(snaprag: &SnapRag, fid: i64, limit: usize) -> Result<()> {
    print_info(&format!("📝 Recent casts by FID {fid}"));

    // Get profile
    let profile = snaprag.database().get_user_profile(fid).await?;
    if profile.is_none() {
        print_error(&format!("❌ Profile not found for FID {fid}"));
        return Ok(());
    }

    let profile = profile.unwrap();
    println!("\n👤 Author:");
    if let Some(username) = &profile.username {
        println!("  @{username}");
    } else if let Some(display_name) = &profile.display_name {
        println!("  {display_name}");
    } else {
        println!("  FID {fid}");
    }
    println!();

    // Get casts
    let casts = snaprag
        .database()
        .get_casts_by_fid(fid, Some(limit as i64), Some(0))
        .await?;

    if casts.is_empty() {
        print_warning("No casts found for this user");
        return Ok(());
    }

    println!("📅 Recent Casts ({} total):", casts.len());
    println!("{}", "─".repeat(100));

    for (idx, cast) in casts.iter().enumerate() {
        let timestamp_str = chrono::DateTime::from_timestamp(cast.timestamp, 0).map_or_else(
            || "Unknown".to_string(),
            |dt| dt.format("%Y-%m-%d %H:%M").to_string(),
        );

        println!("{}. {}", idx + 1, timestamp_str);
        if let Some(text) = &cast.text {
            println!("   {text}");
        } else {
            println!("   (No text content)");
        }

        if cast.parent_hash.is_some() {
            println!("   ↳ Reply");
        }
        println!();
    }

    println!("{}", "─".repeat(100));

    Ok(())
}

/// Handle cast thread command
///
/// # Panics
/// Never panics - unwrap is only called after checking Option is Some
pub async fn handle_cast_thread(snaprag: &SnapRag, hash: String, depth: usize) -> Result<()> {
    print_info(&format!("🧵 Loading cast thread for {}...", &hash[..12]));

    let message_hash = hex::decode(&hash)
        .map_err(|_| crate::SnapRagError::Custom("Invalid hash format".to_string()))?;

    // Get the full thread
    let thread = snaprag
        .database()
        .get_cast_thread(message_hash, depth)
        .await?;

    if thread.root.is_none() {
        print_error(&format!("❌ Cast not found: {hash}"));
        return Ok(());
    }

    let root_cast = thread.root.as_ref().unwrap();

    println!("\n{}", "═".repeat(100));

    // Show parent chain if any
    if !thread.parents.is_empty() {
        println!("⬆️  Parent Context ({} levels):\n", thread.parents.len());

        for (idx, parent) in thread.parents.iter().enumerate() {
            let indent = "  ".repeat(idx);
            let author = snaprag.database().get_user_profile(parent.fid).await?;
            let author_name = if let Some(p) = author {
                p.username
                    .or(p.display_name)
                    .unwrap_or_else(|| format!("FID {}", parent.fid))
            } else {
                format!("FID {}", parent.fid)
            };

            println!("{indent}📝 {author_name}");
            if let Some(text) = &parent.text {
                let display_text = truncate_str(text, 100);
                println!("{indent}   {display_text}");
            }
            println!("{indent}   ↓");
        }
    }

    // Show the target cast
    let indent = "  ".repeat(thread.parents.len());
    let author = snaprag.database().get_user_profile(root_cast.fid).await?;
    let author_name = if let Some(p) = author {
        p.username
            .or(p.display_name)
            .unwrap_or_else(|| format!("FID {}", root_cast.fid))
    } else {
        format!("FID {}", root_cast.fid)
    };

    let timestamp_str = chrono::DateTime::from_timestamp(root_cast.timestamp, 0).map_or_else(
        || "Unknown".to_string(),
        |dt| dt.format("%Y-%m-%d %H:%M").to_string(),
    );

    println!("\n{indent}🎯 {author_name} | {timestamp_str}");
    if let Some(text) = &root_cast.text {
        println!("{indent}   {text}");
    }
    println!("{}   Hash: {}", indent, &hash[..16]);

    // Show replies if any
    if !thread.children.is_empty() {
        println!("\n⬇️  Replies ({}):\n", thread.children.len());

        for (idx, reply) in thread.children.iter().enumerate() {
            let author = snaprag.database().get_user_profile(reply.fid).await?;
            let author_name = if let Some(p) = author {
                p.username
                    .or(p.display_name)
                    .unwrap_or_else(|| format!("FID {}", reply.fid))
            } else {
                format!("FID {}", reply.fid)
            };

            println!("{}. ↳ {}", idx + 1, author_name);
            if let Some(text) = &reply.text {
                let display_text = truncate_str(text, 100);
                println!("      {display_text}");
            }
            println!();
        }
    }

    println!("{}", "═".repeat(100));
    println!(
        "\n📊 Thread Summary: {} parent(s), 1 target, {} reply/replies",
        thread.parents.len(),
        thread.children.len()
    );

    Ok(())
}
