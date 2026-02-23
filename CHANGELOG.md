# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - 2026-02-21

### Added
- Two-pane TUI layout (file list + diff viewer) with header and footer
- Diff display with unified diff format, syntax-aware coloring
- Hunk navigation (next/prev) and line/page scrolling
- DiffSpec switching: Worktree (unstaged/staged) and Compare (base...target)
- Base ref selector with fuzzy filtering
- Target ref selector
- Worktree selector with branch display
- Context selector integrating current, worktrees, recent, and pane discovery
- Pane discovery for tmux and WezTerm (auto-detect or manual config)
- File watcher with debounced auto-reload
- Manual reload
- Copy hunk / file diff to clipboard
- Export diff to file
- In-diff search with match navigation
- Help overlay with all key bindings
- Toast notifications for feedback
- Error display (non-fatal, shown in UI)
- Persistent state: recent contexts, last mode/base
- Configuration via TOML (default base refs, context lines, watch settings, discovery mode, truncation limits)
- `--print-default-config` and `--print-keys` CLI flags for README sync
- Structured logging via `RUST_LOG` / `--log-file`
- Panic hook that restores terminal before crash output
- Virtual scrolling for large diffs with truncation safety
- Generation-based stale update prevention
