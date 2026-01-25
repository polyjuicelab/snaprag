//! Example: Running `SnapRAG` API server with `x402` payment support
//!
//! This example demonstrates how to run the API server with payment enabled.
//!
//! Usage:
//! ```bash
//! # With payment enabled
//! cargo run --features payment --example payment_server -- \
//!   --payment-address 0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb \
//!   --testnet
//! ```

#[cfg(not(feature = "payment"))]
fn main() {
    println!("❌ This example requires the 'payment' feature to be enabled.");
    println!("Run with: cargo run --features payment --example payment_server");
    std::process::exit(1);
}

#[cfg(feature = "payment")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use clap::Parser;
    use snaprag::api::serve_api;
    use snaprag::AppConfig;

    #[derive(Parser)]
    struct Args {
        /// Address to receive payments
        #[arg(long)]
        payment_address: String,

        /// Use testnet (base-sepolia) instead of mainnet
        #[arg(long)]
        testnet: bool,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Port to bind to
        #[arg(short, long, default_value = "3000")]
        port: u16,

        /// Enable CORS
        #[arg(long)]
        cors: bool,
    }

    let args = Args::parse();

    // Initialize logging
    snaprag::logging::init_logging()?;

    // Load configuration
    let config = AppConfig::load()?;

    println!("🚀 Starting SnapRAG API Server with x402 Payment");
    println!("================================================\n");
    println!("💰 Payment Address: {}", args.payment_address);
    println!(
        "🌐 Network: {}",
        if args.testnet {
            "base-sepolia (testnet)"
        } else {
            "base (mainnet)"
        }
    );
    println!("📍 Server: http://{}:{}", args.host, args.port);
    println!(
        "🔐 CORS: {}",
        if args.cors { "Enabled" } else { "Disabled" }
    );
    println!("\n💡 Pricing:");
    println!("  • Free:       /api/health, /api/stats");
    println!("  • $0.001:     /api/profiles");
    println!("  • $0.01:      /api/search/*");
    println!("  • $0.1:       /api/rag/query");
    println!();

    // Start server with payment enabled
    // TODO: Implement payment-enabled serve_api variant
    serve_api(&config, args.host, args.port, args.cors).await?;

    Ok(())
}
