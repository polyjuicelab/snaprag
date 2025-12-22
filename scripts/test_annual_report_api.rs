//! Test script for annual report API
//! Usage: cargo run --bin test_annual_report_api -- <fid> <year>

use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let fid = args.get(1).map(|s| s.as_str()).unwrap_or("460432");
    let year = args.get(2).map(|s| s.as_str()).unwrap_or("2024");
    let api_url = env::var("API_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    println!("🧪 Testing Annual Report API");
    println!("============================");
    println!("API URL: {}", api_url);
    println!("FID: {}", fid);
    println!("Year: {}", year);
    println!();

    // Check if API server is running
    println!("📡 Checking API health...");
    let health_url = format!("{}/api/health", api_url);
    let client = reqwest::Client::new();
    
    match client.get(&health_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                println!("✅ API server is running");
            } else {
                println!("⚠️  API server responded with status: {}", response.status());
            }
        }
        Err(e) => {
            println!("❌ API server is not running: {}", e);
            println!("   Please start the server with: cargo run -- serve api");
            return Err(e.into());
        }
    }
    println!();

    // Test annual report endpoint
    println!("📊 Fetching annual report for FID {}, year {}...", fid, year);
    let report_url = format!("{}/api/users/{}/annual-report/{}", api_url, fid, year);
    
    match client.get(&report_url).send().await {
        Ok(response) => {
            let status = response.status();
            println!("HTTP Status: {}", status);
            println!();

            if !status.is_success() {
                let text = response.text().await?;
                println!("❌ API request failed");
                println!("Response: {}", text);
                return Err(format!("API returned status {}", status).into());
            }

            let json: serde_json::Value = response.json().await?;
            
            println!("✅ API request successful");
            println!();

            // Display summary
            if let Some(data) = json.get("data") {
                println!("📋 Annual Report Summary:");
                println!("=========================");
                
                if let Some(year_val) = data.get("year") {
                    println!("Year: {}", year_val);
                }
                println!();

                // User info
                if let Some(user) = data.get("user") {
                    println!("User:");
                    if let Some(fid_val) = user.get("fid") {
                        println!("  FID: {}", fid_val);
                    }
                    if let Some(username) = user.get("username") {
                        println!("  Username: {}", username);
                    }
                    if let Some(display_name) = user.get("display_name") {
                        println!("  Display Name: {}", display_name);
                    }
                    println!();
                }

                // Activity
                if let Some(activity) = data.get("activity") {
                    println!("Activity:");
                    if let Some(total_casts) = activity.get("total_casts") {
                        println!("  Total Casts (all time): {}", total_casts);
                    }
                    if let Some(total_casts_in_year) = activity.get("total_casts_in_year") {
                        println!("  Total Casts in Year: {}", total_casts_in_year);
                    }
                    println!();
                }

                // Content Style
                if let Some(content_style) = data.get("content_style") {
                    println!("Content Style:");
                    
                    if let Some(top_emojis) = content_style.get("top_emojis") {
                        if let Some(emojis_array) = top_emojis.as_array() {
                            println!("  Top Emojis: {} items", emojis_array.len());
                            for (i, emoji) in emojis_array.iter().take(5).enumerate() {
                                if let Some(emoji_str) = emoji.get("emoji") {
                                    if let Some(count) = emoji.get("count") {
                                        println!("    {}. {}: {}", i + 1, emoji_str, count);
                                    }
                                }
                            }
                        } else {
                            println!("  Top Emojis: 0 items");
                        }
                    }
                    
                    if let Some(top_words) = content_style.get("top_words") {
                        if let Some(words_array) = top_words.as_array() {
                            println!("  Top Words: {} items", words_array.len());
                            for (i, word) in words_array.iter().take(5).enumerate() {
                                if let Some(word_str) = word.get("word") {
                                    if let Some(count) = word.get("count") {
                                        println!("    {}. {}: {}", i + 1, word_str, count);
                                    }
                                }
                            }
                        } else {
                            println!("  Top Words: 0 items");
                        }
                    }
                    
                    if let Some(frames_used) = content_style.get("frames_used") {
                        println!("  Frames Used: {}", frames_used);
                    }
                    println!();
                }

                // Engagement
                if let Some(engagement) = data.get("engagement") {
                    println!("Engagement:");
                    if let Some(reactions) = engagement.get("reactions_received") {
                        println!("  Reactions Received: {}", reactions);
                    }
                    if let Some(recasts) = engagement.get("recasts_received") {
                        println!("  Recasts Received: {}", recasts);
                    }
                    if let Some(replies) = engagement.get("replies_received") {
                        println!("  Replies Received: {}", replies);
                    }
                    println!();
                }

                // Social Growth
                if let Some(social_growth) = data.get("social_growth") {
                    println!("Social Growth:");
                    if let Some(followers) = social_growth.get("current_followers") {
                        println!("  Current Followers: {}", followers);
                    }
                    if let Some(following) = social_growth.get("current_following") {
                        println!("  Current Following: {}", following);
                    }
                }
            }

            println!();
            println!("✅ Test completed successfully!");
            println!();
            println!("Full response saved to: /tmp/annual_report_response.json");
            std::fs::write("/tmp/annual_report_response.json", serde_json::to_string_pretty(&json)?)?;
        }
        Err(e) => {
            println!("❌ Failed to fetch annual report: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}

