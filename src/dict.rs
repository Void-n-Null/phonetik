use std::collections::HashMap;

use crate::phoneme;

/// Precompiled CMU dict blob, baked into the binary at compile time.
static CMUDICT_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/cmudict.bin"));

/// CMU Pronouncing Dictionary backed by u8-encoded phonemes.
///
/// Loaded from a precompiled binary blob embedded in the executable.
/// The dictionary is immutable after construction — no runtime mutations.
pub struct CmuDict {
    entries: HashMap<String, Vec<Vec<u8>>>,
}

impl CmuDict {
    /// Load the embedded precompiled dictionary.
    pub fn load() -> Self {
        CmuDict {
            entries: Self::load_blob(),
        }
    }

    /// Number of words in the dictionary.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get all dictionary keys (for index construction at startup).
    pub fn get_keys(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// Look up a word's phoneme variants.
    pub fn lookup(&self, word: &str) -> Option<Vec<Vec<u8>>> {
        let normalized = Self::normalize(word);
        let result = self.entries.get(&normalized).cloned();

        if let Some(mut variants) = result {
            if ends_with_g_drop(word) {
                for v in &mut variants {
                    patch_g_drop(v);
                }
            }
            Some(variants)
        } else {
            None
        }
    }

    /// Normalize a word for dictionary lookup (uppercase, strip non-alpha except ' and -).
    pub fn normalize(input: &str) -> String {
        let mut buf = String::with_capacity(input.len() + 1);
        for c in input.chars() {
            if c.is_alphabetic() {
                buf.push(c.to_uppercase().next().unwrap_or(c));
            } else if c == '\'' || c == '-' {
                buf.push(c);
            }
        }
        if ends_with_g_drop(input) && buf.ends_with('\'') {
            buf.pop();
            buf.push('G');
        }
        buf
    }

    /// Count syllables in an encoded phoneme sequence.
    #[inline]
    pub fn count_syllables(phonemes: &[u8]) -> usize {
        phoneme::count_syllables(phonemes)
    }

    /// Deserialize the precompiled binary blob into a HashMap.
    fn load_blob() -> HashMap<String, Vec<Vec<u8>>> {
        let blob = CMUDICT_BLOB;
        if blob.len() < 4 {
            return HashMap::new();
        }

        let mut pos = 0;

        let entry_count = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        pos += 4;

        let mut dict = HashMap::with_capacity(entry_count);

        for _ in 0..entry_count {
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

            if pos >= blob.len() {
                break;
            }
            let variant_count = blob[pos] as usize;
            pos += 1;

            let mut variants = Vec::with_capacity(variant_count);
            for _ in 0..variant_count {
                if pos >= blob.len() {
                    break;
                }
                let phoneme_count = blob[pos] as usize;
                pos += 1;

                if pos + phoneme_count > blob.len() {
                    break;
                }
                let phonemes = blob[pos..pos + phoneme_count].to_vec();
                pos += phoneme_count;
                variants.push(phonemes);
            }

            dict.insert(word, variants);
        }

        dict
    }
}

fn ends_with_g_drop(input: &str) -> bool {
    let bytes: Vec<char> = input.chars().collect();
    let mut end = bytes.len();
    if end == 0 {
        return false;
    }
    end -= 1;
    while end < bytes.len() && !bytes[end].is_alphabetic() && bytes[end] != '\'' {
        if end == 0 {
            return false;
        }
        end -= 1;
    }
    if end >= bytes.len() || bytes[end] != '\'' {
        return false;
    }
    if end == 0 {
        return false;
    }
    end -= 1;
    if bytes[end].to_ascii_lowercase() != 'n' {
        return false;
    }
    if end == 0 {
        return false;
    }
    end -= 1;
    if bytes[end].to_ascii_lowercase() != 'i' {
        return false;
    }
    for i in (0..end).rev() {
        if bytes[i].is_alphabetic() {
            return true;
        }
    }
    false
}

fn patch_g_drop(phonemes: &mut Vec<u8>) {
    if phonemes.is_empty() {
        return;
    }
    let last = phonemes.len() - 1;
    if phonemes[last] == phoneme::NG {
        phonemes[last] = phoneme::N;
        if last >= 1 {
            if phonemes[last - 1] == phoneme::IH {
                phonemes[last - 1] = phoneme::AH;
            } else if phonemes[last - 1] == phoneme::IH + 80 {
                phonemes[last - 1] = phoneme::AH + 80;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phoneme;

    // ── Dictionary loading ──────────────────────────────────────────────

    #[test]
    fn dict_loads_with_entries() {
        let dict = CmuDict::load();
        assert!(dict.entry_count() > 100_000, "should have 100K+ entries");
    }

    #[test]
    fn get_keys_matches_entry_count() {
        let dict = CmuDict::load();
        assert_eq!(dict.get_keys().len(), dict.entry_count());
    }

    // ── Lookup ──────────────────────────────────────────────────────────

    #[test]
    fn lookup_known_word() {
        let dict = CmuDict::load();
        let result = dict.lookup("hello");
        assert!(result.is_some());
        let variants = result.unwrap();
        assert!(!variants.is_empty());
        assert!(variants[0].len() > 2, "HELLO should have >2 phonemes");
    }

    #[test]
    fn lookup_case_insensitive() {
        let dict = CmuDict::load();
        let lower = dict.lookup("cat");
        let upper = dict.lookup("CAT");
        let mixed = dict.lookup("Cat");
        assert!(lower.is_some());
        assert_eq!(lower, upper);
        assert_eq!(lower, mixed);
    }

    #[test]
    fn lookup_nonexistent_word() {
        let dict = CmuDict::load();
        assert!(dict.lookup("xyzzyplugh").is_none());
    }

    #[test]
    fn lookup_word_with_multiple_variants() {
        let dict = CmuDict::load();
        // THE has multiple pronunciations in CMUdict
        let result = dict.lookup("the");
        assert!(result.is_some());
        let variants = result.unwrap();
        assert!(variants.len() >= 2, "THE should have multiple variants");
    }

    // ── Normalize ───────────────────────────────────────────────────────

    #[test]
    fn normalize_uppercases() {
        assert_eq!(CmuDict::normalize("hello"), "HELLO");
    }

    #[test]
    fn normalize_preserves_apostrophe_and_hyphen() {
        assert_eq!(CmuDict::normalize("don't"), "DON'T");
        assert_eq!(CmuDict::normalize("well-known"), "WELL-KNOWN");
    }

    #[test]
    fn normalize_strips_other_punctuation() {
        assert_eq!(CmuDict::normalize("hello!"), "HELLO");
        assert_eq!(CmuDict::normalize("(test)"), "TEST");
    }

    #[test]
    fn normalize_empty() {
        assert_eq!(CmuDict::normalize(""), "");
    }

    #[test]
    fn normalize_g_drop_rewrites_apostrophe() {
        // "runnin'" should normalize to "RUNNING" (apostrophe → G)
        assert_eq!(CmuDict::normalize("runnin'"), "RUNNING");
        assert_eq!(CmuDict::normalize("singin'"), "SINGING");
    }

    #[test]
    fn normalize_non_g_drop_apostrophe_preserved() {
        // "don't" is NOT g-drop — apostrophe stays
        assert_eq!(CmuDict::normalize("don't"), "DON'T");
    }

    // ── G-drop detection ────────────────────────────────────────────────

    #[test]
    fn g_drop_positive_cases() {
        assert!(ends_with_g_drop("runnin'"));
        assert!(ends_with_g_drop("singin'"));
        assert!(ends_with_g_drop("lovin'"));
        assert!(ends_with_g_drop("RUNNIN'"));
    }

    #[test]
    fn g_drop_negative_cases() {
        assert!(!ends_with_g_drop("don't"));
        assert!(!ends_with_g_drop("hello"));
        assert!(!ends_with_g_drop("in'")); // too short — no letters before "in'"
        assert!(!ends_with_g_drop(""));
        assert!(!ends_with_g_drop("'"));
    }

    // ── G-drop phoneme patching ─────────────────────────────────────────

    #[test]
    fn patch_g_drop_replaces_ng_with_n() {
        let mut ph = vec![
            phoneme::R,
            phoneme::encode("AH1"),
            phoneme::N,
            phoneme::IH,
            phoneme::NG,
        ];
        patch_g_drop(&mut ph);
        assert_eq!(*ph.last().unwrap(), phoneme::N);
    }

    #[test]
    fn patch_g_drop_also_patches_ih_to_ah() {
        // Simulating RUNNING → RUNNIN': the IH before NG should become AH
        let mut ph = vec![phoneme::K, phoneme::IH, phoneme::NG];
        patch_g_drop(&mut ph);
        assert_eq!(ph[1], phoneme::AH);
        assert_eq!(ph[2], phoneme::N);
    }

    #[test]
    fn patch_g_drop_patches_stressed_ih2_to_ah2() {
        let ih2 = phoneme::IH + 80; // IH with stress 2
        let ah2 = phoneme::AH + 80;
        let mut ph = vec![phoneme::K, ih2, phoneme::NG];
        patch_g_drop(&mut ph);
        assert_eq!(ph[1], ah2);
        assert_eq!(ph[2], phoneme::N);
    }

    #[test]
    fn patch_g_drop_no_ng_unchanged() {
        let mut ph = vec![phoneme::K, phoneme::AE, phoneme::T];
        let original = ph.clone();
        patch_g_drop(&mut ph);
        assert_eq!(ph, original);
    }

    #[test]
    fn patch_g_drop_empty_vec() {
        let mut ph: Vec<u8> = vec![];
        patch_g_drop(&mut ph);
        assert!(ph.is_empty());
    }

    #[test]
    fn patch_g_drop_single_ng() {
        // NG alone — no preceding vowel to patch
        let mut ph = vec![phoneme::NG];
        patch_g_drop(&mut ph);
        assert_eq!(ph, vec![phoneme::N]);
    }

    // ── G-drop integration ──────────────────────────────────────────────

    #[test]
    fn lookup_g_drop_word_returns_patched_phonemes() {
        let dict = CmuDict::load();
        // "runnin'" should look up RUNNING and return patched phonemes
        let result = dict.lookup("runnin'");
        assert!(result.is_some(), "runnin' should resolve via RUNNING");
        let phonemes = &result.unwrap()[0];
        // Last phoneme should be N (not NG)
        assert_eq!(*phonemes.last().unwrap(), phoneme::N);
    }

    // ── count_syllables delegate ────────────────────────────────────────

    #[test]
    fn count_syllables_delegates_correctly() {
        let cat = vec![phoneme::K, phoneme::encode("AE1"), phoneme::T];
        assert_eq!(CmuDict::count_syllables(&cat), 1);
    }
}
