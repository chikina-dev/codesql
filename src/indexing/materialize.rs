use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::Result;
use rayon::prelude::*;

use crate::analyzer::{AnalysisResult, AnalyzerRegistry};
use crate::catalog_types::{FileRecord, IndexedFile};
use crate::constants::{ANALYZER_BINARY, LANGUAGE_BINARY, MAX_INDEXED_FILE_SIZE_BYTES};

use super::scan::ScannedFileMetadata;

#[derive(Debug, Clone)]
pub(crate) struct MaterializedFile {
    pub(crate) indexed_file: IndexedFile,
    pub(crate) indexed_content: Option<String>,
}

pub(crate) fn materialize_changed_files(
    root: &Path,
    scanned_files: &[ScannedFileMetadata],
    changed_paths: &[String],
    registry: &AnalyzerRegistry,
) -> Result<Vec<MaterializedFile>> {
    let changed_paths = changed_paths
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    let mut materialized_files: Vec<MaterializedFile> = scanned_files
        .par_iter()
        .filter_map(|scanned_file| {
            if !changed_paths.contains(scanned_file.path.as_str()) {
                return None;
            }

            let path = root.join(&scanned_file.path);
            let bytes = match fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("failed to read file {}: {}", path.display(), e);
                    return None; // Skip unreadable
                }
            };
            
            let is_text = is_text_file(&bytes) && scanned_file.size <= MAX_INDEXED_FILE_SIZE_BYTES;
            let indexed_content = if is_text {
                String::from_utf8(bytes).ok() // If it fails to parse as UTF-8, it skips or stores None
                    .or_else(|| Some(String::new())) // fallback to empty
            } else {
                None
            };
            
            let analysis = if is_text {
                registry.analyze(Path::new(&scanned_file.path), indexed_content.as_deref())
            } else {
                AnalysisResult {
                    analyzer_name: ANALYZER_BINARY,
                    language: LANGUAGE_BINARY,
                    symbols: Vec::new(),
                }
            };

            Some(MaterializedFile {
                indexed_file: IndexedFile {
                    path: scanned_file.path.clone(),
                    size: scanned_file.size,
                    mtime_ns: scanned_file.mtime_ns,
                    is_text: is_text && indexed_content.as_ref().map(|s| !s.is_empty()).unwrap_or(false),
                    ext: scanned_file.ext.clone(),
                    language: analysis.language.to_owned(),
                    analyzer_name: analysis.analyzer_name.to_owned(),
                    symbols: analysis.symbols,
                },
                indexed_content,
            })
        })
        .collect();

    materialized_files.sort_by(|left, right| left.indexed_file.path.cmp(&right.indexed_file.path));
    Ok(materialized_files)
}

pub(crate) fn collect_snapshot_files(
    existing_files: &HashMap<String, FileRecord>,
    scanned_files: &[ScannedFileMetadata],
    materialized_files: &[MaterializedFile],
) -> Result<Vec<IndexedFile>> {
    let materialized_files = materialized_files
        .iter()
        .map(|file| (file.indexed_file.path.as_str(), &file.indexed_file))
        .collect::<HashMap<_, _>>();
    let mut indexed_files = Vec::with_capacity(scanned_files.len());

    for scanned_file in scanned_files {
        if let Some(materialized_file) = materialized_files.get(scanned_file.path.as_str()) {
            indexed_files.push((*materialized_file).clone());
            continue;
        }

        let Some(existing_file) = existing_files.get(&scanned_file.path) else {
            anyhow::bail!(
                "missing materialized metadata for changed file {}",
                scanned_file.path
            );
        };
        indexed_files.push(IndexedFile {
            path: scanned_file.path.clone(),
            size: scanned_file.size,
            mtime_ns: scanned_file.mtime_ns,
            is_text: existing_file.is_text,
            ext: scanned_file.ext.clone(),
            language: existing_file.language.clone(),
            analyzer_name: existing_file.analyzer_name.clone(),
            symbols: Vec::new(), // If unchanged, symbols are already in DB
        });
    }

    Ok(indexed_files)
}

fn is_text_file(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return false;
    }
    if std::str::from_utf8(bytes).is_err() {
        return false;
    }
    if bytes.is_empty() {
        return true;
    }

    let sample = &bytes[..bytes.len().min(4096)];
    let printable = sample
        .iter()
        .filter(|byte| matches!(byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e))
        .count();
    printable * 100 >= sample.len() * 85
}

#[cfg(test)]
mod tests {
    use super::is_text_file;

    #[test]
    fn text_detection_rejects_null_bytes() {
        assert!(!is_text_file(b"abc\0def"));
        assert!(is_text_file(b"hello\nworld"));
    }
}
