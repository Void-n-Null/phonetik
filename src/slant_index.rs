use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::RwLock;
use serde::Serialize;

use crate::coda_groups::CodaGroupMap;
use crate::dict::CmuDict;
use crate::phoneme;

const NUM_VOWELS: usize = 15;
const VOWEL_ID_OFFSET: usize = 25;

static VOWEL_DISTANCE: [[u8; NUM_VOWELS]; NUM_VOWELS] = [
    //          AA  AE  AH  AO  AW  AY  EH  ER  EY  IH  IY  OW  OY  UH  UW
    /* AA */
    [0, 40, 20, 10, 45, 35, 45, 30, 50, 55, 60, 35, 50, 40, 50],
    /* AE */ [40, 0, 25, 45, 50, 30, 15, 35, 30, 25, 35, 55, 50, 55, 60],
    /* AH */ [20, 25, 0, 25, 40, 35, 25, 20, 35, 30, 40, 35, 45, 30, 40],
    /* AO */ [10, 45, 25, 0, 40, 40, 50, 30, 55, 55, 60, 20, 35, 25, 35],
    /* AW */ [45, 50, 40, 40, 0, 30, 50, 45, 50, 55, 60, 35, 40, 35, 40],
    /* AY */ [35, 30, 35, 40, 30, 0, 35, 40, 30, 30, 35, 50, 40, 50, 55],
    /* EH */ [45, 15, 25, 50, 50, 35, 0, 30, 15, 20, 30, 50, 50, 50, 55],
    /* ER */ [30, 35, 20, 30, 45, 40, 30, 0, 35, 30, 35, 35, 40, 30, 35],
    /* EY */ [50, 30, 35, 55, 50, 30, 15, 35, 0, 20, 15, 50, 45, 55, 55],
    /* IH */ [55, 25, 30, 55, 55, 30, 20, 30, 20, 0, 10, 55, 50, 50, 55],
    /* IY */ [60, 35, 40, 60, 60, 35, 30, 35, 15, 10, 0, 55, 50, 55, 55],
    /* OW */ [35, 55, 35, 20, 35, 50, 50, 35, 50, 55, 55, 0, 30, 20, 15],
    /* OY */ [50, 50, 45, 35, 40, 40, 50, 40, 45, 50, 50, 30, 0, 35, 40],
    /* UH */ [40, 55, 30, 25, 35, 50, 50, 30, 55, 50, 55, 20, 35, 0, 10],
    /* UW */ [50, 60, 40, 35, 40, 55, 55, 35, 55, 55, 55, 15, 40, 10, 0],
];

const SLANT_THRESHOLD: u8 = 50;
const MIN_SUFFIX_LEN: usize = 2;
const SUFFIX_PENALTY_PER_EXTRA: f64 = 0.06;

pub struct SlantIndex {
    /// Shared coda groups (owned by AppState, referenced here)
    coda_groups: Arc<CodaGroupMap>,
    /// Coda suffix → list of full codas that end with this suffix
    suffix_to_codas: HashMap<Vec<u8>, Vec<Vec<u8>>>,
    /// Overlay for custom words
    overlay: RwLock<HashMap<Vec<u8>, HashMap<u8, Vec<String>>>>,
    dict: Arc<CmuDict>,
}

impl SlantIndex {
    pub fn new(dict: Arc<CmuDict>, coda_groups: Arc<CodaGroupMap>) -> Self {
        // Build suffix index from the shared coda groups
        let mut suffix_to_codas: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::new();
        for coda in coda_groups.keys() {
            if coda.len() >= MIN_SUFFIX_LEN {
                for start in 1..coda.len().saturating_sub(MIN_SUFFIX_LEN - 1) {
                    let suffix = coda[start..].to_vec();
                    if suffix.len() >= MIN_SUFFIX_LEN {
                        suffix_to_codas
                            .entry(suffix)
                            .or_default()
                            .push(coda.clone());
                    }
                }
            }
        }
        for codas in suffix_to_codas.values_mut() {
            codas.sort();
            codas.dedup();
        }

        Self {
            coda_groups,
            suffix_to_codas,
            overlay: RwLock::new(HashMap::new()),
            dict,
        }
    }

    pub fn lookup(&self, word: &str, limit: usize) -> Option<SlantRhymeResult> {
        let normalized = CmuDict::normalize(word);
        let variants = self.dict.lookup(word)?;

        let mut best_matches: Vec<SlantRhymeMatch> = Vec::new();
        let mut best_nucleus: Option<u8> = None;
        let mut best_coda: Option<Vec<u8>> = None;

        for phonemes in &variants {
            if let Some((nucleus, coda)) = crate::coda_groups::extract_nucleus_coda(phonemes) {
                let mut matches = Vec::new();
                let mut seen = HashSet::new();

                // Layer 1: Exact coda match
                self.collect_exact(
                    &self.coda_groups,
                    &coda,
                    nucleus,
                    &normalized,
                    limit,
                    &mut matches,
                    &mut seen,
                );

                // Layer 2: Suffix matches
                if matches.len() < limit {
                    self.collect_suffix(
                        &coda,
                        nucleus,
                        &normalized,
                        limit,
                        &mut matches,
                        &mut seen,
                    );
                }

                // Overlay
                {
                    let overlay = self.overlay.read();
                    self.collect_exact_overlay(
                        &overlay,
                        &coda,
                        nucleus,
                        &normalized,
                        limit,
                        &mut matches,
                        &mut seen,
                    );
                }

                if matches.len() > best_matches.len() {
                    best_matches = matches;
                    best_nucleus = Some(nucleus);
                    best_coda = Some(coda);
                }
            }
        }

        if best_matches.is_empty() {
            return None;
        }

        best_matches.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.word.cmp(&b.word))
        });
        if best_matches.len() > limit {
            best_matches.truncate(limit);
        }

        let phonemes_encoded = &variants[0];
        Some(SlantRhymeResult {
            word: normalized,
            phonemes: phoneme::decode_to_strings(phonemes_encoded),
            syllables: phoneme::count_syllables(phonemes_encoded),
            nucleus: phoneme::decode(best_nucleus.unwrap()).to_string(),
            coda: phoneme::decode_to_strings(&best_coda.unwrap()),
            matches: best_matches,
        })
    }

    pub fn add_word(&self, word: &str, phonemes: &[u8]) {
        let normalized = CmuDict::normalize(word);
        if let Some((nucleus, coda)) = crate::coda_groups::extract_nucleus_coda(phonemes) {
            let mut overlay = self.overlay.write();
            overlay
                .entry(coda)
                .or_default()
                .entry(nucleus)
                .or_default()
                .push(normalized);
        }
    }

    pub fn coda_count(&self) -> usize {
        self.coda_groups.len()
    }

    fn collect_exact(
        &self,
        groups: &CodaGroupMap,
        coda: &[u8],
        query_nucleus: u8,
        query_word: &str,
        limit: usize,
        out: &mut Vec<SlantRhymeMatch>,
        seen: &mut HashSet<String>,
    ) {
        if let Some(nuclei) = groups.get(coda) {
            for (&other_nucleus, words) in nuclei {
                if other_nucleus == query_nucleus {
                    continue;
                }
                let distance = vowel_distance(query_nucleus, other_nucleus);
                if distance > SLANT_THRESHOLD {
                    continue;
                }
                let confidence = 1.0 - (distance as f64 / 100.0);
                for w in words {
                    let ws: &str = w;
                    if ws == query_word || seen.contains(ws) {
                        continue;
                    }
                    if out.len() >= limit {
                        return;
                    }
                    seen.insert(ws.to_string());
                    let lookup = self.dict.lookup(ws);
                    let ph = lookup.as_ref().map(|l| l[0].clone()).unwrap_or_default();
                    out.push(SlantRhymeMatch {
                        word: ws.to_string(),
                        phonemes: phoneme::decode_to_strings(&ph),
                        syllables: phoneme::count_syllables(&ph),
                        confidence: round4(confidence),
                        nucleus: phoneme::decode(other_nucleus).to_string(),
                    });
                }
            }
        }
    }

    fn collect_exact_overlay(
        &self,
        groups: &HashMap<Vec<u8>, HashMap<u8, Vec<String>>>,
        coda: &[u8],
        query_nucleus: u8,
        query_word: &str,
        limit: usize,
        out: &mut Vec<SlantRhymeMatch>,
        seen: &mut HashSet<String>,
    ) {
        if let Some(nuclei) = groups.get(coda) {
            for (&other_nucleus, words) in nuclei {
                if other_nucleus == query_nucleus {
                    continue;
                }
                let distance = vowel_distance(query_nucleus, other_nucleus);
                if distance > SLANT_THRESHOLD {
                    continue;
                }
                let confidence = 1.0 - (distance as f64 / 100.0);
                for w in words {
                    if w == query_word || seen.contains(w.as_str()) {
                        continue;
                    }
                    if out.len() >= limit {
                        return;
                    }
                    seen.insert(w.clone());
                    let lookup = self.dict.lookup(w);
                    let ph = lookup.as_ref().map(|l| l[0].clone()).unwrap_or_default();
                    out.push(SlantRhymeMatch {
                        word: w.clone(),
                        phonemes: phoneme::decode_to_strings(&ph),
                        syllables: phoneme::count_syllables(&ph),
                        confidence: round4(confidence),
                        nucleus: phoneme::decode(other_nucleus).to_string(),
                    });
                }
            }
        }
    }

    fn collect_suffix(
        &self,
        coda: &[u8],
        query_nucleus: u8,
        query_word: &str,
        limit: usize,
        out: &mut Vec<SlantRhymeMatch>,
        seen: &mut HashSet<String>,
    ) {
        // Direction A: our coda is a suffix of longer codas
        if coda.len() >= MIN_SUFFIX_LEN {
            if let Some(longer_codas) = self.suffix_to_codas.get(coda) {
                for longer in longer_codas {
                    let extra = longer.len() - coda.len();
                    let penalty = extra as f64 * SUFFIX_PENALTY_PER_EXTRA;
                    self.collect_with_penalty(
                        longer,
                        query_nucleus,
                        query_word,
                        penalty,
                        limit,
                        out,
                        seen,
                    );
                }
            }
        }
        // Direction B: shorter codas are suffixes of ours
        for start in 1..coda.len().saturating_sub(MIN_SUFFIX_LEN - 1) {
            let suffix = &coda[start..];
            if suffix.len() < MIN_SUFFIX_LEN {
                break;
            }
            let penalty = start as f64 * SUFFIX_PENALTY_PER_EXTRA;
            self.collect_with_penalty(suffix, query_nucleus, query_word, penalty, limit, out, seen);
        }
    }

    fn collect_with_penalty(
        &self,
        target_coda: &[u8],
        query_nucleus: u8,
        query_word: &str,
        penalty: f64,
        limit: usize,
        out: &mut Vec<SlantRhymeMatch>,
        seen: &mut HashSet<String>,
    ) {
        if let Some(nuclei) = self.coda_groups.get(target_coda) {
            for (&other_nucleus, words) in nuclei {
                let distance = if other_nucleus == query_nucleus {
                    5
                } else {
                    vowel_distance(query_nucleus, other_nucleus)
                };
                if distance > SLANT_THRESHOLD {
                    continue;
                }
                let confidence = ((1.0 - (distance as f64 / 100.0)) - penalty).max(0.15);
                for w in words {
                    let ws: &str = w;
                    if ws == query_word || seen.contains(ws) {
                        continue;
                    }
                    if out.len() >= limit {
                        return;
                    }
                    seen.insert(ws.to_string());
                    let lookup = self.dict.lookup(ws);
                    let ph = lookup.as_ref().map(|l| l[0].clone()).unwrap_or_default();
                    out.push(SlantRhymeMatch {
                        word: ws.to_string(),
                        phonemes: phoneme::decode_to_strings(&ph),
                        syllables: phoneme::count_syllables(&ph),
                        confidence: round4(confidence),
                        nucleus: phoneme::decode(other_nucleus).to_string(),
                    });
                }
            }
        }
    }
}

#[inline]
fn vowel_distance(a: u8, b: u8) -> u8 {
    let ai = (a as usize).wrapping_sub(VOWEL_ID_OFFSET);
    let bi = (b as usize).wrapping_sub(VOWEL_ID_OFFSET);
    if ai < NUM_VOWELS && bi < NUM_VOWELS {
        VOWEL_DISTANCE[ai][bi]
    } else {
        100
    }
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlantRhymeResult {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
    pub nucleus: String,
    pub coda: Vec<String>,
    pub matches: Vec<SlantRhymeMatch>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlantRhymeMatch {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
    pub confidence: f64,
    pub nucleus: String,
}
