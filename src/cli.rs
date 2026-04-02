use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "codesql")]
#[command(about = "SQL-like code search over a saved project index")]
#[command(long_about = "\
SQL-like code search over a saved project index.

This command is designed to be AI-friendly. AI agents can use this tool to quickly index and search a codebase using SQL-like syntax.
Workflow for AI:
1. Run `codesql init` if the project is not yet initialized.
2. Run `codesql save` to index the current workspace.
3. Run `codesql search \"<SQL_QUERY>\"` to find files and code.
")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Initialize the codesql environment in the current workspace.
    #[command(long_about = "\
Initialize the codesql environment in the current workspace.
Creates a `.codesql` directory to store the index and configuration.
AI agents should run this command first if returning to or entering a workspace without a .codesql directory.")]
    Init,

    /// Optimize and compact the database index.
    #[command(long_about = "\
Optimize and compact the database index.
Run this command when search performance degrades or after a large number of file modifications.
It runs database vacuum and other optimizations.")]
    Optimize,

    /// Scan all files and update the local index.
    #[command(long_about = "\
Scan all files in the workspace and update the local index.
AI agents must run this command after modifying, adding, or deleting files in the workspace to ensure `codesql search` returns up-to-date results.")]
    Save,

    /// Run a SQL query over the codebase.
    #[command(long_about = "\
Run a SQL query over the codebase index.

The target table must be `files`.
Available fields in `SELECT`:
- `path`: file path
- `line_no`: line number (requires using `contains` or `regex` on content in WHERE clause)
- `line`: line content (requires using `contains` or `regex` on content in WHERE clause)

Available fields in `WHERE`:
- `path`: file path (string)
- `ext`: file extension (string)
- `language`: detected language like 'rust', 'typescript', etc. (string)

Available functions in `WHERE`:
- `contains(content, 'needle')`: matches if file content contains the exact string 'needle'
- `regex(content, 'pattern')`: matches if file content matches the regex 'pattern'
- `glob(field, 'pattern')`: matches using glob (e.g., `glob(path, 'src/**/*.rs')`)
- `has_symbol(kind, name)`: checks if the file has a specific symbol. `kind` (e.g., 'Function', 'Struct') and `name` can be exact strings, `regex('...')`, or `glob('...')`.

Example queries for AI:
- codesql search \"SELECT path FROM files WHERE ext = 'rs'\"
- codesql search \"SELECT path, line_no, line FROM files WHERE contains(content, 'TODO') LIMIT 10\"
- codesql search \"SELECT path FROM files WHERE has_symbol('Function', 'main') AND ext = 'rs'\"
- codesql search \"SELECT path FROM files WHERE has_symbol('Class', regex('.*Manager'))\"")]
    Search {
        /// The SQL query string to execute.
        query: String,
    },
}
