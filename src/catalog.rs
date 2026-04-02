use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{Connection, Row, params};

use crate::catalog_types::{FileRecord, IndexedFile};
use crate::constants::{TABLE_FILES, TABLE_SAVE_RUNS, TABLE_SEGMENTS};
const TABLE_SYMBOLS: &str = "symbols";
use crate::git::GitSnapshot;
use crate::paths::CodesqlPaths;

pub struct Catalog {
    connection: Connection,
}

impl Catalog {
    fn initialize_schema(&self) -> Result<()> {
        self.connection.execute_batch(&format!(
            "
            CREATE TABLE IF NOT EXISTS {files} (
                file_id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                size INTEGER NOT NULL,
                mtime_ns INTEGER NOT NULL,
                is_text INTEGER NOT NULL,
                ext TEXT NOT NULL,
                language TEXT NOT NULL,
                analyzer_name TEXT NOT NULL,
                last_indexed_generation INTEGER NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS {segments} (
                generation INTEGER PRIMARY KEY,
                path TEXT NOT NULL,
                trigram_count INTEGER NOT NULL,
                document_count INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS {symbols} (
                file_id INTEGER NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                line INTEGER NOT NULL,
                FOREIGN KEY(file_id) REFERENCES {files}(file_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_{symbols}_lookup ON {symbols}(kind, name);
            CREATE INDEX IF NOT EXISTS idx_{symbols}_file ON {symbols}(file_id);
            CREATE TABLE IF NOT EXISTS {save_runs} (
                generation INTEGER PRIMARY KEY,
                started_at INTEGER NOT NULL,
                finished_at INTEGER NOT NULL,
                status TEXT NOT NULL,
                head_ref TEXT,
                head_commit TEXT,
                branch_name TEXT,
                git_dirty_state INTEGER NOT NULL,
                repo_root TEXT,
                git_dir TEXT,
                changed_paths TEXT NOT NULL DEFAULT '[]'
            );
            ",
            files = TABLE_FILES,
            segments = TABLE_SEGMENTS,
            save_runs = TABLE_SAVE_RUNS,
            symbols = TABLE_SYMBOLS,
        ))?;
        ensure_save_runs_changed_paths_column(&self.connection)?;
        Ok(())
    }

    pub fn active_files(&self) -> Result<Vec<FileRecord>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT file_id, path, size, mtime_ns, is_text, ext, language, analyzer_name, deleted
             FROM {table}
             WHERE deleted = 0
             ORDER BY path",
            table = TABLE_FILES
        ))?;
        let rows = statement.query_map([], file_record_from_row)?;
        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }
        Ok(files)
    }

    pub fn active_file_map(&self) -> Result<HashMap<String, FileRecord>> {
        let mut files = HashMap::new();
        for file in self.active_files()? {
            files.insert(file.path.clone(), file);
        }
        Ok(files)
    }

    pub fn upsert_snapshot(
        &mut self,
        generation: u64,
        files: &[IndexedFile],
        deleted_paths: &[String],
        git_snapshot: Option<&GitSnapshot>,
    ) -> Result<()> {
        let transaction = self.connection.transaction()?;
        {
            let mut insert_file_stmt = transaction.prepare(&format!(
                "INSERT INTO {table}
                 (path, size, mtime_ns, is_text, ext, language, analyzer_name, last_indexed_generation, deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)
                 ON CONFLICT(path) DO UPDATE SET
                   size = excluded.size,
                   mtime_ns = excluded.mtime_ns,
                   is_text = excluded.is_text,
                   ext = excluded.ext,
                   language = excluded.language,
                   analyzer_name = excluded.analyzer_name,
                   last_indexed_generation = excluded.last_indexed_generation,
                   deleted = 0",
                table = TABLE_FILES
            ))?;
            
            let mut get_id_stmt = transaction.prepare(&format!("SELECT file_id FROM {table} WHERE path = ?", table = TABLE_FILES))?;
            let mut del_sym_stmt = transaction.prepare(&format!("DELETE FROM {table} WHERE file_id = ?", table = TABLE_SYMBOLS))?;
            let mut ins_sym_stmt = transaction.prepare(&format!("INSERT INTO {table} (file_id, kind, name, line) VALUES (?, ?, ?, ?)", table = TABLE_SYMBOLS))?;

            for file in files {
                insert_file_stmt.execute(params![
                    file.path,
                    file.size as i64,
                    file.mtime_ns,
                    bool_to_int(file.is_text),
                    file.ext,
                    file.language,
                    file.analyzer_name,
                    generation as i64
                ])?;
                
                let file_id: i64 = get_id_stmt.query_row(params![file.path], |row| row.get(0))?;
                del_sym_stmt.execute(params![file_id])?;

                for sym in &file.symbols {
                    ins_sym_stmt.execute(params![file_id, sym.kind, &sym.name, sym.line as i64])?;
                }
            }
        }

        for path in deleted_paths {
            transaction.execute(
                &format!(
                    "UPDATE {table}
                     SET deleted = 1, last_indexed_generation = ?2
                     WHERE path = ?1",
                    table = TABLE_FILES
                ),
                params![path, generation as i64],
            )?;
        }

        let now = unix_timestamp();
        let changed_paths = match git_snapshot {
            Some(snapshot) => serde_json::to_string(&snapshot.changed_paths)
                .context("failed to serialize git changed paths")?,
            None => "[]".to_owned(),
        };
        transaction.execute(
            &format!(
                "INSERT OR REPLACE INTO {table}
                 (generation, started_at, finished_at, status, head_ref, head_commit, branch_name, git_dirty_state, repo_root, git_dir, changed_paths)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                table = TABLE_SAVE_RUNS
            ),
            params![
                generation as i64,
                now,
                now,
                "completed",
                git_snapshot.map(|snapshot| snapshot.head_ref.as_str()),
                git_snapshot.and_then(|snapshot| snapshot.head_commit.as_deref()),
                git_snapshot.and_then(|snapshot| snapshot.branch_name.as_deref()),
                bool_to_int(git_snapshot.map(|snapshot| snapshot.is_dirty).unwrap_or(false)),
                git_snapshot.map(|snapshot| snapshot.repo_root.as_str()),
                git_snapshot.map(|snapshot| snapshot.git_dir.as_str()),
                changed_paths,
            ],
        )?;

        transaction.commit()?;
        Ok(())
    }

    pub fn file_ids_by_path(&self) -> Result<HashMap<String, i64>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT file_id, path
             FROM {table}
             WHERE deleted = 0",
            table = TABLE_FILES
        ))?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut file_ids = HashMap::new();
        for row in rows {
            let (file_id, path) = row?;
            file_ids.insert(path, file_id);
        }
        Ok(file_ids)
    }

    pub fn record_segment(
        &self,
        generation: u64,
        segment_path: &Path,
        trigram_count: usize,
        document_count: usize,
    ) -> Result<()> {
        self.connection.execute(
            &format!(
                "INSERT OR REPLACE INTO {table}
                 (generation, path, trigram_count, document_count, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                table = TABLE_SEGMENTS
            ),
            params![
                generation as i64,
                segment_path.to_string_lossy().as_ref(),
                trigram_count as i64,
                document_count as i64,
                unix_timestamp(),
            ],
        )?;
        Ok(())
    }



    pub fn active_segment_paths(&self, generation: u64) -> Result<Vec<PathBuf>> {
        let mut statement = self.connection.prepare(&format!(
            "SELECT path FROM {table} WHERE generation <= ?1 ORDER BY generation ASC",
            table = TABLE_SEGMENTS
        ))?;
        let rows = statement.query_map([generation as i64], |row| row.get::<_, String>(0))?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(PathBuf::from(row?));
        }
        Ok(paths)
    }

    pub fn delete_segments_before(&self, generation: u64) -> Result<()> {
        let paths = self.active_segment_paths(generation - 1)?;
        for path in paths {
            let _ = std::fs::remove_file(&path);
        }
        self.connection.execute(
            &format!("DELETE FROM {} WHERE generation < ?1", TABLE_SEGMENTS),
            rusqlite::params![generation as i64],
        )?;
        Ok(())
    }

    pub fn query_files(
        &self,
        where_clause: &str,
        order_by_clause: &str,
        parameters: &[String],
    ) -> Result<Vec<FileRecord>> {
        let sql = format!(
            "SELECT file_id, path, size, mtime_ns, is_text, ext, language, analyzer_name, deleted
             FROM {table}
             WHERE deleted = 0 {where_clause}
             {order_by_clause}",
            table = TABLE_FILES,
            where_clause = where_clause,
            order_by_clause = order_by_clause
        );
        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(parameters.iter()),
            file_record_from_row,
        )?;
        let mut files = Vec::new();
        for row in rows {
            files.push(row?);
        }
        Ok(files)
    }

    pub fn check_symbol(&self, file_id: i64, kind: &crate::query::StringMatch, name: &crate::query::StringMatch) -> Result<bool> {
        let mut parameters: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(file_id)];
        
        let kind_cond = match kind {
            crate::query::StringMatch::Exact(s) => { parameters.push(Box::new(s.clone())); "kind = ?" },
            crate::query::StringMatch::Regex(s) => { parameters.push(Box::new(s.clone())); "REGEXP(?, kind)" },
            crate::query::StringMatch::Glob(s) => { parameters.push(Box::new(s.clone())); "kind GLOB ?" },
        };
        let name_cond = match name {
            crate::query::StringMatch::Exact(s) => { parameters.push(Box::new(s.clone())); "name = ?" },
            crate::query::StringMatch::Regex(s) => { parameters.push(Box::new(s.clone())); "REGEXP(?, name)" },
            crate::query::StringMatch::Glob(s) => { parameters.push(Box::new(s.clone())); "name GLOB ?" },
        };

        let sql = format!(
            "SELECT 1 FROM {} WHERE file_id = ? AND {} AND {} LIMIT 1",
            TABLE_SYMBOLS, kind_cond, name_cond
        );
        
        let mut statement = self.connection.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = parameters.iter().map(|b| b.as_ref()).collect();
        let exists = statement.exists(rusqlite::params_from_iter(refs))?;
        Ok(exists)
    }
}

pub fn open_catalog(paths: &CodesqlPaths) -> Result<Catalog> {
    ensure_catalog_path_is_not_symlink(&paths.catalog_path())?;
    let connection = Connection::open(paths.catalog_path()).context("failed to open catalog.db")?;

    use std::sync::LazyLock;
    use std::collections::HashMap;
    use std::sync::Mutex;
    static REGEX_CACHE: LazyLock<Mutex<HashMap<String, regex::Regex>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

    connection.create_scalar_function(
        "REGEXP",
        2,
        rusqlite::functions::FunctionFlags::SQLITE_UTF8 | rusqlite::functions::FunctionFlags::SQLITE_DETERMINISTIC,
        move |ctx: &rusqlite::functions::Context| {
            let pattern: String = ctx.get(0)?;
            let text: String = ctx.get(1)?;
            
            let mut cache = REGEX_CACHE.lock().unwrap();
            if !cache.contains_key(&pattern) {
                if let Ok(re) = regex::Regex::new(&pattern) {
                    cache.insert(pattern.clone(), re);
                } else {
                    return Ok(false); // Invalid regexes just match nothing
                }
            }
            
            Ok(cache.get(&pattern).unwrap().is_match(&text))
        }
    ).context("failed to bind REGEXP function")?;

    let catalog = Catalog { connection };
    catalog.initialize_schema()?;
    Ok(catalog)
}

fn ensure_catalog_path_is_not_symlink(path: &Path) -> Result<()> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "codesql catalog.db must not be a symlink: {}",
            path.display()
        );
    }
    Ok(())
}

fn bool_to_int(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn file_record_from_row(row: &Row<'_>) -> rusqlite::Result<FileRecord> {
    Ok(FileRecord {
        file_id: row.get(0)?,
        path: row.get(1)?,
        size: row.get::<_, i64>(2)? as u64,
        mtime_ns: row.get(3)?,
        is_text: row.get::<_, i64>(4)? != 0,
        ext: row.get(5)?,
        language: row.get(6)?,
        analyzer_name: row.get(7)?,
    })
}

fn ensure_save_runs_changed_paths_column(connection: &Connection) -> Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({TABLE_SAVE_RUNS})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;

    for column in columns {
        if column? == "changed_paths" {
            return Ok(());
        }
    }

    connection.execute(
        &format!(
            "ALTER TABLE {TABLE_SAVE_RUNS} ADD COLUMN changed_paths TEXT NOT NULL DEFAULT '[]'"
        ),
        [],
    )?;
    Ok(())
}

fn unix_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_secs() as i64
}
