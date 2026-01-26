# octorus

<p align="center">
  <img src="assets/banner.png" alt="octorus banner" width="600">
</p>

[![Crates.io](https://img.shields.io/crates/v/octorus.svg)](https://crates.io/crates/octorus)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

[日本語](./README-jp.md)

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
- **AI Rally**: Automated PR review and fix cycle using AI agents

## Requirements

- [GitHub CLI (gh)](https://cli.github.com/) - Must be installed and authenticated
- Rust 1.70+ (for building from source)
- **For AI Rally feature** (optional, choose one or both):
  - [Claude Code](https://claude.ai/code) - Anthropic's CLI tool
  - [OpenAI Codex CLI](https://github.com/openai/codex) - OpenAI's CLI tool

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

### Initialize Configuration

Create default config files and prompt templates:

```bash
or init          # Create config files (skip if exists)
or init --force  # Overwrite existing files
```

This creates:
- `~/.config/octorus/config.toml` - Main configuration file
- `~/.config/octorus/prompts/` - Prompt template directory
  - `reviewer.md` - Reviewer agent prompt template
  - `reviewee.md` - Reviewee agent prompt template
  - `rereview.md` - Re-review prompt template

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
| `R` | Force refresh (discard cache) |
| `A` | Start AI Rally |
| `?` | Toggle help |
| `q` | Quit |

#### Diff View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `n` | Jump to next comment |
| `N` | Jump to previous comment |
| `Ctrl-d` | Page down |
| `Ctrl-u` | Page up |
| `c` | Add comment at line |
| `s` | Add suggestion at line |
| `q` / `Esc` | Back to file list |

**Note**: Lines with existing comments are marked with `●`. When you select a commented line, the comment content is displayed in a panel below the diff.

#### Comment List View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Enter` | Jump to file/line |
| `q` / `Esc` | Back to file list |

## Configuration

Run `or init` to create default config files, or create `~/.config/octorus/config.toml` manually:

```toml
# Editor to use for writing comments
editor = "vi"

[diff]
# Syntax highlighting theme for diff view
theme = "base16-ocean.dark"

[keybindings]
approve = 'a'
request_changes = 'r'
comment = 'c'
suggestion = 's'

[ai]
# AI agent to use for reviewer/reviewee
# Supported: "claude" (Claude Code), "codex" (OpenAI Codex CLI)
reviewer = "claude"
reviewee = "claude"

# Maximum iterations before stopping
max_iterations = 10

# Timeout per agent execution (seconds)
timeout_secs = 600

# Custom prompt directory (default: ~/.config/octorus/prompts/)
# prompt_dir = "/custom/path/to/prompts"

# Additional tools for reviewer (Claude only)
# Use Claude Code's --allowedTools format
# reviewer_additional_tools = []

# Additional tools for reviewee (Claude only)
# Examples: "Skill", "WebFetch", "WebSearch", "Bash(git push:*)"
# reviewee_additional_tools = ["Skill", "Bash(git push:*)"]
```

### Customizing Prompt Templates

AI Rally uses customizable prompt templates. Run `or init` to generate default templates, then edit them as needed:

```
~/.config/octorus/prompts/
├── reviewer.md    # Prompt for the reviewer agent
├── reviewee.md    # Prompt for the reviewee agent
└── rereview.md    # Prompt for re-review iterations
```

Templates support variable substitution with `{{variable}}` syntax:

| Variable | Description | Available In |
|----------|-------------|--------------|
| `{{repo}}` | Repository name (e.g., "owner/repo") | All |
| `{{pr_number}}` | Pull request number | All |
| `{{pr_title}}` | Pull request title | All |
| `{{pr_body}}` | Pull request description | reviewer |
| `{{diff}}` | PR diff content | reviewer |
| `{{iteration}}` | Current iteration number | All |
| `{{review_summary}}` | Summary from reviewer | reviewee |
| `{{review_action}}` | Review action (Approve/RequestChanges/Comment) | reviewee |
| `{{review_comments}}` | List of review comments | reviewee |
| `{{blocking_issues}}` | List of blocking issues | reviewee |
| `{{external_comments}}` | Comments from external tools | reviewee |
| `{{changes_summary}}` | Summary of changes made | rereview |
| `{{updated_diff}}` | Updated diff after fixes | rereview |

## AI Rally

AI Rally is an automated PR review and fix cycle that uses two AI agents:

- **Reviewer**: Analyzes the PR diff and provides review feedback
- **Reviewee**: Fixes issues based on the review feedback and commits changes

### How it works

```
┌─────────────────┐
│  Start Rally    │  Press 'A' in File List View
└────────┬────────┘
         ▼
┌─────────────────┐
│    Reviewer     │  AI reviews the diff
│ (Claude/Codex)  │  → Posts comments to PR
└────────┬────────┘
         │
    ┌────┴────┐
    │ Approve?│
    └────┬────┘
     No  │  Yes
         │   └──→ Done ✓
         ▼
┌─────────────────┐
│    Reviewee     │  AI fixes issues
│ (Claude/Codex)  │  → Commits changes locally
└────────┬────────┘
         │
         ▼
    Next Iteration
```

### Features

- **PR Integration**: Review comments are automatically posted to the PR
- **External Bot Support**: Collects feedback from Copilot, CodeRabbit, and other bots
- **Safe Operations**: Dangerous git operations (`--force`, `reset --hard`) are prohibited
- **Session Persistence**: Rally state is saved and can be resumed
- **Interactive Flow**: When the AI agent needs clarification or permission, you can respond interactively
- **Background Execution**: Press `b` to run rally in background while continuing to browse files

### Recommended Configuration

Codex uses sandbox mode and cannot control tool permissions at a fine-grained level.
For maximum security, we recommend:

| Role | Recommended | Reason |
|------|-------------|--------|
| Reviewer | Codex or Claude | Read-only operations, both are safe |
| Reviewee | **Claude** | Allows fine-grained tool control via allowedTools |

Example configuration for secure setup:

```toml
[ai]
reviewer = "codex"   # Safe: read-only sandbox
reviewee = "claude"  # Recommended: fine-grained tool control
reviewee_additional_tools = ["Skill"]  # Add only what you need
```

**Note**: If you use Codex as reviewee, it runs in `--full-auto` mode with
workspace write access and no tool restrictions.

### Tool Permissions

#### Default Allowed Tools

**Reviewer** (read-only operations):

| Tool | Description |
|------|-------------|
| Read, Glob, Grep | File reading and searching |
| `gh pr view/diff/checks` | View PR information |
| `gh api --method GET` | GitHub API (GET only) |

**Reviewee** (code modification):

| Category | Commands |
|----------|----------|
| File | Read, Edit, Write, Glob, Grep |
| Git | status, diff, add, commit, log, show, branch, switch, stash |
| GitHub CLI | pr view, pr diff, pr checks, api GET |
| Cargo | build, test, check, clippy, fmt, run |
| npm/pnpm/bun | install, test, run |

#### Additional Tools (Claude only)

Additional tools can be enabled via config using Claude Code's `--allowedTools` format:

| Example | Description |
|---------|-------------|
| `"Skill"` | Execute Claude Code skills |
| `"WebFetch"` | Fetch URL content |
| `"WebSearch"` | Web search |
| `"Bash(git push:*)"` | git push to remote |
| `"Bash(gh api --method POST:*)"` | GitHub API POST requests |

```toml
[ai]
reviewee_additional_tools = ["Skill", "Bash(git push:*)"]
```

**Breaking Change (v0.2.0)**: `git push` is now disabled by default.
To enable, add `"Bash(git push:*)"` to `reviewee_additional_tools`.

### Keybindings (AI Rally View)

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down in log |
| `k` / `↑` | Move up in log |
| `Enter` | Show log detail |
| `g` | Jump to top |
| `G` | Jump to bottom |
| `b` | Run in background (return to file list) |
| `y` | Grant permission / Enter clarification |
| `n` | Deny permission / Skip clarification |
| `r` | Retry (on error) |
| `q` / `Esc` | Abort and exit rally |

## License

MIT
