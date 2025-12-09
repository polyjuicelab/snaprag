//! Database index and autovacuum management handlers
//!
//! This module provides handlers for managing database indexes and autovacuum settings
//! during bulk synchronization operations. Disabling non-essential indexes and autovacuum
//! can significantly speed up bulk inserts (30-70% faster).

use std::io::Write;
use std::io::{
    self,
};

use crate::cli::commands::IndexCommands;
use crate::errors::Result;
use crate::SnapRag;

/// Handle index management commands
pub async fn handle_index_command(snaprag: &SnapRag, command: &IndexCommands) -> Result<()> {
    match command {
        IndexCommands::Unset { force } => handle_index_unset(snaprag, *force).await,
        IndexCommands::Set { force } => handle_index_set(snaprag, *force).await,
        IndexCommands::Status => handle_index_status(snaprag).await,
    }
}

/// Disable non-essential indexes and autovacuum for bulk operations
async fn handle_index_unset(snaprag: &SnapRag, force: bool) -> Result<()> {
    tracing::info!("Preparing to disable non-essential indexes and autovacuum...");

    // Show what will be done
    println!("\n⚠️  This will:");
    println!("  1. Drop non-essential indexes (idx_casts_fid, idx_user_profiles_username, etc.)");
    println!("  2. Keep vector indexes (no vector data during sync)");
    println!("  3. Disable autovacuum on all main tables");
    println!("  4. Speed up bulk inserts by 30-70%");
    println!("\n⚠️  You MUST run 'snaprag index set' after bulk sync completes!");
    println!("     Without indexes, queries will be VERY slow.\n");

    if !force {
        print!("Continue? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("❌ Aborted");
            return Ok(());
        }
    }

    let db = snaprag.database.pool();

    println!("\n🔨 Dropping non-essential indexes...");

    // Drop non-essential indexes (keep primary keys and unique constraints)
    let indexes_to_drop = vec![
        // Casts - basic indexes
        "idx_casts_fid",
        "idx_casts_timestamp",
        // Casts - optimization indexes
        "idx_casts_text_hash",
        "idx_casts_message_hash_desc",
        "idx_casts_text_hash_desc_composite",
        // Casts - pg_trgm trigram indexes
        "idx_casts_text_trgm",
        // Cast embeddings - basic indexes
        "idx_cast_embeddings_fid",
        "idx_cast_embeddings_message_hash",
        "idx_cast_embeddings_message_hash_btree",
        // Cast embeddings - pg_trgm indexes
        "idx_cast_embeddings_text_trgm",
        // Cast embedding chunks - basic indexes
        "idx_cast_embedding_chunks_fid",
        "idx_cast_embedding_chunks_message_hash",
        "idx_cast_embedding_chunks_strategy",
        // Cast embedding chunks - pg_trgm indexes
        "idx_cast_embedding_chunks_text_trgm",
        // Cast embedding aggregated - basic indexes
        "idx_cast_embedding_aggregated_fid",
        "idx_cast_embedding_aggregated_message_hash",
        // Note: Vector indexes (idx_cast_embedding_*_embedding_cosine) are NOT dropped
        // because they are not affected during sync (no vector data is written during sync)
        // User profiles
        "idx_user_profiles_username",
        "idx_user_profiles_display_name",
        // User profile changes - optimized username lookup index
        "idx_profile_changes_username_value",
        // User profile changes - pg_trgm indexes
        "idx_user_profile_changes_value_trgm",
        // User profile snapshots
        "idx_profile_snapshots_fid_timestamp",
        // Username proofs - pg_trgm indexes
        "idx_username_proofs_username_trgm",
        // Links
        "idx_links_source_fid",
        "idx_links_target_fid",
        "idx_links_timestamp",
        // Reactions
        "idx_reactions_fid",
        "idx_reactions_target_cast_hash",
        "idx_reactions_timestamp",
        "idx_reactions_engagement",
        "idx_reactions_shard_block",
        "idx_reactions_target_cast",
        "idx_reactions_target_fid",
        "idx_reactions_type",
        "idx_reactions_user_cast",
        // Verifications
        "idx_verifications_fid",
        "idx_verifications_timestamp",
        // User data
        "idx_user_data_fid",
        "idx_user_data_type",
        // Sync tracking indexes (⚠️ Warning: Removing these may slow down sync operations)
        "idx_processed_messages_hash",
        "idx_processed_shard_height",
        "idx_sync_progress_shard_id",
    ];

    for index_name in &indexes_to_drop {
        match sqlx::query(&format!("DROP INDEX IF EXISTS {index_name} CASCADE"))
            .execute(db)
            .await
        {
            Ok(_) => println!("  ✅ Dropped: {index_name}"),
            Err(e) => println!("  ⚠️  Failed to drop {index_name}: {e}"),
        }
    }

    println!("\n🛑 Disabling autovacuum...");

    // Disable autovacuum on all main tables
    let tables = vec![
        "casts",
        "links",
        "reactions",
        "verifications",
        "user_data",
        "username_proofs",
        "frame_actions",
        "onchain_events",
        "processed_messages",
    ];

    for table in &tables {
        match sqlx::query(&format!(
            "ALTER TABLE {table} SET (autovacuum_enabled = false)"
        ))
        .execute(db)
        .await
        {
            Ok(_) => println!("  ✅ Disabled autovacuum: {table}"),
            Err(e) => println!("  ⚠️  Failed for {table}: {e}"),
        }
    }

    println!("\n✅ Done! Bulk sync mode enabled.");
    println!("   Speed boost: +30-70% for inserts");
    println!("\n⚠️  Remember to run 'snaprag index set' after sync completes!");

    Ok(())
}

/// Re-enable indexes and autovacuum after bulk operations
async fn handle_index_set(snaprag: &SnapRag, force: bool) -> Result<()> {
    tracing::info!("Preparing to re-enable indexes and autovacuum...");

    println!("\n✅ This will:");
    println!("  1. Recreate all non-essential indexes (CONCURRENTLY, won't block writes)");
    println!("  2. Skip vector indexes (already exist, not affected during sync)");
    println!("  3. Re-enable autovacuum on all tables");
    println!("  4. Run VACUUM ANALYZE to optimize query performance");
    println!("\n⏱️  This may take 30-60 minutes for large datasets.\n");

    if !force {
        print!("Continue? [y/N] ");
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("❌ Aborted");
            return Ok(());
        }
    }

    let db = snaprag.database.pool();

    println!("\n🔨 Recreating indexes (CONCURRENTLY)...");

    // Recreate indexes with CONCURRENTLY (won't block writes)
    let indexes_to_create = vec![
        // Casts - basic indexes
        ("idx_casts_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_fid ON casts(fid)"),
        ("idx_casts_timestamp", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_timestamp ON casts(timestamp DESC)"),
        // Casts - optimization indexes
        ("idx_casts_text_hash", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_text_hash ON casts(message_hash) WHERE text IS NOT NULL AND length(text) > 0"),
        ("idx_casts_message_hash_desc", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_message_hash_desc ON casts(message_hash DESC) WHERE text IS NOT NULL AND length(text) > 0"),
        ("idx_casts_text_hash_desc_composite", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_text_hash_desc_composite ON casts(message_hash DESC, fid, timestamp) WHERE text IS NOT NULL AND length(text) > 0"),
        // Casts - pg_trgm trigram indexes
        ("idx_casts_text_trgm", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_casts_text_trgm ON casts USING gin(text gin_trgm_ops) WHERE text IS NOT NULL AND length(text) > 0"),
        // Cast embeddings - basic indexes
        ("idx_cast_embeddings_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embeddings_fid ON cast_embeddings(fid)"),
        ("idx_cast_embeddings_message_hash", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embeddings_message_hash ON cast_embeddings(message_hash)"),
        ("idx_cast_embeddings_message_hash_btree", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embeddings_message_hash_btree ON cast_embeddings USING btree(message_hash)"),
        // Cast embeddings - pg_trgm indexes
        ("idx_cast_embeddings_text_trgm", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embeddings_text_trgm ON cast_embeddings USING gin(text gin_trgm_ops) WHERE text IS NOT NULL AND length(text) > 0"),
        // Cast embedding chunks - basic indexes
        ("idx_cast_embedding_chunks_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_chunks_fid ON cast_embedding_chunks(fid)"),
        ("idx_cast_embedding_chunks_message_hash", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_chunks_message_hash ON cast_embedding_chunks(message_hash)"),
        ("idx_cast_embedding_chunks_strategy", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_chunks_strategy ON cast_embedding_chunks(chunk_strategy)"),
        // Cast embedding chunks - pg_trgm indexes
        ("idx_cast_embedding_chunks_text_trgm", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_chunks_text_trgm ON cast_embedding_chunks USING gin(chunk_text gin_trgm_ops) WHERE chunk_text IS NOT NULL AND length(chunk_text) > 0"),
        // Cast embedding aggregated - basic indexes
        ("idx_cast_embedding_aggregated_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_aggregated_fid ON cast_embedding_aggregated(fid)"),
        ("idx_cast_embedding_aggregated_message_hash", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_cast_embedding_aggregated_message_hash ON cast_embedding_aggregated(message_hash)"),
        // Note: Vector indexes (idx_cast_embedding_*_embedding_cosine) are NOT recreated here
        // because they don't need to be dropped during sync (no vector data during sync)
        // User profiles
        ("idx_user_profiles_username", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_profiles_username ON user_profiles(username)"),
        ("idx_user_profiles_display_name", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_profiles_display_name ON user_profiles(display_name)"),
        // User profile changes - optimized username lookup index
        ("idx_profile_changes_username_value", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_profile_changes_username_value ON user_profile_changes(field_value, fid, timestamp DESC) WHERE field_name = 'username'"),
        // User profile changes - pg_trgm indexes
        ("idx_user_profile_changes_value_trgm", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_profile_changes_value_trgm ON user_profile_changes USING gin(field_value gin_trgm_ops) WHERE field_value IS NOT NULL AND length(field_value) > 0"),
        // User profile snapshots
        ("idx_profile_snapshots_fid_timestamp", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_profile_snapshots_fid_timestamp ON user_profile_snapshots(fid, snapshot_timestamp DESC)"),
        // Username proofs - pg_trgm indexes
        ("idx_username_proofs_username_trgm", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_username_proofs_username_trgm ON username_proofs USING gin(username gin_trgm_ops) WHERE username IS NOT NULL AND length(username) > 0"),
        // Links
        ("idx_links_source_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_links_source_fid ON links(fid)"),
        ("idx_links_target_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_links_target_fid ON links(target_fid)"),
        ("idx_links_timestamp", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_links_timestamp ON links(timestamp DESC)"),
        // Reactions
        ("idx_reactions_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_fid ON reactions(fid)"),
        ("idx_reactions_target_cast_hash", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_target_cast_hash ON reactions(target_cast_hash)"),
        ("idx_reactions_timestamp", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_timestamp ON reactions(timestamp DESC)"),
        ("idx_reactions_engagement", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_engagement ON reactions(target_cast_hash, reaction_type)"),
        ("idx_reactions_shard_block", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_shard_block ON reactions(shard_id, block_height)"),
        ("idx_reactions_target_cast", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_target_cast ON reactions(target_cast_hash)"),
        ("idx_reactions_target_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_target_fid ON reactions(target_fid)"),
        ("idx_reactions_type", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_type ON reactions(reaction_type)"),
        ("idx_reactions_user_cast", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_reactions_user_cast ON reactions(fid, target_cast_hash)"),
        // Verifications
        ("idx_verifications_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_verifications_fid ON verifications(fid)"),
        ("idx_verifications_timestamp", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_verifications_timestamp ON verifications(timestamp DESC)"),
        // User data
        ("idx_user_data_fid", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_data_fid ON user_data(fid)"),
        ("idx_user_data_type", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_user_data_type ON user_data(data_type)"),
        // Sync tracking indexes (⚠️ Important: These are needed for sync operations)
        ("idx_processed_messages_hash", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_processed_messages_hash ON processed_messages(message_hash)"),
        ("idx_processed_shard_height", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_processed_shard_height ON processed_messages(shard_id, block_height DESC)"),
        ("idx_sync_progress_shard_id", "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_sync_progress_shard_id ON sync_progress(shard_id)"),
    ];

    for (name, sql) in &indexes_to_create {
        print!("  🔨 Creating {name}... ");
        io::stdout().flush()?;
        match sqlx::query(sql).execute(db).await {
            Ok(_) => println!("✅"),
            Err(e) => println!("⚠️  Failed: {e}"),
        }
    }

    println!("\n🔄 Re-enabling autovacuum...");

    // Re-enable autovacuum on all main tables
    let tables = vec![
        "casts",
        "links",
        "reactions",
        "verifications",
        "user_data",
        "username_proofs",
        "frame_actions",
        "onchain_events",
        "processed_messages",
    ];

    for table in &tables {
        match sqlx::query(&format!(
            "ALTER TABLE {table} SET (autovacuum_enabled = true)"
        ))
        .execute(db)
        .await
        {
            Ok(_) => println!("  ✅ Enabled autovacuum: {table}"),
            Err(e) => println!("  ⚠️  Failed for {table}: {e}"),
        }
    }

    println!("\n🧹 Running VACUUM ANALYZE (this may take a while)...");

    for table in &tables {
        print!("  🧹 Analyzing {table}... ");
        io::stdout().flush()?;
        match sqlx::query(&format!("VACUUM ANALYZE {table}"))
            .execute(db)
            .await
        {
            Ok(_) => println!("✅"),
            Err(e) => println!("⚠️  Failed: {e}"),
        }
    }

    println!("\n✅ Done! Normal operation mode restored.");
    println!("   All indexes recreated");
    println!("   Autovacuum re-enabled");
    println!("   Query performance optimized");

    Ok(())
}

/// Show current status of indexes and autovacuum
async fn handle_index_status(snaprag: &SnapRag) -> Result<()> {
    let db = snaprag.database.pool();

    println!("\n📊 Database Index & Autovacuum Status\n");

    // Check pg_trgm extension status
    println!("🔍 PostgreSQL Extensions:");
    let extensions = vec![
        ("vector", "Vector similarity search"),
        ("pg_trgm", "Trigram text search"),
    ];

    for (ext_name, description) in &extensions {
        let result: Option<(bool,)> =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = $1)")
                .bind(ext_name)
                .fetch_optional(db)
                .await?;

        if let Some((exists,)) = result {
            if exists {
                println!("  ✅ {ext_name} - {description}");
            } else {
                println!("  ❌ {ext_name} - {description} (missing)");
            }
        }
    }

    // Check which indexes exist
    println!("\n🔍 Non-Essential Indexes:");
    let indexes = vec![
        // Casts - basic indexes
        "idx_casts_fid",
        "idx_casts_timestamp",
        // Casts - optimization indexes
        "idx_casts_text_hash",
        "idx_casts_message_hash_desc",
        "idx_casts_text_hash_desc_composite",
        // Casts - pg_trgm trigram indexes
        "idx_casts_text_trgm",
        // Cast embeddings - basic indexes
        "idx_cast_embeddings_fid",
        "idx_cast_embeddings_message_hash",
        "idx_cast_embeddings_message_hash_btree",
        // Cast embeddings - pg_trgm indexes
        "idx_cast_embeddings_text_trgm",
        // Cast embedding chunks - basic indexes
        "idx_cast_embedding_chunks_fid",
        "idx_cast_embedding_chunks_message_hash",
        "idx_cast_embedding_chunks_strategy",
        // Cast embedding chunks - pg_trgm indexes
        "idx_cast_embedding_chunks_text_trgm",
        // Cast embedding aggregated - basic indexes
        "idx_cast_embedding_aggregated_fid",
        "idx_cast_embedding_aggregated_message_hash",
        // User profiles
        "idx_user_profiles_username",
        "idx_user_profiles_display_name",
        // User profile changes - optimized username lookup index
        "idx_profile_changes_username_value",
        // User profile changes - pg_trgm indexes
        "idx_user_profile_changes_value_trgm",
        // User profile snapshots
        "idx_profile_snapshots_fid_timestamp",
        // Username proofs - pg_trgm indexes
        "idx_username_proofs_username_trgm",
        // Links
        "idx_links_source_fid",
        "idx_links_target_fid",
        "idx_links_timestamp",
        // Reactions
        "idx_reactions_fid",
        "idx_reactions_target_cast_hash",
        "idx_reactions_timestamp",
        "idx_reactions_engagement",
        "idx_reactions_shard_block",
        "idx_reactions_target_cast",
        "idx_reactions_target_fid",
        "idx_reactions_type",
        "idx_reactions_user_cast",
        // Verifications
        "idx_verifications_fid",
        "idx_verifications_timestamp",
        // User data
        "idx_user_data_fid",
        "idx_user_data_type",
        // Sync tracking indexes
        "idx_processed_messages_hash",
        "idx_processed_shard_height",
        "idx_sync_progress_shard_id",
    ];

    let mut existing_count = 0;
    for index_name in &indexes {
        let result: Option<(bool,)> =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = $1)")
                .bind(index_name)
                .fetch_optional(db)
                .await?;

        if let Some((exists,)) = result {
            if exists {
                println!("  ✅ {index_name}");
                existing_count += 1;
            } else {
                println!("  ❌ {index_name} (missing)");
            }
        }
    }

    println!(
        "\n  Status: {}/{} indexes present",
        existing_count,
        indexes.len()
    );

    // Show trigram index details
    let trigram_indexes = vec![
        "idx_casts_text_trgm",
        "idx_cast_embeddings_text_trgm",
        "idx_cast_embedding_chunks_text_trgm",
        "idx_user_profile_changes_value_trgm",
        "idx_username_proofs_username_trgm",
    ];

    let mut trigram_count = 0;
    for index_name in &trigram_indexes {
        let result: Option<(bool,)> =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = $1)")
                .bind(index_name)
                .fetch_optional(db)
                .await?;

        if let Some((exists,)) = result {
            if exists {
                trigram_count += 1;
            }
        }
    }

    println!(
        "\n  Trigram Text Search Indexes: {}/{} present",
        trigram_count,
        trigram_indexes.len()
    );

    // Show vector index status (these are NOT managed by index set/unset)
    println!("\n🎯 Vector Indexes (Not affected by sync):");
    let vector_indexes = vec![
        "idx_cast_embedding_chunks_embedding_cosine",
        "idx_cast_embedding_aggregated_embedding_cosine",
    ];

    let mut vector_count = 0;
    for index_name in &vector_indexes {
        let result: Option<(bool,)> =
            sqlx::query_as("SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = $1)")
                .bind(index_name)
                .fetch_optional(db)
                .await?;

        if let Some((exists,)) = result {
            if exists {
                println!("  ✅ {index_name}");
                vector_count += 1;
            } else {
                println!("  ❌ {index_name} (missing)");
            }
        }
    }

    println!(
        "\n  Status: {}/{} vector indexes present",
        vector_count,
        vector_indexes.len()
    );
    println!("  Note: Vector indexes are NOT dropped during sync (no vector data during sync)");

    // Check autovacuum status
    println!("\n🛑 Autovacuum Status:");
    let tables = vec![
        "casts",
        "links",
        "reactions",
        "verifications",
        "user_data",
        "username_proofs",
        "frame_actions",
        "onchain_events",
        "processed_messages",
    ];

    let mut enabled_count = 0;
    for table in &tables {
        let result: Option<(Option<Vec<String>>,)> =
            sqlx::query_as("SELECT reloptions FROM pg_class WHERE relname = $1")
                .bind(table)
                .fetch_optional(db)
                .await?;

        let is_enabled = if let Some((Some(options),)) = result {
            !options
                .iter()
                .any(|opt| opt.contains("autovacuum_enabled=false"))
        } else {
            true // Default is enabled if no explicit setting
        };

        if is_enabled {
            println!("  ✅ {table} (enabled)");
            enabled_count += 1;
        } else {
            println!("  ❌ {table} (disabled)");
        }
    }

    println!(
        "\n  Status: {}/{} tables have autovacuum enabled",
        enabled_count,
        tables.len()
    );

    // Determine current mode
    println!("\n🎯 Current Mode:");
    if existing_count == indexes.len() && enabled_count == tables.len() {
        println!("  ✅ NORMAL OPERATION MODE");
        println!("     - All indexes present (including trigram text search)");
        println!("     - Autovacuum enabled");
        println!("     - Query performance: FAST");
        println!("     - Text search performance: OPTIMIZED (pg_trgm)");
        println!("     - Insert performance: NORMAL");
    } else if existing_count == 0 && enabled_count == 0 {
        println!("  🚀 BULK SYNC MODE (Turbo)");
        println!("     - Indexes dropped (including trigram indexes)");
        println!("     - Autovacuum disabled");
        println!("     - Query performance: SLOW");
        println!("     - Text search performance: DISABLED");
        println!("     - Insert performance: FAST (+30-70%)");
        println!("\n  ⚠️  Run 'snaprag index set' after sync completes!");
    } else {
        println!("  ⚠️  MIXED/INCONSISTENT STATE");
        println!(
            "     - Some indexes missing: {}/{}",
            indexes.len() - existing_count,
            indexes.len()
        );
        println!(
            "     - Trigram indexes missing: {}/{}",
            trigram_indexes.len() - trigram_count,
            trigram_indexes.len()
        );
        println!(
            "     - Autovacuum disabled on: {}/{}",
            tables.len() - enabled_count,
            tables.len()
        );
        println!("\n  💡 Recommendation:");
        println!("     - Run 'snaprag index unset' before bulk sync");
        println!("     - Run 'snaprag index set' after bulk sync");
    }

    Ok(())
}
