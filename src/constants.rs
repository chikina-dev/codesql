pub const APP_DIR: &str = ".codesql";
pub const CONFIG_FILE: &str = "config.toml";
pub const CATALOG_DB_FILE: &str = "catalog.db";
pub const STATE_DIR: &str = "state";
pub const CURRENT_GENERATION_FILE: &str = "current_generation";
pub const SAVE_STATE_FILE: &str = "save_state.json";
pub const SEGMENTS_DIR: &str = "segments";
pub const TMP_DIR: &str = "tmp";
pub const ANALYZERS_DIR: &str = "analyzers";
pub const ANALYZER_MANIFEST_FILE: &str = "manifest.json";
pub const TABLE_FILES: &str = "files";
pub const TABLE_SAVE_RUNS: &str = "save_runs";
pub const TABLE_SEGMENTS: &str = "segments";
pub const MAX_INDEXED_FILE_SIZE_BYTES: u64 = 2 * 1024 * 1024;
pub const LANGUAGE_RUST: &str = "rust";
pub const LANGUAGE_TYPESCRIPT: &str = "typescript";
pub const LANGUAGE_JAVASCRIPT: &str = "javascript";
pub const LANGUAGE_PLAINTEXT: &str = "plaintext";
pub const LANGUAGE_BINARY: &str = "binary";
pub const ANALYZER_PLAINTEXT: &str = "PlainText";
pub const ANALYZER_RUST: &str = "Rust";
pub const ANALYZER_TYPESCRIPT_JAVASCRIPT: &str = "TypeScript/JavaScript";
pub const ANALYZER_BINARY: &str = "Binary";
pub const CONFIG_TEMPLATE: &str = "\
[save]\n\
max_indexed_file_size_bytes = 2097152\n\
\n\
[analyzers]\n\
enabled = [\"PlainText\", \"Rust\", \"TypeScript/JavaScript\"]\n";

pub fn is_internal_path(relative_path: &str) -> bool {
    matches!(
        relative_path.split('/').next(),
        Some(APP_DIR) | Some(".git")
    )
}
