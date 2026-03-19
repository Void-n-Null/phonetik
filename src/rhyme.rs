use crate::phoneme;

#[derive(Clone)]
pub struct RhymeResult {
    pub rhyme_type: String,
    pub confidence: f64,
    pub tail_a: Vec<u8>,
    pub tail_b: Vec<u8>,
}

pub struct RhymeAnalyzer;

impl RhymeAnalyzer {
    pub fn analyze(phonemes_a: &[u8], phonemes_b: &[u8]) -> RhymeResult {
        let mut stressed_a = false;
        let mut stressed_b = false;

        let mut tail_a = get_rhyme_tail(phonemes_a, 1);
        let mut tail_b = get_rhyme_tail(phonemes_b, 1);
        if !tail_a.is_empty() {
            stressed_a = true;
        }
        if !tail_b.is_empty() {
            stressed_b = true;
        }

        if tail_a.is_empty() {
            tail_a = get_rhyme_tail(phonemes_a, 2);
            if !tail_a.is_empty() {
                stressed_a = true;
            }
        }
        if tail_b.is_empty() {
            tail_b = get_rhyme_tail(phonemes_b, 2);
            if !tail_b.is_empty() {
                stressed_b = true;
            }
        }

        if tail_a.is_empty() {
            tail_a = get_rhyme_tail_any_vowel(phonemes_a);
        }
        if tail_b.is_empty() {
            tail_b = get_rhyme_tail_any_vowel(phonemes_b);
        }

        if tail_a.is_empty() || tail_b.is_empty() {
            return RhymeResult {
                rhyme_type: "none".into(),
                confidence: 0.0,
                tail_a,
                tail_b,
            };
        }

        let both_unstressed = !stressed_a && !stressed_b;

        let stripped_a = phoneme::strip_all(&tail_a);
        let stripped_b = phoneme::strip_all(&tail_b);

        // Suffix rhyme check
        if stripped_a.len() != stripped_b.len() {
            let (shorter, longer, a_is_shorter) = if stripped_a.len() < stripped_b.len() {
                (&stripped_a, &stripped_b, true)
            } else {
                (&stripped_b, &stripped_a, false)
            };

            if is_suffix(shorter, longer) {
                let suffix_len = shorter.len();
                let longer_ph = if a_is_shorter { phonemes_b } else { phonemes_a };
                let shorter_ph = if a_is_shorter { phonemes_a } else { phonemes_b };

                if a_is_shorter {
                    tail_b = phonemes_b[phonemes_b.len().saturating_sub(suffix_len)..].to_vec();
                } else {
                    tail_a = phonemes_a[phonemes_a.len().saturating_sub(suffix_len)..].to_vec();
                }

                let onset_longer = get_onset_cluster(longer_ph, suffix_len);
                let onset_shorter = get_onset_cluster(shorter_ph, suffix_len);
                let onsets_differ = onset_longer != onset_shorter;

                let (mut suffix_type, mut suffix_confidence) = if !onsets_differ {
                    ("identity", 1.0)
                } else {
                    ("perfect", 0.95)
                };

                if both_unstressed && suffix_type != "none" {
                    if suffix_type == "perfect" {
                        suffix_type = "near";
                    }
                    suffix_confidence *= 0.5;
                }

                return RhymeResult {
                    rhyme_type: suffix_type.into(),
                    confidence: round4(f64::clamp(suffix_confidence, 0.0, 1.0)),
                    tail_a,
                    tail_b,
                };
            }
        }

        // Standard tail comparison
        let exact_tail_match = stripped_a == stripped_b;
        let onset_a = get_onset_cluster(phonemes_a, tail_a.len());
        let onset_b = get_onset_cluster(phonemes_b, tail_b.len());
        let onsets_differ = onset_a != onset_b;
        let tail_similarity = compute_tail_similarity(&stripped_a, &stripped_b);
        let nucleus_match =
            !stripped_a.is_empty() && !stripped_b.is_empty() && stripped_a[0] == stripped_b[0];
        let ending_consonants = ending_consonants_match(&stripped_a, &stripped_b);

        let (mut rtype, mut confidence) = if exact_tail_match && !onsets_differ {
            ("identity", tail_similarity)
        } else if exact_tail_match && onsets_differ {
            ("perfect", tail_similarity)
        } else if nucleus_match && tail_similarity >= 0.5 {
            ("near", tail_similarity)
        } else if ending_consonants && tail_similarity >= 0.3 {
            ("slant", tail_similarity * 0.8)
        } else if tail_similarity >= 0.4 {
            ("near", tail_similarity * 0.7)
        } else {
            ("none", tail_similarity * 0.3)
        };

        // Ending rhyme fallback
        if rtype == "none" || confidence < 0.5 {
            if let Some(end_result) = try_ending_rhyme(phonemes_a, phonemes_b, both_unstressed) {
                if end_result.confidence > confidence {
                    return end_result;
                }
            }
        }

        if both_unstressed && rtype != "none" {
            if rtype == "perfect" {
                rtype = "near";
            }
            confidence *= 0.5;
        }

        RhymeResult {
            rhyme_type: rtype.into(),
            confidence: round4(f64::clamp(confidence, 0.0, 1.0)),
            tail_a,
            tail_b,
        }
    }

    pub fn best_rhyme(variants_a: &[Vec<u8>], variants_b: &[Vec<u8>]) -> RhymeResult {
        let mut best = Self::analyze(&variants_a[0], &variants_b[0]);
        for (i, a) in variants_a.iter().enumerate() {
            for (j, b) in variants_b.iter().enumerate() {
                if i == 0 && j == 0 {
                    continue;
                }
                let result = Self::analyze(a, b);
                if result.confidence > best.confidence {
                    best = result;
                }
            }
        }
        best
    }
}

fn try_ending_rhyme(
    phonemes_a: &[u8],
    phonemes_b: &[u8],
    both_unstressed: bool,
) -> Option<RhymeResult> {
    let end_tail_a = get_rhyme_tail_any_vowel(phonemes_a);
    let end_tail_b = get_rhyme_tail_any_vowel(phonemes_b);

    if end_tail_a.is_empty() || end_tail_b.is_empty() {
        return None;
    }

    let end_stripped_a = phoneme::strip_all(&end_tail_a);
    let end_stripped_b = phoneme::strip_all(&end_tail_b);

    let end_exact = end_stripped_a == end_stripped_b;
    let mut end_suffix = false;

    if !end_exact && end_stripped_a.len() != end_stripped_b.len() {
        let (shorter, longer) = if end_stripped_a.len() < end_stripped_b.len() {
            (&end_stripped_a, &end_stripped_b)
        } else {
            (&end_stripped_b, &end_stripped_a)
        };
        end_suffix = is_suffix(shorter, longer);
    }

    if !end_exact && !end_suffix {
        let end_nucleus = !end_stripped_a.is_empty()
            && !end_stripped_b.is_empty()
            && end_stripped_a[0] == end_stripped_b[0];
        let end_sim = compute_tail_similarity(&end_stripped_a, &end_stripped_b);
        if end_nucleus && end_sim >= 0.6 {
            let mut conf = end_sim * 0.75;
            if both_unstressed {
                conf *= 0.5;
            }
            return Some(RhymeResult {
                rhyme_type: "near".into(),
                confidence: round4(f64::clamp(conf, 0.0, 1.0)),
                tail_a: end_tail_a,
                tail_b: end_tail_b,
            });
        }
        let end_cons = ending_consonants_match(&end_stripped_a, &end_stripped_b);
        if end_cons && end_sim >= 0.5 {
            let mut conf = end_sim * 0.65;
            if both_unstressed {
                conf *= 0.5;
            }
            return Some(RhymeResult {
                rhyme_type: "slant".into(),
                confidence: round4(f64::clamp(conf, 0.0, 1.0)),
                tail_a: end_tail_a,
                tail_b: end_tail_b,
            });
        }
        return None;
    }

    let (rtype, mut confidence) = if end_exact {
        let onset_a = get_onset_cluster(phonemes_a, end_tail_a.len());
        let onset_b = get_onset_cluster(phonemes_b, end_tail_b.len());
        if onset_a == onset_b {
            ("identity", 0.85)
        } else {
            ("near", 0.80)
        }
    } else {
        ("near", 0.75)
    };

    if both_unstressed {
        confidence *= 0.5;
    }

    Some(RhymeResult {
        rhyme_type: rtype.into(),
        confidence: round4(f64::clamp(confidence, 0.0, 1.0)),
        tail_a: end_tail_a,
        tail_b: end_tail_b,
    })
}

// ── Helpers (all operate on u8 slices now) ──────────────────────────────

pub fn get_rhyme_tail(phonemes: &[u8], stress_level: u8) -> Vec<u8> {
    for i in (0..phonemes.len()).rev() {
        if phoneme::stress(phonemes[i]) == stress_level {
            return phonemes[i..].to_vec();
        }
    }
    vec![]
}

pub fn get_rhyme_tail_any_vowel(phonemes: &[u8]) -> Vec<u8> {
    for i in (0..phonemes.len()).rev() {
        if phoneme::is_vowel(phonemes[i]) {
            return phonemes[i..].to_vec();
        }
    }
    vec![]
}

fn get_onset_cluster(phonemes: &[u8], tail_length: usize) -> Vec<u8> {
    let onset_len = phonemes.len().saturating_sub(tail_length);
    if onset_len == 0 {
        return vec![];
    }
    phonemes[..onset_len].to_vec()
}

fn is_suffix(shorter: &[u8], longer: &[u8]) -> bool {
    if shorter.is_empty() || shorter.len() > longer.len() {
        return false;
    }
    if !phoneme::is_vowel_base(shorter[0]) {
        return false;
    }
    let offset = longer.len() - shorter.len();
    shorter == &longer[offset..]
}

fn ending_consonants_match(a: &[u8], b: &[u8]) -> bool {
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    let cons_a = &a[1..];
    let cons_b = &b[1..];
    cons_a == cons_b
}

fn compute_tail_similarity(a: &[u8], b: &[u8]) -> f64 {
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein_u8(a, b);
    1.0 - (dist as f64 / max_len as f64)
}

fn levenshtein_u8(source: &[u8], target: &[u8]) -> usize {
    let (source, target) = if source.len() > target.len() {
        (target, source)
    } else {
        (source, target)
    };
    let s_len = source.len();
    let t_len = target.len();
    let mut prev = vec![0usize; s_len + 1];
    let mut curr = vec![0usize; s_len + 1];
    for j in 0..=s_len {
        prev[j] = j;
    }
    for i in 1..=t_len {
        curr[0] = i;
        for j in 1..=s_len {
            let cost = if source[j - 1] == target[i - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[s_len]
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}
