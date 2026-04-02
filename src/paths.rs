use std::path::{Path, PathBuf};

use crate::constants::{
    ANALYZER_MANIFEST_FILE, ANALYZERS_DIR, APP_DIR, CATALOG_DB_FILE, CONFIG_FILE,
    CURRENT_GENERATION_FILE, SAVE_STATE_FILE, SEGMENTS_DIR, STATE_DIR, TMP_DIR,
};

#[derive(Debug, Clone)]
pub struct CodesqlPaths {
    root: PathBuf,
}

impl CodesqlPaths {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn app_dir(&self) -> PathBuf {
        self.root.join(APP_DIR)
    }

    pub fn config_path(&self) -> PathBuf {
        self.app_dir().join(CONFIG_FILE)
    }

    pub fn gitignore_path(&self) -> PathBuf {
        self.app_dir().join(".gitignore")
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.app_dir().join(CATALOG_DB_FILE)
    }

    pub fn state_dir(&self) -> PathBuf {
        self.app_dir().join(STATE_DIR)
    }

    pub fn current_generation_path(&self) -> PathBuf {
        self.state_dir().join(CURRENT_GENERATION_FILE)
    }

    pub fn save_state_path(&self) -> PathBuf {
        self.state_dir().join(SAVE_STATE_FILE)
    }

    pub fn segments_dir(&self) -> PathBuf {
        self.app_dir().join(SEGMENTS_DIR)
    }

    pub fn segment_path(&self, generation: u64) -> PathBuf {
        self.segments_dir().join(format!("{generation:06}.seg"))
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.app_dir().join(TMP_DIR)
    }

    pub fn analyzers_dir(&self) -> PathBuf {
        self.app_dir().join(ANALYZERS_DIR)
    }

    pub fn analyzer_manifest_path(&self) -> PathBuf {
        self.analyzers_dir().join(ANALYZER_MANIFEST_FILE)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::CodesqlPaths;

    #[test]
    fn builds_codesql_paths_under_workspace_root() {
        let paths = CodesqlPaths::new(PathBuf::from("/tmp/workspace"));

        assert_eq!(paths.app_dir(), PathBuf::from("/tmp/workspace/.codesql"));
        assert_eq!(
            paths.segment_path(7),
            PathBuf::from("/tmp/workspace/.codesql/segments/000007.seg")
        );
        assert_eq!(
            paths.save_state_path(),
            PathBuf::from("/tmp/workspace/.codesql/state/save_state.json")
        );
    }
}
