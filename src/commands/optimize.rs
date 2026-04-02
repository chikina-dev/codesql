use std::path::PathBuf;

use anyhow::Result;

use crate::catalog::{open_catalog, Catalog};
use crate::indexing::{compaction, segment};
use crate::paths::CodesqlPaths;
use crate::state::{current_generation, ensure_initialized, write_current_generation};

pub fn run(root: PathBuf) -> Result<()> {
    let paths = CodesqlPaths::new(root);
    ensure_initialized(&paths)?;

    let mut catalog = open_catalog(&paths)?;
    let previous_gen = current_generation(&paths)?;

    if catalog.active_segment_paths(previous_gen)?.len() <= 1 {
        println!("already optimized");
        return Ok(());
    }

    run_optimization(&paths, &mut catalog, previous_gen)?;
    println!("optimized index");
    Ok(())
}

pub fn run_optimization(paths: &CodesqlPaths, catalog: &mut Catalog, max_generation: u64) -> Result<()> {
    let compacted_opt = compaction::compact_segments(catalog, max_generation)?;
    let Some(mut compacted) = compacted_opt else {
        return Ok(());
    };

    let new_generation = max_generation + 1;
    compacted.generation = new_generation;

    let segment_path = segment::publish_segment(paths, new_generation, &compacted)?;
    let file_ids = catalog.file_ids_by_path()?;

    catalog.record_segment(
        new_generation,
        &segment_path,
        compacted.trigram_count(),
        file_ids.len(),
    )?;

    write_current_generation(paths, new_generation)?;
    catalog.delete_segments_before(new_generation)?;

    Ok(())
}
