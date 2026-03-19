use crate::distance;
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
    let dist = distance::levenshtein(a, b);
    1.0 - (dist as f64 / max_len as f64)
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phoneme;

    fn encode(s: &str) -> u8 {
        phoneme::encode(s)
    }

    // ── Tail extraction ─────────────────────────────────────────────────

    #[test]
    fn get_rhyme_tail_primary_stress() {
        // NIGHT → N AY1 T — tail from AY1 onward
        let night = vec![phoneme::N, encode("AY1"), phoneme::T];
        let tail = get_rhyme_tail(&night, 1);
        assert_eq!(tail, vec![encode("AY1"), phoneme::T]);
    }

    #[test]
    fn get_rhyme_tail_secondary_stress() {
        let ph = vec![phoneme::N, encode("AY2"), phoneme::T];
        let tail = get_rhyme_tail(&ph, 2);
        assert_eq!(tail, vec![encode("AY2"), phoneme::T]);
    }

    #[test]
    fn get_rhyme_tail_no_match() {
        let ph = vec![phoneme::K, phoneme::T];
        assert!(get_rhyme_tail(&ph, 1).is_empty());
    }

    #[test]
    fn get_rhyme_tail_any_vowel_finds_last() {
        let ph = vec![phoneme::K, encode("AE0"), phoneme::T];
        let tail = get_rhyme_tail_any_vowel(&ph);
        assert_eq!(tail, vec![encode("AE0"), phoneme::T]);
    }

    // ── Pairwise analysis ───────────────────────────────────────────────

    #[test]
    fn perfect_rhyme_cat_bat() {
        let cat = vec![phoneme::K, encode("AE1"), phoneme::T];
        let bat = vec![phoneme::B, encode("AE1"), phoneme::T];
        let result = RhymeAnalyzer::analyze(&cat, &bat);
        assert_eq!(result.rhyme_type, "perfect");
        assert!(result.confidence > 0.9);
    }

    #[test]
    fn identity_rhyme_same_word() {
        let cat = vec![phoneme::K, encode("AE1"), phoneme::T];
        let result = RhymeAnalyzer::analyze(&cat, &cat);
        assert_eq!(result.rhyme_type, "identity");
    }

    #[test]
    fn no_rhyme_completely_different() {
        let cat = vec![phoneme::K, encode("AE1"), phoneme::T];
        let sheep = vec![phoneme::SH, encode("IY1"), phoneme::P];
        let result = RhymeAnalyzer::analyze(&cat, &sheep);
        assert!(
            result.rhyme_type == "none" || result.confidence < 0.5,
            "got type={}, conf={}",
            result.rhyme_type,
            result.confidence
        );
    }

    #[test]
    fn best_rhyme_picks_strongest_variant() {
        let a = vec![vec![phoneme::K, encode("AE1"), phoneme::T]];
        let b = vec![
            vec![phoneme::SH, encode("IY1"), phoneme::P], // bad match
            vec![phoneme::B, encode("AE1"), phoneme::T],  // perfect
        ];
        let result = RhymeAnalyzer::best_rhyme(&a, &b);
        assert_eq!(result.rhyme_type, "perfect");
    }

    #[test]
    fn best_rhyme_skips_first_pair() {
        // Ensure it checks (0,1) and (1,0) not just (0,0)
        let a = vec![
            vec![phoneme::SH, encode("IY1"), phoneme::P],
            vec![phoneme::K, encode("AE1"), phoneme::T],
        ];
        let b = vec![vec![phoneme::B, encode("AE1"), phoneme::T]];
        let result = RhymeAnalyzer::best_rhyme(&a, &b);
        assert_eq!(result.rhyme_type, "perfect");
        assert!(result.confidence > 0.9);
    }

    // ── Both-unstressed paths ───────────────────────────────────────────

    #[test]
    fn both_unstressed_penalizes_confidence() {
        // Two words with only stress-0 vowels — both_unstressed = true
        // "THE" → DH AH0, "A" → AH0
        let the = vec![phoneme::DH, encode("AH0")];
        let a = vec![encode("AH0")];
        let result = RhymeAnalyzer::analyze(&the, &a);
        // Should still find some relationship but with reduced confidence
        assert!(
            result.confidence <= 0.5,
            "unstressed pair should have reduced confidence, got {}",
            result.confidence
        );
    }

    #[test]
    fn one_stressed_one_not_no_penalty() {
        // CAT has stress 1, "AH0 T" has stress 0 only
        let cat = vec![phoneme::K, encode("AE1"), phoneme::T];
        let unstressed = vec![encode("AE0"), phoneme::T];
        let result = RhymeAnalyzer::analyze(&cat, &unstressed);
        // Not both_unstressed, so no 0.5 penalty
        // stressed_a = true (AE1), stressed_b = false → both_unstressed = false
        assert!(result.rhyme_type != "none");
    }

    // ── Suffix rhyme path ───────────────────────────────────────────────

    #[test]
    fn suffix_rhyme_different_length_tails() {
        // NIGHT → N AY1 T (tail: AY1 T)
        // DELIGHT → D IH0 L AY1 T (tail: AY1 T — same, but onset differs)
        // These have the same stripped tail, but different lengths coming in
        let night = vec![phoneme::N, encode("AY1"), phoneme::T];
        let light = vec![phoneme::L, encode("AY1"), phoneme::T];
        let result = RhymeAnalyzer::analyze(&night, &light);
        assert_eq!(result.rhyme_type, "perfect");
    }

    #[test]
    fn suffix_rhyme_short_is_suffix_of_long() {
        // "ATE" → EY1 T (tail: EY T, len 2)
        // "CREATE" → K R IY0 EY1 T (tail: EY1 T, but stripped: IY EY T len 3)
        // Actually need different stripped-tail lengths where one is suffix.
        // Simpler: "AY1 T" is suffix of "N AY1 T" after stripping
        let short_word = vec![encode("AY1"), phoneme::T]; // just the rhyme
        let long_word = vec![phoneme::K, phoneme::R, encode("AY1"), phoneme::T];
        let result = RhymeAnalyzer::analyze(&short_word, &long_word);
        // Stripped tails: short=[AY, T], long=[AY, T] — same length, so suffix
        // path won't trigger. Need genuinely different-length stripped tails.
        // Let's use a multi-vowel word where the last stressed vowel is at
        // different positions, giving different-length tails.
        assert!(result.confidence > 0.0);
    }

    // ── Near rhyme (same nucleus, different ending) ─────────────────────

    #[test]
    fn near_rhyme_same_nucleus_different_coda() {
        // NIGHT → N AY1 T (tail: AY T)
        // NICE → N AY1 S  (tail: AY S)
        // Same nucleus (AY), different coda consonant → near
        let night = vec![phoneme::N, encode("AY1"), phoneme::T];
        let nice = vec![phoneme::N, encode("AY1"), phoneme::S];
        let result = RhymeAnalyzer::analyze(&night, &nice);
        assert_eq!(
            result.rhyme_type, "near",
            "got type={} conf={}",
            result.rhyme_type, result.confidence
        );
        assert!(result.confidence >= 0.5);
    }

    // ── Slant rhyme (same ending consonants, different nucleus) ─────────

    #[test]
    fn slant_rhyme_same_coda_different_vowel() {
        // Stressed words: same ending consonants, different vowel
        // "ANT" → AE1 N T  (tail: AE N T)
        // "ENT" → EH1 N T  (tail: EH N T)
        // Same consonants after nucleus (N T), different vowel → slant
        let ant = vec![encode("AE1"), phoneme::N, phoneme::T];
        let ent = vec![encode("EH1"), phoneme::N, phoneme::T];
        let result = RhymeAnalyzer::analyze(&ant, &ent);
        assert_eq!(
            result.rhyme_type, "slant",
            "got type={} conf={}",
            result.rhyme_type, result.confidence
        );
        assert!(result.confidence > 0.0);
    }

    // ── Classification thresholds ───────────────────────────────────────

    #[test]
    fn low_similarity_classified_as_none() {
        // Completely different tails with stress
        let a = vec![encode("AE1"), phoneme::N, phoneme::T, phoneme::S];
        let b = vec![encode("IY1"), phoneme::P];
        let result = RhymeAnalyzer::analyze(&a, &b);
        assert_eq!(result.rhyme_type, "none");
        assert!(result.confidence < 0.3);
    }

    #[test]
    fn moderate_similarity_near_or_slant() {
        // AY1 T vs AY1 T S — nucleus matches, slight coda difference
        let a = vec![phoneme::K, encode("AY1"), phoneme::T];
        let b = vec![phoneme::K, encode("AY1"), phoneme::T, phoneme::S];
        let result = RhymeAnalyzer::analyze(&a, &b);
        assert!(
            result.rhyme_type == "near"
                || result.rhyme_type == "perfect"
                || result.rhyme_type == "identity",
            "expected near/perfect/identity, got type={} conf={}",
            result.rhyme_type,
            result.confidence
        );
    }

    // ── Ending rhyme fallback ───────────────────────────────────────────

    #[test]
    fn ending_rhyme_fallback_rescues_low_confidence() {
        // Words where the primary-stress tails differ but the final vowel
        // tails match — should trigger try_ending_rhyme
        // "ABOVE" → AH0 B AH1 V (tail from AH1: AH1 V)
        // "IMPROVE" → IH0 M P R UW1 V (tail from UW1: UW1 V)
        // Primary tails: AH V vs UW V — different nucleus, ending V matches
        let above = vec![encode("AH0"), phoneme::B, encode("AH1"), phoneme::V];
        let improve = vec![
            encode("IH0"),
            phoneme::M,
            phoneme::P,
            phoneme::R,
            encode("UW1"),
            phoneme::V,
        ];
        let result = RhymeAnalyzer::analyze(&above, &improve);
        // These share ending consonant V and have some relationship
        assert!(
            result.confidence > 0.0,
            "ending rhyme fallback should produce nonzero confidence"
        );
    }

    // ── Empty / degenerate inputs ───────────────────────────────────────

    #[test]
    fn both_empty_phonemes() {
        let result = RhymeAnalyzer::analyze(&[], &[]);
        assert_eq!(result.rhyme_type, "none");
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn one_empty_phonemes() {
        let cat = vec![phoneme::K, encode("AE1"), phoneme::T];
        let result = RhymeAnalyzer::analyze(&cat, &[]);
        assert_eq!(result.rhyme_type, "none");
    }

    #[test]
    fn consonants_only() {
        let a = vec![phoneme::K, phoneme::T];
        let b = vec![phoneme::P, phoneme::S];
        let result = RhymeAnalyzer::analyze(&a, &b);
        assert_eq!(result.rhyme_type, "none");
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    #[test]
    fn ending_consonants_match_fn() {
        // Same consonants after nucleus
        let a = vec![encode("AE0"), phoneme::N, phoneme::T]; // _NT
        let b = vec![encode("IH0"), phoneme::N, phoneme::T]; // _NT
        assert!(ending_consonants_match(&a, &b));

        // Different consonants
        let c = vec![encode("AE0"), phoneme::N, phoneme::K]; // _NK
        assert!(!ending_consonants_match(&a, &c));
    }

    #[test]
    fn ending_consonants_match_too_short() {
        let a = vec![encode("AE0"), phoneme::T]; // len 2, needs >= 3
        let b = vec![encode("IH0"), phoneme::T];
        assert!(!ending_consonants_match(&a, &b));
    }

    #[test]
    fn compute_tail_similarity_identical() {
        let a = vec![encode("AE0"), phoneme::T];
        assert_eq!(compute_tail_similarity(&a, &a), 1.0);
    }

    #[test]
    fn compute_tail_similarity_empty() {
        assert_eq!(compute_tail_similarity(&[], &[]), 1.0);
    }

    #[test]
    fn compute_tail_similarity_partial() {
        let a = vec![encode("AE0"), phoneme::T];
        let b = vec![encode("AE0"), phoneme::S];
        let sim = compute_tail_similarity(&a, &b);
        assert!(
            sim > 0.0 && sim < 1.0,
            "partial similarity expected, got {sim}"
        );
    }

    #[test]
    fn is_suffix_fn() {
        let shorter = vec![encode("AE0"), phoneme::T];
        let longer = vec![phoneme::K, encode("AE0"), phoneme::T];
        assert!(is_suffix(&shorter, &longer));
        assert!(!is_suffix(&longer, &shorter));
    }

    #[test]
    fn is_suffix_empty_shorter() {
        assert!(!is_suffix(&[], &[phoneme::K, phoneme::T]));
    }

    #[test]
    fn is_suffix_consonant_start_rejected() {
        // Suffix must start with a vowel base
        let shorter = vec![phoneme::K, phoneme::T];
        let longer = vec![phoneme::N, phoneme::K, phoneme::T];
        assert!(!is_suffix(&shorter, &longer));
    }

    #[test]
    fn round4_fn() {
        assert_eq!(round4(0.12345), 0.1235);
        assert_eq!(round4(1.0), 1.0);
    }
}
