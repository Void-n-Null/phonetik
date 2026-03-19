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
