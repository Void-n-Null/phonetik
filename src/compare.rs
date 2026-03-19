use crate::phoneme;

pub struct PhoneticComparer;

impl PhoneticComparer {
    pub fn similarity(a: &[u8], b: &[u8]) -> f64 {
        let sa = phoneme::strip_all(a);
        let sb = phoneme::strip_all(b);
        let dist = levenshtein(&sa, &sb);
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

fn levenshtein(source: &[u8], target: &[u8]) -> usize {
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
