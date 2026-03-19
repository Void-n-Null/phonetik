//! # Phonetik
//!
//! Phonetic analysis engine for English. Sub-millisecond rhyme detection,
//! stress scanning, meter analysis, and syllable counting backed by a
//! 126K-word CMU Pronouncing Dictionary embedded directly in the binary.
//!
//! ## Quick start
//!
//! ```rust
//! use phonetik::Phonetik;
//!
//! let ph = Phonetik::new();
//!
//! // Look up a word
//! let info = ph.lookup("extraordinary").unwrap();
//! println!("{}: {} syllables, {}", info.word, info.syllable_count, info.stress_display);
//!
//! // Find rhymes (all types, best first)
//! for r in ph.rhymes("love", 10) {
//!     println!("  {} ({:?}, {:.0}%)", r.word, r.rhyme_type, r.confidence * 100.0);
//! }
//!
    //! // Scan a line of verse
    //! let scan = ph.scan("uneasy lies the head that wears the crown");
    //! println!("{} — {} ({})", scan.stressed_display, scan.meter.name, scan.meter.regularity);
//!
//! // Compare two words
//! let cmp = ph.compare("cat", "bat").unwrap();
//! println!("similarity: {:.0}%, rhyme: {:?}", cmp.similarity * 100.0, cmp.rhyme_type);
//! ```
//!
//! ## Architecture
//!
//! All dictionary data is compiled into the binary at build time:
//!
//! | Blob | Size | Contents |
//! |------|------|----------|
//! | `cmudict.bin` | 2.3 MB | 126K words, u8-encoded phonemes |
//! | `rhyme_groups.bin` | 993 KB | 14K perfect rhyme tail groups |
//! | `near_neighbors.bin` | 350 KB | 3.2K coda edit-distance graph |
//!
//! No filesystem access required. The binary is fully self-contained.
//!
//! ## Feature flags
//!
//! - **`server`** (default): Includes the HTTP server binary (`phonetik-server`)
//!   and dependencies (axum, tokio, reqwest). Disable with `default-features = false`
//!   for library-only use.

// ── Modules ─────────────────────────────────────────────────────────────
// Public for advanced use and the server binary, but the primary API
// is the `Phonetik` struct. Most users won't need to touch these directly.

#[doc(hidden)]
pub mod coda_groups;
#[doc(hidden)]
pub mod compare;
#[doc(hidden)]
pub mod dict;
#[doc(hidden)]
pub mod distance;
#[doc(hidden)]
pub mod meter;
#[doc(hidden)]
pub mod near_index;
#[doc(hidden)]
pub mod rhyme;
#[doc(hidden)]
pub mod rhyme_index;
#[doc(hidden)]
pub mod rhymemap;
#[doc(hidden)]
pub mod slant_index;
#[doc(hidden)]
pub mod stress;
#[doc(hidden)]
pub mod syllable;

/// Low-level phoneme encoding/decoding.
pub mod phoneme;

#[cfg(feature = "server")]
pub mod server;

use std::sync::Arc;

use serde::Serialize;

use crate::dict::CmuDict;
pub use crate::stress::StressMode;

// ── Public API types ────────────────────────────────────────────────────

/// The phonetic analysis engine. Create one with [`Phonetik::new()`] and
/// call methods on it. Thread-safe and cheaply cloneable — all internal
/// data is reference-counted, so `clone()` is just a handful of pointer
/// bumps with zero allocation. Share freely across threads.
pub struct Phonetik {
    dict: Arc<CmuDict>,
    rhyme_index: Arc<rhyme_index::RhymeIndex>,
    slant_index: Arc<slant_index::SlantIndex>,
    near_index: Arc<near_index::NearIndex>,
    stress_analyzer: Arc<stress::StressAnalyzer>,
    rhyme_mapper: Arc<rhymemap::RhymeMapAnalyzer>,
}

impl Clone for Phonetik {
    fn clone(&self) -> Self {
        Self {
            dict: self.dict.clone(),
            rhyme_index: self.rhyme_index.clone(),
            slant_index: self.slant_index.clone(),
            near_index: self.near_index.clone(),
            stress_analyzer: self.stress_analyzer.clone(),
            rhyme_mapper: self.rhyme_mapper.clone(),
        }
    }
}

impl Phonetik {
    /// Create a new engine with the embedded dictionary.
    ///
    /// Loads the precompiled CMU dict, perfect rhyme groups, and coda
    /// neighbor graph from blobs baked into the binary. No filesystem
    /// access needed.
    ///
    /// Takes ~100ms on first call (HashMap construction from blobs).
    pub fn new() -> Self {
        let dict = Arc::new(CmuDict::load());
        let rhyme_idx = rhyme_index::RhymeIndex::new(dict.clone());
        let (coda_map, _) = coda_groups::build(&dict);
        let shared_codas = Arc::new(coda_map);
        let slant_idx = slant_index::SlantIndex::new(dict.clone(), shared_codas.clone());
        let near_idx = near_index::NearIndex::new(dict.clone(), shared_codas);
        let stress_a = stress::StressAnalyzer::new(dict.clone());
        let rhyme_m = rhymemap::RhymeMapAnalyzer::new(dict.clone());

        Self {
            dict,
            rhyme_index: Arc::new(rhyme_idx),
            slant_index: Arc::new(slant_idx),
            near_index: Arc::new(near_idx),
            stress_analyzer: Arc::new(stress_a),
            rhyme_mapper: Arc::new(rhyme_m),
        }
    }

    // ── Word lookup ─────────────────────────────────────────────────────

    /// Look up a word's phonetic information.
    ///
    /// Returns syllable count, stress pattern, phonemes, and display form.
    /// Returns `None` if the word is not in the dictionary.
    ///
    /// ```rust
    /// # let ph = phonetik::Phonetik::new();
    /// let info = ph.lookup("hello").unwrap();
    /// assert_eq!(info.syllable_count, 2);
    /// ```
    pub fn lookup(&self, word: &str) -> Option<WordInfo> {
        let normalized = CmuDict::normalize(word);
        let variants = self.dict.lookup(word)?;
        let encoded = &variants[0];
        let count = phoneme::count_syllables(encoded);
        let stresses = phoneme::extract_stresses(encoded);
        let syllables = syllable::SyllableSplitter::split(&normalized, count);
        let stress_display = syllable::SyllableSplitter::stress_display(&normalized, &stresses);

        Some(WordInfo {
            word: normalized,
            phonemes: phoneme::decode_to_strings(encoded),
            syllable_count: count,
            syllables,
            stress_pattern: stresses,
            stress_display,
            variant_count: variants.len(),
        })
    }

    /// Count syllables in a word. Returns 0 if not found (estimates for unknown words).
    pub fn syllable_count(&self, word: &str) -> usize {
        if let Some(variants) = self.dict.lookup(word) {
            phoneme::count_syllables(&variants[0])
        } else {
            estimate_syllable_count(word)
        }
    }

    /// Count syllables for each word in each line.
    pub fn syllable_counts(&self, lines: &[&str]) -> Vec<LineSyllableCount> {
        lines
            .iter()
            .map(|line| {
                let tokens = tokenize_words(line);
                let mut total = 0;
                let words: Vec<WordSyllableCount> = tokens
                    .iter()
                    .map(|tok| {
                        let count = self.syllable_count(tok);
                        total += count;
                        WordSyllableCount {
                            word: tok.clone(),
                            syllables: count,
                        }
                    })
                    .collect();
                LineSyllableCount { words, total }
            })
            .collect()
    }

    // ── Rhyme finding ───────────────────────────────────────────────────

    /// Find all rhymes for a word, merged across types.
    ///
    /// Returns perfect rhymes first (confidence 1.0), then slant, then near.
    /// Results are deduplicated and capped at `limit`.
    ///
    /// ```rust
    /// # let ph = phonetik::Phonetik::new();
    /// let rhymes = ph.rhymes("love", 10);
    /// assert!(!rhymes.is_empty());
    /// assert_eq!(rhymes[0].rhyme_type, phonetik::RhymeType::Perfect);
    /// ```
    pub fn rhymes(&self, word: &str, limit: usize) -> Vec<RhymeMatch> {
        let limit = limit.min(500);
        let mut matches = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Perfect rhymes first
        for m in self.perfect_rhymes(word) {
            if seen.insert(m.word.clone()) {
                matches.push(m);
            }
        }

        // Slant rhymes
        let slant_limit = limit.saturating_sub(matches.len());
        if slant_limit > 0 {
            for m in self.slant_rhymes(word, slant_limit) {
                if seen.insert(m.word.clone()) {
                    matches.push(m);
                }
            }
        }

        // Near rhymes
        let near_limit = limit.saturating_sub(matches.len());
        if near_limit > 0 {
            for m in self.near_rhymes(word, near_limit) {
                if seen.insert(m.word.clone()) {
                    matches.push(m);
                }
            }
        }

        matches.truncate(limit);
        matches
    }

    /// Find perfect rhymes only (identical sound from last stressed vowel onward).
    pub fn perfect_rhymes(&self, word: &str) -> Vec<RhymeMatch> {
        self.rhyme_index
            .lookup(word)
            .map(|r| {
                r.matches
                    .into_iter()
                    .map(|m| RhymeMatch {
                        word: m.word,
                        phonemes: m.phonemes,
                        syllables: m.syllables,
                        rhyme_type: RhymeType::Perfect,
                        confidence: 1.0,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find slant rhymes (same coda consonants, different vowel).
    pub fn slant_rhymes(&self, word: &str, limit: usize) -> Vec<RhymeMatch> {
        self.slant_index
            .lookup(word, limit.min(500))
            .map(|r| {
                r.matches
                    .into_iter()
                    .map(|m| RhymeMatch {
                        word: m.word,
                        phonemes: m.phonemes,
                        syllables: m.syllables,
                        rhyme_type: RhymeType::Slant,
                        confidence: m.confidence,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Find near rhymes (same vowel, coda differs by one consonant).
    pub fn near_rhymes(&self, word: &str, limit: usize) -> Vec<RhymeMatch> {
        self.near_index
            .lookup(word, limit.min(500))
            .map(|r| {
                r.matches
                    .into_iter()
                    .map(|m| RhymeMatch {
                        word: m.word,
                        phonemes: m.phonemes,
                        syllables: m.syllables,
                        rhyme_type: RhymeType::Near,
                        confidence: 0.5,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // ── Prosody analysis ────────────────────────────────────────────────

    /// Perform scansion on a line of text — identify its stress pattern,
    /// meter (e.g. iambic pentameter), and syllable count.
    ///
    /// Uses [`StressMode::Spoken`] by default, which demotes function
    /// words (I, the, to, shall, etc.) to unstressed — matching how
    /// verse is naturally read aloud. Use [`scan_with_mode`] for raw
    /// dictionary stress.
    ///
    /// ```rust
    /// # let ph = phonetik::Phonetik::new();
    /// let scan = ph.scan("uneasy lies the head that wears the crown");
    /// assert_eq!(scan.syllable_count, 10); // iambic pentameter
    /// ```
    pub fn scan(&self, line: &str) -> LineScan {
        self.scan_with_mode(line, StressMode::Spoken)
    }

    /// Perform scansion with an explicit stress mode.
    ///
    /// - [`StressMode::Spoken`] — natural speech stress (default for [`scan`]).
    /// - [`StressMode::Dictionary`] — raw CMUdict citation stress.
    pub fn scan_with_mode(&self, line: &str, mode: StressMode) -> LineScan {
        let analysis = self.stress_analyzer.analyze_line_with_mode(line, mode);
        let meter_result = meter::MeterDetector::detect(&analysis.binary_pattern);
        let visual = format_stress_visual(&analysis.binary_pattern);

        LineScan {
            text: line.to_string(),
            stressed_display: analysis.stressed_display,
            stress_pattern: analysis.stress_pattern,
            binary_pattern: analysis.binary_pattern,
            syllable_count: analysis.syllable_count,
            visual,
            meter: MeterInfo {
                name: meter_result.meter_name,
                foot_type: meter_result.foot_type,
                foot_count: meter_result.foot_count,
                regularity: meter_result.regularity,
            },
        }
    }

    /// Compare two words phonetically.
    ///
    /// Returns similarity score, rhyme classification, and confidence.
    pub fn compare(&self, word1: &str, word2: &str) -> Option<Comparison> {
        let l1 = self.dict.lookup(word1)?;
        let l2 = self.dict.lookup(word2)?;

        let (score, best_a, best_b) = compare::PhoneticComparer::best_similarity(&l1, &l2);
        let rhyme_result = rhyme::RhymeAnalyzer::best_rhyme(&l1, &l2);

        let rhyme_type = match rhyme_result.rhyme_type.as_str() {
            "perfect" => RhymeType::Perfect,
            "identity" => RhymeType::Perfect,
            "near" => RhymeType::Near,
            "slant" => RhymeType::Slant,
            _ => RhymeType::None,
        };

        Some(Comparison {
            word1: CmuDict::normalize(word1),
            word2: CmuDict::normalize(word2),
            similarity: (score * 10000.0).round() / 10000.0,
            rhyme_type,
            confidence: rhyme_result.confidence,
            phonemes1: phoneme::decode_to_strings(best_a),
            phonemes2: phoneme::decode_to_strings(best_b),
        })
    }

    /// Detect phoneme repetition patterns across lines of verse.
    ///
    /// Returns n-gram patterns sorted by score, with per-word highlights.
    pub fn rhyme_map(&self, lines: &[&str]) -> rhymemap::RhymeMapResult {
        let owned: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        let opts = rhymemap::RhymeMapOptions::default();
        self.rhyme_mapper.analyze(&owned, &opts)
    }

    // ── Dictionary access ───────────────────────────────────────────────

    /// Check if a word is in the dictionary.
    pub fn contains(&self, word: &str) -> bool {
        self.dict.lookup(word).is_some()
    }

    /// Number of words in the dictionary.
    pub fn word_count(&self) -> usize {
        self.dict.entry_count()
    }
}

impl Default for Phonetik {
    fn default() -> Self {
        Self::new()
    }
}

// ── Public return types ─────────────────────────────────────────────────

/// Information about a single word's phonetic properties.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordInfo {
    /// Normalized (uppercase) spelling.
    pub word: String,
    /// ARPAbet phoneme sequence (e.g. `["HH", "AH0", "L", "OW1"]`).
    pub phonemes: Vec<String>,
    /// Number of syllables.
    pub syllable_count: usize,
    /// Estimated syllable boundaries (e.g. `["HEL", "LO"]`).
    pub syllables: Vec<String>,
    /// Stress values per syllable: 0=unstressed, 1=primary, 2=secondary.
    pub stress_pattern: Vec<i32>,
    /// Human-readable stress display (e.g. `"hel-LO"`).
    pub stress_display: String,
    /// Number of pronunciation variants in the dictionary.
    pub variant_count: usize,
}

/// A word that rhymes with the query.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhymeMatch {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
    pub rhyme_type: RhymeType,
    /// 0.0–1.0. Perfect = 1.0, slant varies by vowel distance, near = 0.5.
    pub confidence: f64,
}

/// Classification of a rhyme relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RhymeType {
    /// Identical sound from last stressed vowel onward (night/fight).
    Perfect,
    /// Same coda consonants, different vowel (love/move).
    Slant,
    /// Same vowel, coda differs by one consonant (night/nice).
    Near,
    /// No meaningful rhyme relationship.
    None,
}

/// Result of scansion — stress and meter analysis of a line of text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineScan {
    pub text: String,
    /// Stress display with uppercase=stressed (e.g. `"shall I com-PARE thee TO a SUM-mer's DAY"`).
    pub stressed_display: String,
    /// Raw stress values per syllable (0, 1, 2).
    pub stress_pattern: Vec<i32>,
    /// Binary stress (0=unstressed, 1=stressed).
    pub binary_pattern: Vec<i32>,
    pub syllable_count: usize,
    /// Visual stress marks (e.g. `"x / x / x / x / x /"`).
    pub visual: String,
    pub meter: MeterInfo,
}

/// Detected metrical pattern.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeterInfo {
    /// Full name (e.g. `"iambic pentameter"`).
    pub name: String,
    /// Foot type (e.g. `"iamb"`).
    pub foot_type: String,
    /// Number of feet.
    pub foot_count: usize,
    /// 0.0–1.0 regularity score.
    pub regularity: f64,
}

/// Result of comparing two words phonetically.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub word1: String,
    pub word2: String,
    /// 0.0–1.0 phonetic similarity.
    pub similarity: f64,
    pub rhyme_type: RhymeType,
    pub confidence: f64,
    pub phonemes1: Vec<String>,
    pub phonemes2: Vec<String>,
}

/// Syllable count for a single word.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordSyllableCount {
    pub word: String,
    pub syllables: usize,
}

/// Syllable counts for a line of text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSyllableCount {
    pub words: Vec<WordSyllableCount>,
    pub total: usize,
}

// ── Private helpers ─────────────────────────────────────────────────────

fn format_stress_visual(binary: &[i32]) -> String {
    if binary.is_empty() {
        return String::new();
    }
    let mut s = String::with_capacity(binary.len() * 2);
    for (i, &b) in binary.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push(if b == 1 { '/' } else { 'x' });
    }
    s
}

fn tokenize_words(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for c in line.chars() {
        if c.is_alphabetic() || c == '\'' || c == '-' {
            current.push(c);
        } else if !current.is_empty() {
            tokens.push(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn estimate_syllable_count(word: &str) -> usize {
    let mut count = 0;
    let mut in_vowel = false;
    for c in word.chars() {
        let is_v = "aeiouyAEIOUY".contains(c);
        if is_v && !in_vowel {
            count += 1;
        }
        in_vowel = is_v;
    }
    if count == 0 {
        1
    } else {
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ph() -> Phonetik {
        Phonetik::new()
    }

    // ── Clone ───────────────────────────────────────────────────────────

    #[test]
    fn clone_shares_data() {
        let a = ph();
        let b = a.clone();
        assert_eq!(a.word_count(), b.word_count());
        // Both should resolve the same word
        assert!(a.lookup("cat").is_some());
        assert!(b.lookup("cat").is_some());
    }

    // ── Lookup ──────────────────────────────────────────────────────────

    #[test]
    fn lookup_returns_word_info() {
        let p = ph();
        let info = p.lookup("extraordinary").unwrap();
        assert_eq!(info.word, "EXTRAORDINARY");
        assert!(info.syllable_count >= 5);
        assert!(!info.phonemes.is_empty());
        assert!(!info.syllables.is_empty());
        assert!(info.variant_count >= 1);
    }

    #[test]
    fn lookup_unknown_word() {
        let p = ph();
        assert!(p.lookup("xyzzyplugh").is_none());
    }

    // ── Syllable counting ───────────────────────────────────────────────

    #[test]
    fn syllable_count_known_word() {
        let p = ph();
        assert_eq!(p.syllable_count("cat"), 1);
        assert_eq!(p.syllable_count("hello"), 2);
    }

    #[test]
    fn syllable_count_unknown_falls_back_to_estimate() {
        let p = ph();
        let count = p.syllable_count("xyzzyplugh");
        assert!(count >= 1);
    }

    #[test]
    fn syllable_counts_batch() {
        let p = ph();
        let result = p.syllable_counts(&["hello world", "the cat"]);
        assert_eq!(result.len(), 2);
        assert!(result[0].total >= 3);
        assert!(result[1].total >= 2);
    }

    // ── Rhymes ──────────────────────────────────────────────────────────

    #[test]
    fn rhymes_returns_perfect_first() {
        let p = ph();
        let results = p.rhymes("cat", 20);
        assert!(!results.is_empty());
        assert_eq!(results[0].rhyme_type, RhymeType::Perfect);
    }

    #[test]
    fn perfect_rhymes_known_pair() {
        let p = ph();
        let results = p.perfect_rhymes("cat");
        let words: Vec<&str> = results.iter().map(|r| r.word.as_str()).collect();
        assert!(words.contains(&"BAT"));
    }

    #[test]
    fn slant_rhymes_returns_results() {
        let p = ph();
        let results = p.slant_rhymes("love", 20);
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.rhyme_type, RhymeType::Slant);
        }
    }

    #[test]
    fn near_rhymes_returns_results() {
        let p = ph();
        let results = p.near_rhymes("night", 20);
        assert!(!results.is_empty());
        for r in &results {
            assert_eq!(r.rhyme_type, RhymeType::Near);
        }
    }

    #[test]
    fn rhymes_respects_limit() {
        let p = ph();
        let results = p.rhymes("the", 5);
        assert!(results.len() <= 5);
    }

    #[test]
    fn rhymes_deduplicates() {
        let p = ph();
        let results = p.rhymes("cat", 200);
        let mut words: Vec<&str> = results.iter().map(|r| r.word.as_str()).collect();
        let len_before = words.len();
        words.sort();
        words.dedup();
        assert_eq!(words.len(), len_before, "duplicates found in rhyme results");
    }

    // ── Scan ────────────────────────────────────────────────────────────

    #[test]
    fn scan_iambic_pentameter() {
        let p = ph();
        let scan = p.scan("uneasy lies the head that wears the crown");
        assert_eq!(scan.syllable_count, 10);
        assert!(scan.meter.name.contains("iambic"));
        assert!(!scan.visual.is_empty());
    }

    #[test]
    fn scan_empty_line() {
        let p = ph();
        let scan = p.scan("");
        assert_eq!(scan.syllable_count, 0);
    }

    // ── Compare ─────────────────────────────────────────────────────────

    #[test]
    fn compare_rhyming_pair() {
        let p = ph();
        let cmp = p.compare("cat", "bat").unwrap();
        assert!(cmp.similarity > 0.5);
        assert_eq!(cmp.rhyme_type, RhymeType::Perfect);
    }

    #[test]
    fn compare_unknown_word_returns_none() {
        let p = ph();
        assert!(p.compare("cat", "xyzzyplugh").is_none());
    }

    // ── Rhyme map ───────────────────────────────────────────────────────

    #[test]
    fn rhyme_map_finds_patterns() {
        let p = ph();
        let result = p.rhyme_map(&["the cat sat on the mat", "the bat sat on the hat"]);
        assert!(!result.patterns.is_empty());
    }

    // ── contains / word_count ───────────────────────────────────────────

    #[test]
    fn contains_known_and_unknown() {
        let p = ph();
        assert!(p.contains("hello"));
        assert!(!p.contains("xyzzyplugh"));
    }

    #[test]
    fn word_count_is_substantial() {
        let p = ph();
        assert!(p.word_count() > 100_000);
    }

    // ── Private helpers ─────────────────────────────────────────────────

    #[test]
    fn format_stress_visual_fn() {
        assert_eq!(format_stress_visual(&[0, 1, 0, 1]), "x / x /");
        assert_eq!(format_stress_visual(&[]), "");
    }

    #[test]
    fn tokenize_words_fn() {
        assert_eq!(tokenize_words("hello, world!"), vec!["hello", "world"]);
        assert_eq!(tokenize_words("don't stop"), vec!["don't", "stop"]);
        assert!(tokenize_words("").is_empty());
    }

    #[test]
    fn estimate_syllable_count_fn() {
        assert_eq!(estimate_syllable_count("cat"), 1);
        assert_eq!(estimate_syllable_count("hello"), 2);
        assert_eq!(estimate_syllable_count("brr"), 1); // no vowels → 1
    }

    #[test]
    fn default_impl_works() {
        let p = Phonetik::default();
        assert!(p.word_count() > 100_000);
    }
}
