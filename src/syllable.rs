use crate::stress::WordStress;

pub struct SyllableSplitter;

const VOWELS: &[char] = &['A', 'E', 'I', 'O', 'U', 'Y'];

impl SyllableSplitter {
    pub fn split(word: &str, expected_syllables: usize) -> Vec<String> {
        let expected = if expected_syllables == 0 {
            1
        } else {
            expected_syllables
        };
        if word.is_empty() {
            return vec![word.to_string()];
        }
        if expected == 1 {
            return vec![word.to_string()];
        }

        let mut nuclei = find_nuclei(word);
        if nuclei.is_empty() {
            return vec![word.to_string()];
        }

        if nuclei.len() > expected {
            nuclei = merge_nuclei(nuclei, expected);
        } else if nuclei.len() < expected {
            nuclei = split_nuclei(nuclei, expected);
        }

        build_syllables(word, &nuclei)
    }

    pub fn stress_display(word: &str, stresses: &[i32]) -> String {
        let syllables = Self::split(word, stresses.len());
        let parts: Vec<String> = syllables
            .iter()
            .enumerate()
            .map(|(i, syl)| {
                let stress = stresses.get(i).copied().unwrap_or(0);
                if stress > 0 {
                    syl.to_uppercase()
                } else {
                    syl.to_lowercase()
                }
            })
            .collect();
        parts.join("-")
    }

    pub fn format_line(words: &[WordStress]) -> String {
        words
            .iter()
            .map(|w| Self::stress_display(&w.word, &w.stresses))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn is_vowel(c: char) -> bool {
    VOWELS.contains(&c.to_uppercase().next().unwrap_or(c))
}

fn find_nuclei(word: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = word.chars().collect();
    let mut nuclei = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if is_vowel(chars[i]) {
            let start = i;
            while i < chars.len() && is_vowel(chars[i]) {
                i += 1;
            }
            let is_silent_e = i == chars.len()
                && (i - start) == 1
                && chars[start].to_uppercase().next() == Some('E')
                && !nuclei.is_empty()
                && start > 0;

            if !is_silent_e {
                nuclei.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    if nuclei.is_empty() && !word.is_empty() {
        nuclei.push((0, word.len()));
    }
    nuclei
}

fn merge_nuclei(mut nuclei: Vec<(usize, usize)>, target: usize) -> Vec<(usize, usize)> {
    while nuclei.len() > target && nuclei.len() > 1 {
        let mut best_idx = 0;
        let mut best_gap = usize::MAX;
        for i in 0..nuclei.len() - 1 {
            let gap = nuclei[i + 1].0 - nuclei[i].1;
            if gap < best_gap {
                best_gap = gap;
                best_idx = i;
            }
        }
        let merged = (nuclei[best_idx].0, nuclei[best_idx + 1].1);
        nuclei[best_idx] = merged;
        nuclei.remove(best_idx + 1);
    }
    nuclei
}

fn split_nuclei(mut nuclei: Vec<(usize, usize)>, target: usize) -> Vec<(usize, usize)> {
    while nuclei.len() < target {
        let mut best_idx = 0;
        let mut best_width = 0;
        for (i, n) in nuclei.iter().enumerate() {
            let width = n.1 - n.0;
            if width > best_width {
                best_width = width;
                best_idx = i;
            }
        }
        if best_width <= 1 {
            break;
        }
        let (s, e) = nuclei[best_idx];
        let mid = s + (e - s) / 2;
        nuclei[best_idx] = (s, mid);
        nuclei.insert(best_idx + 1, (mid, e));
    }
    nuclei
}

fn build_syllables(word: &str, nuclei: &[(usize, usize)]) -> Vec<String> {
    if nuclei.len() <= 1 {
        return vec![word.to_string()];
    }
    let chars: Vec<char> = word.chars().collect();
    let mut boundaries = vec![0usize; nuclei.len() + 1];
    boundaries[0] = 0;
    *boundaries.last_mut().unwrap() = chars.len();

    for i in 0..nuclei.len() - 1 {
        let gap_start = nuclei[i].1;
        let gap_end = nuclei[i + 1].0;
        let gap_len = gap_end - gap_start;

        let split_point = if gap_len <= 1 {
            gap_start
        } else {
            gap_start + 1
        };
        boundaries[i + 1] = split_point;
    }

    (0..nuclei.len())
        .map(|i| chars[boundaries[i]..boundaries[i + 1]].iter().collect())
        .collect()
}
