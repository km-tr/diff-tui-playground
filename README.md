# git-review-tui

A terminal UI tool for reviewing Git diffs. Built for speed, resilience, and daily use.

## What it does

- **Two-pane diff viewer**: file list on the left, unified diff on the right
- **DiffSpec switching**: toggle between Worktree (unstaged/staged) and Compare (base...target) modes
- **Selectors with fuzzy filter**: switch base ref, target ref, worktree, or repository context with a single keystroke
- **Pane discovery**: detects repos from tmux and WezTerm panes for quick context switching
- **Auto-reload**: watches the filesystem and updates on changes (debounced)
- **Copy/export**: yank hunks or file diffs to clipboard, or export to file
- **Resilient**: handles unborn HEAD, detached HEAD, binary files, renames, submodules, and huge diffs without crashing

## What it doesn't do

- No destructive Git operations (no staging, reverting, or committing)
- No GitHub/GitLab integration
- No pane discovery for terminals other than tmux and WezTerm

## Installation

```sh
cargo install --path .
```

Or build from source:

```sh
git clone <repo-url>
cd git-review-tui
cargo build --release
# Binary is at target/release/git-review-tui
```

## Usage

```sh
# Review current directory
git-review-tui

# Review a specific repo
git-review-tui -C /path/to/repo

# With a custom config
git-review-tui --config path/to/config.toml

# Enable debug logging
RUST_LOG=debug git-review-tui --log-file /tmp/grt.log
```

## Key Bindings

### Navigation

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |
| `PgUp` | Page up |
| `PgDn` | Page down |
| `Tab` | Switch focus (files <-> diff) |

### Diff

| Key | Action |
|-----|--------|
| `n` | Next hunk |
| `p` | Previous hunk |
| `f` | Search in diff |

### Mode

| Key | Action |
|-----|--------|
| `m` | Toggle Worktree <-> Compare |
| `s` | Toggle unstaged <-> staged (Worktree mode) |

### Selectors

| Key | Action |
|-----|--------|
| `b` | Base ref selector (Compare mode) |
| `t` | Target ref selector (Compare mode) |
| `w` | Worktree selector |
| `c` | Context selector (repo/worktree switch) |

### Output

| Key | Action |
|-----|--------|
| `y` | Copy current hunk to clipboard |
| `Y` | Copy file diff to clipboard |
| `o` | Export diff |

### Other

| Key | Action |
|-----|--------|
| `r` | Reload |
| `?` | Help |
| `q` / `Esc` | Quit (or close overlay) |

To get the current key bindings from the binary:

```sh
git-review-tui --print-keys
```

## Modes

### Worktree mode

Shows changes in your working tree. Toggle between unstaged (`git diff`) and staged (`git diff --cached`) with `s`.

### Compare mode

Shows changes between two refs. Default is `<detected-base>...HEAD`. Switch base with `b`, target with `t`.

The base ref is auto-detected from: `main`, `master`, `origin/main`, `origin/master` (configurable).

## Pane Discovery

When you press `c` (context selector), git-review-tui scans for repos open in other terminal panes:

- **tmux**: reads `tmux list-panes -a` to get pane working directories
- **WezTerm**: reads `wezterm cli list --format json` to get pane working directories

Non-local panes (e.g., SSH sessions in WezTerm) are excluded. Discovery mode is configurable: `auto`, `tmux`, `wezterm`, or `off`.

## Configuration

Default config location: `~/.config/git-review-tui/config.toml`

To see the default configuration:

```sh
git-review-tui --print-default-config
```

### Config options

```toml
# Candidate refs for auto-detecting the base branch
default_base = ["main", "master", "origin/main", "origin/master"]

# Number of context lines in unified diff
unified_context = 3

# Maximum diff lines before truncation
truncate_max_lines = 10000

# Maximum diff size in bytes before truncation
truncate_max_bytes = 5000000

# Maximum number of recent contexts to remember
max_recent_contexts = 20

# Pane discovery mode: "auto", "tmux", "wezterm", "off"
discovery = "auto"

[watch]
enabled = true
debounce_ms = 300
poll_interval_secs = 30
```

## Persistence

State is saved to `~/.local/share/git-review-tui/state.json`:

- Recent repository contexts (up to `max_recent_contexts`)
- Last mode (worktree/compare)
- Last worktree mode (unstaged/staged)
- Last base ref

## Troubleshooting

**"Not a git repository"**: Run from inside a git repo, or use `-C /path/to/repo`.

**Clipboard not working**: Falls back to displaying the diff text. Use `o` to export to a file instead.

**WezTerm CLI not connecting**: Ensure `wezterm cli` works from your terminal. The WezTerm Unix socket must be accessible.

**tmux not found**: Set `discovery = "off"` or `discovery = "wezterm"` in config to skip tmux detection.

**Large diff is slow**: The viewer truncates diffs beyond `truncate_max_lines`. Use `o` to export the full diff to a file.

**Logging for debugging**: Set `RUST_LOG=debug` and optionally `--log-file /tmp/grt.log`.

## Development

```sh
# Format
cargo fmt

# Lint
cargo clippy --all-targets

# Test
cargo test

# Build release
cargo build --release
```

## License

MIT
