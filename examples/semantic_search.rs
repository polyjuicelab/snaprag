//! Semantic search example
//!
//! Run with: cargo run --example `semantic_search`

use snaprag::AppConfig;
use snaprag::SnapRag;

#[allow(clippy::significant_drop_tightening)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    let snaprag = SnapRag::new(&config).await?;

    println!("🧠 Semantic Search Example\n");

    // Profile semantic search
    println!("Searching profiles: 'AI and machine learning enthusiasts'");
    let profile_results = snaprag
        .semantic_search_profiles("AI and machine learning enthusiasts", 5, Some(0.7))
        .await?;

    println!("\n📊 Profile Results:");
    for result in &profile_results {
        println!(
            "  • @{} (score: {:.2})",
            result.profile.username.as_deref().unwrap_or("unknown"),
            result.score
        );
        if let Some(bio) = &result.profile.bio {
            println!("    {}", &bio[..bio.len().min(80)]);
        }
    }

    // Cast semantic search
    println!("\n\nSearching casts: 'discussions about Farcaster protocol'");
    let cast_results = snaprag
        .semantic_search_casts("discussions about Farcaster protocol", 5, Some(0.7))
        .await?;

    println!("\n💬 Cast Results:");
    for cast in &cast_results {
        println!(
            "  • FID {} (similarity: {:.1}%)",
            cast.fid,
            cast.similarity * 100.0
        );
        println!("    {}", &cast.text[..cast.text.len().min(100)]);
        let replies = cast.reply_count.unwrap_or(0);
        let reactions = cast.reaction_count.unwrap_or(0);
        println!("    📈 {replies} replies, {reactions} reactions\n");
    }

    Ok(())
}
