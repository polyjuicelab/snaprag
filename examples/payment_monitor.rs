//! USDC Payment Monitor
//!
//! Real-time monitoring of USDC payments to your address.
//!
//! Usage:
//! ```bash
//! cargo run --example payment_monitor -- \
//!   --address 0xYourAddress \
//!   --testnet
//! ```

use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use ethers::prelude::*;
use ethers::providers::Http;
use ethers::providers::Provider;
use ethers::types::Address;
use ethers::types::U256;

#[derive(Parser)]
#[command(name = "payment_monitor")]
#[command(about = "Monitor USDC payments in real-time")]
struct Args {
    /// Ethereum address to monitor
    #[arg(short, long)]
    address: String,

    /// Use testnet (base-sepolia) instead of mainnet
    #[arg(long)]
    testnet: bool,

    /// Check interval in seconds
    #[arg(long, default_value = "10")]
    interval: u64,
}

// USDC balanceOf ABI
abigen!(
    IERC20,
    r#"[
        function balanceOf(address account) external view returns (uint256)
        function decimals() external view returns (uint8)
        function symbol() external view returns (string)
    ]"#
);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Network configuration
    let (rpc_url, usdc_address, explorer_url, network_name) = if args.testnet {
        (
            "https://sepolia.base.org",
            "0x036CbD53842c5426634e7929541eC2318f3dCF7e",
            "https://sepolia.basescan.org",
            "Base Sepolia Testnet",
        )
    } else {
        (
            "https://mainnet.base.org",
            "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
            "https://basescan.org",
            "Base Mainnet",
        )
    };

    println!("💰 USDC Payment Monitor");
    println!("=======================\n");

    // Connect to provider
    println!("🔗 Connecting to {}...", network_name);
    let provider = Provider::<Http>::try_from(rpc_url)?;
    let provider = Arc::new(provider);

    // Parse address
    let address: Address = args.address.parse()?;
    let usdc_addr: Address = usdc_address.parse()?;

    // Create USDC contract instance
    let usdc = IERC20::new(usdc_addr, provider.clone());

    // Get initial balance
    let initial_balance = usdc.balance_of(address).call().await?;
    let decimals = usdc.decimals().call().await?;
    let symbol = usdc.symbol().call().await?;

    let balance_display = format_balance(initial_balance, decimals);

    println!("✅ Connected successfully\n");
    println!("📊 Monitoring Configuration");
    println!("===========================");
    println!("📍 Address: {}", address);
    println!("🌐 Network: {}", network_name);
    println!("💵 Current {} balance: {}", symbol, balance_display);
    println!("⏱️  Check interval: {} seconds", args.interval);
    println!(
        "🔗 Explorer: {}/address/{}#tokentxns",
        explorer_url, address
    );
    println!("\n⏳ Monitoring... (Press Ctrl+C to stop)\n");

    let mut last_balance = initial_balance;
    let mut payment_count = 0u64;
    let mut total_received = U256::zero();

    loop {
        tokio::time::sleep(Duration::from_secs(args.interval)).await;

        match usdc.balance_of(address).call().await {
            Ok(current_balance) => {
                if current_balance != last_balance {
                    let change = current_balance.saturating_sub(last_balance);

                    if change > U256::zero() {
                        // Payment received
                        payment_count += 1;
                        total_received = total_received.saturating_add(change);

                        let change_display = format_balance(change, decimals);
                        let balance_display = format_balance(current_balance, decimals);
                        let total_display = format_balance(total_received, decimals);

                        println!("═══════════════════════════════════════════════");
                        println!("🎉 Payment Received #{}", payment_count);
                        println!("═══════════════════════════════════════════════");
                        println!("   Amount: +{} {}", change_display, symbol);
                        println!("   New Balance: {} {}", balance_display, symbol);
                        println!("   Total Received: {} {}", total_display, symbol);
                        println!("   View: {}/address/{}#tokentxns", explorer_url, address);
                        println!("═══════════════════════════════════════════════\n");

                        // Send system notification (macOS)
                        #[cfg(target_os = "macos")]
                        {
                            let _ = std::process::Command::new("osascript")
                                .arg("-e")
                                .arg(format!(
                                    "display notification \"Received {} {} (Balance: {})\" with title \"💰 Payment Received\" sound name \"Glass\"",
                                    change_display, symbol, balance_display
                                ))
                                .output();
                        }
                    } else {
                        // Balance decreased (withdrawal)
                        let decrease = last_balance.saturating_sub(current_balance);
                        let decrease_display = format_balance(decrease, decimals);
                        let balance_display = format_balance(current_balance, decimals);

                        println!("⚠️  Balance decreased: -{} {}", decrease_display, symbol);
                        println!("   New Balance: {} {}", balance_display, symbol);
                        println!();
                    }

                    last_balance = current_balance;
                }
            }
            Err(e) => {
                eprintln!("⚠️ Error fetching balance: {}", e);
            }
        }
    }
}

/// Format balance with decimals
fn format_balance(balance: U256, decimals: u8) -> String {
    let divisor = U256::from(10u128.pow(decimals as u32));
    let whole = balance / divisor;
    let remainder = balance % divisor;

    // Format with 6 decimal places
    let decimal_part = format!("{:0width$}", remainder, width = decimals as usize);
    let decimal_trimmed = decimal_part.trim_end_matches('0');

    if decimal_trimmed.is_empty() {
        format!("{}", whole)
    } else {
        format!("{}.{}", whole, decimal_trimmed)
    }
}
