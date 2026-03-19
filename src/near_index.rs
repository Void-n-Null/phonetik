use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;

use crate::coda_groups::CodaGroupMap;
use crate::dict::CmuDict;
use crate::phoneme;

const MAX_CODA_LEN: usize = 3;

static NEAR_NEIGHBORS_BLOB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/near_neighbors.bin"));

pub struct NearIndex {
    coda_groups: Arc<CodaGroupMap>,
    coda_neighbors: HashMap<Vec<u8>, Vec<Vec<u8>>>,
    dict: Arc<CmuDict>,
}

impl NearIndex {
    pub fn new(dict: Arc<CmuDict>, coda_groups: Arc<CodaGroupMap>) -> Self {
        let coda_neighbors = Self::load_neighbors_blob();
        Self {
            coda_groups,
            coda_neighbors,
            dict,
        }
    }

    pub fn lookup(&self, word: &str, limit: usize) -> Option<NearRhymeResult> {
        let normalized = CmuDict::normalize(word);
        let variants = self.dict.lookup(word)?;
        let collect_limit = limit.saturating_mul(4).max(200);

        struct Candidate {
            word: String,
            match_coda: Vec<u8>,
        }

        let mut best_candidates: Vec<Candidate> = Vec::new();
        let mut best_nucleus: Option<u8> = None;
        let mut best_coda: Option<Vec<u8>> = None;

        for phonemes in &variants {
            if let Some((nucleus, coda)) = crate::coda_groups::extract_nucleus_coda(phonemes) {
                if coda.len() > MAX_CODA_LEN {
                    continue;
                }
                let mut candidates: Vec<Candidate> = Vec::new();
                let mut seen = HashSet::new();

                if let Some(neighbors) = self.coda_neighbors.get(&coda) {
                    for neighbor_coda in neighbors {
                        if let Some(nuclei) = self.coda_groups.get(neighbor_coda) {
                            if let Some(words) = nuclei.get(&nucleus) {
                                for w in words {
                                    let ws: &str = w;
                                    if ws == normalized || seen.contains(ws) {
                                        continue;
                                    }
                                    if candidates.len() >= collect_limit {
                                        break;
                                    }
                                    seen.insert(ws.to_string());
                                    candidates.push(Candidate {
                                        word: ws.to_string(),
                                        match_coda: neighbor_coda.clone(),
                                    });
                                }
                            }
                        }
                    }
                }

                if coda.is_empty() {
                    for (other_coda, nuclei) in self.coda_groups.as_ref() {
                        if other_coda.len() == 1 {
                            if let Some(words) = nuclei.get(&nucleus) {
                                for w in words {
                                    let ws: &str = w;
                                    if ws == normalized || seen.contains(ws) {
                                        continue;
                                    }
                                    if candidates.len() >= collect_limit {
                                        break;
                                    }
                                    seen.insert(ws.to_string());
                                    candidates.push(Candidate {
                                        word: ws.to_string(),
                                        match_coda: other_coda.clone(),
                                    });
                                }
                            }
                        }
                    }
                } else if coda.len() == 1 {
                    let empty: Vec<u8> = vec![];
                    if let Some(nuclei) = self.coda_groups.get(&empty) {
                        if let Some(words) = nuclei.get(&nucleus) {
                            for w in words {
                                let ws: &str = w;
                                if ws == normalized || seen.contains(ws) {
                                    continue;
                                }
                                if candidates.len() >= collect_limit {
                                    break;
                                }
                                seen.insert(ws.to_string());
                                candidates.push(Candidate {
                                    word: ws.to_string(),
                                    match_coda: empty.clone(),
                                });
                            }
                        }
                    }
                }

                if candidates.len() > best_candidates.len() {
                    best_candidates = candidates;
                    best_nucleus = Some(nucleus);
                    best_coda = Some(coda);
                }
            }
        }

        if best_candidates.is_empty() {
            return None;
        }
        let coda = best_coda.unwrap();
        let query_coda_len = coda.len();

        best_candidates.sort_by(|a, b| {
            a.match_coda
                .len()
                .abs_diff(query_coda_len)
                .cmp(&b.match_coda.len().abs_diff(query_coda_len))
                .then_with(|| a.word.cmp(&b.word))
        });
        if best_candidates.len() > limit {
            best_candidates.truncate(limit);
        }

        let matches: Vec<NearRhymeMatch> = best_candidates
            .into_iter()
            .map(|c| {
                let lookup = self.dict.lookup(&c.word);
                let ph = lookup.as_ref().map(|l| l[0].clone()).unwrap_or_default();
                NearRhymeMatch {
                    word: c.word,
                    phonemes: phoneme::decode_to_strings(&ph),
                    syllables: phoneme::count_syllables(&ph),
                    coda: phoneme::decode_to_strings(&c.match_coda),
                    coda_len: c.match_coda.len(),
                }
            })
            .collect();

        let phonemes_encoded = &variants[0];
        let neighbor_codas = self.coda_neighbors.get(&coda).map(|n| n.len()).unwrap_or(0);
        Some(NearRhymeResult {
            word: normalized,
            phonemes: phoneme::decode_to_strings(phonemes_encoded),
            syllables: phoneme::count_syllables(phonemes_encoded),
            nucleus: phoneme::decode(best_nucleus.unwrap()).to_string(),
            coda: phoneme::decode_to_strings(&coda),
            neighbor_codas,
            matches,
        })
    }

    pub fn neighbor_count(&self) -> usize {
        self.coda_neighbors.len()
    }

    fn load_neighbors_blob() -> HashMap<Vec<u8>, Vec<Vec<u8>>> {
        let blob = NEAR_NEIGHBORS_BLOB;
        if blob.len() < 4 {
            return HashMap::new();
        }
        let mut pos = 0;
        let count = u32::from_le_bytes([blob[0], blob[1], blob[2], blob[3]]) as usize;
        pos += 4;
        let mut neighbors = HashMap::with_capacity(count);
        for _ in 0..count {
            if pos >= blob.len() {
                break;
            }
            let clen = blob[pos] as usize;
            pos += 1;
            if pos + clen > blob.len() {
                break;
            }
            let coda = blob[pos..pos + clen].to_vec();
            pos += clen;
            if pos + 2 > blob.len() {
                break;
            }
            let ncount = u16::from_le_bytes([blob[pos], blob[pos + 1]]) as usize;
            pos += 2;
            let mut nlist = Vec::with_capacity(ncount);
            for _ in 0..ncount {
                if pos >= blob.len() {
                    break;
                }
                let nl = blob[pos] as usize;
                pos += 1;
                if pos + nl > blob.len() {
                    break;
                }
                nlist.push(blob[pos..pos + nl].to_vec());
                pos += nl;
            }
            neighbors.insert(coda, nlist);
        }
        neighbors
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearRhymeResult {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
    pub nucleus: String,
    pub coda: Vec<String>,
    pub neighbor_codas: usize,
    pub matches: Vec<NearRhymeMatch>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearRhymeMatch {
    pub word: String,
    pub phonemes: Vec<String>,
    pub syllables: usize,
    pub coda: Vec<String>,
    #[serde(skip)]
    pub coda_len: usize,
}
