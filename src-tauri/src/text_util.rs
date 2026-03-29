/// Normalize text into lowercase alphanumeric words for comparison.
///
/// Expands common contractions before stripping punctuation so that "I'm"
/// and "I am" produce the same word set — critical for dedup similarity
/// matching between Whisper outputs that may use different forms.
pub fn normalize_words(text: &str) -> Vec<String> {
    // Expand contractions (case-insensitive, applied before lowercasing).
    // Only the subset that Whisper commonly produces and that affect dedup.
    const CONTRACTIONS: &[(&str, &str)] = &[
        ("i'm", "i am"),
        ("I'm", "I am"),
        ("i've", "i have"),
        ("I've", "I have"),
        ("i'd", "i would"),
        ("I'd", "I would"),
        ("i'll", "i will"),
        ("I'll", "I will"),
        ("it's", "it is"),
        ("It's", "It is"),
        ("that's", "that is"),
        ("That's", "That is"),
        ("don't", "do not"),
        ("Don't", "Do not"),
        ("can't", "cannot"),
        ("Can't", "Cannot"),
        ("won't", "will not"),
        ("Won't", "Will not"),
        ("there's", "there is"),
        ("There's", "There is"),
        ("they're", "they are"),
        ("They're", "They are"),
        ("we're", "we are"),
        ("We're", "We are"),
        ("you're", "you are"),
        ("You're", "You are"),
        ("he's", "he is"),
        ("He's", "He is"),
        ("she's", "she is"),
        ("She's", "She is"),
        ("isn't", "is not"),
        ("Isn't", "Is not"),
        ("wasn't", "was not"),
        ("Wasn't", "Was not"),
        ("let's", "let us"),
        ("Let's", "Let us"),
    ];

    let mut s = text.to_string();
    for (contraction, expansion) in CONTRACTIONS {
        if s.contains(contraction) {
            s = s.replace(contraction, expansion);
        }
    }

    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect()
}
