//! Real RAG integration tests (no mocks)
//!
//! These tests validate the complete RAG pipeline:
//! 1. Data ingestion from real blocks
//! 2. Embedding generation for profiles and casts
//! 3. Semantic search and retrieval
//! 4. Context assembly
//! 5. LLM query execution
//!
//! Requirements:
//! - Database must be accessible
//! - Embedding service (OpenAI/Ollama) must be configured
//! - LLM service must be configured
//! - Test data should exist (run sync first)

use std::sync::Arc;

use crate::config::AppConfig;
use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::errors::Result;
use crate::llm::LlmService;
use crate::rag::CastContextAssembler;
use crate::rag::CastRetriever;
use crate::rag::ContextAssembler;
use crate::rag::Retriever;

#[tokio::test]
#[ignore = "Requires external services (DB, embeddings, LLM)"]
async fn test_profile_rag_pipeline() -> Result<()> {
    // Load config
    let config = crate::tests::load_test_config()?;

    // Initialize services
    let db = Arc::new(Database::from_config(&config).await?);
    let embedding_service = Arc::new(EmbeddingService::new(&config)?);
    let llm_service = Arc::new(LlmService::new(&config)?);
    let retriever = Retriever::new(Arc::clone(&db), Arc::clone(&embedding_service));
    let context_assembler = ContextAssembler::new(4096);

    // Test query
    let query = "Tell me about users interested in crypto and Web3";

    // Step 1: Verify we have profile embeddings
    let embed_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM profile_embeddings")
        .fetch_one(db.pool())
        .await?;

    assert!(
        embed_count > 0,
        "No profile embeddings found. Run: snaprag embeddings backfill"
    );

    // Step 2: Semantic search for relevant profiles
    let search_results = retriever.semantic_search(query, 5, None).await?;
    assert!(
        !search_results.is_empty(),
        "Semantic search returned no results"
    );
    assert!(
        search_results.len() <= 5,
        "Search returned more results than limit"
    );

    // Step 3: Verify similarity scores are reasonable
    for result in &search_results {
        assert!(
            result.score > 0.0 && result.score <= 1.0,
            "Invalid similarity score: {}",
            result.score
        );
    }

    // Step 4: Assemble context from search results
    let context = context_assembler.assemble(&search_results);
    assert!(!context.is_empty(), "Context assembly failed");
    assert!(
        context.len() < 5000,
        "Context too long: {} chars",
        context.len()
    );

    // Step 5: Build RAG prompt
    let prompt = format!(
        "Based on the following user profiles:\n\n{context}\n\nAnswer this question: {query}\n\nProvide a concise answer based only on the information above."
    );

    // Step 6: Query LLM
    let response = (*llm_service)
        .generate_with_params(&prompt, 0.7, 500)
        .await?;
    assert!(!response.is_empty(), "LLM response is empty");
    assert!(
        response.len() > 20,
        "LLM response too short: {}",
        response.len()
    );

    println!("\n✅ Profile RAG Pipeline Test:");
    println!("   Query: {query}");
    println!("   Results: {} profiles", search_results.len());
    println!("   Context size: {} chars", context.len());
    println!("   Response length: {} chars", response.len());
    println!(
        "   Sample response: {}...",
        &response[..response.len().min(100)]
    );

    Ok(())
}

#[tokio::test]
#[ignore = "Requires external services (DB, embeddings, LLM)"]
async fn test_cast_rag_pipeline() -> Result<()> {
    // Load config
    let config = crate::tests::load_test_config()?;

    // Initialize services
    let db = Arc::new(Database::from_config(&config).await?);
    let embedding_service = Arc::new(EmbeddingService::new(&config)?);
    let llm_service = Arc::new(LlmService::new(&config)?);
    let cast_retriever = CastRetriever::new(Arc::clone(&db), Arc::clone(&embedding_service));
    let context_assembler = CastContextAssembler::new(4096);

    // Test query
    let query = "What are people saying about Farcaster frames?";

    // Step 1: Verify we have cast embeddings
    let embed_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cast_embeddings")
        .fetch_one(db.pool())
        .await?;

    assert!(
        embed_count > 0,
        "No cast embeddings found. Run: snaprag embeddings backfill-casts"
    );

    // Step 2: Semantic search for relevant casts
    let search_results = cast_retriever.semantic_search(query, 10, None).await?;
    assert!(
        !search_results.is_empty(),
        "Semantic search returned no results"
    );
    assert!(
        search_results.len() <= 10,
        "Search returned more results than limit"
    );

    // Step 3: Verify similarity scores and engagement metrics
    for result in &search_results {
        assert!(
            result.similarity > 0.0 && result.similarity <= 1.0,
            "Invalid similarity score: {}",
            result.similarity
        );
        assert!(
            result.reply_count.unwrap_or(0) >= 0,
            "Invalid reply count: {:?}",
            result.reply_count
        );
        assert!(
            result.reaction_count.unwrap_or(0) >= 0,
            "Invalid reaction count: {:?}",
            result.reaction_count
        );
    }

    // Step 4: Assemble context with author information
    let context = context_assembler
        .assemble_with_authors(&search_results, &db)
        .await?;
    assert!(!context.is_empty(), "Context assembly failed");

    // Step 5: Build RAG prompt
    let prompt = format!(
        "Based on the following casts:\n\n{context}\n\nAnswer this question: {query}\n\nProvide a concise summary based only on the information above."
    );

    // Step 6: Query LLM
    let response = (*llm_service)
        .generate_with_params(&prompt, 0.7, 500)
        .await?;
    assert!(!response.is_empty(), "LLM response is empty");
    assert!(
        response.len() > 20,
        "LLM response too short: {}",
        response.len()
    );

    println!("\n✅ Cast RAG Pipeline Test:");
    println!("   Query: {query}");
    println!("   Results: {} casts", search_results.len());
    println!(
        "   Total engagement: {} replies, {} reactions",
        search_results
            .iter()
            .map(|r| r.reply_count.unwrap_or(0))
            .sum::<i64>(),
        search_results
            .iter()
            .map(|r| r.reaction_count.unwrap_or(0))
            .sum::<i64>()
    );
    println!("   Context size: {} chars", context.len());
    println!("   Response length: {} chars", response.len());
    println!(
        "   Sample response: {}...",
        &response[..response.len().min(100)]
    );

    Ok(())
}

#[tokio::test]
#[ignore = "Requires external services (DB, embeddings, LLM)"]
async fn test_hybrid_search_quality() -> Result<()> {
    // Load config
    let config = crate::tests::load_test_config()?;

    // Initialize services
    let db = Arc::new(Database::from_config(&config).await?);
    let embedding_service = Arc::new(EmbeddingService::new(&config)?);
    let cast_retriever = CastRetriever::new(Arc::clone(&db), Arc::clone(&embedding_service));

    // Test query with specific keyword
    let query = "Farcaster protocol";

    // Step 1: Semantic search
    let semantic_results = cast_retriever.semantic_search(query, 10, None).await?;

    // Step 2: Keyword search
    let keyword_results = cast_retriever.keyword_search(query, 10).await?;

    // Step 3: Hybrid search (should combine both)
    let hybrid_results = cast_retriever.hybrid_search(query, 10, None).await?;

    // Verify hybrid search quality
    assert!(
        !hybrid_results.is_empty(),
        "Hybrid search returned no results"
    );

    // Hybrid should ideally have results from both methods
    // Check if hybrid results have diverse similarity scores (indicates RRF fusion)
    let similarities: Vec<f32> = hybrid_results.iter().map(|r| r.similarity).collect();
    let unique_similarities: std::collections::HashSet<_> =
        similarities.iter().map(|&s| (s * 100.0) as i32).collect();

    assert!(
        unique_similarities.len() >= 2,
        "Hybrid search results lack diversity (may not be fusing properly)"
    );

    println!("\n✅ Hybrid Search Quality Test:");
    println!("   Query: {query}");
    println!("   Semantic results: {}", semantic_results.len());
    println!("   Keyword results: {}", keyword_results.len());
    println!("   Hybrid results: {}", hybrid_results.len());
    println!(
        "   Similarity range: {:.2} - {:.2}",
        similarities.iter().copied().fold(f32::INFINITY, f32::min),
        similarities
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max)
    );

    Ok(())
}

#[tokio::test]
#[ignore = "Requires external services (DB, embeddings, LLM)"]
async fn test_retrieval_consistency() -> Result<()> {
    // This test verifies that retrieval is deterministic and consistent
    let config = crate::tests::load_test_config()?;
    let db = Arc::new(Database::from_config(&config).await?);
    let embedding_service = Arc::new(EmbeddingService::new(&config)?);
    let retriever = Retriever::new(Arc::clone(&db), Arc::clone(&embedding_service));

    let query = "blockchain developers";

    // Run search multiple times
    let results1 = retriever.semantic_search(query, 5, None).await?;
    let results2 = retriever.semantic_search(query, 5, None).await?;

    // Results should be identical (same order, same scores)
    assert_eq!(results1.len(), results2.len(), "Inconsistent result count");

    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.profile.fid, r2.profile.fid, "Inconsistent FID ordering");
        assert_eq!(r1.score, r2.score, "Inconsistent scores");
    }

    println!("\n✅ Retrieval Consistency Test:");
    println!("   Query: {query}");
    println!(
        "   Results: {} profiles (consistent across runs)",
        results1.len()
    );

    Ok(())
}

#[tokio::test]
#[ignore = "Requires external services"]
async fn test_cast_thread_retrieval() -> Result<()> {
    // Test retrieving and assembling cast threads
    let config = crate::tests::load_test_config()?;
    let db = Arc::new(Database::from_config(&config).await?);

    // Find a cast with replies
    let cast_with_replies: Option<(Vec<u8>, i64)> = sqlx::query_as(
        r"
        SELECT message_hash, 
               (SELECT COUNT(*) FROM casts WHERE parent_hash = c.message_hash) as reply_count
        FROM casts c
        WHERE (SELECT COUNT(*) FROM casts WHERE parent_hash = c.message_hash) > 0
        LIMIT 1
        ",
    )
    .fetch_optional(db.pool())
    .await?;

    if let Some((hash, reply_count)) = cast_with_replies {
        // Get thread
        let thread = db.get_cast_thread(hash.clone(), 5).await?;

        assert!(thread.root.is_some(), "Thread root should exist");
        assert_eq!(
            thread.root.as_ref().unwrap().message_hash,
            hash,
            "Thread root mismatch"
        );

        assert!(
            !thread.children.is_empty(),
            "Thread has no children despite reply_count > 0"
        );

        assert!(
            thread.children.len() as i64 <= reply_count,
            "More children retrieved than expected"
        );

        println!("\n✅ Cast Thread Retrieval Test:");
        println!("   Root hash: {}", hex::encode(&hash));
        println!("   Parent chain: {} casts", thread.parents.len());
        println!("   Children: {} casts", thread.children.len());
        println!(
            "   Total thread size: {} casts",
            1 + thread.parents.len() + thread.children.len()
        );
    } else {
        println!("\n⚠️  No casts with replies found, skipping thread test");
    }

    Ok(())
}
