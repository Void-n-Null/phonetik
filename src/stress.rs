use serde::Serialize;

use crate::dict::CmuDict;
use crate::phoneme;
use crate::syllable::SyllableSplitter;

use std::sync::Arc;

pub struct StressAnalyzer {
    dict: Arc<CmuDict>,
}

impl StressAnalyzer {
    pub fn new(dict: Arc<CmuDict>) -> Self {
        Self { dict }
    }

    pub fn analyze_line(&self, line: &str) -> LineStress {
        let tokens = tokenize(line);
        let mut words = Vec::with_capacity(tokens.len());
        let mut full_pattern = Vec::new();

        for token in &tokens {
            let normalized = CmuDict::normalize(token);
            if normalized.is_empty() {
                continue;
            }
            if let Some(lookup) = self.dict.lookup(token) {
                let phonemes = &lookup[0];
                let stresses = phoneme::extract_stresses(phonemes);
                let display = SyllableSplitter::stress_display(token, &stresses);
                full_pattern.extend_from_slice(&stresses);
                words.push(WordStress {
                    word: token.clone(),
                    normalized,
                    phonemes: phoneme::decode_to_strings(phonemes),
                    stresses,
                    in_dictionary: true,
                    display,
                });
            } else {
                let estimated = estimate_stresses(&normalized);
                let display = SyllableSplitter::stress_display(token, &estimated);
                full_pattern.extend_from_slice(&estimated);
                words.push(WordStress {
                    word: token.clone(),
                    normalized,
                    phonemes: vec![],
                    stresses: estimated,
                    in_dictionary: false,
                    display,
                });
            }
        }

        let binary_pattern: Vec<i32> = full_pattern
            .iter()
            .map(|&s| if s > 0 { 1 } else { 0 })
            .collect();
        let syllable_count = full_pattern.len();
        let stressed_display = SyllableSplitter::format_line(&words);

        LineStress {
            words,
            stress_pattern: full_pattern,
            binary_pattern,
            syllable_count,
            stressed_display,
        }
    }
}

fn estimate_stresses(word: &str) -> Vec<i32> {
    let mut syllables = 0;
    let mut in_vowel = false;
    for c in word.chars() {
        let is_v = "AEIOUY".contains(c.to_uppercase().next().unwrap_or(c));
        if is_v && !in_vowel {
            syllables += 1;
        }
        in_vowel = is_v;
    }
    if syllables == 0 {
        syllables = 1;
    }
    let mut stresses = vec![0i32; syllables];
    stresses[0] = 1;
    stresses
}

fn tokenize(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;

    for i in 0..=chars.len() {
        let mut is_word_char = false;
        if i < chars.len() {
            if chars[i].is_alphabetic() {
                is_word_char = true;
            } else if chars[i] == '\'' && start.is_some() {
                if i + 1 < chars.len() && chars[i + 1].is_alphabetic() {
                    is_word_char = true;
                } else if let Some(s) = start {
                    is_word_char = is_g_drop_apostrophe(&chars, s, i);
                }
            }
        }
        if is_word_char {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start {
            let token: String = chars[s..i].iter().collect();
            tokens.push(token);
            start = None;
        }
    }
    tokens
}

fn is_g_drop_apostrophe(chars: &[char], token_start: usize, apo_idx: usize) -> bool {
    let len = apo_idx - token_start;
    if len < 3 {
        return false;
    }
    let n = chars[apo_idx - 1];
    let i = chars[apo_idx - 2];
    (n == 'n' || n == 'N') && (i == 'i' || i == 'I')
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WordStress {
    pub word: String,
    pub normalized: String,
    pub phonemes: Vec<String>,
    pub stresses: Vec<i32>,
    pub in_dictionary: bool,
    pub display: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStress {
    pub words: Vec<WordStress>,
    pub stress_pattern: Vec<i32>,
    pub binary_pattern: Vec<i32>,
    pub syllable_count: usize,
    pub stressed_display: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_analyzer() -> StressAnalyzer {
        StressAnalyzer::new(Arc::new(crate::dict::CmuDict::load()))
    }

    // ── Tokenizer ───────────────────────────────────────────────────────

    #[test]
    fn tokenize_simple_words() {
        assert_eq!(tokenize("hello world"), vec!["hello", "world"]);
    }

    #[test]
    fn tokenize_preserves_apostrophe_contractions() {
        let tokens = tokenize("don't stop");
        assert_eq!(tokens, vec!["don't", "stop"]);
    }

    #[test]
    fn tokenize_g_drop_apostrophe() {
        let tokens = tokenize("runnin' fast");
        assert_eq!(tokens, vec!["runnin'", "fast"]);
    }

    #[test]
    fn tokenize_strips_punctuation() {
        let tokens = tokenize("hello, world!");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn tokenize_empty_string() {
        assert_eq!(tokenize(""), Vec::<String>::new());
    }

    #[test]
    fn is_g_drop_apostrophe_fn() {
        let chars: Vec<char> = "runnin'".chars().collect();
        assert!(is_g_drop_apostrophe(&chars, 0, 6));

        let chars: Vec<char> = "don't".chars().collect();
        assert!(!is_g_drop_apostrophe(&chars, 0, 3));

        // Too short — "in'" starting at 0, apostrophe at 2
        let chars: Vec<char> = "in'".chars().collect();
        assert!(!is_g_drop_apostrophe(&chars, 0, 2));
    }

    // ── Stress estimation ───────────────────────────────────────────────

    #[test]
    fn estimate_stresses_monosyllabic() {
        assert_eq!(estimate_stresses("CAT"), vec![1]);
    }

    #[test]
    fn estimate_stresses_multisyllabic() {
        // Default: stress on first syllable
        let stresses = estimate_stresses("HELLO");
        assert_eq!(stresses.len(), 2);
        assert_eq!(stresses[0], 1);
        assert_eq!(stresses[1], 0);
    }

    #[test]
    fn estimate_stresses_no_vowels() {
        // Consonant-only word gets 1 syllable with stress
        assert_eq!(estimate_stresses("BRR"), vec![1]);
    }

    // ── Line analysis ───────────────────────────────────────────────────

    #[test]
    fn analyze_known_words() {
        let a = make_analyzer();
        let result = a.analyze_line("hello world");
        assert!(result.syllable_count >= 3);
        assert!(!result.words.is_empty());
        assert!(result.words[0].in_dictionary);
    }

    #[test]
    fn analyze_binary_pattern_is_zero_or_one() {
        let a = make_analyzer();
        let result = a.analyze_line("shall I compare thee to a summer's day");
        for &b in &result.binary_pattern {
            assert!(b == 0 || b == 1);
        }
    }

    #[test]
    fn analyze_unknown_word_marked() {
        let a = make_analyzer();
        let result = a.analyze_line("xyzzyplugh");
        assert_eq!(result.words.len(), 1);
        assert!(!result.words[0].in_dictionary);
    }

    #[test]
    fn analyze_empty_line() {
        let a = make_analyzer();
        let result = a.analyze_line("");
        assert_eq!(result.syllable_count, 0);
        assert!(result.words.is_empty());
    }

    #[test]
    fn stressed_display_is_nonempty_for_known_words() {
        let a = make_analyzer();
        let result = a.analyze_line("hello");
        assert!(!result.stressed_display.is_empty());
    }
}
