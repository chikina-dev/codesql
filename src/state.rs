use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::constants::CONFIG_TEMPLATE;
use crate::paths::CodesqlPaths;

pub fn initialize_layout(paths: &CodesqlPaths, analyzer_manifest: &str) -> Result<()> {
    ensure_managed_path_is_not_symlink(&paths.app_dir(), ".codesql directory")?;
    ensure_managed_path_is_not_symlink(&paths.state_dir(), "state directory")?;
    ensure_managed_path_is_not_symlink(&paths.segments_dir(), "segments directory")?;
    ensure_managed_path_is_not_symlink(&paths.tmp_dir(), "tmp directory")?;
    ensure_managed_path_is_not_symlink(&paths.analyzers_dir(), "analyzers directory")?;
    ensure_managed_path_is_not_symlink(&paths.config_path(), "config.toml")?;
    ensure_managed_path_is_not_symlink(&paths.current_generation_path(), "current_generation")?;
    ensure_managed_path_is_not_symlink(&paths.save_state_path(), "save_state.json")?;
    ensure_managed_path_is_not_symlink(&paths.analyzer_manifest_path(), "manifest.json")?;
    fs::create_dir_all(paths.app_dir()).context("failed to create .codesql directory")?;
    fs::create_dir_all(paths.state_dir()).context("failed to create state directory")?;
    fs::create_dir_all(paths.segments_dir()).context("failed to create segments directory")?;
    fs::create_dir_all(paths.tmp_dir()).context("failed to create tmp directory")?;
    fs::create_dir_all(paths.analyzers_dir()).context("failed to create analyzers directory")?;
    fs::write(paths.config_path(), CONFIG_TEMPLATE).context("failed to write config.toml")?;
    fs::write(paths.gitignore_path(), "*\n").context("failed to write .gitignore")?;
    fs::write(paths.current_generation_path(), "0").context("failed to write generation state")?;
    fs::write(
        paths.save_state_path(),
        serde_json::json!({ "current_generation": 0 }).to_string(),
    )
    .context("failed to write save state")?;
    fs::write(paths.analyzer_manifest_path(), analyzer_manifest)
        .context("failed to write analyzer manifest")?;
    ensure_runtime_directories(paths)?;
    Ok(())
}

pub fn ensure_initialized(paths: &CodesqlPaths) -> Result<()> {
    ensure_runtime_directories(paths)?;
    Ok(())
}

fn ensure_runtime_directories(paths: &CodesqlPaths) -> Result<()> {
    ensure_managed_directory(&paths.app_dir(), ".codesql directory")?;
    ensure_managed_directory(&paths.state_dir(), "state directory")?;
    ensure_managed_directory(&paths.segments_dir(), "segments directory")?;
    ensure_managed_directory(&paths.tmp_dir(), "tmp directory")?;
    ensure_managed_file(&paths.current_generation_path(), "current_generation")?;
    ensure_managed_file(&paths.save_state_path(), "save_state.json")?;
    Ok(())
}

fn ensure_managed_path_is_not_symlink(path: &Path, label: &str) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!("codesql {label} must not be a symlink: {}", path.display());
    }
    Ok(())
}

fn ensure_managed_directory(path: &Path, label: &str) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("codesql is not initialized; run `codesql init` first");
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect codesql {label} {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!("codesql {label} must not be a symlink: {}", path.display());
    }
    if !metadata.is_dir() {
        anyhow::bail!("codesql {label} must be a directory: {}", path.display());
    }
    Ok(())
}

fn ensure_managed_file(path: &Path, label: &str) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("codesql is not initialized; run `codesql init` first");
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect codesql {label} {}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!("codesql {label} must not be a symlink: {}", path.display());
    }
    if !metadata.is_file() {
        anyhow::bail!("codesql {label} must be a file: {}", path.display());
    }
    Ok(())
}

pub fn current_generation(paths: &CodesqlPaths) -> Result<u64> {
    ensure_managed_file(&paths.current_generation_path(), "current_generation")?;
    let contents = fs::read_to_string(paths.current_generation_path())
        .context("failed to read current generation")?;
    contents
        .trim()
        .parse::<u64>()
        .context("failed to parse current generation")
}

pub fn write_current_generation(paths: &CodesqlPaths, generation: u64) -> Result<()> {
    ensure_managed_file(&paths.current_generation_path(), "current_generation")?;
    fs::write(paths.current_generation_path(), generation.to_string())
        .context("failed to update current generation")
}

pub fn write_save_state(paths: &CodesqlPaths, generation: u64) -> Result<()> {
    ensure_managed_file(&paths.save_state_path(), "save_state.json")?;
    fs::write(
        paths.save_state_path(),
        serde_json::json!({ "current_generation": generation }).to_string(),
    )
    .context("failed to update save state")
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        CodesqlPaths, current_generation, ensure_initialized, initialize_layout,
        write_current_generation, write_save_state,
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn reads_and_writes_generation_state_files() {
        let root = unique_test_root();
        let paths = CodesqlPaths::new(root.clone());
        initialize_layout(&paths, "[]").expect("layout should initialize");

        assert_eq!(
            current_generation(&paths).expect("generation should be readable"),
            0
        );

        write_current_generation(&paths, 3).expect("generation should update");
        write_save_state(&paths, 3).expect("save state should update");

        assert_eq!(
            current_generation(&paths).expect("generation should be readable"),
            3
        );
        let save_state = fs::read_to_string(paths.save_state_path())
            .expect("save_state.json should be readable");
        assert!(save_state.contains("\"current_generation\":3"));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_initialized_rejects_symlinked_runtime_directories() {
        let root = unique_test_root();
        let paths = CodesqlPaths::new(root.clone());
        initialize_layout(&paths, "[]").expect("layout should initialize");
        let external = root.join("external");
        fs::create_dir_all(&external).expect("external dir should be created");

        for relative_path in [".codesql/tmp", ".codesql/segments"] {
            let managed_dir = root.join(relative_path);
            fs::remove_dir(&managed_dir).expect("managed directory should be removed");
            symlink(&external, &managed_dir).expect("managed directory symlink should be created");

            let error =
                ensure_initialized(&paths).expect_err("symlinked runtime directory must fail");

            assert!(
                error.to_string().contains("must not be a symlink"),
                "error was: {error:#}"
            );

            fs::remove_file(&managed_dir).expect("symlink should be removed");
            fs::create_dir(&managed_dir).expect("managed directory should be restored");
        }

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn initialize_layout_rejects_symlinked_managed_paths() {
        assert_initialize_layout_rejects_symlink(".codesql");
        assert_initialize_layout_rejects_symlink(".codesql/tmp");
        assert_initialize_layout_rejects_symlink(".codesql/config.toml");
        assert_initialize_layout_rejects_symlink(".codesql/state/current_generation");
        assert_initialize_layout_rejects_symlink(".codesql/state/save_state.json");
        assert_initialize_layout_rejects_symlink(".codesql/analyzers/manifest.json");
    }

    #[cfg(unix)]
    fn assert_initialize_layout_rejects_symlink(relative_path: &str) {
        let root = unique_test_root();
        let paths = CodesqlPaths::new(root.clone());
        let external = root.join("external");
        fs::create_dir_all(&root).expect("test root should be created");
        if relative_path.ends_with(".toml")
            || relative_path.ends_with(".json")
            || relative_path.ends_with("current_generation")
        {
            fs::write(&external, "external\n").expect("external file should be created");
        } else {
            fs::create_dir_all(&external).expect("external dir should be created");
        }

        if relative_path != ".codesql" {
            fs::create_dir_all(root.join(".codesql"))
                .expect(".codesql parent directory should be created");
        }

        if let Some(parent) = root.join(relative_path).parent() {
            fs::create_dir_all(parent).expect("managed path parent should be created");
        }

        symlink(&external, root.join(relative_path))
            .expect("managed path symlink should be created");

        let error = initialize_layout(&paths, "[]").expect_err("symlinked managed path must fail");

        assert!(
            error.to_string().contains("must not be a symlink"),
            "error was: {error:#}"
        );

        let _ = fs::remove_dir_all(root);
    }

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after unix epoch")
            .as_nanos();
        let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("codesql-state-tests-{timestamp}-{counter}"))
    }
}
