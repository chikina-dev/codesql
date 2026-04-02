pub mod cli;

mod analyzer;
mod catalog;
mod catalog_types;
mod commands;
pub mod config;
mod constants;
mod git;
mod indexing;
mod paths;
mod query;
pub mod segment;
mod state;
mod verifier;

use anyhow::Result;
use cli::{Cli, Commands};

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init => commands::init::run(std::env::current_dir()?),
        Commands::Optimize => commands::optimize::run(std::env::current_dir()?),
        Commands::Save => commands::save::run(std::env::current_dir()?),
        Commands::Search { query } => commands::search::run(std::env::current_dir()?, &query),
    }
}
