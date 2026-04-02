#[derive(Debug, Clone)]
pub struct FileRecord {
    pub file_id: i64,
    pub path: String,
    pub size: u64,
    pub mtime_ns: i64,
    pub is_text: bool,
    pub ext: String,
    pub language: String,
    pub analyzer_name: String,
}

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: String,
    pub size: u64,
    pub mtime_ns: i64,
    pub is_text: bool,
    pub ext: String,
    pub language: String,
    pub analyzer_name: String,
    pub symbols: Vec<crate::analyzer::Symbol>,
}
