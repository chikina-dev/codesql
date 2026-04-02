use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::analyzer::AnalyzerRegistry;
use crate::catalog::open_catalog;
use crate::paths::CodesqlPaths;
use crate::state::initialize_layout;

pub fn run(root: PathBuf) -> Result<()> {
    let paths = CodesqlPaths::new(root);
    let registry = AnalyzerRegistry::new();
    let manifest = serde_json::to_string_pretty(&registry.manifest_entries())
        .context("failed to serialize analyzer manifest")?;

    initialize_layout(&paths, &manifest)?;
    open_catalog(&paths)?;
    println!("initialized {}", paths.app_dir().display());
    Ok(())
}
