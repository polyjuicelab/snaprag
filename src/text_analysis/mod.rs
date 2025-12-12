//! Text analysis module for multi-language tokenization and POS tagging

use std::collections::HashMap;

use tracing::debug;

/// Detected language information
#[derive(Debug, Clone)]
pub struct LanguageInfo {
    pub lang_code: String, // ISO 639-1 code
    pub confidence: f64,
}

/// Word with POS tag
#[derive(Debug, Clone)]
pub struct TaggedWord {
    pub word: String,
    pub pos: String, // Part of speech tag
    pub language: String,
}

/// Detect language of text
pub fn detect_language(text: &str) -> LanguageInfo {
    use whatlang::detect;
    use whatlang::Lang;

    if text.trim().is_empty() {
        return LanguageInfo {
            lang_code: "en".to_string(),
            confidence: 0.0,
        };
    }

    match detect(text) {
        Some(info) => {
            let lang_code = match info.lang() {
                Lang::Eng => "en",
                Lang::Cmn => "zh", // Chinese (Mandarin)
                Lang::Jpn => "ja",
                Lang::Kor => "ko",
                Lang::Spa => "es",
                Lang::Fra => "fr",
                Lang::Deu => "de",
                Lang::Ita => "it",
                Lang::Por => "pt",
                Lang::Rus => "ru",
                _ => "en", // Default to English for other languages
            };

            LanguageInfo {
                lang_code: lang_code.to_string(),
                confidence: info.confidence(),
            }
        }
        None => LanguageInfo {
            lang_code: "en".to_string(),
            confidence: 0.0,
        },
    }
}

/// Tokenize and tag words for a given language
pub fn tokenize_and_tag(text: &str, language: &str) -> Vec<TaggedWord> {
    match language {
        "zh" => tokenize_chinese(text),
        "en" => tokenize_english(text),
        "ja" => tokenize_japanese(text),
        "ko" => tokenize_korean(text),
        _ => tokenize_english(text), // Fallback to English tokenization
    }
}

/// Tokenize Chinese text using jieba-rs
fn tokenize_chinese(text: &str) -> Vec<TaggedWord> {
    use jieba_rs::Jieba;

    let jieba = Jieba::new();
    let words = jieba.tag(text, true);

    words
        .into_iter()
        .map(|tag| TaggedWord {
            word: tag.word.to_string(),
            pos: tag.tag.to_string(),
            language: "zh".to_string(),
        })
        .collect()
}

/// Tokenize English text (whitespace-based with simple POS tagging)
fn tokenize_english(text: &str) -> Vec<TaggedWord> {
    text.split_whitespace()
        .filter_map(|word| {
            let cleaned = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();

            if cleaned.is_empty() {
                return None;
            }

            // Simple POS tagging based on word endings
            let pos = if cleaned.ends_with("ing")
                || cleaned.ends_with("ed")
                || cleaned.ends_with("es")
                || cleaned.ends_with("s")
            {
                "v" // verb
            } else if cleaned.ends_with("tion")
                || cleaned.ends_with("ness")
                || cleaned.ends_with("er")
                || cleaned.ends_with("or")
                || cleaned.ends_with("ity")
                || cleaned.ends_with("ment")
            {
                "n" // noun
            } else if cleaned.ends_with("ly") {
                "adv" // adverb
            } else if cleaned.ends_with("ful") || cleaned.ends_with("ous") {
                "adj" // adjective
            } else {
                "unknown"
            };

            Some(TaggedWord {
                word: cleaned,
                pos: pos.to_string(),
                language: "en".to_string(),
            })
        })
        .collect()
}

/// Tokenize Japanese text (character-based fallback)
fn tokenize_japanese(text: &str) -> Vec<TaggedWord> {
    // Simple character-based tokenization for Japanese
    // In production, you might want to use lindera or similar
    text.chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .map(|c| TaggedWord {
            word: c.to_string(),
            pos: "unknown".to_string(),
            language: "ja".to_string(),
        })
        .collect()
}

/// Tokenize Korean text (character-based with syllable boundaries)
fn tokenize_korean(text: &str) -> Vec<TaggedWord> {
    // Simple word-based tokenization for Korean
    // Korean words are typically separated by spaces
    text.split_whitespace()
        .filter_map(|word| {
            let cleaned = word.trim().to_string();
            if cleaned.is_empty() {
                return None;
            }

            // Simple POS detection for Korean
            let pos = if cleaned.ends_with("하다") || cleaned.ends_with("되다") {
                "v" // verb
            } else if cleaned.ends_with("것") || cleaned.ends_with("들") {
                "n" // noun
            } else {
                "unknown"
            };

            Some(TaggedWord {
                word: cleaned,
                pos: pos.to_string(),
                language: "ko".to_string(),
            })
        })
        .collect()
}

/// Extract nouns from tagged words
pub fn extract_nouns(tagged_words: &[TaggedWord]) -> Vec<String> {
    tagged_words
        .iter()
        .filter(|tw| {
            matches!(
                tw.pos.as_str(),
                "n" | "nr" | "ns" | "nt" | "nz" | "noun" | "NN" | "NNS" | "NNP"
            ) || tw.language == "zh" && tw.pos.starts_with('n')
        })
        .map(|tw| tw.word.clone())
        .collect()
}

/// Extract verbs from tagged words
pub fn extract_verbs(tagged_words: &[TaggedWord]) -> Vec<String> {
    tagged_words
        .iter()
        .filter(|tw| {
            matches!(
                tw.pos.as_str(),
                "v" | "vd" | "vn" | "verb" | "VB" | "VBD" | "VBG" | "VBN" | "VBP" | "VBZ"
            ) || tw.language == "zh" && tw.pos.starts_with('v')
        })
        .map(|tw| tw.word.clone())
        .collect()
}

/// Count word frequencies with stop word filtering
pub fn count_word_frequencies(words: &[String], language: &str) -> HashMap<String, usize> {
    use std::collections::HashSet;

    let stop_words_vec = match language {
        "en" => stop_words::get(stop_words::LANGUAGE::English),
        "zh" => stop_words::get(stop_words::LANGUAGE::Chinese),
        "ja" => stop_words::get(stop_words::LANGUAGE::Japanese),
        "ko" => stop_words::get(stop_words::LANGUAGE::Korean),
        "es" => stop_words::get(stop_words::LANGUAGE::Spanish),
        "fr" => stop_words::get(stop_words::LANGUAGE::French),
        "de" => stop_words::get(stop_words::LANGUAGE::German),
        "it" => stop_words::get(stop_words::LANGUAGE::Italian),
        "pt" => stop_words::get(stop_words::LANGUAGE::Portuguese),
        "ru" => stop_words::get(stop_words::LANGUAGE::Russian),
        _ => stop_words::get(stop_words::LANGUAGE::English), // Default to English
    };

    let stop_words_set: HashSet<String> = stop_words_vec
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();

    let mut word_counts: HashMap<String, usize> = HashMap::new();

    for word in words {
        let cleaned = word.to_lowercase().trim().to_string();

        // Skip if empty, too short, or stop word
        if cleaned.len() < 2 || stop_words_set.contains(&cleaned) {
            continue;
        }

        // Skip URLs
        if cleaned.starts_with("http") || cleaned.contains("://") {
            continue;
        }

        // Skip mentions and hashtags
        if cleaned.starts_with('@') || cleaned.starts_with('#') {
            continue;
        }

        // Skip if only punctuation
        if cleaned.chars().all(|c| !c.is_alphanumeric()) {
            continue;
        }

        *word_counts.entry(cleaned).or_insert(0) += 1;
    }

    word_counts
}

/// Analyze text and extract nouns/verbs with language detection
pub fn analyze_text(text: &str) -> (Vec<TaggedWord>, LanguageInfo) {
    let lang_info = detect_language(text);
    let tagged_words = tokenize_and_tag(text, &lang_info.lang_code);

    debug!(
        "Analyzed text: {} chars, language: {} (confidence: {:.2}), {} words",
        text.len(),
        lang_info.lang_code,
        lang_info.confidence,
        tagged_words.len()
    );

    (tagged_words, lang_info)
}
