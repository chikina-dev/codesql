use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

use crate::catalog::Catalog;
use crate::catalog_types::FileRecord;
use crate::paths::CodesqlPaths;
use crate::segment::{SegmentData, extract_trigrams};

use super::diff::DiffSummary;
use super::materialize::MaterializedFile;

pub(crate) fn build_segment(
    generation: u64,
    _previous_generation: u64,
    _catalog: &Catalog,
    existing_files: &HashMap<String, FileRecord>,
    diff: &DiffSummary,
    materialized_files: &[MaterializedFile],
    file_ids: &HashMap<String, i64>,
) -> Result<SegmentData> {
    let mut segment = SegmentData {
        generation,
        postings: BTreeMap::new(),
        tombstones: HashSet::new(),
    };

    for path in diff.changed_paths.iter().chain(diff.deleted_paths.iter()) {
        if let Some(existing_file) = existing_files.get(path) {
            segment.tombstones.insert(existing_file.file_id);
        }
    }

    append_materialized_documents(&mut segment, materialized_files, file_ids)?;
    Ok(segment)
}

pub(crate) fn publish_segment(
    paths: &CodesqlPaths,
    generation: u64,
    segment: &SegmentData,
) -> Result<PathBuf> {
    let segment_path = paths.segment_path(generation);
    let mut tmp_file = NamedTempFile::new_in(paths.tmp_dir())
        .context("failed to create temporary segment file")?;
    let tmp_path = tmp_file.path().to_path_buf();
    let contents = bincode::serialize(segment).context("failed to serialize segment to bincode")?;
    tmp_file
        .write_all(&contents)
        .context("failed to write temporary segment file")?;
    tmp_file
        .flush()
        .context("failed to flush temporary segment file")?;
    tmp_file
        .persist(&segment_path)
        .map_err(|error| error.error)
        .with_context(|| {
            format!(
                "failed to publish segment {} -> {}",
                tmp_path.display(),
                segment_path.display()
            )
        })?;
    Ok(segment_path)
}

pub(crate) fn prefiltered_ids(
    catalog: &Catalog,
    generation: u64,
    terms: &[String],
) -> Result<Option<HashSet<i64>>> {
    if terms.is_empty() || terms.iter().any(|term| term.len() < 3) {
        return Ok(None);
    }

    let segment_paths = catalog.active_segment_paths(generation)?;
    if segment_paths.is_empty() {
        return Ok(None);
    }

    let mut all_trigrams = HashSet::new();
    for term in terms {
        all_trigrams.extend(extract_trigrams(term));
    }
    if all_trigrams.is_empty() {
        return Ok(None);
    }

    let mut trigram_candidates: HashMap<[u8; 3], HashSet<i64>> = HashMap::new();
    for &t in &all_trigrams {
        trigram_candidates.insert(t, HashSet::new());
    }

    for path in segment_paths {
        let segment = SegmentData::read_from_path(&path)?;

        for &t in &all_trigrams {
            if let Some(candidates) = trigram_candidates.get_mut(&t) {
                if !segment.tombstones.is_empty() {
                    candidates.retain(|id| !segment.tombstones.contains(id));
                }
                if let Some(postings) = segment.postings.get(&t) {
                    candidates.extend(postings.iter().copied());
                }
            }
        }
    }

    let mut intersected_term_ids: Option<HashSet<i64>> = None;
    for term in terms {
        let trigrams = extract_trigrams(term);
        let mut term_candidate_ids: Option<HashSet<i64>> = None;
        for t in trigrams {
            let ids = trigram_candidates.get(&t).unwrap();
            match term_candidate_ids {
                None => term_candidate_ids = Some(ids.clone()),
                Some(mut existing) => {
                    existing.retain(|id| ids.contains(id));
                    term_candidate_ids = Some(existing);
                }
            }
        }
        
        let ids = term_candidate_ids.unwrap_or_default();
        match intersected_term_ids {
            None => {
                intersected_term_ids = Some(ids);
            }
            Some(mut existing) => {
                existing.retain(|id| ids.contains(id));
                intersected_term_ids = Some(existing);
            }
        }
        if let Some(ids) = &intersected_term_ids {
            if ids.is_empty() {
                break;
            }
        }
    }

    Ok(intersected_term_ids)
}



fn append_materialized_documents(
    segment: &mut SegmentData,
    materialized_files: &[MaterializedFile],
    file_ids: &HashMap<String, i64>,
) -> Result<()> {
    for materialized_file in materialized_files {
        if !materialized_file.indexed_file.is_text {
            continue;
        }

        let file_id = *file_ids
            .get(&materialized_file.indexed_file.path)
            .with_context(|| {
                format!(
                    "missing file id for {}",
                    materialized_file.indexed_file.path
                )
            })?;
        let content = materialized_file
            .indexed_content
            .as_deref()
            .context("indexed text file was missing content")?;

        for trigram in extract_trigrams(content) {
            let postings = segment.postings.entry(trigram).or_default();
            postings.push(file_id);
            postings.sort_unstable();
            postings.dedup();
        }
    }

    Ok(())
}
#[cfg(test)]
mod tests;
