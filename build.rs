//! Build script: precompiles cmudict.dict into two binary blobs.
//!
//! 1. cmudict.bin  — every word + pre-encoded u8 phonemes
//! 2. rhyme_groups.bin — perfect rhyme index: tail → word list
//!
//! Both are embedded via include_bytes!() so the binary ships with the
//! entire dictionary and perfect rhyme index baked in. Zero filesystem
//! access needed for these at runtime.
//!
//! == cmudict.bin format ==
//!   [u32 LE]  entry_count
//!   For each entry (sorted by word):
//!     [u16 LE]  word_len
//!     [bytes]   word (uppercase ASCII)
//!     [u8]      variant_count
//!     For each variant:
//!       [u8]    phoneme_count
//!       [bytes] phoneme IDs (one u8 per phoneme)
//!
//! == rhyme_groups.bin format ==
//!   [u32 LE]  group_count
//!   For each group (sorted by tail for deterministic builds):
//!     [u8]      tail_len (phoneme count in the stripped tail)
//!     [bytes]   tail phoneme IDs (stress-stripped)
//!     [u32 LE]  word_count
//!     For each word (sorted):
//!       [u16 LE]  word_len
//!       [bytes]   word (uppercase ASCII)

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const VOWEL_BASE: u8 = 25;
const VOWEL_END: u8 = 39;
const STRESS_1_OFFSET: u8 = 40;
const STRESS_2_OFFSET: u8 = 80;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dict_blob_path = Path::new(&out_dir).join("cmudict.bin");
    let rhyme_blob_path = Path::new(&out_dir).join("rhyme_groups.bin");

    let dict_path = Path::new("cmudict.dict");
    println!("cargo::rerun-if-changed=cmudict.dict");

    if !dict_path.exists() {
        panic!(
            "cmudict.dict not found at {:?}. This file should be included \
             in the crate package. If you're building from source, make sure \
             cmudict.dict is in the crate root directory.",
            std::env::current_dir().unwrap().join("cmudict.dict")
        );
    }

    let contents = std::fs::read_to_string(dict_path).unwrap();

    // ── Phase 1: Parse dict ─────────────────────────────────────────────
    let mut dict: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();

    for line in contents.lines() {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let space_idx = line.find("  ").or_else(|| line.find(' '));
        let space_idx = match space_idx {
            Some(i) => i,
            None => continue,
        };
        let raw_word = &line[..space_idx];
        let phonemes_str = line[space_idx..].trim();

        let phonemes: Vec<u8> = phonemes_str
            .split_whitespace()
            .map(|s| encode_phoneme(s))
            .collect();

        let word = if let Some(paren) = raw_word.find('(') {
            &raw_word[..paren]
        } else {
            raw_word
        };
        let word = word.to_uppercase();

        dict.entry(word).or_default().push(phonemes);
    }

    // ── Phase 2: Write cmudict.bin ──────────────────────────────────────
    let mut blob = Vec::with_capacity(2 * 1024 * 1024);
    blob.extend_from_slice(&(dict.len() as u32).to_le_bytes());

    for (word, variants) in &dict {
        let word_bytes = word.as_bytes();
        blob.extend_from_slice(&(word_bytes.len() as u16).to_le_bytes());
        blob.extend_from_slice(word_bytes);
        blob.push(variants.len() as u8);
        for variant in variants {
            blob.push(variant.len() as u8);
            blob.extend_from_slice(variant);
        }
    }

    std::fs::write(&dict_blob_path, &blob).unwrap();
    eprintln!("cmudict.bin: {} entries, {} bytes", dict.len(), blob.len());

    // ── Phase 3: Build rhyme groups ─────────────────────────────────────
    // Group words by their stress-stripped primary rhyme tail.
    // A tail is: from the last primary-stressed vowel (stress 1) to end,
    // falling back to secondary stress (2) if no primary found.
    // The tail is stored stress-stripped so that AA1 T and AA2 T map to
    // the same group (AA T).

    let mut tail_groups: BTreeMap<Vec<u8>, Vec<String>> = BTreeMap::new();

    for (word, variants) in &dict {
        for phonemes in variants {
            if let Some(tail) = extract_stripped_tail(phonemes) {
                tail_groups.entry(tail).or_default().push(word.clone());
            }
        }
    }

    // Deduplicate words within each group (a word with multiple variants
    // might map to the same tail twice) and drop groups with < 2 members
    let mut useful_groups: BTreeMap<Vec<u8>, Vec<String>> = BTreeMap::new();
    let mut total_words_in_groups = 0usize;

    for (tail, mut words) in tail_groups {
        words.sort();
        words.dedup();
        if words.len() >= 2 {
            total_words_in_groups += words.len();
            useful_groups.insert(tail, words);
        }
    }

    // ── Phase 4: Write rhyme_groups.bin ─────────────────────────────────
    let mut rbuf = Vec::with_capacity(1024 * 1024);
    rbuf.extend_from_slice(&(useful_groups.len() as u32).to_le_bytes());

    for (tail, words) in &useful_groups {
        rbuf.push(tail.len() as u8);
        rbuf.extend_from_slice(tail);
        rbuf.extend_from_slice(&(words.len() as u32).to_le_bytes());
        for word in words {
            let wb = word.as_bytes();
            rbuf.extend_from_slice(&(wb.len() as u16).to_le_bytes());
            rbuf.extend_from_slice(wb);
        }
    }

    std::fs::write(&rhyme_blob_path, &rbuf).unwrap();
    eprintln!(
        "rhyme_groups.bin: {} groups, {} words covered, {} bytes",
        useful_groups.len(),
        total_words_in_groups,
        rbuf.len()
    );

    // ── Phase 5: Build near-rhyme coda neighbor graph ───────────────────
    // For each unique coda (consonants after the nucleus) of length <= 3,
    // find all other codas that differ by exactly one edit (substitution,
    // insertion, or deletion). This is the O(n²) work we want to do once
    // at compile time, not at every startup.

    const MAX_CODA_LEN: usize = 3;

    let mut all_codas: BTreeSet<Vec<u8>> = BTreeSet::new();
    for variants in dict.values() {
        for phonemes in variants {
            if let Some(coda) = extract_coda(phonemes) {
                if coda.len() <= MAX_CODA_LEN {
                    all_codas.insert(coda);
                }
            }
        }
    }

    // Group codas by length for efficient pairwise comparison
    let mut by_length: BTreeMap<usize, Vec<Vec<u8>>> = BTreeMap::new();
    for coda in &all_codas {
        by_length.entry(coda.len()).or_default().push(coda.clone());
    }

    let mut neighbors: BTreeMap<Vec<u8>, Vec<Vec<u8>>> = BTreeMap::new();
    let max_len = by_length.keys().copied().max().unwrap_or(0);

    for len in 0..=max_len {
        let codas_at_len = match by_length.get(&len) {
            Some(v) => v,
            None => continue,
        };

        // Same length: substitutions
        for i in 0..codas_at_len.len() {
            for j in (i + 1)..codas_at_len.len() {
                if is_substitution(&codas_at_len[i], &codas_at_len[j]) {
                    neighbors
                        .entry(codas_at_len[i].clone())
                        .or_default()
                        .push(codas_at_len[j].clone());
                    neighbors
                        .entry(codas_at_len[j].clone())
                        .or_default()
                        .push(codas_at_len[i].clone());
                }
            }
        }

        // Adjacent length: insertions/deletions
        if let Some(codas_next) = by_length.get(&(len + 1)) {
            for short in codas_at_len {
                for long in codas_next {
                    if is_single_insert(short, long) {
                        neighbors
                            .entry(short.clone())
                            .or_default()
                            .push(long.clone());
                        neighbors
                            .entry(long.clone())
                            .or_default()
                            .push(short.clone());
                    }
                }
            }
        }
    }

    // Deduplicate
    for nlist in neighbors.values_mut() {
        nlist.sort();
        nlist.dedup();
    }

    // Write near_neighbors.bin
    // Format: [u32 LE] entry_count
    //   For each: [u8] coda_len [bytes] coda [u16 LE] neighbor_count
    //     For each neighbor: [u8] neighbor_len [bytes] neighbor_coda
    let near_blob_path = Path::new(&out_dir).join("near_neighbors.bin");
    let mut nbuf = Vec::with_capacity(64 * 1024);
    nbuf.extend_from_slice(&(neighbors.len() as u32).to_le_bytes());

    for (coda, nlist) in &neighbors {
        nbuf.push(coda.len() as u8);
        nbuf.extend_from_slice(coda);
        nbuf.extend_from_slice(&(nlist.len() as u16).to_le_bytes());
        for ncoda in nlist {
            nbuf.push(ncoda.len() as u8);
            nbuf.extend_from_slice(ncoda);
        }
    }

    std::fs::write(&near_blob_path, &nbuf).unwrap();
    eprintln!(
        "near_neighbors.bin: {} codas with neighbors, {} bytes",
        neighbors.len(),
        nbuf.len()
    );
}

/// Extract the stress-stripped rhyme tail from encoded phonemes.
/// Finds the last vowel with primary stress (1), falls back to secondary (2).
fn extract_stripped_tail(phonemes: &[u8]) -> Option<Vec<u8>> {
    // Try stress 1 first, then stress 2
    for target_stress in [1u8, 2u8] {
        for i in (0..phonemes.len()).rev() {
            let id = phonemes[i];
            if is_vowel(id) && stress_of(id) == target_stress {
                // Build stripped tail from this position
                return Some(phonemes[i..].iter().map(|&p| strip(p)).collect());
            }
        }
    }
    None
}

// ── Phoneme helpers (duplicated from src/phoneme.rs for build script) ───

fn encode_phoneme(s: &str) -> u8 {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return 0;
    }
    let last = bytes[len - 1];
    let (base_bytes, stress) = if last.is_ascii_digit() && len >= 3 {
        (&bytes[..len - 1], last - b'0')
    } else {
        (bytes, 0u8)
    };
    let base = match base_bytes {
        b"B" => 1,
        b"CH" => 2,
        b"D" => 3,
        b"DH" => 4,
        b"F" => 5,
        b"G" => 6,
        b"HH" => 7,
        b"JH" => 8,
        b"K" => 9,
        b"L" => 10,
        b"M" => 11,
        b"N" => 12,
        b"NG" => 13,
        b"P" => 14,
        b"R" => 15,
        b"S" => 16,
        b"SH" => 17,
        b"T" => 18,
        b"TH" => 19,
        b"V" => 20,
        b"W" => 21,
        b"Y" => 22,
        b"Z" => 23,
        b"ZH" => 24,
        b"AA" => 25,
        b"AE" => 26,
        b"AH" => 27,
        b"AO" => 28,
        b"AW" => 29,
        b"AY" => 30,
        b"EH" => 31,
        b"ER" => 32,
        b"EY" => 33,
        b"IH" => 34,
        b"IY" => 35,
        b"OW" => 36,
        b"OY" => 37,
        b"UH" => 38,
        b"UW" => 39,
        _ => return 0,
    };
    if base < 25 {
        base
    } else {
        match stress {
            0 => base,
            1 => base + STRESS_1_OFFSET,
            2 => base + STRESS_2_OFFSET,
            _ => base,
        }
    }
}

fn is_vowel(id: u8) -> bool {
    let base = strip(id);
    base >= VOWEL_BASE && base <= VOWEL_END
}

fn stress_of(id: u8) -> u8 {
    if id >= VOWEL_BASE + STRESS_2_OFFSET && id <= VOWEL_END + STRESS_2_OFFSET {
        2
    } else if id >= VOWEL_BASE + STRESS_1_OFFSET && id <= VOWEL_END + STRESS_1_OFFSET {
        1
    } else {
        0
    }
}

fn strip(id: u8) -> u8 {
    if id >= VOWEL_BASE + STRESS_2_OFFSET && id <= VOWEL_END + STRESS_2_OFFSET {
        id - STRESS_2_OFFSET
    } else if id >= VOWEL_BASE + STRESS_1_OFFSET && id <= VOWEL_END + STRESS_1_OFFSET {
        id - STRESS_1_OFFSET
    } else {
        id
    }
}

/// Extract the coda (consonants after last stressed vowel, stress-stripped).
fn extract_coda(phonemes: &[u8]) -> Option<Vec<u8>> {
    for target_stress in [1u8, 2u8] {
        for i in (0..phonemes.len()).rev() {
            let id = phonemes[i];
            if is_vowel(id) && stress_of(id) == target_stress {
                return Some(phonemes[i + 1..].iter().map(|&p| strip(p)).collect());
            }
        }
    }
    None
}

fn is_substitution(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diffs = 0;
    for i in 0..a.len() {
        if a[i] != b[i] {
            diffs += 1;
            if diffs > 1 {
                return false;
            }
        }
    }
    diffs == 1
}

fn is_single_insert(short: &[u8], long: &[u8]) -> bool {
    if long.len() != short.len() + 1 {
        return false;
    }
    let mut i = 0;
    let mut j = 0;
    let mut skipped = false;
    while i < short.len() && j < long.len() {
        if short[i] == long[j] {
            i += 1;
            j += 1;
        } else if !skipped {
            skipped = true;
            j += 1;
        } else {
            return false;
        }
    }
    true
}
