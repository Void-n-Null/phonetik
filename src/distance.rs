/// Edit distance on u8 phoneme slices.
///
/// Single-row Levenshtein with a stack-allocated buffer. No heap
/// allocation for inputs up to 32 phonemes (covers the entire CMU
/// dictionary with headroom). Falls back to heap for longer inputs.

/// Stack buffer capacity. The longest CMUdict entry is ~18 phonemes;
/// 32 covers any realistic input.
const STACK_CAP: usize = 32;

/// Levenshtein edit distance between two phoneme ID slices.
#[inline]
pub fn levenshtein(source: &[u8], target: &[u8]) -> usize {
    // Short side as column axis minimizes buffer size
    let (short, long) = if source.len() <= target.len() {
        (source, target)
    } else {
        (target, source)
    };
    let s_len = short.len();
    let l_len = long.len();

    if s_len == 0 {
        return l_len;
    }

    // Stack path — covers all phoneme data
    if s_len < STACK_CAP {
        let mut row = [0usize; STACK_CAP];
        for j in 0..=s_len {
            row[j] = j;
        }
        for i in 1..=l_len {
            let mut prev_diag = row[0];
            row[0] = i;
            for j in 1..=s_len {
                let old = row[j];
                let cost = if short[j - 1] == long[i - 1] { 0 } else { 1 };
                row[j] = (row[j - 1] + 1).min(row[j] + 1).min(prev_diag + cost);
                prev_diag = old;
            }
        }
        return row[s_len];
    }

    // Heap fallback for pathologically long inputs
    let mut row: Vec<usize> = (0..=s_len).collect();
    for i in 1..=l_len {
        let mut prev_diag = row[0];
        row[0] = i;
        for j in 1..=s_len {
            let old = row[j];
            let cost = if short[j - 1] == long[i - 1] { 0 } else { 1 };
            row[j] = (row[j - 1] + 1).min(row[j] + 1).min(prev_diag + cost);
            prev_diag = old;
        }
    }
    row[s_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_slices() {
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 2, 3]), 0);
    }

    #[test]
    fn both_empty() {
        assert_eq!(levenshtein(&[], &[]), 0);
    }

    #[test]
    fn one_empty() {
        assert_eq!(levenshtein(&[], &[1, 2, 3]), 3);
        assert_eq!(levenshtein(&[1, 2, 3], &[]), 3);
    }

    #[test]
    fn single_substitution() {
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 9, 3]), 1);
    }

    #[test]
    fn single_insertion() {
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 2, 9, 3]), 1);
    }

    #[test]
    fn single_deletion() {
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 3]), 1);
    }

    #[test]
    fn completely_different() {
        assert_eq!(levenshtein(&[1, 2, 3], &[4, 5, 6]), 3);
    }

    #[test]
    fn argument_order_irrelevant() {
        let a = &[1, 2, 3, 4];
        let b = &[1, 9, 3];
        assert_eq!(levenshtein(a, b), levenshtein(b, a));
    }

    #[test]
    fn single_element_slices() {
        assert_eq!(levenshtein(&[1], &[1]), 0);
        assert_eq!(levenshtein(&[1], &[2]), 1);
    }

    #[test]
    fn heap_fallback_path() {
        // Force the heap path by exceeding STACK_CAP
        let a: Vec<u8> = (0..33).collect();
        let b: Vec<u8> = (0..33).collect();
        assert_eq!(levenshtein(&a, &b), 0);

        let mut c = a.clone();
        c[16] = 255;
        assert_eq!(levenshtein(&a, &c), 1);
    }
}
