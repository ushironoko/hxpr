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
- Split view with file list and diff preview side by side
- View diffs with syntax highlighting (Markdown rich display mode with `M` key)
- Add inline comments on specific lines
- Add code suggestions
- View and navigate review comments with jump-to-line
- Submit reviews (Approve / Request Changes / Comment)
- Fast startup with intelligent caching
- **Local Diff Mode**: Preview local `git diff HEAD` in real time with file watcher — no PR required
- Toggle between PR mode and Local mode on the fly (`L` key)
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
# 1. Initialize config (recommended for AI Rally)
or init

# 2. Open PR list for current repository (auto-detected from git remote)
or

# 3. Open specific PR
or --repo owner/repo --pr 123

# 4. Start AI Rally (select PR from list, then auto-start)
or --ai-rally

# 5. Preview local working tree diff in real time
or --local
```

### Options

| Option | Description |
|--------|-------------|
| `-r, --repo <REPO>` | Repository name (e.g., "owner/repo") |
| `-p, --pr <PR>` | Pull request number |
| `--ai-rally` | Start AI Rally mode directly |
| `--working-dir <DIR>` | Working directory for AI agents (default: current directory) |
| `--local` | Show local git diff against current `HEAD` (no GitHub PR fetch) |
| `--auto-focus` | In local mode, automatically focus the changed file when diff updates |

### Subcommands

| Subcommand | Description |
|------------|-------------|
| `or init` | Initialize configuration files and prompt templates |
| `or init --force` | Overwrite existing configuration files |
| `or clean` | Remove AI Rally session data |

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
| `Enter` / `→` / `l` | Open split view |
| `a` | Approve PR |
| `r` | Request changes |
| `c` | Comment only |
| `C` | View review comments |
| `R` | Force refresh (discard cache) |
| `A` | Start AI Rally |
| `L` | Toggle local diff mode |
| `F` | Toggle auto-focus (local mode) |
| `?` | Toggle help |
| `q` | Quit |

#### Split View

The split view shows the file list (left, 35%) and a diff preview (right, 65%). The focused pane is highlighted with a yellow border.

**File List Focus:**

| Key | Action |
|-----|--------|
| `j` / `↓` | Move file selection (diff follows) |
| `k` / `↑` | Move file selection (diff follows) |
| `Enter` / `→` / `l` | Focus diff pane |
| `←` / `h` / `q` | Back to file list |

**Diff Focus:**

| Key | Action |
|-----|--------|
| `j` / `↓` | Scroll diff |
| `k` / `↑` | Scroll diff |
| `gd` | Go to definition |
| `gf` | Open file in $EDITOR |
| `gg` / `G` | Jump to first/last line |
| `Ctrl-o` | Jump back |
| `Ctrl-d` | Page down |
| `Ctrl-u` | Page up |
| `n` | Jump to next comment |
| `N` | Jump to previous comment |
| `Enter` | Open comment panel |
| `Tab` / `→` / `l` | Open fullscreen diff view |
| `←` / `h` | Focus file list |
| `q` | Back to file list |

#### Diff View

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `gd` | Go to definition |
| `gf` | Open file in $EDITOR |
| `gg` / `G` | Jump to first/last line |
| `Ctrl-o` | Jump back |
| `n` | Jump to next comment |
| `N` | Jump to previous comment |
| `Ctrl-d` | Page down |
| `Ctrl-u` | Page up |
| `M` | Toggle Markdown rich display |
| `Enter` | Open comment panel |
| `←` / `h` / `q` / `Esc` | Back to previous view |

**Note**: Lines with existing comments are marked with `●`. When you select a commented line, the comment content is displayed in a panel below the diff.

**Comment Panel (when focused):**

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll panel |
| `c` | Add comment |
| `s` | Add suggestion |
| `r` | Reply to comment |
| `Tab` / `Shift-Tab` | Select reply target |
| `n` / `N` | Jump to next/prev comment |
| `Esc` / `q` | Close panel |

#### Input Mode (Comment/Suggestion/Reply)

When adding a comment, suggestion, or reply, you enter the built-in text input mode:

| Key | Action |
|-----|--------|
| `Ctrl+S` | Submit |
| `Esc` | Cancel |

Multi-line input is supported. Press `Enter` to insert a newline.

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
# Editor for writing review body.
# Resolved in order: this value → $VISUAL → $EDITOR → vi
# Supports arguments: editor = "code --wait"
# editor = "vim"

[diff]
# Syntax highlighting theme for diff view
# See "Theme" section below for available options
theme = "base16-ocean.dark"

[keybindings]
# See "Configurable Keybindings" section below for all options
approve = "a"
request_changes = "r"
comment = "c"
suggestion = "s"

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

# Auto-post review/fix comments to PR without confirmation prompt
# Default is false (asks for confirmation before posting)
# auto_post = true
```

### Configurable Keybindings

All keybindings can be customized in the `[keybindings]` section. Three formats are supported:

```toml
[keybindings]
# Simple key
move_down = "j"

# Key with modifiers
page_down = { key = "d", ctrl = true }

# Two-key sequence
go_to_definition = ["g", "d"]
```

#### Available Keybindings

| Key | Default | Description |
|-----|---------|-------------|
| **Navigation** |||
| `move_down` | `j` | Move down |
| `move_up` | `k` | Move up |
| `move_left` | `h` | Move left / back |
| `move_right` | `l` | Move right / select |
| `page_down` | `Ctrl+d` | Page down |
| `page_up` | `Ctrl+u` | Page up |
| `jump_to_first` | `gg` | Jump to first line |
| `jump_to_last` | `G` | Jump to last line |
| `jump_back` | `Ctrl+o` | Jump to previous position |
| `next_comment` | `n` | Jump to next comment |
| `prev_comment` | `N` | Jump to previous comment |
| **Actions** |||
| `approve` | `a` | Approve PR |
| `request_changes` | `r` | Request changes |
| `comment` | `c` | Add comment |
| `suggestion` | `s` | Add suggestion |
| `reply` | `r` | Reply to comment |
| `refresh` | `R` | Force refresh |
| `submit` | `Ctrl+s` | Submit input |
| **Mode Switching** |||
| `quit` | `q` | Quit / back |
| `help` | `?` | Toggle help |
| `comment_list` | `C` | Open comment list |
| `ai_rally` | `A` | Start AI Rally |
| `open_panel` | `Enter` | Open panel / select |
| `open_in_browser` | `O` | Open PR in browser |
| `toggle_local_mode` | `L` | Toggle local diff mode |
| `toggle_auto_focus` | `F` | Toggle auto-focus (local mode) |
| `toggle_markdown_rich` | `M` | Toggle Markdown rich display |
| **Diff Operations** |||
| `go_to_definition` | `gd` | Go to definition |
| `go_to_file` | `gf` | Open file in $EDITOR |

**Note**: Arrow keys (`↑/↓/←/→`) always work as alternatives to Vim-style keys and cannot be remapped.

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

### Theme

The `[diff]` section's `theme` option controls the syntax highlighting color scheme in the diff view.

#### Built-in Themes

| Theme | Description |
|-------|-------------|
| `base16-ocean.dark` | Dark theme based on Base16 Ocean (default) |
| `base16-ocean.light` | Light theme based on Base16 Ocean |
| `base16-eighties.dark` | Dark theme based on Base16 Eighties |
| `base16-mocha.dark` | Dark theme based on Base16 Mocha |
| `Dracula` | Dracula color scheme |
| `InspiredGitHub` | Light theme inspired by GitHub |
| `Solarized (dark)` | Solarized dark |
| `Solarized (light)` | Solarized light |

```toml
[diff]
theme = "Dracula"
```

Theme names are **case-insensitive** (`dracula`, `Dracula`, and `DRACULA` all work).

If a specified theme is not found, it falls back to `base16-ocean.dark`.

#### Custom Themes

You can add custom themes by placing `.tmTheme` (TextMate theme) files in `~/.config/octorus/themes/`:

```
~/.config/octorus/themes/
├── MyCustomTheme.tmTheme
└── nord.tmTheme
```

The filename (without `.tmTheme` extension) becomes the theme name:

```toml
[diff]
theme = "MyCustomTheme"
```

Custom themes with the same name as a built-in theme will override it.

## Local Diff Mode

Local Diff Mode lets you preview your uncommitted changes (`git diff HEAD`) directly in the TUI — no pull request required. A file watcher detects changes in real time and refreshes the diff automatically.

### Starting Local Diff Mode

```bash
# Start in local diff mode
or --local

# With auto-focus: automatically jump to the changed file on each update
or --local --auto-focus
```

### Real-Time File Watching

When running in local mode, octorus watches your working directory for file changes (ignoring `.git/` internals and access-only events). As soon as you save a file, the diff view updates automatically.

### Auto-Focus

When `--auto-focus` is enabled (or toggled with `F`), octorus automatically selects and focuses the file that changed most recently. If you're in the file list, it transitions to the split view diff. The selection algorithm picks the nearest changed file relative to your current cursor position.

The header displays `[LOCAL]` in local mode, or `[LOCAL AF]` when auto-focus is active.

### Switching Between PR and Local Mode

You can toggle between PR mode and Local mode at any time by pressing `L`:

```
PR mode ──[L]──► Local mode
  │                │
  │  UI state is   │  Starts file watcher
  │  saved/restored│  Shows git diff HEAD
  │                │
Local mode ──[L]──► PR mode
```

Your UI state (selected file, scroll position) is preserved across mode switches. If you started from a PR, pressing `L` in local mode returns you to that PR with its cached data.

### Differences from PR Mode

The following features are **disabled** in local mode since there is no associated pull request:

| Feature | Available? |
|---------|-----------|
| Browse changed files | ✅ |
| Syntax-highlighted diff | ✅ |
| Split view | ✅ |
| Go to definition (`gd`) | ✅ |
| Open file in editor (`gf`) | ✅ |
| Add inline comments | ❌ |
| Add suggestions | ❌ |
| Submit reviews | ❌ |
| View comment list | ❌ |
| Open PR in browser (`O`) | ❌ |

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
│    Reviewer     │  AI reviews the PR diff
│ (Claude/Codex)  │  → Posts review comments to PR
└────────┬────────┘
         │
    ┌────┴────┐
    │ Approve?│
    └────┬────┘
     No  │  Yes ──→ Done ✓
         ▼
┌─────────────────┐
│    Reviewee     │  AI fixes issues
│ (Claude/Codex)  │  → Commits locally (no push by default)
└────────┬────────┘
         │
    ┌────┴──────────────┐
    │                   │
    ▼                   ▼
 Completed    NeedsClarification /
    │          NeedsPermission
    │                   │
    │          User responds (y/n)
    │                   │
    └─────────┬─────────┘
              ▼
┌───────────────────────┐
│  Re-review (Reviewer) │  Updated diff:
│                       │  git diff (local) or
│                       │  gh pr diff (if pushed)
└───────────┬───────────┘
            │
       ┌────┴────┐
       │ Approve?│  ... repeat until approved
       └─────────┘       or max iterations
```

### Features

- **PR Integration**: Review comments are automatically posted to the PR
- **External Bot Support**: Collects feedback from Copilot, CodeRabbit, and other bots
- **Safe Operations**: Dangerous git operations (`--force`, `reset --hard`) are prohibited
- **Session Persistence**: Rally state is saved locally and can be resumed
- **Interactive Flow**: When the AI agent needs clarification or permission, you can respond interactively
- **Local Diff Support**: Re-review iterations prioritize local `git diff` for unpushed changes; falls back to `gh pr diff` when changes have been pushed
- **Background Execution**: Press `b` to run rally in background while continuing to browse files
- **Auto Post**: Set `auto_post = true` in `[ai]` config to skip confirmation prompts and automatically post review/fix comments to the PR

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
