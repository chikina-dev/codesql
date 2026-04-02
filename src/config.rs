use std::path::Path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveConfig {
    pub max_indexed_file_size_bytes: u64,
    #[serde(default = "default_auto_optimize_segment_count")]
    pub auto_optimize_segment_count: u64,
}

fn default_auto_optimize_segment_count() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzersConfig {
    pub enabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub save: SaveConfig,
    pub analyzers: AnalyzersConfig,
}

impl Config {
    pub fn read_from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let config: Config = toml::from_str(&content)
            .context("failed to parse config.toml")?;
        Ok(config)
    }
}
