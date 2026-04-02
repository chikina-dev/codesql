use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    pub line_no: usize,
    pub line: String,
}

pub fn read_text(root: &Path, relative_path: &str) -> Result<Option<String>> {
    let absolute_path = root.join(relative_path);
    let bytes = fs::read(&absolute_path)
        .with_context(|| format!("failed to read {}", absolute_path.display()))?;
    match String::from_utf8(bytes) {
        Ok(content) => Ok(Some(content)),
        Err(_) => Ok(None),
    }
}

pub fn find_matches(content: &str, needle: &str) -> Vec<ContentMatch> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if line.contains(needle) {
                Some(ContentMatch {
                    line_no: index + 1,
                    line: line.to_owned(),
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::find_matches;

    #[test]
    fn find_matches_returns_matching_lines_with_numbers() {
        let matches = find_matches("one\nTODO: fix\nthree\n", "TODO");

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_no, 2);
        assert_eq!(matches[0].line, "TODO: fix");
    }
}
