mod support;

use rusqlite::Connection;

use support::{TestWorkspace, assert_success, run_codesql};

#[test]
fn init_creates_expected_codesql_layout() {
    // Given
    let workspace = TestWorkspace::new();

    // When
    let output = run_codesql(&workspace, ["init"]);

    // Then
    assert_success(&output);
    assert!(workspace.path().join(".codesql").is_dir());
    assert!(workspace.path().join(".codesql/config.toml").is_file());
    assert!(workspace.path().join(".codesql/catalog.db").is_file());
    assert!(workspace.path().join(".codesql/state").is_dir());
    assert!(
        workspace
            .path()
            .join(".codesql/state/current_generation")
            .is_file()
    );
    assert!(workspace.path().join(".codesql/segments").is_dir());
    assert!(workspace.path().join(".codesql/tmp").is_dir());
    assert!(workspace.path().join(".codesql/analyzers").is_dir());
    assert!(
        workspace
            .path()
            .join(".codesql/analyzers/manifest.json")
            .is_file()
    );
}

#[test]
fn init_bootstraps_catalog_schema() {
    // Given
    let workspace = TestWorkspace::new();

    // When
    let output = run_codesql(&workspace, ["init"]);

    // Then
    assert_success(&output);
    let connection = Connection::open(workspace.path().join(".codesql/catalog.db"))
        .expect("catalog.db should be readable");
    for table in ["files", "segments", "save_runs"] {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .expect("schema query should succeed");
        assert_eq!(exists, 1, "missing table: {table}");
    }
}
