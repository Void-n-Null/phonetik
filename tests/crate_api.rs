//! Integration tests for phonetik as an external crate consumer.
//!
//! These tests import `phonetik` the same way a downstream project would
//! and exercise the entire public API surface. Nothing here accesses
//! internal modules — only what's visible through `phonetik::*`.

use phonetik::{
    Comparison, LineScan, LineSyllableCount, MeterInfo, Phonetik, RhymeMatch, RhymeType, WordInfo,
    WordSyllableCount,
};

// ── Construction & cloning ──────────────────────────────────────────────

#[test]
fn construct_and_clone() {
    let engine = Phonetik::new();
    assert!(engine.word_count() > 100_000);

    let clone = engine.clone();
    assert_eq!(engine.word_count(), clone.word_count());
}

#[test]
fn default_is_equivalent_to_new() {
    let a = Phonetik::new();
    let b = Phonetik::default();
    assert_eq!(a.word_count(), b.word_count());
}

// ── Lookup ──────────────────────────────────────────────────────────────

#[test]
fn lookup_known_word_returns_word_info() {
    let p = Phonetik::new();
    let info: WordInfo = p.lookup("extraordinary").unwrap();
    assert_eq!(info.word, "EXTRAORDINARY");
    assert!(info.syllable_count >= 5);
    assert!(!info.phonemes.is_empty());
    assert!(!info.syllables.is_empty());
    assert!(info.variant_count >= 1);
}

#[test]
fn lookup_is_case_insensitive() {
    let p = Phonetik::new();
    let a = p.lookup("Hello").unwrap();
    let b = p.lookup("HELLO").unwrap();
    let c = p.lookup("hello").unwrap();
    assert_eq!(a.word, b.word);
    assert_eq!(b.word, c.word);
}

#[test]
fn lookup_unknown_returns_none() {
    let p = Phonetik::new();
    assert!(p.lookup("xyzzyplugh").is_none());
}

#[test]
fn contains_matches_lookup() {
    let p = Phonetik::new();
    assert!(p.contains("hello"));
    assert!(!p.contains("xyzzyplugh"));
}

// ── Syllable counting ───────────────────────────────────────────────────

#[test]
fn syllable_count_known_words() {
    let p = Phonetik::new();
    assert_eq!(p.syllable_count("cat"), 1);
    assert_eq!(p.syllable_count("hello"), 2);
    assert_eq!(p.syllable_count("beautiful"), 3);
}

#[test]
fn syllable_count_unknown_word_estimates() {
    let p = Phonetik::new();
    let count = p.syllable_count("blarglesnarf");
    assert!(
        count >= 1,
        "unknown word should still estimate at least 1 syllable"
    );
}

#[test]
fn syllable_counts_batch_returns_per_line() {
    let p = Phonetik::new();
    let results: Vec<LineSyllableCount> =
        p.syllable_counts(&["hello world", "the quick brown fox"]);
    assert_eq!(results.len(), 2);
    assert!(results[0].total >= 3); // hel-lo world
    assert!(results[1].total >= 4); // the quick brown fox
    assert!(!results[0].words.is_empty());
    // Each word entry should have the right types
    let _: &Vec<WordSyllableCount> = &results[0].words;
}

// ── Rhyme analysis ──────────────────────────────────────────────────────

#[test]
fn rhymes_returns_mixed_types() {
    let p = Phonetik::new();
    let matches: Vec<RhymeMatch> = p.rhymes("cat", 50);
    assert!(!matches.is_empty());

    // Should contain at least one perfect rhyme
    assert!(
        matches.iter().any(|m| m.rhyme_type == RhymeType::Perfect),
        "cat should have perfect rhymes"
    );
}

#[test]
fn rhymes_deduplicates_across_types() {
    let p = Phonetik::new();
    let matches = p.rhymes("love", 200);
    let mut words: Vec<&str> = matches.iter().map(|m| m.word.as_str()).collect();
    let before = words.len();
    words.sort();
    words.dedup();
    assert_eq!(words.len(), before, "rhymes should not contain duplicates");
}

#[test]
fn rhymes_respects_limit() {
    let p = Phonetik::new();
    let matches = p.rhymes("the", 10);
    assert!(matches.len() <= 10);
}

#[test]
fn perfect_rhymes_are_all_perfect() {
    let p = Phonetik::new();
    let matches: Vec<RhymeMatch> = p.perfect_rhymes("night");
    assert!(!matches.is_empty());
    for m in &matches {
        assert_eq!(m.rhyme_type, RhymeType::Perfect);
    }
}

#[test]
fn slant_rhymes_are_all_slant() {
    let p = Phonetik::new();
    let matches: Vec<RhymeMatch> = p.slant_rhymes("love", 20);
    assert!(!matches.is_empty());
    for m in &matches {
        assert_eq!(m.rhyme_type, RhymeType::Slant);
    }
}

#[test]
fn near_rhymes_are_all_near() {
    let p = Phonetik::new();
    let matches: Vec<RhymeMatch> = p.near_rhymes("night", 20);
    assert!(!matches.is_empty());
    for m in &matches {
        assert_eq!(m.rhyme_type, RhymeType::Near);
    }
}

#[test]
fn rhyme_match_fields_are_populated() {
    let p = Phonetik::new();
    let matches = p.perfect_rhymes("cat");
    let m = &matches[0];
    assert!(!m.word.is_empty());
    assert!(!m.phonemes.is_empty());
    assert!(m.syllables >= 1);
    assert!(m.confidence > 0.0);
}

// ── Meter scanning ──────────────────────────────────────────────────────

#[test]
fn scan_iambic_pentameter() {
    let p = Phonetik::new();
    let scan: LineScan = p.scan("uneasy lies the head that wears the crown");
    assert_eq!(scan.syllable_count, 10);
    assert!(!scan.visual.is_empty());

    let meter: &MeterInfo = &scan.meter;
    assert!(
        meter.name.contains("iambic"),
        "expected iambic, got {}",
        meter.name
    );
    assert!(meter.regularity > 0.0);
}

#[test]
fn scan_empty_line() {
    let p = Phonetik::new();
    let scan = p.scan("");
    assert_eq!(scan.syllable_count, 0);
    assert_eq!(scan.binary_pattern.len(), 0);
}

#[test]
fn scan_returns_binary_pattern_of_correct_length() {
    let p = Phonetik::new();
    let scan = p.scan("hello world");
    assert_eq!(scan.binary_pattern.len(), scan.syllable_count);
    for &b in &scan.binary_pattern {
        assert!(b == 0 || b == 1, "binary pattern should be 0 or 1, got {b}");
    }
}

// ── Phonetic comparison ─────────────────────────────────────────────────

#[test]
fn compare_rhyming_pair() {
    let p = Phonetik::new();
    let cmp: Comparison = p.compare("cat", "bat").unwrap();
    assert!(cmp.similarity > 0.5);
    assert_eq!(cmp.rhyme_type, RhymeType::Perfect);
    assert!(!cmp.word1.is_empty());
    assert!(!cmp.word2.is_empty());
}

#[test]
fn compare_non_rhyming_pair() {
    let p = Phonetik::new();
    let cmp = p.compare("cat", "elephant").unwrap();
    assert!(cmp.similarity < 0.5);
}

#[test]
fn compare_unknown_word_returns_none() {
    let p = Phonetik::new();
    assert!(p.compare("cat", "xyzzyplugh").is_none());
    assert!(p.compare("xyzzyplugh", "cat").is_none());
}

// ── Rhyme map ───────────────────────────────────────────────────────────

#[test]
fn rhyme_map_detects_patterns() {
    let p = Phonetik::new();
    let result = p.rhyme_map(&["the cat sat on the mat", "the bat sat on the hat"]);
    assert_eq!(result.lines.len(), 2);
    assert!(!result.words.is_empty());
    assert!(!result.patterns.is_empty());
}

#[test]
fn rhyme_map_single_line() {
    let p = Phonetik::new();
    let result = p.rhyme_map(&["hello world"]);
    assert_eq!(result.lines.len(), 1);
    assert!(!result.words.is_empty());
}

// ── Thread safety ───────────────────────────────────────────────────────

#[test]
fn engine_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Phonetik>();
}

#[test]
fn clone_across_threads() {
    let p = Phonetik::new();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let engine = p.clone();
            std::thread::spawn(move || {
                assert!(engine.contains("hello"));
                engine.syllable_count("beautiful")
            })
        })
        .collect();

    for h in handles {
        let count = h.join().unwrap();
        assert_eq!(count, 3);
    }
}

// ── Serialization ───────────────────────────────────────────────────────

#[test]
fn public_types_serialize_to_json() {
    let p = Phonetik::new();

    // WordInfo
    let info = p.lookup("cat").unwrap();
    let json = serde_json::to_value(&info).unwrap();
    assert!(json.get("word").is_some());
    assert!(json.get("phonemes").is_some());

    // RhymeMatch
    let rhymes = p.perfect_rhymes("cat");
    let json = serde_json::to_value(&rhymes[0]).unwrap();
    assert!(json.get("word").is_some());
    assert!(json.get("rhymeType").is_some()); // camelCase

    // LineScan
    let scan = p.scan("hello world");
    let json = serde_json::to_value(&scan).unwrap();
    assert!(json.get("syllableCount").is_some());
    assert!(json.get("binaryPattern").is_some());

    // Comparison
    let cmp = p.compare("cat", "bat").unwrap();
    let json = serde_json::to_value(&cmp).unwrap();
    assert!(json.get("similarity").is_some());
    assert!(json.get("rhymeType").is_some());
}
