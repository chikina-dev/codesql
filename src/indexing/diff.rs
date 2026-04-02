use std::collections::{HashMap, HashSet};

use crate::catalog_types::FileRecord;

use super::scan::ScannedFileMetadata;

#[derive(Debug, Clone)]
pub(crate) struct DiffSummary {
    pub(crate) changed_paths: Vec<String>,
    pub(crate) deleted_paths: Vec<String>,
}

impl DiffSummary {
    pub(crate) fn is_empty(&self) -> bool {
        self.changed_paths.is_empty() && self.deleted_paths.is_empty()
    }
}

pub(crate) fn diff_files(
    existing_files: &HashMap<String, FileRecord>,
    scanned_files: &[ScannedFileMetadata],
) -> DiffSummary {
    let current_paths = scanned_files
        .iter()
        .map(|file| file.path.clone())
        .collect::<HashSet<_>>();
    let mut changed_paths = Vec::new();
    let mut deleted_paths = Vec::new();

    for scanned_file in scanned_files {
        match existing_files.get(&scanned_file.path) {
            Some(existing)
                if existing.size == scanned_file.size
                    && existing.mtime_ns == scanned_file.mtime_ns
                    && existing.ext == scanned_file.ext => {}
            _ => changed_paths.push(scanned_file.path.clone()),
        }
    }

    for existing_path in existing_files.keys() {
        if !current_paths.contains(existing_path) {
            deleted_paths.push(existing_path.clone());
        }
    }

    changed_paths.sort();
    changed_paths.dedup();
    deleted_paths.sort();
    deleted_paths.dedup();

    DiffSummary {
        changed_paths,
        deleted_paths,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::catalog_types::FileRecord;
    use crate::constants::{LANGUAGE_RUST, LANGUAGE_TYPESCRIPT};

    use super::{DiffSummary, diff_files};
    use crate::indexing::scan::ScannedFileMetadata;

    #[test]
    fn diff_detects_metadata_changes_and_deletes() {
        let mut existing = HashMap::new();
        existing.insert(
            "src/lib.rs".to_owned(),
            FileRecord {
                file_id: 1,
                path: "src/lib.rs".to_owned(),
                size: 10,
                mtime_ns: 10,
                is_text: true,
                ext: "rs".to_owned(),
                language: LANGUAGE_RUST.to_owned(),
                analyzer_name: "Rust".to_owned(),
            },
        );
        existing.insert(
            "web/app.ts".to_owned(),
            FileRecord {
                file_id: 2,
                path: "web/app.ts".to_owned(),
                size: 20,
                mtime_ns: 20,
                is_text: true,
                ext: "ts".to_owned(),
                language: LANGUAGE_TYPESCRIPT.to_owned(),
                analyzer_name: "TypeScript/JavaScript".to_owned(),
            },
        );

        let scanned = vec![ScannedFileMetadata {
            path: "src/lib.rs".to_owned(),
            size: 11,
            mtime_ns: 11,
            ext: "rs".to_owned(),
        }];

        let diff = diff_files(&existing, &scanned);
        assert_eq!(diff.changed_paths, vec!["src/lib.rs"]);
        assert_eq!(diff.deleted_paths, vec!["web/app.ts"]);
        assert!(
            !DiffSummary {
                changed_paths: diff.changed_paths,
                deleted_paths: diff.deleted_paths,
            }
            .is_empty()
        );
    }
}
