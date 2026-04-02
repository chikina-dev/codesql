use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use ignore::WalkBuilder;

use crate::constants::is_internal_path;

#[derive(Debug, Clone)]
pub(crate) struct ScannedFileMetadata {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) mtime_ns: i64,
    pub(crate) ext: String,
}

pub(crate) fn scan_file_metadata(root: &Path) -> Result<Vec<ScannedFileMetadata>> {
    let mut scanned_files = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(true)
        .build();

    for entry in walker {
        let entry = entry?;
        if !entry
            .file_type()
            .map(|value| value.is_file())
            .unwrap_or(false)
        {
            continue;
        }

        let path = entry.path();
        let relative_path = relative_path(root, path)?;
        if is_internal_path(&relative_path) {
            continue;
        }

        let metadata = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?;
        let mtime_ns = modified_time_ns(&metadata)?;
        scanned_files.push(ScannedFileMetadata {
            path: relative_path.clone(),
            size: metadata.len(),
            mtime_ns,
            ext: extension(&relative_path),
        });
    }

    scanned_files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(scanned_files)
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .with_context(|| format!("{} was not under {}", path.display(), root.display()))?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn extension(relative_path: &str) -> String {
    Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default()
}

fn modified_time_ns(metadata: &fs::Metadata) -> Result<i64> {
    let modified = metadata.modified().context("failed to read file mtime")?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .context("file mtime was before unix epoch")?;
    Ok(duration.as_nanos() as i64)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::constants::is_internal_path;

    use super::{extension, relative_path};

    #[test]
    fn extension_is_lowercased() {
        assert_eq!(extension("src/LIB.RS"), "rs");
    }

    #[test]
    fn internal_paths_are_excluded() {
        assert!(is_internal_path(".codesql/catalog.db"));
        assert!(is_internal_path(".git/index"));
        assert!(!is_internal_path("src/lib.rs"));
    }

    #[test]
    fn relative_path_normalizes_separators() {
        let result = relative_path(
            Path::new("/tmp/project"),
            Path::new("/tmp/project/src/lib.rs"),
        )
        .expect("relative path should resolve");
        assert_eq!(result, "src/lib.rs");
    }
}
