use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::dict::CmuDict;
use crate::phoneme;

pub struct RhymeMapAnalyzer {
    dict: Arc<CmuDict>,
}

const MAX_NGRAM_LEN: usize = 4;

impl RhymeMapAnalyzer {
    pub fn new(dict: Arc<CmuDict>) -> Self {
        Self { dict }
    }

    pub fn analyze(&self, lines: &[String], opts: &RhymeMapOptions) -> RhymeMapResult {
        // Phase 1: Tokenize and look up
        let mut entries: Vec<WordEntry> = Vec::new();
        for (li, line) in lines.iter().enumerate() {
            let tokens = tokenize(line);
            for (wi, token) in tokens.iter().enumerate() {
                let lookup = self.dict.lookup(token);
                let (raw, in_dict) = if let Some(ref l) = lookup {
                    (l[0].clone(), true)
                } else {
                    (vec![], false)
                };
                let stripped = phoneme::strip_all(&raw);
                entries.push(WordEntry {
                    word: token.clone(),
                    line: li,
                    position: wi,
                    raw,
                    stripped,
                    in_dictionary: in_dict,
                });
            }
        }

        // Build last position per line for end-of-line boosting
        let mut last_pos_per_line: HashMap<usize, usize> = HashMap::new();
        for e in &entries {
            let entry = last_pos_per_line.entry(e.line).or_insert(0);
            if e.position > *entry {
                *entry = e.position;
            }
        }

        // Phase 2: Count all phoneme n-grams
        // Key: [u8; 4] packed phoneme IDs (unused slots = 0), value: hits
        let mut ngram_hits: HashMap<[u8; MAX_NGRAM_LEN], Vec<NgramHit>> = HashMap::new();

        for (ei, entry) in entries.iter().enumerate() {
            let stripped = &entry.stripped;
            if stripped.is_empty() {
                continue;
            }
            for start in 0..stripped.len() {
                let max_len = MAX_NGRAM_LEN.min(stripped.len() - start);
                let mut key = [0u8; MAX_NGRAM_LEN];
                for len in 1..=max_len {
                    key[len - 1] = stripped[start + len - 1];
                    ngram_hits.entry(key).or_default().push(NgramHit {
                        entry_index: ei,
                        start,
                        length: len,
                    });
                }
            }
        }

        // Phase 3: Build, score, filter patterns
        let mut patterns: Vec<RhymePattern> = Vec::new();

        for (key, hits) in &ngram_hits {
            let mut distinct: std::collections::HashSet<u64> = std::collections::HashSet::new();
            for h in hits {
                let e = &entries[h.entry_index];
                distinct.insert(((e.line as u64) << 32) | (e.position as u64));
            }
            let count = distinct.len();
            if count < opts.min_count {
                continue;
            }

            // Compute actual n-gram length from the key
            let ngram_length = key.iter().filter(|&&b| b != 0).count();
            if ngram_length < opts.min_length {
                continue;
            }

            let vowel_anchored = phoneme::is_vowel_base(key[0]);

            let mut end_of_line_count = 0;
            for h in hits {
                let e = &entries[h.entry_index];
                if let Some(&last) = last_pos_per_line.get(&e.line) {
                    if e.position == last {
                        end_of_line_count += 1;
                    }
                }
            }

            let end_of_line_fraction = if !hits.is_empty() {
                end_of_line_count as f64 / hits.len() as f64
            } else {
                0.0
            };
            let position_mult = 1.0 + (end_of_line_fraction * (opts.end_of_line_boost - 1.0));

            let length_factor = (ngram_length as f64).powf(opts.length_weight);
            let count_factor = (count as f64).log2();
            let type_mult = if vowel_anchored {
                opts.vowel_boost
            } else {
                opts.consonant_penalty
            };
            let score = (length_factor * count_factor * type_mult * position_mult * 10000.0)
                .round()
                / 10000.0;

            if score < opts.min_score {
                continue;
            }

            // Format n-gram key back to string for JSON output
            let ngram_str = format_ngram_key(key, ngram_length);

            let members: Vec<PatternMember> = hits
                .iter()
                .map(|h| {
                    let e = &entries[h.entry_index];
                    let indices: Vec<usize> = (0..h.length).map(|k| h.start + k).collect();
                    let matched: Vec<String> = e.stripped[h.start..h.start + h.length]
                        .iter()
                        .map(|&id| phoneme::decode(id).to_string())
                        .collect();
                    PatternMember {
                        word: e.word.clone(),
                        line: e.line,
                        position: e.position,
                        phoneme_indices: indices,
                        matched_phonemes: matched,
                    }
                })
                .collect();

            patterns.push(RhymePattern {
                id: 0,
                ngram: ngram_str,
                length: ngram_length,
                count,
                score,
                is_vowel_anchored: vowel_anchored,
                members,
            });
        }

        // Phase 4: Sort, cap, re-number
        patterns.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.count.cmp(&a.count))
        });

        if opts.max_patterns > 0 && patterns.len() > opts.max_patterns {
            patterns.truncate(opts.max_patterns);
        }

        for (i, p) in patterns.iter_mut().enumerate() {
            p.id = i;
        }

        // Phase 5: Per-word highlight summary
        let mut pos_to_entry: HashMap<u64, usize> = HashMap::new();
        for (ei, e) in entries.iter().enumerate() {
            let pk = ((e.line as u64) << 32) | (e.position as u64);
            pos_to_entry.entry(pk).or_insert(ei);
        }

        let mut entry_pattern_refs: HashMap<usize, Vec<WordPatternRef>> = HashMap::new();
        for (pi, pattern) in patterns.iter().enumerate() {
            for m in &pattern.members {
                let pk = ((m.line as u64) << 32) | (m.position as u64);
                if let Some(&ei) = pos_to_entry.get(&pk) {
                    entry_pattern_refs
                        .entry(ei)
                        .or_default()
                        .push(WordPatternRef {
                            pattern_id: pi,
                            phoneme_indices: m.phoneme_indices.clone(),
                        });
                }
            }
        }

        let words: Vec<RhymeMapWord> = entries
            .iter()
            .enumerate()
            .map(|(ei, e)| {
                let prefs = entry_pattern_refs.get(&ei).cloned().unwrap_or_default();
                RhymeMapWord {
                    word: e.word.clone(),
                    line: e.line,
                    position: e.position,
                    phonemes: phoneme::decode_to_strings(&e.raw),
                    stripped_phonemes: phoneme::decode_to_strings(&e.stripped),
                    in_dictionary: e.in_dictionary,
                    syllables: phoneme::count_syllables(&e.raw),
                    patterns: prefs,
                }
            })
            .collect();

        RhymeMapResult {
            lines: lines.to_vec(),
            words,
            patterns,
        }
    }
}

/// Format a packed [u8;4] n-gram key to the dash-separated string for JSON.
fn format_ngram_key(key: &[u8; MAX_NGRAM_LEN], len: usize) -> String {
    let mut s = String::with_capacity(len * 4);
    for i in 0..len {
        if i > 0 {
            s.push('-');
        }
        s.push_str(phoneme::decode(key[i]));
    }
    s
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
            } else if chars[i] == '-'
                && start.is_some()
                && i + 1 < chars.len()
                && chars[i + 1].is_alphabetic()
            {
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
            tokens.push(chars[s..i].iter().collect());
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

struct WordEntry {
    word: String,
    line: usize,
    position: usize,
    raw: Vec<u8>,
    stripped: Vec<u8>,
    in_dictionary: bool,
}

struct NgramHit {
    entry_index: usize,
    start: usize,
    length: usize,
}

// ── Public types ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RhymeMapRequest {
    pub lines: Vec<String>,
    pub options: Option<RhymeMapOptions>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RhymeMapOptions {
    #[serde(default = "default_min_count")]
    pub min_count: usize,
    #[serde(default = "default_min_length")]
    pub min_length: usize,
    #[serde(default)]
    pub min_score: f64,
    #[serde(default = "default_max_patterns")]
    pub max_patterns: usize,
    #[serde(default = "default_length_weight")]
    pub length_weight: f64,
    #[serde(default = "default_vowel_boost")]
    pub vowel_boost: f64,
    #[serde(default = "default_consonant_penalty")]
    pub consonant_penalty: f64,
    #[serde(default = "default_end_of_line_boost")]
    pub end_of_line_boost: f64,
}

fn default_min_count() -> usize {
    2
}
fn default_min_length() -> usize {
    2
}
fn default_max_patterns() -> usize {
    50
}
fn default_length_weight() -> f64 {
    2.0
}
fn default_vowel_boost() -> f64 {
    1.0
}
fn default_consonant_penalty() -> f64 {
    0.3
}
fn default_end_of_line_boost() -> f64 {
    3.0
}

impl Default for RhymeMapOptions {
    fn default() -> Self {
        Self {
            min_count: 2,
            min_length: 2,
            min_score: 0.0,
            max_patterns: 50,
            length_weight: 2.0,
            vowel_boost: 1.0,
            consonant_penalty: 0.3,
            end_of_line_boost: 3.0,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhymeMapResult {
    pub lines: Vec<String>,
    pub words: Vec<RhymeMapWord>,
    pub patterns: Vec<RhymePattern>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhymeMapWord {
    pub word: String,
    pub line: usize,
    pub position: usize,
    pub phonemes: Vec<String>,
    pub stripped_phonemes: Vec<String>,
    pub in_dictionary: bool,
    pub syllables: usize,
    pub patterns: Vec<WordPatternRef>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WordPatternRef {
    pub pattern_id: usize,
    pub phoneme_indices: Vec<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RhymePattern {
    pub id: usize,
    pub ngram: String,
    pub length: usize,
    pub count: usize,
    pub score: f64,
    pub is_vowel_anchored: bool,
    pub members: Vec<PatternMember>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternMember {
    pub word: String,
    pub line: usize,
    pub position: usize,
    pub phoneme_indices: Vec<usize>,
    pub matched_phonemes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_analyzer() -> RhymeMapAnalyzer {
        RhymeMapAnalyzer::new(Arc::new(crate::dict::CmuDict::load()))
    }

    #[test]
    fn analyze_finds_patterns_in_rhyming_couplet() {
        let a = make_analyzer();
        let lines = vec![
            "the cat sat on the mat".to_string(),
            "the bat sat on the hat".to_string(),
        ];
        let result = a.analyze(&lines, &RhymeMapOptions::default());
        assert!(!result.patterns.is_empty());
        assert_eq!(result.lines.len(), 2);
    }

    #[test]
    fn analyze_words_are_tracked() {
        let a = make_analyzer();
        let lines = vec!["hello world".to_string()];
        let result = a.analyze(&lines, &RhymeMapOptions::default());
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words[0].line, 0);
        assert_eq!(result.words[0].position, 0);
        assert_eq!(result.words[1].position, 1);
    }

    #[test]
    fn analyze_empty_lines() {
        let a = make_analyzer();
        let result = a.analyze(&[], &RhymeMapOptions::default());
        assert!(result.words.is_empty());
        assert!(result.patterns.is_empty());
    }

    #[test]
    fn patterns_sorted_by_score_desc() {
        let a = make_analyzer();
        let lines = vec![
            "the cat sat on the mat".to_string(),
            "the bat sat on the hat".to_string(),
        ];
        let result = a.analyze(&lines, &RhymeMapOptions::default());
        for w in result.patterns.windows(2) {
            assert!(w[0].score >= w[1].score);
        }
    }

    #[test]
    fn max_patterns_option_caps_output() {
        let a = make_analyzer();
        let lines = vec![
            "the cat sat on the mat".to_string(),
            "the bat sat on the hat".to_string(),
        ];
        let opts = RhymeMapOptions {
            max_patterns: 3,
            ..RhymeMapOptions::default()
        };
        let result = a.analyze(&lines, &opts);
        assert!(result.patterns.len() <= 3);
    }

    #[test]
    fn pattern_ids_are_sequential() {
        let a = make_analyzer();
        let lines = vec![
            "the cat sat on the mat".to_string(),
            "the bat sat on the hat".to_string(),
        ];
        let result = a.analyze(&lines, &RhymeMapOptions::default());
        for (i, p) in result.patterns.iter().enumerate() {
            assert_eq!(p.id, i);
        }
    }

    #[test]
    fn tokenize_handles_hyphens_and_g_drops() {
        let tokens = tokenize("well-known runnin'");
        assert_eq!(tokens, vec!["well-known", "runnin'"]);
    }

    #[test]
    fn unknown_words_marked() {
        let a = make_analyzer();
        let lines = vec!["xyzzyplugh".to_string()];
        let result = a.analyze(&lines, &RhymeMapOptions::default());
        assert_eq!(result.words.len(), 1);
        assert!(!result.words[0].in_dictionary);
    }
}
