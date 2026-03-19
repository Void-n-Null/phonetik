use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::dict::CmuDict;
use crate::phoneme;

/// Precompiled rhyme groups blob, baked into the binary at compile time.
static RHYME_GROUPS_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rhyme_groups.bin"));

/// Perfect rhyme index: maps stress-stripped tails to word lists.
///
/// The base index is loaded from an embedded binary blob at startup.
/// Custom words added at runtime are inserted into the appropriate
/// group via the overlay, making them instantly rhymeable.
pub struct RhymeIndex {
    /// tail (stripped phoneme IDs) → list of words in the group
    groups: HashMap<Vec<u8>, Vec<String>>,
    /// Custom words added at runtime that need to be included in lookups
    overlay: RwLock<HashMap<Vec<u8>, Vec<String>>>,
    dict: Arc<CmuDict>,
}

impl RhymeIndex {
    pub fn new(dict: Arc<CmuDict>) -> Self {
        let groups = Self::load_blob();
        Self {
            groups,
            overlay: RwLock::new(HashMap::new()),
            dict,
        }
    }

    /// Look up all perfect rhymes for a word.
    /// Returns the group members minus the query word itself.
    pub fn lookup(&self, word: &str) -> Option<PerfectRhymeResult> {
        let normalized = CmuDict::normalize(word);
        let variants = self.dict.lookup(word)?;

        // Try each pronunciation variant's tail
        let mut best_matches: Option<Vec<String>> = None;
        let mut best_tail: Option<Vec<u8>> = None;

        for phonemes in &variants {
            if let Some(tail) = extract_stripped_tail(phonemes) {
                let mut members = Vec::new();

                // Check embedded groups
                if let Some(group) = self.groups.get(&tail) {
                    members.extend(group.iter().cloned());
                }

                // Check overlay (custom words)
                {
                    let overlay = self.overlay.read();
                    if let Some(custom) = overlay.get(&tail) {
                        members.extend(custom.iter().cloned());
                    }
                }

                // Remove the query word itself and dedup
                members.retain(|w| w != &normalized);
                members.sort();
                members.dedup();

                if best_matches
                    .as_ref()
                    .map_or(true, |b| members.len() > b.len())
                {
                    best_matches = Some(members);
                    best_tail = Some(tail);
                }
            }
        }

        let matches = best_matches?;
        if matches.is_empty() {
            return None;
        }

        let tail = best_tail.unwrap();
        let phonemes_encoded = &variants[0];

        Some(PerfectRhymeResult {
            word: normalized,
            phonemes: phoneme::decode_to_strings(phonemes_encoded),
            syllables: phoneme::count_syllables(phonemes_encoded),
            tail: phoneme::decode_to_strings(&tail),
            matches: matches
                .into_iter()
                .map(|w| {
                    let lookup = self.dict.lookup(&w);
                    let ph = lookup.as_ref().map(|l| &l[0]).cloned().unwrap_or_default();
                    let syl = phoneme::count_syllables(&ph);
                    let ph_strings = phoneme::decode_to_strings(&ph);
                    PerfectRhymeMatch {
                        word: w,
                        phonemes: ph_strings,
                        syllables: syl,
                    }
                })
                .collect(),
        })
    }

    /// Add a custom word to the perfect rhyme index.
    /// Called when PhonemeGenerator creates a new entry.
    pub fn add_word(&self, word: &str, phonemes: &[u8]) {
        let normalized = CmuDict::normalize(word);
        if let Some(tail) = extract_stripped_tail(phonemes) {
            let mut overlay = self.overlay.write();
            overlay.entry(tail).or_default().push(normalized);
        }
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    fn load_blob() -> HashMap<Vec<u8>, Vec<String>> {
        let blob = RHYME_GROUPS_BLOB;
        if blob.len() < 4 {
            return HashMap::new();
        }

        let mut pos = 0;
        let group_count = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        pos += 4;

        let mut groups = HashMap::with_capacity(group_count);

        for _ in 0..group_count {
            if pos >= blob.len() {
                break;
            }
            let tail_len = blob[pos] as usize;
            pos += 1;

            if pos + tail_len > blob.len() {
                break;
            }
            let tail = blob[pos..pos + tail_len].to_vec();
            pos += tail_len;

            if pos + 4 > blob.len() {
                break;
            }
            let word_count =
                u32::from_le_bytes([blob[pos], blob[pos + 1], blob[pos + 2], blob[pos + 3]])
                    as usize;
            pos += 4;

            let mut words = Vec::with_capacity(word_count);
            for _ in 0..word_count {
                if pos + 2 > blob.len() {
                    break;
                }
                let word_len = u16::from_le_bytes([blob[pos], blob[pos + 1]]) as usize;
                pos += 2;

                if pos + word_len > blob.len() {
                    break;
                }
                let word = String::from_utf8_lossy(&blob[pos..pos + word_len]).into_owned();
                pos += word_len;
                words.push(word);
            }

            groups.insert(tail, words);
        }

        groups
    }
}

/// Extract stress-stripped rhyme tail: from last primary-stressed vowel
/// to end, falling back to secondary stress.
fn extract_stripped_tail(phonemes: &[u8]) -> Option<Vec<u8>> {
    for target_stress in [1u8, 2u8] {
        for i in (0..phonemes.len()).rev() {
            if phoneme::is_vowel(phonemes[i]) && phoneme::stress(phonemes[i]) == target_stress {
                return Some(phonemes[i..].iter().map(|&p| phoneme::strip(p)).collect());
            }
        }
    }
    None
}

// ── Response types ──────────────────────────────────────────────────────

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfectRhymeResult {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
    pub tail: Vec<String>,
    pub matches: Vec<PerfectRhymeMatch>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerfectRhymeMatch {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
}
