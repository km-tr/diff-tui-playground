use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use tracing::debug;

use super::backend::GitBackend;
use super::model::*;
use super::parse::{diff, name_status, worktree};

pub struct GitCli {
    max_bytes: usize,
}

impl GitCli {
    pub fn new() -> Self {
        Self {
            max_bytes: 5_000_000,
        }
    }

    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self { max_bytes }
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
        match String::from_utf8(output.stdout) {
            Ok(s) => Ok(s),
            Err(e) => Ok(String::from_utf8_lossy(e.as_bytes()).into_owned()),
        }
    }

    fn run_allow_empty(cmd: &mut Command) -> Result<String> {
        debug!("Running: {:?}", cmd);
        let output = cmd.output().context("Failed to execute git command")?;
        // Some git porcelain commands exit with 1 for non-error conditions
        // (e.g., `git diff --exit-code`, `git grep` with no matches).
        // Exit codes >= 128 are fatal errors (e.g., invalid revision, bad range).
        let code = output.status.code().unwrap_or(-1);
        if !(0..128).contains(&code) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git command failed (exit {}): {}", code, stderr.trim());
        }
        // Log stderr on non-zero exit for diagnostic purposes (exit 1 is
        // often benign, e.g. diff --exit-code, but stderr can still help).
        if code != 0 && !output.stderr.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!("git exited {} with stderr: {}", code, stderr.trim());
        }
        // Prefer lossless UTF-8 conversion; fall back to lossy for non-UTF-8
        // paths (rare on modern systems). With -z output, raw bytes are used
        // and lossy conversion would replace invalid bytes with U+FFFD,
        // potentially breaking path identity for later git commands.
        match String::from_utf8(output.stdout) {
            Ok(s) => Ok(s),
            Err(e) => {
                debug!("Non-UTF-8 git output; using lossy conversion");
                Ok(String::from_utf8_lossy(e.as_bytes()).into_owned())
            }
        }
    }

    /// Run a command and read at most `limit` bytes from stdout, then kill
    /// the process to avoid buffering arbitrarily large output into memory.
    /// Returns (output, was_truncated).
    fn run_with_limit(cmd: &mut Command, limit: usize) -> Result<(String, bool)> {
        debug!("Running (limited to {} bytes): {:?}", limit, cmd);
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to execute git command")?;

        let mut buf = vec![0u8; limit + 1];
        let stdout = child.stdout.take().unwrap();
        let mut reader = std::io::BufReader::new(stdout);
        let mut total = 0;
        loop {
            let n = reader
                .read(&mut buf[total..])
                .context("Failed to read git stdout")?;
            if n == 0 {
                break;
            }
            total += n;
            if total > limit {
                break;
            }
        }

        let was_truncated = total > limit;
        let keep = if was_truncated { limit } else { total };

        // Kill the process early if we truncated to avoid wasting resources
        if was_truncated {
            let _ = child.kill();
        }
        let _ = child.wait();

        // Find a valid UTF-8 boundary
        let mut end = keep;
        while end > 0 && std::str::from_utf8(&buf[..end]).is_err() {
            end -= 1;
        }
        let output = String::from_utf8_lossy(&buf[..end]).into_owned();
        Ok((output, was_truncated))
    }

    fn diff_args(spec: &DiffSpec) -> Vec<String> {
        // Always enable rename detection (-M) so rename handling is consistent
        // regardless of the user's diff.renames config setting.
        match spec {
            DiffSpec::Worktree(WorktreeMode::Unstaged) => {
                vec!["diff".to_string(), "-M".to_string()]
            }
            DiffSpec::Worktree(WorktreeMode::Staged) => {
                vec!["diff".to_string(), "--cached".to_string(), "-M".to_string()]
            }
            DiffSpec::Compare { base, target } => {
                let range = if let Some(t) = target {
                    format!("{}...{}", base, t)
                } else {
                    format!("{}...HEAD", base)
                };
                vec!["diff".to_string(), "-M".to_string(), range]
            }
        }
    }
}

impl GitBackend for GitCli {
    fn repo_root(&self, path: &Path) -> Result<PathBuf> {
        let output = Self::run(Self::git_cmd(path).arg("rev-parse").arg("--show-toplevel"))?;
        Ok(PathBuf::from(output.trim()))
    }

    fn git_common_dir(&self, repo: &Path) -> Result<PathBuf> {
        let output = Self::run(Self::git_cmd(repo).arg("rev-parse").arg("--git-common-dir"))?;
        let raw = PathBuf::from(output.trim());
        // --git-common-dir may return a relative path; resolve against the repo
        let absolute = if raw.is_relative() {
            repo.join(&raw)
        } else {
            raw
        };
        // Canonicalize to resolve `.git/../.git` etc.
        absolute.canonicalize().with_context(|| {
            format!(
                "Failed to canonicalize git common dir: {}",
                absolute.display()
            )
        })
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
        // Use -z for NUL-delimited output to avoid C-quoting of non-ASCII paths
        args.push("-z".to_string());

        let mut cmd = Self::git_cmd(repo);
        cmd.args(&args);
        let name_status_output = Self::run_allow_empty(&mut cmd)?;
        let mut entries = name_status::parse_name_status(&name_status_output);

        // Get numstat too.
        // Note: two separate git invocations create a small TOCTOU window where
        // the working tree could change between commands. This is an accepted
        // trade-off since git does not support combining --name-status and
        // --numstat in a single invocation.
        let mut args2 = Self::diff_args(spec);
        args2.push("--numstat".to_string());
        args2.push("--no-ext-diff".to_string());
        args2.push("-z".to_string());
        let mut cmd2 = Self::git_cmd(repo);
        cmd2.args(&args2);
        let numstat_output = Self::run_allow_empty(&mut cmd2)?;
        name_status::merge_numstat(&mut entries, &numstat_output);

        // In Worktree Unstaged mode, also list untracked files so the user
        // can see new files that haven't been staged yet.
        if matches!(spec, DiffSpec::Worktree(WorktreeMode::Unstaged)) {
            let mut cmd3 = Self::git_cmd(repo);
            cmd3.args(["ls-files", "--others", "--exclude-standard", "-z"]);
            let untracked_output = Self::run_allow_empty(&mut cmd3)?;
            for path in untracked_output.split('\0') {
                if !path.is_empty() {
                    entries.push(FileEntry {
                        path: path.to_string(),
                        old_path: None,
                        status: FileStatus::Untracked,
                        additions: 0,
                        deletions: 0,
                    });
                }
            }
        }

        Ok(entries)
    }

    fn file_diff(
        &self,
        repo: &Path,
        spec: &DiffSpec,
        file_path: &str,
        old_path: Option<&str>,
        context_lines: u32,
    ) -> Result<FileDiff> {
        let mut args = Self::diff_args(spec);
        args.push("--patch".to_string());
        args.push(format!("--unified={}", context_lines));
        args.push("--no-ext-diff".to_string());
        args.push("--".to_string());
        // For renames/copies, pass both old and new paths so git emits
        // proper rename/copy metadata instead of an add+delete pair.
        if let Some(old) = old_path {
            args.push(old.to_string());
        }
        args.push(file_path.to_string());

        let mut cmd = Self::git_cmd(repo);
        cmd.args(&args);

        // Stream stdout up to max_bytes to avoid buffering huge diffs into memory.
        let (output, was_truncated) = if self.max_bytes > 0 {
            Self::run_with_limit(&mut cmd, self.max_bytes)?
        } else {
            let full = Self::run_allow_empty(&mut cmd)?;
            (full, false)
        };

        // If truncated, trim to last complete line so the parser never
        // receives a partial line mid-hunk.
        let truncated = if was_truncated {
            if let Some(nl) = output.rfind('\n') {
                &output[..nl + 1]
            } else {
                &output
            }
        } else {
            &output
        };
        let mut parsed = diff::parse_file_diff(truncated);
        parsed.is_truncated = was_truncated;
        Ok(parsed)
    }

    fn list_refs(&self, repo: &Path) -> Result<Vec<RefEntry>> {
        // Single for-each-ref call with all ref patterns; include full refname
        // to classify each ref as local branch, remote, or tag.
        let output = Self::run(
            Self::git_cmd(repo)
                .arg("for-each-ref")
                .arg("--format=%(refname)\t%(refname:short)")
                .arg("refs/heads/")
                .arg("refs/remotes/")
                .arg("refs/tags/"),
        )?;

        let mut refs = Vec::new();
        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (full, short) = match line.split_once('\t') {
                Some(pair) => pair,
                None => continue,
            };
            if full.starts_with("refs/heads/") {
                refs.push(RefEntry {
                    name: short.to_string(),
                    kind: RefKind::Branch,
                    is_remote: false,
                });
            } else if full.starts_with("refs/remotes/") {
                if !short.ends_with("/HEAD") {
                    refs.push(RefEntry {
                        name: short.to_string(),
                        kind: RefKind::Branch,
                        is_remote: true,
                    });
                }
            } else if full.starts_with("refs/tags/") {
                refs.push(RefEntry {
                    name: short.to_string(),
                    kind: RefKind::Tag,
                    is_remote: false,
                });
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
