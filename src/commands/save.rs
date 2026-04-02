use std::path::PathBuf;

use anyhow::Result;

use crate::analyzer::AnalyzerRegistry;
use crate::catalog::open_catalog;
use crate::git;
use crate::indexing::{diff, materialize, scan, segment};
use crate::paths::CodesqlPaths;
use crate::state::{
    current_generation, ensure_initialized, write_current_generation, write_save_state,
};

pub fn run(root: PathBuf) -> Result<()> {
    let paths = CodesqlPaths::new(root);
    ensure_initialized(&paths)?;

    let mut catalog = open_catalog(&paths)?;
    let registry = AnalyzerRegistry::new();
    let git_snapshot = git::collect_snapshot(paths.root())?;
    let scanned_files = scan::scan_file_metadata(paths.root())?;
    let existing_files = catalog.active_file_map()?;
    let diff = diff::diff_files(&existing_files, &scanned_files);

    if diff.is_empty() {
        println!("no changes");
        return Ok(());
    }

    let previous_generation = current_generation(&paths)?;
    let generation = previous_generation + 1;
    let materialized_files = materialize::materialize_changed_files(
        paths.root(),
        &scanned_files,
        &diff.changed_paths,
        &registry,
    )?;
    let indexed_files =
        materialize::collect_snapshot_files(&existing_files, &scanned_files, &materialized_files)?;
    catalog.upsert_snapshot(
        generation,
        &indexed_files,
        &diff.deleted_paths,
        git_snapshot.as_ref(),
    )?;
    let file_ids = catalog.file_ids_by_path()?;
    let built_segment = segment::build_segment(
        generation,
        previous_generation,
        &catalog,
        &existing_files,
        &diff,
        &materialized_files,
        &file_ids,
    )?;
    let segment_path = segment::publish_segment(&paths, generation, &built_segment)?;
    catalog.record_segment(
        generation,
        &segment_path,
        built_segment.trigram_count(),
        file_ids.len(),
    )?;
    write_current_generation(&paths, generation)?;
    write_save_state(&paths, generation)?;
    println!("saved generation {generation}");

    if let Ok(config) = crate::config::Config::read_from_path(&paths.config_path()) {
        let threshold = config.save.auto_optimize_segment_count;
        if threshold > 0 {
            if let Ok(segment_paths) = catalog.active_segment_paths(generation) {
                if segment_paths.len() >= threshold as usize {
                    crate::commands::optimize::run_optimization(&paths, &mut catalog, generation)?;
                    println!("auto-optimized index (reached {} segments)", segment_paths.len());
                }
            }
        }
    }

    Ok(())
}
