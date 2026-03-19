use crate::distance;
use crate::phoneme;

pub struct PhoneticComparer;

impl PhoneticComparer {
    pub fn similarity(a: &[u8], b: &[u8]) -> f64 {
        let sa = phoneme::strip_all(a);
        let sb = phoneme::strip_all(b);
        let dist = distance::levenshtein(&sa, &sb);
        let max_len = sa.len().max(sb.len());
        if max_len == 0 {
            return 1.0;
        }
        1.0 - (dist as f64 / max_len as f64)
    }

    pub fn best_similarity<'a>(
        variants_a: &'a [Vec<u8>],
        variants_b: &'a [Vec<u8>],
    ) -> (f64, &'a [u8], &'a [u8]) {
        let mut best = -1.0_f64;
        let mut best_a = &variants_a[0][..];
        let mut best_b = &variants_b[0][..];
        for a in variants_a {
            for b in variants_b {
                let score = Self::similarity(a, b);
                if score > best {
                    best = score;
                    best_a = a;
                    best_b = b;
                }
            }
        }
        (best, best_a, best_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phoneme;

    #[test]
    fn identical_words_have_similarity_one() {
        let cat = vec![phoneme::K, phoneme::encode("AE1"), phoneme::T];
        assert_eq!(PhoneticComparer::similarity(&cat, &cat), 1.0);
    }

    #[test]
    fn completely_different_words() {
        let a = vec![phoneme::K, phoneme::encode("AE1"), phoneme::T];
        let b = vec![phoneme::SH, phoneme::encode("IY1"), phoneme::P];
        let score = PhoneticComparer::similarity(&a, &b);
        assert!(score < 0.5);
    }

    #[test]
    fn similar_words_have_high_score() {
        // CAT vs BAT — differ only in first consonant
        let cat = vec![phoneme::K, phoneme::encode("AE1"), phoneme::T];
        let bat = vec![phoneme::B, phoneme::encode("AE1"), phoneme::T];
        let score = PhoneticComparer::similarity(&cat, &bat);
        assert!(score > 0.5, "cat/bat should be > 50% similar, got {score}");
    }

    #[test]
    fn best_similarity_picks_highest() {
        let v1 = vec![vec![phoneme::K, phoneme::encode("AE1"), phoneme::T]];
        let v2 = vec![
            vec![phoneme::B, phoneme::encode("AE1"), phoneme::T],
            vec![phoneme::K, phoneme::encode("AE1"), phoneme::T], // identical to v1
        ];
        let (score, _, _) = PhoneticComparer::best_similarity(&v1, &v2);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn empty_slices() {
        assert_eq!(PhoneticComparer::similarity(&[], &[]), 1.0);
    }
}
