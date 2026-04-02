use std::collections::{BTreeMap, HashSet};
#[cfg(test)]
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SegmentData {
    pub generation: u64,
    pub postings: BTreeMap<[u8; 3], Vec<i64>>,
    #[serde(default)]
    pub tombstones: std::collections::HashSet<i64>,
}

impl SegmentData {
    #[cfg(test)]
    pub fn from_documents<'a, I>(generation: u64, documents: I) -> Self
    where
        I: IntoIterator<Item = (i64, &'a str)>,
    {
        let mut postings = BTreeMap::<[u8; 3], BTreeSet<i64>>::new();
        for (file_id, content) in documents {
            for trigram in extract_trigrams(content) {
                postings.entry(trigram).or_default().insert(file_id);
            }
        }

        let postings = postings
            .into_iter()
            .map(|(trigram, file_ids)| (trigram, file_ids.into_iter().collect()))
            .collect();

        Self {
            generation,
            postings,
            tombstones: std::collections::HashSet::new(),
        }
    }

    pub fn read_from_path(path: &Path) -> Result<Self> {
        let bytes =
            fs::read(path).with_context(|| format!("failed to read bincode segment {}", path.display()))?;
        bincode::deserialize(&bytes).context("failed to deserialize bincode segment")
    }

    pub fn trigram_count(&self) -> usize {
        self.postings.len()
    }
}pub fn extract_trigrams(text: &str) -> Vec<[u8; 3]> {
    let bytes = text.as_bytes();
    if bytes.len() < 3 {
        return Vec::new();
    }

    let mut trigrams = HashSet::new();
    for window in bytes.windows(3) {
        trigrams.insert([window[0], window[1], window[2]]);
    }
    
    let mut vec: Vec<[u8; 3]> = trigrams.into_iter().collect();
    vec.sort_unstable();
    vec
}

#[cfg(test)]
mod tests {
    use super::extract_trigrams;

    #[test]
    fn extract_trigrams_returns_unique_sorted_values() {
        assert_eq!(extract_trigrams("ababa"), vec![*b"aba", *b"bab"]);
    }
}
