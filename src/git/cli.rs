use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use tracing::debug;

use super::backend::GitBackend;
use super::model::*;
use super::parse::{diff, name_status, worktree};

pub struct GitCli;

impl GitCli {
    pub fn new() -> Self {
        Self
    }

    fn git_cmd(repo: &Path) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo);
        cmd.env("GIT_TERMINAL_PROMPT", "0");
        cmd.env("GIT_OPTIONAL_LOCKS", "0");
        cmd
    }

    fn run(cmd: &mut Command) -> Result<String> {
        debug!("Running: {:?}", cmd);
        let output = cmd.output().context("Failed to execute git command")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git command failed: {}", stderr.trim());
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn run_allow_empty(cmd: &mut Command) -> Result<String> {
        debug!("Running: {:?}", cmd);
        let output = cmd.output().context("Failed to execute git command")?;
        // Allow failure for some commands (e.g., diff returns 1 when there are changes)
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn diff_args(spec: &DiffSpec) -> Vec<String> {
        match spec {
            DiffSpec::Worktree(WorktreeMode::Unstaged) => vec!["diff".to_string()],
            DiffSpec::Worktree(WorktreeMode::Staged) => {
                vec!["diff".to_string(), "--cached".to_string()]
            }
            DiffSpec::Compare { base, target } => {
                let range = if let Some(t) = target {
                    format!("{}...{}", base, t)
                } else {
                    format!("{}...HEAD", base)
                };
                vec!["diff".to_string(), range]
            }
        }
    }
}

impl GitBackend for GitCli {
    fn repo_root(&self, path: &Path) -> Result<PathBuf> {
        let output = Self::run(Self::git_cmd(path).arg("rev-parse").arg("--show-toplevel"))?;
        Ok(PathBuf::from(output.trim()))
    }

    fn current_branch(&self, repo: &Path) -> Result<Option<String>> {
        let result = Self::run(
            Self::git_cmd(repo)
                .arg("symbolic-ref")
                .arg("--short")
                .arg("HEAD"),
        );
        match result {
            Ok(output) => {
                let branch = output.trim().to_string();
                if branch.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(branch))
                }
            }
            Err(_) => Ok(None), // detached or unborn
        }
    }

    fn head_ref(&self, repo: &Path) -> Result<Option<String>> {
        let result = Self::run(
            Self::git_cmd(repo)
                .arg("rev-parse")
                .arg("--short")
                .arg("HEAD"),
        );
        match result {
            Ok(output) => {
                let r = output.trim().to_string();
                if r.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(r))
                }
            }
            Err(_) => Ok(None),
        }
    }

    fn is_unborn(&self, repo: &Path) -> Result<bool> {
        let result = Self::run(Self::git_cmd(repo).arg("rev-parse").arg("HEAD"));
        Ok(result.is_err())
    }

    fn is_detached(&self, repo: &Path) -> Result<bool> {
        let result = Self::run(
            Self::git_cmd(repo)
                .arg("symbolic-ref")
                .arg("--quiet")
                .arg("HEAD"),
        );
        Ok(result.is_err())
    }

    fn changed_files(&self, repo: &Path, spec: &DiffSpec) -> Result<Vec<FileEntry>> {
        let mut args = Self::diff_args(spec);
        args.push("--name-status".to_string());
        args.push("--no-ext-diff".to_string());

        let mut cmd = Self::git_cmd(repo);
        for a in &args {
            cmd.arg(a);
        }
        let name_status_output = Self::run_allow_empty(&mut cmd)?;
        let mut entries = name_status::parse_name_status(&name_status_output);

        // Get numstat too
        let mut args2 = Self::diff_args(spec);
        args2.push("--numstat".to_string());
        args2.push("--no-ext-diff".to_string());
        let mut cmd2 = Self::git_cmd(repo);
        for a in &args2 {
            cmd2.arg(a);
        }
        let numstat_output = Self::run_allow_empty(&mut cmd2)?;
        name_status::merge_numstat(&mut entries, &numstat_output);

        Ok(entries)
    }

    fn file_diff(
        &self,
        repo: &Path,
        spec: &DiffSpec,
        file_path: &str,
        context_lines: u32,
    ) -> Result<FileDiff> {
        let mut args = Self::diff_args(spec);
        args.push("--patch".to_string());
        args.push(format!("--unified={}", context_lines));
        args.push("--no-ext-diff".to_string());
        args.push("--".to_string());
        args.push(file_path.to_string());

        let mut cmd = Self::git_cmd(repo);
        for a in &args {
            cmd.arg(a);
        }
        let output = Self::run_allow_empty(&mut cmd)?;
        Ok(diff::parse_file_diff(&output))
    }

    fn list_refs(&self, repo: &Path) -> Result<Vec<RefEntry>> {
        let mut refs = Vec::new();

        // Local branches
        let output = Self::run(
            Self::git_cmd(repo)
                .arg("for-each-ref")
                .arg("--format=%(refname:short)")
                .arg("refs/heads/"),
        );
        if let Ok(out) = output {
            for line in out.lines() {
                let name = line.trim();
                if !name.is_empty() {
                    refs.push(RefEntry {
                        name: name.to_string(),
                        kind: RefKind::Branch,
                        is_remote: false,
                    });
                }
            }
        }

        // Remote branches
        let output = Self::run(
            Self::git_cmd(repo)
                .arg("for-each-ref")
                .arg("--format=%(refname:short)")
                .arg("refs/remotes/"),
        );
        if let Ok(out) = output {
            for line in out.lines() {
                let name = line.trim();
                if !name.is_empty() && !name.ends_with("/HEAD") {
                    refs.push(RefEntry {
                        name: name.to_string(),
                        kind: RefKind::Branch,
                        is_remote: true,
                    });
                }
            }
        }

        // Tags
        let output = Self::run(
            Self::git_cmd(repo)
                .arg("for-each-ref")
                .arg("--format=%(refname:short)")
                .arg("refs/tags/"),
        );
        if let Ok(out) = output {
            for line in out.lines() {
                let name = line.trim();
                if !name.is_empty() {
                    refs.push(RefEntry {
                        name: name.to_string(),
                        kind: RefKind::Tag,
                        is_remote: false,
                    });
                }
            }
        }

        Ok(refs)
    }

    fn list_worktrees(&self, repo: &Path) -> Result<Vec<WorktreeEntry>> {
        let output = Self::run(
            Self::git_cmd(repo)
                .arg("worktree")
                .arg("list")
                .arg("--porcelain"),
        )?;
        Ok(worktree::parse_worktree_list(&output))
    }

    fn merge_base(&self, repo: &Path, ref1: &str, ref2: &str) -> Result<String> {
        let output = Self::run(Self::git_cmd(repo).arg("merge-base").arg(ref1).arg(ref2))?;
        Ok(output.trim().to_string())
    }

    fn find_default_base(&self, repo: &Path, candidates: &[String]) -> Option<String> {
        for candidate in candidates {
            let result = Self::run(
                Self::git_cmd(repo)
                    .arg("rev-parse")
                    .arg("--verify")
                    .arg(format!("{}^{{commit}}", candidate)),
            );
            if result.is_ok() {
                return Some(candidate.clone());
            }
        }
        None
    }
}

impl Default for GitCli {
    fn default() -> Self {
        Self::new()
    }
}
