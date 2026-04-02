use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result};

use crate::constants::is_internal_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSnapshot {
    pub repo_root: String,
    pub git_dir: String,
    pub head_ref: String,
    pub head_commit: Option<String>,
    pub branch_name: Option<String>,
    pub is_dirty: bool,
    pub changed_paths: Vec<String>,
}

pub fn collect_snapshot(root: &Path) -> Result<Option<GitSnapshot>> {
    if !is_git_repository(root)? {
        return Ok(None);
    }

    let repo_root = git_output(root, &["rev-parse", "--show-toplevel"])?;
    let git_dir = git_output(root, &["rev-parse", "--git-dir"])?;
    let head_ref = resolve_head_ref(root)?;
    let head_commit = resolve_head_commit(root)?;
    let changed_paths = git_status_paths(root)?;
    let is_dirty = !changed_paths.is_empty();
    let branch_name = if head_ref == "HEAD" {
        None
    } else {
        Some(head_ref.clone())
    };

    Ok(Some(GitSnapshot {
        repo_root,
        git_dir: absolutize_git_dir(root, &git_dir).display().to_string(),
        head_ref,
        head_commit,
        branch_name,
        is_dirty,
        changed_paths,
    }))
}

fn is_git_repository(root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(root)
        .output();

    match output {
        Ok(output) if output.status.success() => Ok(true),
        Ok(output) => {
            if root.join(".git").exists() {
                anyhow::bail!(
                    "git command failed: git rev-parse --is-inside-work-tree\n{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("failed to execute git"),
    }
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = git_command(root, args)?;
    if !output.status.success() {
        anyhow::bail!(
            "git command failed: git {}\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    git_stdout(output)
}

fn git_command(root: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))
}

fn git_stdout(output: Output) -> Result<String> {
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn resolve_head_ref(root: &Path) -> Result<String> {
    let args = ["symbolic-ref", "--quiet", "--short", "HEAD"];
    let output = git_command(root, &args)?;
    if output.status.success() {
        return git_stdout(output);
    }
    if output.status.code() == Some(1) {
        return git_output(root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    }

    anyhow::bail!(
        "git command failed: git {}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn resolve_head_commit(root: &Path) -> Result<Option<String>> {
    let args = ["rev-parse", "HEAD"];
    let output = git_command(root, &args)?;
    if output.status.success() {
        return Ok(Some(git_stdout(output)?));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_unborn_head_failure(&stderr) {
        return Ok(None);
    }

    anyhow::bail!("git command failed: git {}\n{}", args.join(" "), stderr);
}

fn is_unborn_head_failure(stderr: &str) -> bool {
    stderr.contains("ambiguous argument 'HEAD'")
        && stderr.contains("unknown revision or path not in the working tree")
}

fn git_status_paths(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "-z", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .context("failed to execute git status")?;
    if !output.status.success() {
        anyhow::bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let mut entries = output.stdout.split(|byte| *byte == b'\0');
    let mut changed_paths = Vec::new();

    while let Some(entry) = entries.next() {
        if entry.is_empty() {
            continue;
        }

        if entry.len() < 4 {
            anyhow::bail!("unexpected git status entry: {:?}", entry);
        }

        let status = &entry[..2];
        let path = String::from_utf8(entry[3..].to_vec())?;
        if !path.is_empty() && !is_internal_path(&path) {
            changed_paths.push(path);
        }

        if is_rename_or_copy(status) {
            let original_path = entries
                .next()
                .context("missing source path for git rename/copy entry")?;
            if original_path.is_empty() {
                anyhow::bail!("unexpected empty source path for git rename/copy entry");
            }
        }
    }

    Ok(changed_paths)
}

fn is_rename_or_copy(status: &[u8]) -> bool {
    matches!(status, [x, y] if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C'))
}

fn absolutize_git_dir(root: &Path, git_dir: &str) -> PathBuf {
    let path = Path::new(git_dir);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    root.join(path)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::absolutize_git_dir;

    #[test]
    fn resolves_relative_git_dir_against_workspace() {
        let resolved = absolutize_git_dir(Path::new("/tmp/workspace"), ".git");
        assert_eq!(resolved, Path::new("/tmp/workspace/.git"));
    }
}
