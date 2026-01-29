//! octorus - A TUI tool for GitHub PR review
//!
//! This library exposes modules for benchmarking purposes.

// All modules need to be public for the library to compile,
// as they have internal dependencies.
pub mod ai;
pub mod app;
pub mod cache;
pub mod config;
pub mod diff;
pub mod editor;
pub mod github;
pub mod keybinding;
pub mod loader;
pub mod symbol;
pub mod syntax;
pub mod ui;

// Re-export commonly used types for benchmarks
pub use app::CachedDiffLine;
pub use diff::{classify_line, get_line_info, LineType};
pub use ui::diff_view::{build_diff_cache, render_cached_lines};
