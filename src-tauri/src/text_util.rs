/// Normalize text into lowercase alphanumeric words for comparison.
pub fn normalize_words(text: &str) -> Vec<String> {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect()
}
