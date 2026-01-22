# octorus

<p align="center">
  <img src="assets/banner.png" alt="octorus banner" width="600">
</p>

[![Crates.io](https://img.shields.io/crates/v/octorus.svg)](https://crates.io/crates/octorus)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A TUI tool for GitHub PR review with Vim-style keybindings.

## Features

- Browse changed files in a PR
- View diffs with syntax highlighting
- Add inline comments on specific lines
- Add code suggestions
- View and navigate review comments with jump-to-line
- Submit reviews (Approve / Request Changes / Comment)
- Fast startup with intelligent caching
- Configurable keybindings and editor

## Requirements

- [GitHub CLI (gh)](https://cli.github.com/) - Must be installed and authenticated
- Rust 1.70+ (for building from source)

## Installation

```bash
cargo install octorus
```

Or build from source:

```bash
git clone https://github.com/ushironoko/octorus.git
cd octorus
cargo build --release
cp target/release/or ~/.local/bin/
```

## Usage

```bash
or --repo owner/repo --pr 123
```

### Options

| Option | Description |
|--------|-------------|
| `-r, --repo <REPO>` | Repository name (e.g., "owner/repo") |
| `-p, --pr <PR>` | Pull request number |
| `--refresh` | Force refresh, ignore cache |
| `--cache-ttl <SECS>` | Cache TTL in seconds (default: 300) |

### Keybindings

#### File List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Open diff view |
| `a` | Approve PR |
| `r` | Request changes |
| `c` | Comment only |
| `C` | View review comments |
| `?` | Toggle help |
| `q` | Quit |

#### Diff View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Ctrl-d` | Page down |
| `Ctrl-u` | Page up |
| `c` | Add comment at line |
| `s` | Add suggestion at line |
| `q` / `Esc` | Back to file list |

#### Comment List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Jump to file/line |
| `q` / `Esc` | Back to file list |

## Configuration

Create `~/.config/octorus/config.toml`:

```toml
# Editor to use for writing comments
editor = "hx"

[keybindings]
approve = 'a'
request_changes = 'r'
comment = 'c'
```

## License

MIT
