#![allow(dead_code)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(1);

pub struct TestWorkspace {
    root: PathBuf,
}

impl TestWorkspace {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(unique_workspace_name());
        fs::create_dir_all(&path).expect("failed to create temporary workspace");
        Self { root: path }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn write_text_file(&self, relative_path: &str, contents: &str) {
        let path = self.root.join(relative_path);
        let parent = path.parent().expect("text file path must have a parent");
        fs::create_dir_all(parent).expect("failed to create parent directories");
        fs::write(path, contents).expect("failed to write text file");
    }

    pub fn write_binary_file(&self, relative_path: &str, contents: &[u8]) {
        let path = self.root.join(relative_path);
        let parent = path.parent().expect("binary file path must have a parent");
        fs::create_dir_all(parent).expect("failed to create parent directories");
        fs::write(path, contents).expect("failed to write binary file");
    }

    pub fn read_to_string(&self, relative_path: &str) -> String {
        fs::read_to_string(self.root.join(relative_path)).expect("failed to read file")
    }

    pub fn entries_in(&self, relative_path: &str) -> Vec<PathBuf> {
        let mut entries = fs::read_dir(self.root.join(relative_path))
            .expect("failed to read directory")
            .map(|entry| entry.expect("failed to read directory entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn run_codesql<I, S>(workspace: &TestWorkspace, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(env!("CARGO_BIN_EXE_codesql"))
        .args(args)
        .current_dir(workspace.path())
        .output()
        .expect("failed to run codesql binary")
}

pub fn run_git<I, S>(workspace: &TestWorkspace, args: I) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new("git")
        .args(args)
        .current_dir(workspace.path())
        .output()
        .expect("failed to run git")
}

pub fn init_codesql(workspace: &TestWorkspace) {
    let output = run_codesql(workspace, ["init"]);
    assert_success(&output);
}

pub fn save_codesql(workspace: &TestWorkspace) {
    let output = run_codesql(workspace, ["save"]);
    assert_success(&output);
}

pub fn current_generation(workspace: &TestWorkspace) -> u64 {
    workspace
        .read_to_string(".codesql/state/current_generation")
        .trim()
        .parse()
        .expect("failed to parse current generation")
}

pub fn save_state_generation(workspace: &TestWorkspace) -> u64 {
    let raw = workspace.read_to_string(".codesql/state/save_state.json");
    let json: serde_json::Value =
        serde_json::from_str(&raw).expect("failed to parse save_state.json");
    json.get("current_generation")
        .and_then(serde_json::Value::as_u64)
        .expect("save_state.json must contain current_generation")
}

pub fn save_run_changed_paths(workspace: &TestWorkspace, generation: u64) -> Vec<String> {
    let connection = Connection::open(workspace.path().join(".codesql/catalog.db"))
        .expect("failed to open catalog.db");
    let raw: String = connection
        .query_row(
            "SELECT changed_paths FROM save_runs WHERE generation = ?1",
            [generation as i64],
            |row| row.get(0),
        )
        .expect("failed to read changed_paths from save_runs");
    serde_json::from_str(&raw).expect("changed_paths must be valid JSON")
}

pub fn search_codesql(workspace: &TestWorkspace, query: &str) -> Output {
    run_codesql(workspace, ["search", query])
}

pub fn init_git_repository(workspace: &TestWorkspace) {
    assert_success(&run_git(workspace, ["init"]));
    assert_success(&run_git(
        workspace,
        ["config", "user.email", "codesql@example.com"],
    ));
    assert_success(&run_git(
        workspace,
        ["config", "user.name", "codesql-tests"],
    ));
}

pub fn commit_all(workspace: &TestWorkspace, message: &str) {
    assert_success(&run_git(workspace, ["add", "."]));
    assert_success(&run_git(workspace, ["commit", "-m", message]));
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "expected success.\nstatus: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout_string(output),
        stderr_string(output)
    );
}

pub fn stdout_string(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn stderr_string(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[cfg(unix)]
pub fn set_file_mode(workspace: &TestWorkspace, relative_path: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;

    let path = workspace.path().join(relative_path);
    let mut permissions = fs::metadata(&path)
        .expect("failed to read file metadata")
        .permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).expect("failed to update file permissions");
}

fn unique_workspace_name() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let counter = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("codesql-tests-{timestamp}-{counter}")
}
