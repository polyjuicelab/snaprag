//! Emoji extraction and analysis utilities

use std::collections::HashMap;

/// Extract emojis from text
///
/// Returns a vector of emoji characters found in the text.
///
/// # Examples
///
/// ```
/// use snaprag::utils::emoji::extract_emojis;
///
/// let text = "Hello 🔥 world 💎";
/// let emojis = extract_emojis(text);
/// assert_eq!(emojis, vec!["🔥", "💎"]);
/// ```
pub fn extract_emojis(text: &str) -> Vec<String> {
    text.chars()
        .filter(|c| is_emoji(*c))
        .map(|c| c.to_string())
        .collect()
}

/// Check if a character is an emoji
///
/// This checks for emoji characters including:
/// - Emoticons (😀-🙏)
/// - Symbols & Pictographs (🌀-🏿)
/// - Transport & Map Symbols (🚀-🛿)
/// - Supplemental Symbols (🩰-🩿)
/// - Symbols & Pictographs Extended-A (🪀-🪿)
/// - Symbols & Pictographs Extended-B (🫀-🫿)
fn is_emoji(c: char) -> bool {
    // Check various emoji ranges
    let code = c as u32;
    matches!(
        code,
        0x1F300..=0x1F9FF // Miscellaneous Symbols and Pictographs
            | 0x2600..=0x26FF // Miscellaneous Symbols
            | 0x2700..=0x27BF // Dingbats
            | 0xFE00..=0xFE0F // Variation Selectors
            | 0x1F1E0..=0x1F1FF // Regional Indicator Symbols (flags)
            | 0x200D // Zero Width Joiner
            | 0x20E3 // Combining Enclosing Keycap
    ) || is_emoji_extended(c)
}

/// Check for extended emoji ranges (newer Unicode versions)
fn is_emoji_extended(c: char) -> bool {
    let code = c as u32;
    // Extended ranges for newer emoji
    matches!(code, 0x1FA00..=0x1FAFF) // Chess Symbols, Symbols and Pictographs Extended-A
}

/// Count emoji frequencies in text
///
/// Returns a HashMap mapping emoji strings to their counts.
///
/// # Examples
///
/// ```
/// use snaprag::utils::emoji::count_emoji_frequencies;
///
/// let text = "🔥 Hello 🔥 world 💎";
/// let frequencies = count_emoji_frequencies(text);
/// assert_eq!(frequencies.get("🔥"), Some(&2));
/// assert_eq!(frequencies.get("💎"), Some(&1));
/// ```
pub fn count_emoji_frequencies(text: &str) -> HashMap<String, usize> {
    let mut frequencies = HashMap::new();
    let emojis = extract_emojis(text);
    for emoji in emojis {
        *frequencies.entry(emoji).or_insert(0) += 1;
    }
    frequencies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_emojis() {
        let text = "Hello 🔥 world 💎";
        let emojis = extract_emojis(text);
        assert_eq!(emojis.len(), 2);
        assert!(emojis.contains(&"🔥".to_string()));
        assert!(emojis.contains(&"💎".to_string()));
    }

    #[test]
    fn test_count_emoji_frequencies() {
        let text = "🔥 Hello 🔥 world 💎";
        let frequencies = count_emoji_frequencies(text);
        assert_eq!(frequencies.get("🔥"), Some(&2));
        assert_eq!(frequencies.get("💎"), Some(&1));
    }

    #[test]
    fn test_no_emojis() {
        let text = "Hello world";
        let emojis = extract_emojis(text);
        assert_eq!(emojis.len(), 0);
    }
}
