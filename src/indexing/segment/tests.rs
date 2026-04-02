use std::collections::HashMap;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

use crate::catalog_types::IndexedFile;
use crate::constants::LANGUAGE_RUST;
use crate::indexing::materialize::MaterializedFile;
#[cfg(unix)]
use crate::paths::CodesqlPaths;
use crate::segment::SegmentData;

use super::{append_materialized_documents, publish_segment};

#[cfg(unix)]
static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(1);


#[test]
fn append_materialized_documents_inserts_changed_text_content() {
    let mut segment = SegmentData::from_documents(1, [(1, "public")]);
    let materialized_files = vec![MaterializedFile {
        indexed_file: IndexedFile {
            path: "src/lib.rs".to_owned(),
            size: 10,
            mtime_ns: 10,
            is_text: true,
            ext: "rs".to_owned(),
            language: LANGUAGE_RUST.to_owned(),
            analyzer_name: "Rust".to_owned(),
            symbols: Vec::new(),
        },
        indexed_content: Some("unsafe".to_owned()),
    }];
    let file_ids = HashMap::from([("src/lib.rs".to_owned(), 1)]);

    append_materialized_documents(&mut segment, &materialized_files, &file_ids)
        .expect("materialized documents should be appended");

    assert!(segment.postings.contains_key(b"uns"));
}

#[cfg(unix)]
#[test]
fn publish_segment_writes_tmp_file_before_renaming_to_segments_dir() {
    let workspace = SegmentWorkspace::new();
    let paths = CodesqlPaths::new(workspace.root.clone());
    let segment = SegmentData::from_documents(1, [(1, "unsafe")]);
    workspace.set_mode(Path::new(".codesql/tmp"), 0o555);

    let error =
        publish_segment(&paths, 1, &segment).expect_err("tmp write should fail before rename");

    workspace.set_mode(Path::new(".codesql/tmp"), 0o755);

    let error_text = error.to_string();
    assert!(
        error_text.contains("failed to create temporary segment file"),
        "error was: {error_text}"
    );
    assert!(
        !paths.segment_path(1).exists(),
        "segment should not be written directly to the final path"
    );
}

#[cfg(unix)]
struct SegmentWorkspace {
    root: PathBuf,
}

#[cfg(unix)]
impl SegmentWorkspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(unique_workspace_name());
        fs::create_dir_all(root.join(".codesql/tmp")).expect("tmp dir should be created");
        fs::create_dir_all(root.join(".codesql/segments")).expect("segments dir should be created");
        Self { root }
    }

    fn set_mode(&self, relative_path: &Path, mode: u32) {
        let permissions = fs::Permissions::from_mode(mode);
        fs::set_permissions(self.root.join(relative_path), permissions)
            .expect("permissions should be updated");
    }
}

#[cfg(unix)]
impl Drop for SegmentWorkspace {
    fn drop(&mut self) {
        let _ = fs::set_permissions(
            self.root.join(".codesql/tmp"),
            fs::Permissions::from_mode(0o755),
        );
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
fn unique_workspace_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let counter = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("codesql-segment-tests-{timestamp}-{counter}")
}
