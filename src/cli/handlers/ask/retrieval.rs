use std::io::Write;

/// Retrieval and ranking pipeline for ask
use crate::cli::output::truncate_str;
use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::Result;

/// Simple spinner for showing progress
pub struct Spinner {
    message: String,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Spinner {
    #[must_use]
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            running: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn start(&self) {
        let message = self.message.clone();
        let running = self.running.clone();
        running.store(true, std::sync::atomic::Ordering::Relaxed);

        std::thread::spawn(move || {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut idx = 0;

            while running.load(std::sync::atomic::Ordering::Relaxed) {
                print!("\r   {} {}...", frames[idx], message);
                std::io::stdout().flush().ok();
                idx = (idx + 1) % frames.len();
                std::thread::sleep(std::time::Duration::from_millis(80));
            }

            // Clear the line
            print!("\r{}\r", " ".repeat(80));
            std::io::stdout().flush().ok();
        });
    }

    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(100)); // Give time to clear
    }
}

/// Find relevant casts using semantic search and heuristics
///
/// # Panics
/// Panics if the system time is before UNIX_EPOCH (1970-01-01), which is impossible on modern systems
pub async fn find_relevant_casts(
    database: &Database,
    embedding_service: &EmbeddingService,
    fid: u64,
    question: &str,
    context_limit: usize,
    verbose: bool,
) -> Result<Vec<crate::models::CastSearchResult>> {
    // Generate query embedding with spinner
    let spinner = Spinner::new("Searching");
    spinner.start();

    tracing::debug!("Generating query embedding...");
    let query_embedding = embedding_service.generate(question).await?;
    tracing::debug!("Query embedding generated");

    // Search for semantically similar casts (use simple version without engagement metrics)
    // Use a larger limit initially to ensure we get enough from this FID
    let search_limit = (context_limit * 5).max(100);
    tracing::debug!(
        "Executing vector search with limit={}, threshold=0.3",
        search_limit
    );
    let search_results = database
        .semantic_search_casts_simple(query_embedding, search_limit as i64, Some(0.3))
        .await?;
    tracing::debug!(
        "Vector search completed, found {} results",
        search_results.len()
    );

    spinner.stop();

    // Filter to only include casts from this FID
    let mut user_casts: Vec<_> = search_results
        .into_iter()
        .filter(|result| result.fid == fid as i64)
        .collect();

    // Calculate recency score - newer posts get higher weight
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Prioritize: relevance + substance + recency
    // Score = similarity * log(length) * recency_factor
    user_casts.sort_by(|a, b| {
        // Helper function to calculate score safely, handling all edge cases
        fn calculate_score(similarity: f32, text_len: usize, recency: f32) -> f32 {
            // Ensure similarity is valid (not NaN, not negative, not inf)
            let sim = if !similarity.is_finite() || similarity < 0.0 {
                0.0
            } else {
                similarity.min(1.0) // Clamp to [0, 1]
            };

            // Ensure text length is at least 1 to avoid ln(0) = -inf
            let len = text_len.max(1).min(10000); // Clamp to reasonable range
            let substance = (len as f32).ln().max(1.0).min(10.0); // Clamp ln result

            // Ensure recency is valid and clamped
            let rec = if !recency.is_finite() || recency < 0.0 {
                0.5
            } else {
                recency.min(1.0) // Clamp to [0, 1]
            };

            // Calculate score and ensure it's finite
            let score = sim * substance * rec;

            // Return 0.0 for any invalid scores
            if !score.is_finite() || score < 0.0 {
                0.0
            } else {
                score
            }
        }

        // Recency factor: 1.0 for recent (< 30 days), decays to 0.5 for old (> 1 year)
        let age_days_a = ((now - a.timestamp) as f32) / 86400.0;
        let age_days_b = ((now - b.timestamp) as f32) / 86400.0;
        let recency_a = (1.0 - (age_days_a / 365.0).min(0.5)).max(0.5);
        let recency_b = (1.0 - (age_days_b / 365.0).min(0.5)).max(0.5);

        // Calculate scores
        let score_a = calculate_score(a.similarity, a.text.len(), recency_a);
        let score_b = calculate_score(b.similarity, b.text.len(), recency_b);

        // Use total_cmp for f32 which provides a total ordering (handles NaN)
        // Reverse order: higher scores first
        score_b.total_cmp(&score_a)
    });

    // Take top results
    user_casts.truncate(context_limit);

    let user_relevant_casts = user_casts;

    if verbose && !user_relevant_casts.is_empty() {
        println!();
        println!("   📋 Top relevant casts (sorted by: relevance × substance × recency):");

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        for (idx, result) in user_relevant_casts.iter().take(5).enumerate() {
            let preview = truncate_str(&result.text, 80);

            // Calculate days ago
            let age_days = ((now - result.timestamp) as f32) / 86400.0;
            let age_str = if age_days < 1.0 {
                "today".to_string()
            } else if age_days < 7.0 {
                format!("{age_days:.0}d ago")
            } else if age_days < 30.0 {
                format!("{age_days:.0}d ago")
            } else if age_days < 365.0 {
                format!("{:.0}mo ago", age_days / 30.0)
            } else {
                format!("{:.1}y ago", age_days / 365.0)
            };

            println!(
                "      {}. [sim: {:.2}, len: {}, age: {}] {}",
                idx + 1,
                result.similarity,
                result.text.len(),
                age_str,
                preview
            );
        }
        println!();
    }

    Ok(user_relevant_casts)
}

/// Analyze user's writing style from their casts
#[must_use]
pub fn analyze_writing_style(casts: &[crate::models::CastSearchResult]) -> String {
    if casts.is_empty() {
        return "casual and friendly".to_string();
    }

    let mut style_notes = Vec::new();
    let mut detailed_analysis = Vec::new();

    // 1. Analyze emoji usage with more detail
    let total_emojis: usize = casts
        .iter()
        .map(|c| {
            c.text
                .chars()
                .filter(|ch| {
                    // Enhanced emoji detection (Unicode ranges)
                    matches!(*ch as u32,
                        0x1F300..=0x1F9FF | // Emoticons, symbols, pictographs (includes 1F600-1F64F, 1F680-1F6FF, 1F900-1F9FF)
                        0x2600..=0x26FF |   // Miscellaneous symbols
                        0x2700..=0x27BF     // Dingbats
                    )
                })
                .count()
        })
        .sum();

    let emoji_per_post = total_emojis as f32 / casts.len() as f32;
    if emoji_per_post > 2.0 {
        style_notes.push("HEAVY emoji user (2-3+ per post) 🎨");
        detailed_analysis.push("USE lots of emojis like the examples show");
    } else if emoji_per_post > 0.5 {
        style_notes.push("moderate emoji usage");
        detailed_analysis.push("Use emojis moderately, about 1 per response");
    } else if emoji_per_post > 0.0 {
        style_notes.push("minimal emojis");
        detailed_analysis.push("Use emojis sparingly or not at all");
    } else {
        style_notes.push("NO emojis - pure text");
        detailed_analysis.push("NEVER use emojis - stay text-only");
    }

    // 2. Analyze sentence length and structure
    let avg_length: usize = casts.iter().map(|c| c.text.len()).sum::<usize>() / casts.len().max(1);

    if avg_length < 50 {
        style_notes.push("ULTRA SHORT responses (< 50 chars)");
        detailed_analysis.push("Keep it EXTREMELY brief - just a few words or one sentence");
    } else if avg_length < 100 {
        style_notes.push("very concise (50-100 chars)");
        detailed_analysis.push("Keep responses short - 1-2 sentences max");
    } else if avg_length < 200 {
        style_notes.push("moderate length (100-200 chars)");
        detailed_analysis.push("Write 2-3 sentences, stay focused");
    } else {
        style_notes.push("detailed explanations (200+ chars)");
        detailed_analysis.push("Write detailed, thoughtful responses");
    }

    // 3. Analyze punctuation and energy
    let exclamation_count = casts.iter().filter(|c| c.text.contains('!')).count();
    let question_count = casts.iter().filter(|c| c.text.contains('?')).count();

    if exclamation_count > casts.len() / 2 {
        style_notes.push("HIGH ENERGY! Lots of exclamation marks!");
        detailed_analysis.push("Match the HIGH ENERGY - use exclamation marks!");
    } else if exclamation_count > casts.len() / 4 {
        style_notes.push("enthusiastic tone");
        detailed_analysis.push("Show some enthusiasm with occasional exclamation marks");
    }

    if question_count > casts.len() / 3 {
        style_notes.push("often asks questions");
        detailed_analysis.push("Feel free to ask questions back");
    }

    // 4. Check for informal/slang markers with more examples
    let informal_markers = [
        "lol", "lmao", "omg", "tbh", "ngl", "fr", "gonna", "wanna", "yea", "yeah", "nah", "kinda",
        "sorta", "gotta", "idk", "imo", "btw", "rn", "af", "asf", "lowkey", "highkey",
    ];

    let informal_count = casts
        .iter()
        .filter(|c| {
            let lower = c.text.to_lowercase();
            informal_markers.iter().any(|marker| lower.contains(marker))
        })
        .count();

    if informal_count > casts.len() / 2 {
        style_notes.push("VERY casual/slang heavy");
        detailed_analysis.push("Use slang and casual language - lol, fr, ngl, etc.");
    } else if informal_count > casts.len() / 4 {
        style_notes.push("relaxed and conversational");
        detailed_analysis.push("Keep it conversational and relaxed");
    } else {
        style_notes.push("professional and articulate");
        detailed_analysis.push("Stay professional and well-articulated");
    }

    // 5. Check for technical language
    let tech_markers = [
        "build",
        "dev",
        "code",
        "api",
        "tech",
        "protocol",
        "onchain",
        "contract",
        "blockchain",
        "crypto",
        "web3",
        "deploy",
        "ship",
        "feature",
        "bug",
        "frontend",
        "backend",
    ];

    let tech_count = casts
        .iter()
        .filter(|c| {
            let lower = c.text.to_lowercase();
            tech_markers.iter().any(|marker| lower.contains(marker))
        })
        .count();

    if tech_count > casts.len() / 2 {
        style_notes.push("HIGHLY technical/builder-focused");
        detailed_analysis.push("Use technical jargon and builder language");
    } else if tech_count > casts.len() / 4 {
        style_notes.push("tech-aware");
        detailed_analysis.push("Mix in some technical terms when relevant");
    }

    // 6. Analyze sentence structure patterns
    let short_sentences = casts.iter().filter(|c| c.text.len() < 50).count();
    if short_sentences as f32 / casts.len() as f32 > 0.7 {
        style_notes.push("prefers SHORT punchy sentences");
        detailed_analysis.push("Keep sentences SHORT and punchy - no long explanations");
    }

    // 7. Check for common words/phrases (fingerprint)
    let mut common_starters = Vec::new();
    for cast in casts.iter().take(10) {
        let words: Vec<&str> = cast.text.split_whitespace().collect();
        if !words.is_empty() {
            common_starters.push(words[0].to_lowercase());
        }
    }

    // 8. Analyze link sharing patterns
    let link_analysis = analyze_link_sharing(casts);

    // Return combined analysis
    format!(
        "STYLE PROFILE: {}\n\nLINK SHARING: {}\n\nKEY INSTRUCTIONS:\n{}",
        style_notes.join(" | "),
        link_analysis,
        detailed_analysis.join("\n")
    )
}

/// Analyze user's link sharing patterns
fn analyze_link_sharing(casts: &[crate::models::CastSearchResult]) -> String {
    use std::collections::HashMap;

    let mut total_links = 0;
    let mut casts_with_links = 0;
    let mut domain_counts: HashMap<String, usize> = HashMap::new();

    for cast in casts {
        if let Some(embeds) = &cast.embeds {
            if let Some(embeds_array) = embeds.as_array() {
                let mut has_link = false;
                for embed in embeds_array {
                    // Check for URL embed
                    if let Some(url_value) = embed.get("url") {
                        if let Some(url) = url_value.as_str() {
                            total_links += 1;
                            has_link = true;

                            // Extract and count domain
                            if let Some(domain) = extract_domain(url) {
                                *domain_counts.entry(domain).or_insert(0) += 1;
                            }
                        }
                    }
                }
                if has_link {
                    casts_with_links += 1;
                }
            }
        }
    }

    // No links found
    if total_links == 0 {
        return "NO links - pure text posts only. Don't mention or reference links in responses."
            .to_string();
    }

    // Calculate link frequency
    let link_frequency = casts_with_links as f32 / casts.len() as f32;

    let mut result = if link_frequency > 0.5 {
        format!("⛓️ FREQUENT link sharer ({total_links} links in {casts_with_links} posts)")
    } else if link_frequency > 0.2 {
        format!("🔗 Occasional link sharer ({total_links} links in {casts_with_links} posts)")
    } else {
        format!("Rarely shares links ({total_links} in {casts_with_links} posts)")
    };

    // Add top domains and categorize them
    let mut sorted_domains: Vec<_> = domain_counts.iter().collect();
    sorted_domains.sort_by(|a, b| b.1.cmp(a.1));

    if !sorted_domains.is_empty() {
        result.push_str("\n   Top domains: ");
        let top_domains: Vec<String> = sorted_domains
            .iter()
            .take(3)
            .map(|(domain, count)| {
                let category = categorize_domain(domain);
                format!("{domain} ({count}x, {category})")
            })
            .collect();
        result.push_str(&top_domains.join(", "));

        // Add usage instruction based on domains
        let has_tech_links = sorted_domains
            .iter()
            .any(|(d, _)| matches!(categorize_domain(d), "code" | "docs"));
        let has_social_links = sorted_domains
            .iter()
            .any(|(d, _)| categorize_domain(d) == "social");
        let has_content_links = sorted_domains
            .iter()
            .any(|(d, _)| matches!(categorize_domain(d), "article" | "video"));

        result.push_str("\n   → ");
        if has_tech_links {
            result.push_str("Tech-savvy, shares code/docs. ");
        }
        if has_social_links {
            result.push_str("Social sharer, quotes others. ");
        }
        if has_content_links {
            result.push_str("Content curator, shares articles/videos. ");
        }

        if link_frequency > 0.3 {
            result.push_str("\n   → You can reference \"like that thing you shared\" in responses");
        }
    }

    result
}

/// Extract domain from URL
fn extract_domain(url: &str) -> Option<String> {
    // Remove protocol
    let without_protocol = url.split("://").nth(1).or(Some(url))?;

    // Get domain (before first /)
    let domain = without_protocol.split('/').next()?;

    // Remove www. prefix
    let clean_domain = domain.strip_prefix("www.").unwrap_or(domain);

    Some(clean_domain.to_lowercase())
}

/// Categorize domain into content type
fn categorize_domain(domain: &str) -> &'static str {
    let lower = domain.to_lowercase();

    // Code/Development
    if lower.contains("github") || lower.contains("gitlab") || lower.contains("bitbucket") {
        return "code";
    }

    // Documentation
    if lower.contains("docs.") || lower.contains("documentation") {
        return "docs";
    }

    // Social platforms
    if lower.contains("twitter")
        || lower.contains("x.com")
        || lower.contains("warpcast")
        || lower.contains("farcaster")
    {
        return "social";
    }

    // Content platforms
    if lower.contains("medium")
        || lower.contains("substack")
        || lower.contains("mirror.xyz")
        || lower.contains("paragraph.xyz")
    {
        return "article";
    }

    // Video
    if lower.contains("youtube") || lower.contains("vimeo") || lower.contains("twitch") {
        return "video";
    }

    // Web3 specific
    if lower.contains("etherscan")
        || lower.contains("opensea")
        || lower.contains("zora")
        || lower.contains("lens")
    {
        return "web3";
    }

    // Default
    "link"
}
