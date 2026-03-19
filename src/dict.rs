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
