# codesql

`codesql` is an AI-friendly, SQL-like code search CLI tool. 
It allows you (or your AI agents) to index a codebase and perform complex, blazing-fast searches over files and their contents using SQL queries.
I created this tool because I was tired of AI agents repeatedly running grep or rg over a codebase, and I thought indexing the codebase would make the search faster.

## Workflow

To use `codesql`, follow this basic workflow:

1. **Initialize** the workspace:
    ```sh
    codesql init
    ```
    This creates a `.codesql` directory to store the local database index.

2. **Save** the current state of the workspace into the index:
    ```sh
    codesql save
    ```
    *Note: Run this command after adding, modifying, or deleting files to keep the index up to date.*

3. **Search** utilizing SQL-like syntax:
    ```sh
    codesql search "SELECT path FROM files WHERE ext = 'rs'"
    ```

## Search Syntax

The `search` query targets a virtual table called `files`.

### Available `SELECT` Fields
- `path`: The relative path to the file.
- `line_no`: The line number (Wait: this requires using `contains` or `regex` on `content` in the `WHERE` clause).
- `line`: The exact line of text matched (requires using `contains` or `regex` on `content` in the `WHERE` clause).

### Available `WHERE` Fields
- `path` (String): The file path.
- `ext` (String): The file extension.
- `language` (String): The automatically detected language (e.g., `'rust'`, `'typescript'`, `'javascript'`).

### Available Functions
- `contains(content, 'needle')`: Matches if the file content contains the exact string `'needle'`.
- `regex(content, 'pattern')`: Matches if the file content matches the regular expression `'pattern'`.
- `glob(field, 'pattern')`: Evaluates true if the given `field` matches the glob `'pattern'` (e.g., `glob(path, 'src/**/*.rs')`).
- `has_symbol(kind, name)`: Checks if the file contains a specific symbol. 
  - `kind` can be `'Function'`, `'Class'`, `'Struct'`, etc.
  - `name` can be exact strings, `regex('...')`, or `glob('...')`.

## Example Queries

- **Find all Rust files:**
  ```sql
  SELECT path FROM files WHERE ext = 'rs'
  ```

- **Find files containing a specific string, outputting line content and numbers:**
  ```sql
  SELECT path, line_no, line FROM files WHERE contains(content, 'TODO') LIMIT 10
  ```

- **Find all files that define a specific function:**
  ```sql
  SELECT path FROM files WHERE has_symbol('Function', 'main') AND ext = 'rs'
  ```

- **Find any class that handles an "API":**
  ```sql
  SELECT path FROM files WHERE has_symbol('Class', regex('.*API.*'))
  ```

## Database Maintenance

If search performance degrades after a large number of file modifications, you can compact and optimize the index with:
```sh
codesql optimize
```
