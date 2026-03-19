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

#[cfg(test)]
mod tests {
    use super::*;

    fn word_stress(word: &str, stresses: &[i32]) -> WordStress {
        WordStress {
            word: word.to_string(),
            normalized: word.to_uppercase(),
            phonemes: vec![],
            stresses: stresses.to_vec(),
            in_dictionary: true,
            display: String::new(),
        }
    }

    #[test]
    fn split_empty_and_single_syllable_words() {
        assert_eq!(SyllableSplitter::split("", 1), vec![""]);
        assert_eq!(SyllableSplitter::split("cat", 1), vec!["cat"]);
        assert_eq!(SyllableSplitter::split("cat", 0), vec!["cat"]);
    }

    #[test]
    fn split_basic_multi_syllable_words() {
        assert_eq!(SyllableSplitter::split("hello", 2), vec!["hel", "lo"]);
        assert_eq!(SyllableSplitter::split("summer", 2), vec!["sum", "mer"]);
    }

    #[test]
    fn stress_display_uses_case_and_hyphens() {
        assert_eq!(SyllableSplitter::stress_display("hello", &[0, 1]), "hel-LO");
        assert_eq!(
            SyllableSplitter::stress_display("summer", &[1, 0]),
            "SUM-mer"
        );
    }

    #[test]
    fn format_line_joins_words() {
        let words = vec![
            word_stress("hello", &[0, 1]),
            word_stress("summer", &[1, 0]),
        ];
        assert_eq!(SyllableSplitter::format_line(&words), "hel-LO SUM-mer");
    }

    #[test]
    fn vowel_detection_is_case_insensitive() {
        assert!(is_vowel('a'));
        assert!(is_vowel('A'));
        assert!(is_vowel('y'));
        assert!(!is_vowel('b'));
    }

    #[test]
    fn find_nuclei_handles_basic_words_and_silent_e() {
        assert_eq!(find_nuclei("cat"), vec![(1, 2)]);
        assert_eq!(find_nuclei("make"), vec![(1, 2)]);
        assert_eq!(find_nuclei("hello"), vec![(1, 2), (4, 5)]);
    }

    #[test]
    fn find_nuclei_falls_back_for_words_without_vowels() {
        assert_eq!(find_nuclei("rhythms"), vec![(2, 3)]);
        assert_eq!(find_nuclei("brrr"), vec![(0, 4)]);
    }

    #[test]
    fn merge_nuclei_reduces_to_target() {
        let nuclei = vec![(0, 1), (2, 3), (5, 6)];
        assert_eq!(merge_nuclei(nuclei, 2), vec![(0, 3), (5, 6)]);
    }

    #[test]
    fn split_nuclei_expands_wide_nucleus() {
        let nuclei = vec![(0, 4)];
        assert_eq!(split_nuclei(nuclei, 2), vec![(0, 2), (2, 4)]);
    }

    #[test]
    fn split_nuclei_stops_when_nuclei_too_narrow() {
        let nuclei = vec![(0, 1)];
        assert_eq!(split_nuclei(nuclei.clone(), 2), nuclei);
    }

    #[test]
    fn build_syllables_uses_gap_boundaries() {
        let nuclei = vec![(1, 2), (4, 5)];
        assert_eq!(build_syllables("hello", &nuclei), vec!["hel", "lo"]);

        let wider_gap = vec![(1, 2), (5, 6)];
        assert_eq!(build_syllables("camera", &wider_gap), vec!["cam", "era"]);
    }
}
