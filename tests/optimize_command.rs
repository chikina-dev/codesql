mod support;

use support::{
    TestWorkspace, assert_success, init_codesql, run_codesql, save_codesql, search_codesql, stdout_string,
};

#[test]
fn optimize_compacts_multiple_segments_into_one() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "one alpha\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    workspace.write_text_file("b.txt", "two beta\n");
    save_codesql(&workspace);

    workspace.write_text_file("c.txt", "three gamma\n");
    save_codesql(&workspace);

    let segments_before = workspace.entries_in(".codesql/segments");
    assert_eq!(segments_before.len(), 3);

    let output = run_codesql(&workspace, ["optimize"]);
    assert_success(&output);

    let segments_after = workspace.entries_in(".codesql/segments");
    assert_eq!(segments_after.len(), 1, "segments should be compacted to 1");

    // verify search still hits
    let search_alpha = search_codesql(&workspace, "SELECT path FROM files WHERE contains(content, 'alpha')");
    let search_beta = search_codesql(&workspace, "SELECT path FROM files WHERE contains(content, 'beta')");
    let search_gamma = search_codesql(&workspace, "SELECT path FROM files WHERE contains(content, 'gamma')");

    assert!(stdout_string(&search_alpha).contains("a.txt"));
    assert!(stdout_string(&search_beta).contains("b.txt"));
    assert!(stdout_string(&search_gamma).contains("c.txt"));
}

#[test]
fn save_auto_optimizes_based_on_config_threshold() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "one\n");
    init_codesql(&workspace);
    
    // Rewrite config to auto_optimize_segment_count = 3
    let mut config = workspace.read_to_string(".codesql/config.toml");
    config = config.replace(
        "[save]",
        "[save]\nauto_optimize_segment_count = 3"
    );
    workspace.write_text_file(".codesql/config.toml", &config);

    save_codesql(&workspace); // gen 1
    
    workspace.write_text_file("b.txt", "two\n");
    save_codesql(&workspace); // gen 2
    
    workspace.write_text_file("c.txt", "three\n");
    save_codesql(&workspace); // gen 3 (should trigger optimize!)

    let segments = workspace.entries_in(".codesql/segments");
    assert_eq!(segments.len(), 1, "segments should be auto-compacted to 1 when threshold is reached");
}

#[test]
fn optimize_drops_tombstoned_records_from_segments() {
    let workspace = TestWorkspace::new();
    workspace.write_text_file("a.txt", "alpha\n");
    workspace.write_text_file("b.txt", "beta\n");
    init_codesql(&workspace);
    save_codesql(&workspace);

    workspace.write_text_file("a.txt", "changed\n");
    std::fs::remove_file(workspace.path().join("b.txt")).unwrap();
    save_codesql(&workspace);

    let output = run_codesql(&workspace, ["optimize"]);
    assert_success(&output);

    let segments = workspace.entries_in(".codesql/segments");
    let seg_path = &segments[0];

    // Read the binary bincode SegmentData correctly instead of text parsing
    let seg_data = codesql::segment::SegmentData::read_from_path(seg_path).unwrap();

    assert!(!seg_data.postings.contains_key(b"alp"), "tombstoned content should be completely removed from compacted segment");
    assert!(!seg_data.postings.contains_key(b"bet"));
    assert!(seg_data.postings.contains_key(b"cha"), "new content must be preserved");
}
