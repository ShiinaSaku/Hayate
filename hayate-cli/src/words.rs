//! Cryptographically secure random 3-word phrase generator.

use rand::RngExt;

const WORDS: &[&str] = &[
    "apple", "apricot", "acoustic", "active", "arrow", "anchor", "artist", "atom", "banana",
    "beacon", "bronze", "brave", "bridge", "breeze", "bright", "bubble", "cherry", "canyon",
    "crystal", "copper", "castle", "coyote", "crater", "circus", "doctor", "desert", "dragon",
    "dialog", "domain", "double", "dental", "donkey", "engine", "exile", "emerald", "energy",
    "epoch", "eagle", "effect", "exotic", "forest", "frozen", "fossil", "falcon", "famous",
    "future", "factor", "flight", "galaxy", "garden", "garlic", "guitar", "genius", "gloria",
    "gravel", "growth", "harbor", "hunter", "hazard", "hybrid", "header", "humble", "honest",
    "impact", "island", "invent", "indigo", "infant", "inside", "irony", "infuse", "invite",
    "jungle", "jacket", "jordan", "jockey", "jaguar", "jovial", "jersey", "junior", "knight",
    "kernel", "keyboard", "kitchen", "kodiak", "krypton", "karate", "koala", "lemon", "lizard",
    "liquid", "lantern", "legend", "legacy", "laptop", "logger", "mountain", "marble", "matrix",
    "magnet", "melody", "memory", "modern", "museum", "network", "nature", "nebula", "neutral",
    "nomad", "notable", "notice", "novice", "ocean", "orange", "orbit", "oxygen", "oyster",
    "online", "octave", "object", "planet", "pocket", "python", "pioneer", "pattern", "plastic",
    "purple", "phantom", "quartz", "quiver", "quasar", "quality", "quantum", "quarter", "queens",
    "quench", "river", "rabbit", "radar", "rescue", "rhythm", "rust", "rapid", "random", "shadow",
    "silver", "spring", "summit", "sphere", "silent", "system", "spiral", "tunnel", "timber",
    "target", "theory", "tiger", "triple", "travel", "trophy", "urban", "unify", "unique",
    "update", "upload", "urgent", "utility", "vacuum", "valley", "velvet", "vector", "violin",
    "vintage", "virtual", "vortex", "volume", "winter", "wisdom", "wizard", "wonder", "wooden",
    "worker", "web", "weather", "xenon", "xerox", "xylem", "xanadu", "yacht", "yellow", "youth",
    "yoga", "zebra", "zenith", "zero", "zipper", "zodiac", "zombie", "zeroize", "zone", "alpha",
    "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet", "kilo",
    "lima", "mike", "november", "oscar", "papa", "quebec", "romeo", "sierra", "tango", "uniform",
    "victor", "whiskey", "xray", "yankee", "zulu", "absent", "absorb", "accent", "acid", "adapt",
    "admit", "advice", "agent", "agree", "ahead", "aim", "alarm", "album", "alert", "alike",
    "alive", "alley", "alone", "along", "alter", "always", "amaze", "amber", "among", "amount",
    "arcade", "angel", "angle", "angry", "animal", "annual", "answer", "anthem", "antique",
    "anxious", "apex", "apology", "appeal", "appear", "append", "arm", "army", "around", "layout",
    "arrest", "arrival",
];

pub fn generate_phrase() -> String {
    let mut rng = rand::rng();
    let w1 = WORDS[rng.random_range(0..WORDS.len())];
    let w2 = WORDS[rng.random_range(0..WORDS.len())];
    let w3 = WORDS[rng.random_range(0..WORDS.len())];
    format!("{}-{}-{}", w1, w2, w3)
}
