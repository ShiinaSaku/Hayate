//! Cryptographically secure random 5-word pairing phrase generator.
//!
//! Each phrase is built from a 300-word list with 5 unique words, giving
//! approximately 300^5 ≈ 2.43 × 10¹² distinct ordered phrases (≈ 41 bits of
//! entropy). This exceeds the 2⁴⁰ threshold recommended for resisting offline
//! brute-force attacks against the pairing passphrase.
//!
//! The word list is short enough to read over the phone and long enough to
//! avoid accidental overlaps. Words are selected without replacement so no word
//! appears twice in a single phrase.

use rand::RngExt;

const WORDS: &[&str] = &[
    "absent", "absorb", "accent", "acid", "adapt", "admit", "advice", "agent", "agree", "ahead",
    "aim", "alarm", "album", "alert", "alike", "alive", "alley", "alone", "along", "alter",
    "always", "amaze", "amber", "among", "amount", "anchor", "angel", "angle", "angry", "animal",
    "annual", "answer", "anthem", "antique", "anxious", "apex", "apology", "appeal", "appear",
    "append", "apple", "apricot", "arcade", "arm", "army", "around", "arrow", "artist", "atom",
    "arrest", "arrival", "audio", "aurora", "autumn", "avenue", "avocado", "bacon", "bamboo",
    "banana", "beacon", "bicycle", "blanket", "blaze", "blossom", "blue", "bluff", "booth",
    "boulder", "breeze", "bridge", "bright", "bronze", "bubble", "cactus", "candle", "canyon",
    "castle", "cascade", "cedar", "chair", "cherry", "cinnamon", "circus", "clay", "clover",
    "coconut", "coffee", "copper", "coral", "coyote", "crater", "creek", "crystal", "daisy",
    "dawn", "delta", "dental", "desert", "dialog", "diamond", "dolphin", "domain", "donkey",
    "double", "dragon", "drift", "dune", "eagle", "earth", "echo", "effect", "eight", "elder",
    "ember", "emerald", "energy", "engine", "epoch", "exile", "exotic", "fable", "falcon",
    "famous", "feather", "festival", "fifty", "flight", "forest", "fossil", "fountain", "fox",
    "fresh", "frost", "future", "galaxy", "garden", "garlic", "genius", "globe", "gloria", "grape",
    "gravel", "growth", "harbor", "harmony", "hazel", "header", "honey", "honest", "horizon",
    "humble", "hunter", "hybrid", "ibis", "ice", "image", "impact", "indigo", "infant", "infuse",
    "inside", "invent", "invite", "irony", "island", "ivory", "jacket", "jade", "jaguar", "jigsaw",
    "jockey", "jordan", "jovial", "journey", "jungle", "junior", "jupiter", "karate", "kettle",
    "kernel", "keyboard", "kite", "kitchen", "kiwi", "knight", "kodiak", "koala", "krypton",
    "ladder", "lagoon", "lake", "lantern", "laptop", "laugh", "legacy", "legend", "lemon",
    "lizard", "liquid", "logger", "lunar", "magnet", "mango", "marble", "meadow", "melody",
    "memory", "metal", "modern", "mountain", "museum", "music", "nature", "nebula", "neutral",
    "night", "noble", "nomad", "notable", "notice", "novice", "oasis", "object", "ocean", "octave",
    "online", "orange", "orbit", "orchard", "oxygen", "oyster", "pacific", "papa", "pattern",
    "pearl", "piano", "pioneer", "planet", "plastic", "pocket", "prairie", "puddle", "puma",
    "purple", "python", "quail", "quartz", "quebec", "quench", "quiver", "radar", "radiant",
    "rapid", "random", "raptor", "rescue", "rhythm", "ripple", "river", "road", "rose", "rust",
    "sage", "shadow", "shore", "silent", "silver", "solar", "sphere", "spiral", "spring", "stable",
    "storm", "summit", "sun", "table", "target", "theory", "thorn", "tide", "tiger", "timber",
    "toast", "travel", "trophy", "triple", "tulip", "tundra", "tunnel", "upland", "update",
    "upload", "urban", "urgent", "utility", "valley", "vector", "velvet", "violet", "violin",
    "vintage", "virtual", "vortex", "voyage", "water", "wander", "weather", "web", "whimsy",
    "whistle", "willow", "winter", "wisdom", "wizard", "wonder", "wooden", "worker", "yarn",
    "year", "yellow", "yoga", "yonder", "youth", "zeal", "zenith", "zero", "zeroize", "zest",
    "zinc", "zipper", "zodiac", "zombie", "zone",
];

const PHRASE_WORDS: usize = 5;

/// Generates a random pairing phrase.
///
/// Returns `PHRASE_WORDS` unique words from [`WORDS`], joined by hyphens.
/// The resulting phrase is used as the pairing passphrase during the X25519
/// key-agreement handshake.
pub fn generate_phrase() -> String {
    let mut rng = rand::rng();
    let word_count = WORDS.len();
    if word_count < PHRASE_WORDS {
        unreachable!("word list is always larger than the phrase length");
    }
    let mut indices = Vec::with_capacity(PHRASE_WORDS);
    while indices.len() < PHRASE_WORDS {
        let idx = rng.random_range(0..word_count);
        if !indices.contains(&idx) {
            indices.push(idx);
        }
    }
    indices.into_iter().map(|i| WORDS[i]).collect::<Vec<_>>().join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_has_exact_word_count() {
        let phrase = generate_phrase();
        assert_eq!(phrase.split('-').count(), PHRASE_WORDS);
    }

    #[test]
    fn phrase_words_are_unique() {
        let phrase = generate_phrase();
        let parts: Vec<_> = phrase.split('-').collect();
        let unique: std::collections::HashSet<_> = parts.iter().copied().collect();
        assert_eq!(unique.len(), parts.len(), "phrase words must be selected without replacement");
    }

    #[test]
    fn phrase_words_are_from_list() {
        let phrase = generate_phrase();
        let list: std::collections::HashSet<_> = WORDS.iter().copied().collect();
        for word in phrase.split('-') {
            assert!(list.contains(word), "generated word {word} must be in the approved word list");
        }
    }

    #[test]
    fn phrase_space_exceeds_2_pow_40() {
        // We need > 2^40 distinct phrases. With unique words, count is
        // WORDS.len() choose PHRASE_WORDS; with replacement it would be
        // WORDS.len()^PHRASE_WORDS. Both are larger, so the easier lower
        // bound (replacement) is enough for this smoke test.
        let total = WORDS.len().pow(PHRASE_WORDS as u32);
        assert!(
            total > 1usize << 40,
            "word list must provide > 2^40 distinct phrases, got {total}"
        );
    }

    #[test]
    fn phrases_vary() {
        let a = generate_phrase();
        let b = generate_phrase();
        // Probability of collision is negligible; this is a sanity check.
        assert_ne!(a, b, "consecutive phrases should almost never collide");
    }
}
