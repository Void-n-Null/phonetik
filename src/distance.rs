/// Edit distance on u8 phoneme slices.
///
/// Standard Levenshtein using the two-row optimization — O(min(m,n))
/// memory. The shorter slice is always used as the column axis to
/// minimize allocation.

/// Levenshtein edit distance between two phoneme ID slices.
#[inline]
pub fn levenshtein(source: &[u8], target: &[u8]) -> usize {
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
