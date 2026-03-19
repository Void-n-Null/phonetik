use std::collections::HashMap;
use std::sync::Arc;

use crate::dict::CmuDict;
use crate::phoneme;

/// Shared coda groups index used by both SlantIndex and NearIndex.
///
/// Words are stored as `Arc<str>` — each word string is interned once
/// in the dict and referenced by pointer everywhere. No duplication.
pub type WordRef = Arc<str>;
pub type CodaGroupMap = HashMap<Vec<u8>, HashMap<u8, Vec<WordRef>>>;

/// Build the shared coda groups from the dictionary.
/// Also returns the word intern table for use by other indexes.
pub fn build(dict: &CmuDict) -> (CodaGroupMap, HashMap<String, WordRef>) {
    let mut intern: HashMap<String, WordRef> = HashMap::with_capacity(130_000);
    let mut groups: CodaGroupMap = HashMap::new();

    for word in dict.get_keys() {
        if let Some(variants) = dict.lookup(&word) {
            // Intern the word string — one allocation, shared everywhere
            let word_ref = intern
                .entry(word.clone())
                .or_insert_with(|| Arc::from(word.as_str()))
                .clone();

            for phonemes in &variants {
                if let Some((nucleus, coda)) = extract_nucleus_coda(phonemes) {
                    groups
                        .entry(coda)
                        .or_default()
                        .entry(nucleus)
                        .or_default()
                        .push(word_ref.clone());
                }
            }
        }
    }

    // Deduplicate within each nucleus bucket
    for nuclei in groups.values_mut() {
        for words in nuclei.values_mut() {
            words.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
            words.dedup_by(|a, b| a.as_ref() == b.as_ref());
        }
    }

    (groups, intern)
}

/// Extract nucleus vowel (stripped) and coda (everything after, stripped).
pub fn extract_nucleus_coda(phonemes: &[u8]) -> Option<(u8, Vec<u8>)> {
    for target_stress in [1u8, 2u8] {
        for i in (0..phonemes.len()).rev() {
            if phoneme::is_vowel(phonemes[i]) && phoneme::stress(phonemes[i]) == target_stress {
                let nucleus = phoneme::strip(phonemes[i]);
                let coda: Vec<u8> = phonemes[i + 1..]
                    .iter()
                    .map(|&p| phoneme::strip(p))
                    .collect();
                return Some((nucleus, coda));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_nucleus_coda_primary_stress() {
        // CAT → K AE1 T — nucleus AE, coda [T]
        let cat = vec![phoneme::K, phoneme::encode("AE1"), phoneme::T];
        let (nuc, coda) = extract_nucleus_coda(&cat).unwrap();
        assert_eq!(nuc, phoneme::AE);
        assert_eq!(coda, vec![phoneme::T]);
    }

    #[test]
    fn extract_nucleus_coda_secondary_stress_fallback() {
        // Word with only stress 2
        let ph = vec![phoneme::K, phoneme::encode("AE2"), phoneme::T];
        let (nuc, _coda) = extract_nucleus_coda(&ph).unwrap();
        assert_eq!(nuc, phoneme::AE);
    }

    #[test]
    fn extract_nucleus_coda_empty_coda() {
        // SEE → S IY1 — nucleus IY, coda []
        let see = vec![phoneme::S, phoneme::encode("IY1")];
        let (nuc, coda) = extract_nucleus_coda(&see).unwrap();
        assert_eq!(nuc, phoneme::IY);
        assert!(coda.is_empty());
    }

    #[test]
    fn extract_nucleus_coda_no_stressed_vowel() {
        // All consonants — returns None
        assert!(extract_nucleus_coda(&[phoneme::K, phoneme::T]).is_none());
        // Empty
        assert!(extract_nucleus_coda(&[]).is_none());
    }

    #[test]
    fn build_produces_nonempty_groups() {
        let dict = crate::dict::CmuDict::load();
        let (groups, intern) = build(&dict);
        assert!(!groups.is_empty());
        assert!(!intern.is_empty());
        // Every nucleus bucket should have at least one word
        for nuclei in groups.values() {
            for words in nuclei.values() {
                assert!(!words.is_empty());
            }
        }
    }
}
