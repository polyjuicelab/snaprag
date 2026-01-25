//! RAG query example
//!
//! Run with: cargo run --example `rag_query`

use snaprag::AppConfig;
use snaprag::RagQuery;
use snaprag::RetrievalMethod;
use snaprag::SnapRag;

#[allow(clippy::significant_drop_tightening)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::load()?;
    let snaprag = SnapRag::new(&config).await?;

    println!("🤖 RAG Query Example\n");

    // Create RAG service
    let rag_service = snaprag.create_rag_service().await?;

    // Simple query
    println!("Query: 'Find developers building on Farcaster'\n");
    let response = rag_service
        .query("Find developers building on Farcaster")
        .await?;

    println!("📝 Answer:");
    println!("{}\n", response.answer);

    println!("📚 Sources ({} profiles):", response.sources.len());
    for (i, source) in response.sources.iter().take(3).enumerate() {
        println!(
            "  {}. @{} (score: {:.2})",
            i + 1,
            source.profile.username.as_deref().unwrap_or("unknown"),
            source.score
        );
    }

    // Advanced query with custom options
    println!("\n\n--- Advanced Query ---\n");
    let query = RagQuery {
        question: "Who are the most active contributors to Farcaster ecosystem?".to_string(),
        retrieval_limit: 15,
        retrieval_method: RetrievalMethod::Hybrid,
        temperature: 0.6,
        max_tokens: 800,
    };

    let response = rag_service.query_with_options(query).await?;

    println!("📝 Answer:");
    println!("{}\n", response.answer);
    println!("📚 Used {} source profiles", response.sources.len());

    Ok(())
}
