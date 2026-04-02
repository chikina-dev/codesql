mod support;

use support::{
    TestWorkspace, assert_success, commit_all, current_generation, init_codesql,
    init_git_repository, run_codesql, save_codesql, save_run_changed_paths, save_state_generation,
    search_codesql, stderr_string, stdout_string,
};

#[cfg(unix)]
use std::os::unix::fs::symlink;

#[cfg(unix)]
use support::set_file_mode;

#[test]
fn save_indexes_rust_files_for_extension_and_content_search() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file(
        "src/lib.rs",
        "pub fn trigger() {\n    unsafe { core::ptr::read_volatile(&0); }\n}\n",
    );
    init_codesql(&workspace);

    // When
    save_codesql(&workspace);
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE ext = 'rs' AND contains(content, 'unsafe') LIMIT 20",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("src/lib.rs"), "stdout was:\n{stdout}");
}

#[test]
fn save_indexes_plain_text_files_for_content_search() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("docs/notes.md", "# Notes\nrelease plan\n");
    init_codesql(&workspace);

    // When
    save_codesql(&workspace);
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE ext = 'md' AND contains(content, 'release plan') LIMIT 20",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("docs/notes.md"), "stdout was:\n{stdout}");
}

#[test]
fn save_stores_binary_files_without_making_them_content_searchable() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_binary_file("assets/logo.png", b"\x89PNG\r\n\x1a\nunsafe\x00payload");
    init_codesql(&workspace);

    // When
    save_codesql(&workspace);
    let metadata_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE ext = 'png' LIMIT 20",
    );
    let content_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'unsafe') LIMIT 20",
    );

    // Then
    assert_success(&metadata_output);
    assert_success(&content_output);
    let metadata_stdout = stdout_string(&metadata_output);
    let content_stdout = stdout_string(&content_output);
    assert!(
        metadata_stdout.contains("assets/logo.png"),
        "stdout was:\n{metadata_stdout}"
    );
    assert!(
        !content_stdout.contains("assets/logo.png"),
        "stdout was:\n{content_stdout}"
    );
}

#[test]
fn save_without_changes_keeps_generation_and_segments_stable() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn sample() {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);
    let first_generation = workspace.read_to_string(".codesql/state/current_generation");
    let first_segments = workspace.entries_in(".codesql/segments");

    // When
    save_codesql(&workspace);

    // Then
    let second_generation = workspace.read_to_string(".codesql/state/current_generation");
    let second_segments = workspace.entries_in(".codesql/segments");
    assert_eq!(second_generation, first_generation);
    assert_eq!(second_segments, first_segments);
}

#[test]
fn save_updates_save_state_to_latest_generation() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn sample() {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);
    workspace.write_text_file(
        "src/lib.rs",
        "pub fn sample() {\n    println!(\"v2\");\n}\n",
    );

    // When
    save_codesql(&workspace);

    // Then
    assert_eq!(current_generation(&workspace), 2);
    assert_eq!(save_state_generation(&workspace), 2);
}

#[test]
fn save_persists_git_changed_paths_for_the_generation() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn version_one() {}\n");
    init_git_repository(&workspace);
    commit_all(&workspace, "initial commit");
    init_codesql(&workspace);
    commit_all(&workspace, "commit codesql layout");
    workspace.write_text_file("src/lib.rs", "pub fn version_two() {}\n");
    workspace.write_text_file("docs/notes.md", "# Notes\nship it\n");

    // When
    save_codesql(&workspace);

    // Then
    let changed_paths = save_run_changed_paths(&workspace, 1);
    assert!(
        changed_paths.contains(&"src/lib.rs".to_owned()),
        "changed_paths was: {changed_paths:?}"
    );
    assert!(
        changed_paths.contains(&"docs/notes.md".to_owned()),
        "changed_paths was: {changed_paths:?}"
    );
}

#[test]
fn save_succeeds_in_uncommitted_git_repository() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn version_one() {}\n");
    init_git_repository(&workspace);
    init_codesql(&workspace);

    // When
    let output = run_codesql(&workspace, ["save"]);

    // Then
    assert_success(&output);
    assert_eq!(current_generation(&workspace), 1);
    let changed_paths = save_run_changed_paths(&workspace, 1);
    assert!(
        changed_paths.contains(&"src/lib.rs".to_owned()),
        "changed_paths was: {changed_paths:?}"
    );
}

#[test]
fn save_persists_git_changed_paths_with_spaces_without_quotes() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn version_one() {}\n");
    init_git_repository(&workspace);
    commit_all(&workspace, "initial commit");
    init_codesql(&workspace);
    commit_all(&workspace, "commit codesql layout");
    workspace.write_text_file("docs/file with space.txt", "ship it\n");

    // When
    save_codesql(&workspace);

    // Then
    let changed_paths = save_run_changed_paths(&workspace, 1);
    assert!(
        changed_paths.contains(&"docs/file with space.txt".to_owned()),
        "changed_paths was: {changed_paths:?}"
    );
    assert!(
        !changed_paths.contains(&"\"docs/file with space.txt\"".to_owned()),
        "changed_paths unexpectedly contained quoted path: {changed_paths:?}"
    );
}

#[test]
fn save_excludes_internal_codesql_paths_from_second_generation_changed_paths() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn version_one() {}\n");
    init_git_repository(&workspace);
    commit_all(&workspace, "initial commit");
    init_codesql(&workspace);
    commit_all(&workspace, "commit codesql layout");
    workspace.write_text_file("src/lib.rs", "pub fn version_two() {}\n");

    // When
    save_codesql(&workspace);
    workspace.write_text_file("docs/notes.md", "# Notes\nship v2\n");
    save_codesql(&workspace);

    // Then
    let changed_paths = save_run_changed_paths(&workspace, 2);
    assert!(
        changed_paths.contains(&"docs/notes.md".to_owned()),
        "changed_paths was: {changed_paths:?}"
    );
    assert!(
        !changed_paths
            .iter()
            .any(|path| path.starts_with(".codesql/")),
        "changed_paths unexpectedly contained codesql internals: {changed_paths:?}"
    );
}

#[test]
fn save_replaces_old_search_hits_when_a_file_changes() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn sample() { println!(\"alpha\"); }\n");
    init_codesql(&workspace);
    save_codesql(&workspace);
    let first_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'alpha') LIMIT 20",
    );
    workspace.write_text_file("src/lib.rs", "pub fn sample() { println!(\"beta\"); }\n");

    // When
    save_codesql(&workspace);
    let old_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'alpha') LIMIT 20",
    );
    let new_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'beta') LIMIT 20",
    );

    // Then
    assert_success(&first_output);
    assert_success(&old_output);
    assert_success(&new_output);
    let first_stdout = stdout_string(&first_output);
    let old_stdout = stdout_string(&old_output);
    let new_stdout = stdout_string(&new_output);
    assert!(
        first_stdout.contains("src/lib.rs"),
        "stdout was:\n{first_stdout}"
    );
    assert!(
        !old_stdout.contains("src/lib.rs"),
        "stdout still contained stale content:\n{old_stdout}"
    );
    assert!(
        new_stdout.contains("src/lib.rs"),
        "stdout was:\n{new_stdout}"
    );
}

#[test]
fn save_removes_deleted_files_from_search_results() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn sample() { println!(\"alpha\"); }\n");
    init_codesql(&workspace);
    save_codesql(&workspace);
    let initial_path_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE path LIKE 'src/lib.rs' LIMIT 20",
    );
    let initial_content_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'alpha') LIMIT 20",
    );
    std::fs::remove_file(workspace.path().join("src/lib.rs")).expect("file should be deleted");

    // When
    save_codesql(&workspace);
    let path_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE path LIKE 'src/lib.rs' LIMIT 20",
    );
    let content_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'alpha') LIMIT 20",
    );

    // Then
    assert_success(&initial_path_output);
    assert_success(&initial_content_output);
    assert_success(&path_output);
    assert_success(&content_output);
    let initial_path_stdout = stdout_string(&initial_path_output);
    let initial_content_stdout = stdout_string(&initial_content_output);
    let path_stdout = stdout_string(&path_output);
    let content_stdout = stdout_string(&content_output);
    assert!(
        initial_path_stdout.contains("src/lib.rs"),
        "stdout was:\n{initial_path_stdout}"
    );
    assert!(
        initial_content_stdout.contains("src/lib.rs"),
        "stdout was:\n{initial_content_stdout}"
    );
    assert!(
        !path_stdout.contains("src/lib.rs"),
        "stdout still contained deleted path:\n{path_stdout}"
    );
    assert!(
        !content_stdout.contains("src/lib.rs"),
        "stdout still contained deleted content:\n{content_stdout}"
    );
}

#[cfg(unix)]
#[test]
fn save_without_changes_does_not_reread_unchanged_files() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn stable() {}\n");
    init_codesql(&workspace);
    save_codesql(&workspace);
    let first_generation = current_generation(&workspace);
    set_file_mode(&workspace, "src/lib.rs", 0o000);

    // When
    let output = run_codesql(&workspace, ["save"]);
    set_file_mode(&workspace, "src/lib.rs", 0o644);

    // Then
    assert_success(&output);
    assert_eq!(current_generation(&workspace), first_generation);
    let stdout = stdout_string(&output);
    assert!(stdout.contains("no changes"), "stdout was:\n{stdout}");
}

#[test]
fn save_in_git_repository_does_not_index_dot_git_files() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn tracked() {}\n");
    init_git_repository(&workspace);
    commit_all(&workspace, "initial commit");
    init_codesql(&workspace);

    // When
    save_codesql(&workspace);
    let output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE path LIKE '.git/%' LIMIT 20",
    );

    // Then
    assert_success(&output);
    let stdout = stdout_string(&output);
    assert!(
        !stdout.contains(".git/"),
        "stdout unexpectedly contained git internals:\n{stdout}"
    );
}

#[test]
fn save_fails_when_git_head_is_invalid() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn tracked() {}\n");
    init_git_repository(&workspace);
    commit_all(&workspace, "initial commit");
    init_codesql(&workspace);
    std::fs::write(workspace.path().join(".git/HEAD"), "broken-head\n")
        .expect("git HEAD should be overwritten");

    // When
    let output = run_codesql(&workspace, ["save"]);

    // Then
    assert!(
        !output.status.success(),
        "save unexpectedly succeeded.\nstdout:\n{}\nstderr:\n{}",
        stdout_string(&output),
        stderr_string(&output)
    );
    assert_eq!(current_generation(&workspace), 0);
    let stderr = stderr_string(&output);
    assert!(
        stderr.contains("git rev-parse --is-inside-work-tree"),
        "stderr was:\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn save_rereads_only_changed_files_when_other_files_are_unreadable() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file(
        "src/stable.rs",
        "pub fn stable() { println!(\"stable\"); }\n",
    );
    workspace.write_text_file(
        "src/changed.rs",
        "pub fn changed() { println!(\"alpha\"); }\n",
    );
    init_codesql(&workspace);
    save_codesql(&workspace);
    set_file_mode(&workspace, "src/stable.rs", 0o000);
    workspace.write_text_file(
        "src/changed.rs",
        "pub fn changed() { println!(\"beta\"); }\n",
    );

    // When
    let output = run_codesql(&workspace, ["save"]);
    set_file_mode(&workspace, "src/stable.rs", 0o644);

    // Then
    assert_success(&output);
    assert_eq!(current_generation(&workspace), 2);
    let stable_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'stable') LIMIT 20",
    );
    let alpha_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'alpha') LIMIT 20",
    );
    let beta_output = search_codesql(
        &workspace,
        "SELECT path FROM files WHERE contains(content, 'beta') LIMIT 20",
    );
    assert_success(&stable_output);
    assert_success(&alpha_output);
    assert_success(&beta_output);
    let stable_stdout = stdout_string(&stable_output);
    let alpha_stdout = stdout_string(&alpha_output);
    let beta_stdout = stdout_string(&beta_output);
    assert!(
        stable_stdout.contains("src/stable.rs"),
        "stdout was:\n{stable_stdout}"
    );
    assert!(
        !alpha_stdout.contains("src/changed.rs"),
        "stdout still contained stale content:\n{alpha_stdout}"
    );
    assert!(
        beta_stdout.contains("src/changed.rs"),
        "stdout was:\n{beta_stdout}"
    );
}

#[test]
fn save_publishes_segment_file_without_leaving_tmp_artifacts() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn indexed() {}\n");
    init_codesql(&workspace);

    // When
    save_codesql(&workspace);

    // Then
    let generation = current_generation(&workspace);
    let segment_path = workspace
        .path()
        .join(format!(".codesql/segments/{generation:06}.seg"));
    assert!(
        segment_path.is_file(),
        "segment file was not published: {}",
        segment_path.display()
    );
    let tmp_entries = workspace.entries_in(".codesql/tmp");
    assert!(
        tmp_entries.is_empty(),
        "tmp artifacts were left behind: {tmp_entries:?}"
    );
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_tmp_directory() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn indexed() {}\n");
    init_codesql(&workspace);
    let external_dir = workspace.path().join("external-tmp");
    std::fs::create_dir(&external_dir).expect("external tmp dir should be created");
    std::fs::remove_dir(workspace.path().join(".codesql/tmp"))
        .expect("managed tmp dir should be removed");
    symlink(&external_dir, workspace.path().join(".codesql/tmp"))
        .expect("managed tmp dir symlink should be created");

    // When
    let output = run_codesql(&workspace, ["save"]);

    // Then
    assert!(
        !output.status.success(),
        "save unexpectedly succeeded.\nstdout:\n{}\nstderr:\n{}",
        stdout_string(&output),
        stderr_string(&output)
    );
    assert_eq!(
        std::fs::read_dir(&external_dir)
            .expect("external tmp dir should stay readable")
            .count(),
        0
    );
    let stderr = stderr_string(&output);
    assert!(
        stderr.contains("tmp directory must not be a symlink"),
        "stderr was:\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_save_state_file() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn indexed() {}\n");
    init_codesql(&workspace);
    let external_target = workspace.path().join("external-save-state.txt");
    std::fs::write(&external_target, "protected\n").expect("external target should be written");
    std::fs::remove_file(workspace.path().join(".codesql/state/save_state.json"))
        .expect("managed save_state.json should be removed");
    symlink(
        &external_target,
        workspace.path().join(".codesql/state/save_state.json"),
    )
    .expect("managed save_state.json symlink should be created");

    // When
    let output = run_codesql(&workspace, ["save"]);

    // Then
    assert!(
        !output.status.success(),
        "save unexpectedly succeeded.\nstdout:\n{}\nstderr:\n{}",
        stdout_string(&output),
        stderr_string(&output)
    );
    assert_eq!(
        std::fs::read_to_string(&external_target).expect("external target should stay readable"),
        "protected\n"
    );
    let stderr = stderr_string(&output);
    assert!(
        stderr.contains("save_state.json must not be a symlink"),
        "stderr was:\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_current_generation_file() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn indexed() {}\n");
    init_codesql(&workspace);
    let external_target = workspace.path().join("external-current-generation.txt");
    std::fs::write(&external_target, "protected\n").expect("external target should be written");
    std::fs::remove_file(workspace.path().join(".codesql/state/current_generation"))
        .expect("managed current_generation should be removed");
    symlink(
        &external_target,
        workspace.path().join(".codesql/state/current_generation"),
    )
    .expect("managed current_generation symlink should be created");

    // When
    let output = run_codesql(&workspace, ["save"]);

    // Then
    assert!(
        !output.status.success(),
        "save unexpectedly succeeded.\nstdout:\n{}\nstderr:\n{}",
        stdout_string(&output),
        stderr_string(&output)
    );
    assert_eq!(
        std::fs::read_to_string(&external_target).expect("external target should stay readable"),
        "protected\n"
    );
    let stderr = stderr_string(&output);
    assert!(
        stderr.contains("current_generation must not be a symlink"),
        "stderr was:\n{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn save_rejects_symlinked_catalog_db() {
    // Given
    let workspace = TestWorkspace::new();
    workspace.write_text_file("src/lib.rs", "pub fn indexed() {}\n");
    init_codesql(&workspace);
    let external_target = workspace.path().join("external-catalog.db");
    std::fs::write(&external_target, "protected\n").expect("external target should be written");
    std::fs::remove_file(workspace.path().join(".codesql/catalog.db"))
        .expect("managed catalog.db should be removed");
    symlink(
        &external_target,
        workspace.path().join(".codesql/catalog.db"),
    )
    .expect("managed catalog.db symlink should be created");

    // When
    let output = run_codesql(&workspace, ["save"]);

    // Then
    assert!(
        !output.status.success(),
        "save unexpectedly succeeded.\nstdout:\n{}\nstderr:\n{}",
        stdout_string(&output),
        stderr_string(&output)
    );
    assert_eq!(
        std::fs::read_to_string(&external_target).expect("external target should stay readable"),
        "protected\n"
    );
    let stderr = stderr_string(&output);
    assert!(
        stderr.contains("catalog.db must not be a symlink"),
        "stderr was:\n{stderr}"
    );
}

#[test]
fn save_writes_append_only_segments() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "first file content\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    workspace.write_text_file("b.txt", "second file content\n");
    save_codesql(&workspace);

    let seg1 = codesql::segment::SegmentData::read_from_path(&workspace.path().join(".codesql/segments/000001.seg")).unwrap();
    let seg2 = codesql::segment::SegmentData::read_from_path(&workspace.path().join(".codesql/segments/000002.seg")).unwrap();

    // Gen 2 should ONLY contain diff (b.txt), not 'first' from a.txt
    assert!(seg1.postings.contains_key(b"fir"));
    assert!(!seg2.postings.contains_key(b"fir"), "Segment 2 should be append-only and not contain old documents");
    assert!(seg2.postings.contains_key(b"sec"));
}

#[test]
fn save_creates_tombstones_for_updated_or_deleted_files() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "one\n");
    workspace.write_text_file("b.txt", "two\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    workspace.write_text_file("a.txt", "changed\n");
    std::fs::remove_file(workspace.path().join("b.txt")).unwrap();
    save_codesql(&workspace);

    let seg2 = codesql::segment::SegmentData::read_from_path(&workspace.path().join(".codesql/segments/000002.seg")).unwrap();
    
    // Gen 2 must contain tombstones key indicating it registered the deletes/updates
    assert!(!seg2.tombstones.is_empty(), "Segment 2 should register tombstones");
}
