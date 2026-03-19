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
    fn multiple_edits_stack_path() {
        // Distance 2: two substitutions — exercises DP accumulation
        assert_eq!(levenshtein(&[1, 2, 3, 4], &[1, 9, 3, 8]), 2);
        // Distance 3: three edits in sequence
        assert_eq!(levenshtein(&[1, 2, 3, 4, 5], &[1, 9, 8, 7, 5]), 3);
        // Prefix match then divergence
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 2, 3, 4, 5]), 2);
    }

    #[test]
    fn stack_boundary_at_cap_minus_one() {
        // s_len = 31 — last valid stack path entry (STACK_CAP = 32)
        let a: Vec<u8> = (0..31).collect();
        let b: Vec<u8> = (0..31).collect();
        assert_eq!(levenshtein(&a, &b), 0);

        let mut c = a.clone();
        c[0] = 255;
        c[30] = 255;
        assert_eq!(levenshtein(&a, &c), 2);
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

    #[test]
    fn heap_path_multiple_edits() {
        // Distance > 1 on the heap path — exercises DP accumulation there
        let a: Vec<u8> = (0..35).collect();
        let mut b = a.clone();
        b[0] = 255;
        b[17] = 255;
        b[34] = 255;
        assert_eq!(levenshtein(&a, &b), 3);
    }

    #[test]
    fn asymmetric_lengths_exercises_both_roles() {
        // Short=3, long=6. Verify correct distance regardless of arg order.
        let short = &[1, 2, 3];
        let long = &[1, 2, 3, 4, 5, 6];
        assert_eq!(levenshtein(short, long), 3);
        assert_eq!(levenshtein(long, short), 3);
    }

    #[test]
    fn stack_boundary_exact_cap() {
        // s_len = 32 exactly — must go to heap path, not stack.
        // If the guard were `<=` instead of `<`, this would OOB-panic
        // on the 32-element stack array.
        let a: Vec<u8> = (0..32).collect();
        let mut b: Vec<u8> = (0..32).collect();
        b[0] = 255;
        b[31] = 255;
        assert_eq!(levenshtein(&a, &b), 2);
    }

    #[test]
    fn deletion_heavy_stack() {
        // Optimal path is 4 deletions: [1,2,3,4,5] → [5]
        // If deletion cost were free, result would be < 4.
        assert_eq!(levenshtein(&[1, 2, 3, 4, 5], &[5]), 4);
    }

    #[test]
    fn insertion_heavy_stack() {
        // Optimal path is 4 insertions: [5] → [1,2,3,4,5]
        assert_eq!(levenshtein(&[5], &[1, 2, 3, 4, 5]), 4);
    }

    #[test]
    fn mixed_ops_known_distance() {
        // Manually verified: "kitten" → "sitting" = 3 edits
        // k→s, e→i, +g
        let kitten = &[11, 9, 20, 20, 5, 14]; // 6 elements
        let sitting = &[19, 9, 20, 20, 9, 14, 7]; // 7 elements
        assert_eq!(levenshtein(kitten, sitting), 3);
    }

    #[test]
    fn heap_deletion_heavy() {
        // Force heap path, verify deletion-heavy case
        let a: Vec<u8> = (0..40).collect();
        let b: Vec<u8> = vec![39]; // just the last element
        assert_eq!(levenshtein(&a, &b), 39);
    }
}
