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
