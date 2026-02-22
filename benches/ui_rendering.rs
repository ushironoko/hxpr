//! UI rendering benchmarks for octorus TUI.
//!
//! ## Production benchmarks (matches actual production execution)
//! - `diff_cache/` – cache building with/without syntax highlighting
//! - `visible_range/visible_borrowed` – visible range processing (production)
//! - `highlighter/tree_sitter_rust` – Rust highlighting
//! - `highlighter/tree_sitter_haskell` – Haskell highlighting (complex syntax)
//! - `highlighter/tree_sitter_vue` – Vue SFC highlighting (injection: HTML/TS/CSS)
//!
//! ## Reference benchmarks (production code, synthetic execution)
//! - `selected_line/span_clone` – baseline (clone each span)
//! - `selected_line/borrowed_spans` – production function but all-lines execution
//! - `visible_range/all_lines` – baseline (process all lines)
//!
//! ## Archive benchmarks (historical, not used in production)
//! - `archive/selected_line/line_style` – intermediate approach (clone + Line::style)
//! - `archive/visible_range/visible_only` – visible range with clone (superseded by borrow)

mod common;

use std::collections::HashSet;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use common::{generate_diff_patch, generate_haskell_diff_patch, generate_vue_diff_patch};
use octorus::{build_diff_cache, render_cached_lines, ParserPool};

/// Benchmark diff cache building with syntax highlighting.
///
/// Tests various diff sizes: 100, 500, 1000, 5000 lines.
fn bench_build_diff_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_cache/build_cache");

    for line_count in [100, 500, 1000, 5000] {
        let patch = generate_diff_patch(line_count);

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &patch,
            |b, patch| {
                b.iter_batched(
                    ParserPool::new,
                    |mut parser_pool| {
                        black_box(build_diff_cache(
                            black_box(patch),
                            black_box("test.rs"),
                            black_box("base16-ocean.dark"),
                            black_box(&mut parser_pool),
                            black_box(false),
                            black_box(4),
                        ))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark diff cache building without syntax highlighting.
///
/// Uses a file extension that doesn't have syntax highlighting support
/// to measure the baseline overhead without syntect processing.
fn bench_build_diff_cache_no_highlight(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_cache/build_cache_no_highlight");

    for line_count in [100, 500, 1000, 5000] {
        let patch = generate_diff_patch(line_count);

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &patch,
            |b, patch| {
                b.iter_batched(
                    ParserPool::new,
                    |mut parser_pool| {
                        // Use an unknown extension to skip syntax highlighting
                        black_box(build_diff_cache(
                            black_box(patch),
                            black_box("file.unknown_ext"),
                            black_box("base16-ocean.dark"),
                            black_box(&mut parser_pool),
                            black_box(false),
                            black_box(4),
                        ))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark selected line rendering approaches.
///
/// Compares the current approach (cloning spans and adding REVERSED to each)
/// vs the improved approach (using Line::style()).
fn bench_selected_line_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("selected_line");

    for line_count in [100, 500, 1000] {
        let patch = generate_diff_patch(line_count);
        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(&patch, "test.rs", "base16-ocean.dark", &mut parser_pool, false, 4);
        let empty_comments: HashSet<usize> = HashSet::new();

        // Benchmark current approach: resolve and clone each span, add REVERSED
        group.bench_with_input(
            BenchmarkId::new("span_clone", line_count),
            &cache,
            |b, cache| {
                b.iter(|| {
                    let lines: Vec<Line> = cache
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(i, cached)| {
                            let is_selected = i == line_count / 2; // Select middle line
                            if is_selected {
                                let spans: Vec<_> = cached
                                    .spans
                                    .iter()
                                    .map(|span| {
                                        Span::styled(
                                            cache.resolve(span.content).to_string(),
                                            span.style.add_modifier(Modifier::REVERSED),
                                        )
                                    })
                                    .collect();
                                Line::from(spans)
                            } else {
                                let spans: Vec<_> = cached
                                    .spans
                                    .iter()
                                    .map(|span| {
                                        Span::styled(
                                            cache.resolve(span.content).to_string(),
                                            span.style,
                                        )
                                    })
                                    .collect();
                                Line::from(spans)
                            }
                        })
                        .collect();
                    black_box(lines)
                });
            },
        );

        // Benchmark zero-clone approach: calls the actual production function
        group.bench_with_input(
            BenchmarkId::new("borrowed_spans", line_count),
            &(cache, empty_comments),
            |b, (cache, comments)| {
                b.iter(|| {
                    let selected = line_count / 2;
                    black_box(render_cached_lines(
                        black_box(cache),
                        0..cache.lines.len(),
                        selected,
                        comments,
                    ))
                });
            },
        );
    }

    group.finish();
}

/// Benchmark visible range processing optimization.
///
/// Compares processing all lines vs only visible lines.
fn bench_visible_range_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("visible_range");

    for total_lines in [1000, 5000] {
        let patch = generate_diff_patch(total_lines);
        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(&patch, "test.rs", "base16-ocean.dark", &mut parser_pool, false, 4);
        let empty_comments: HashSet<usize> = HashSet::new();

        let visible_height = 50_usize;
        let scroll_offset = total_lines / 2; // Scroll to middle

        // Process all lines (current approach)
        group.bench_with_input(
            BenchmarkId::new("all_lines", total_lines),
            &cache,
            |b, cache| {
                b.iter(|| {
                    let lines: Vec<Line> = cache
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(i, cached)| {
                            let is_selected = i == scroll_offset;
                            let spans: Vec<_> = cached
                                .spans
                                .iter()
                                .map(|span| {
                                    Span::styled(
                                        cache.resolve(span.content).to_string(),
                                        span.style,
                                    )
                                })
                                .collect();
                            if is_selected {
                                Line::from(spans)
                                    .style(Style::default().add_modifier(Modifier::REVERSED))
                            } else {
                                Line::from(spans)
                            }
                        })
                        .collect();
                    black_box(lines)
                });
            },
        );

        // Process only visible range with borrowed spans: calls the actual production function
        group.bench_with_input(
            BenchmarkId::new("visible_borrowed", total_lines),
            &(cache, empty_comments),
            |b, (cache, comments)| {
                b.iter(|| {
                    let visible_start = scroll_offset.saturating_sub(2);
                    let visible_end = (scroll_offset + visible_height + 5).min(cache.lines.len());

                    black_box(render_cached_lines(
                        black_box(cache),
                        visible_start..visible_end,
                        scroll_offset,
                        comments,
                    ))
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Tree-sitter vs Syntect comparison benchmarks
// ---------------------------------------------------------------------------

/// Benchmark tree-sitter highlighting (Rust files).
fn bench_highlighter_tree_sitter_rust(c: &mut Criterion) {
    let mut group = c.benchmark_group("highlighter/tree_sitter_rust");

    for line_count in [100, 500, 1000, 10000] {
        let patch = generate_diff_patch(line_count); // Rust-like code

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &patch,
            |b, patch| {
                b.iter_batched(
                    ParserPool::new,
                    |mut parser_pool| {
                        black_box(build_diff_cache(
                            black_box(patch),
                            black_box("test.rs"), // tree-sitter
                            black_box("Dracula"),
                            black_box(&mut parser_pool),
                            black_box(false),
                            black_box(4),
                        ))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark tree-sitter highlighting (Haskell files).
///
/// Haskell has complex syntax: type classes, GADTs, pattern matching,
/// do-notation, and type-level programming. Tests tree-sitter with
/// a language that has significant syntactic complexity.
fn bench_highlighter_tree_sitter_haskell(c: &mut Criterion) {
    let mut group = c.benchmark_group("highlighter/tree_sitter_haskell");

    for line_count in [100, 500, 1000, 10000] {
        let patch = generate_haskell_diff_patch(line_count);

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &patch,
            |b, patch| {
                b.iter_batched(
                    ParserPool::new,
                    |mut parser_pool| {
                        black_box(build_diff_cache(
                            black_box(patch),
                            black_box("test.hs"), // tree-sitter
                            black_box("Dracula"),
                            black_box(&mut parser_pool),
                            black_box(false),
                            black_box(4),
                        ))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark tree-sitter highlighting (Vue SFC files).
///
/// Vue SFC uses injection: template (HTML/Vue), script (TypeScript), style (CSS).
/// Tests tree-sitter multi-language handling within a single file.
fn bench_highlighter_tree_sitter_vue(c: &mut Criterion) {
    let mut group = c.benchmark_group("highlighter/tree_sitter_vue");

    for line_count in [100, 500, 1000, 10000] {
        let patch = generate_vue_diff_patch(line_count);

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &patch,
            |b, patch| {
                b.iter_batched(
                    ParserPool::new,
                    |mut parser_pool| {
                        black_box(build_diff_cache(
                            black_box(patch),
                            black_box("test.vue"), // tree-sitter with injection
                            black_box("Dracula"),
                            black_box(&mut parser_pool),
                            black_box(false),
                            black_box(4),
                        ))
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Archive benchmarks: historical approaches no longer used in production.
// Kept for regression tracking and comparison with current production code.
// ---------------------------------------------------------------------------

/// Archive: selected line rendering with Line::style() + clone.
///
/// Intermediate approach that was superseded by zero-copy borrowed_spans.
/// Useful as a reference point between span_clone (worst) and borrowed_spans (best).
fn bench_archive_selected_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("archive/selected_line");

    for line_count in [100, 500, 1000] {
        let patch = generate_diff_patch(line_count);
        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(&patch, "test.rs", "base16-ocean.dark", &mut parser_pool, false, 4);

        group.bench_with_input(
            BenchmarkId::new("line_style", line_count),
            &cache,
            |b, cache| {
                b.iter(|| {
                    let lines: Vec<Line> = cache
                        .lines
                        .iter()
                        .enumerate()
                        .map(|(i, cached)| {
                            let is_selected = i == line_count / 2;
                            let spans: Vec<_> = cached
                                .spans
                                .iter()
                                .map(|span| {
                                    Span::styled(
                                        cache.resolve(span.content).to_string(),
                                        span.style,
                                    )
                                })
                                .collect();
                            if is_selected {
                                Line::from(spans)
                                    .style(Style::default().add_modifier(Modifier::REVERSED))
                            } else {
                                Line::from(spans)
                            }
                        })
                        .collect();
                    black_box(lines)
                });
            },
        );
    }

    group.finish();
}

/// Archive: visible range processing with clone (no borrowing).
///
/// Superseded by visible_borrowed which uses zero-copy render_cached_lines().
/// Useful as a reference to show the benefit of borrowing over cloning
/// within the same visible-range optimization.
fn bench_archive_visible_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("archive/visible_range");

    for total_lines in [1000, 5000] {
        let patch = generate_diff_patch(total_lines);
        let mut parser_pool = ParserPool::new();
        let cache = build_diff_cache(&patch, "test.rs", "base16-ocean.dark", &mut parser_pool, false, 4);

        let visible_height = 50_usize;
        let scroll_offset = total_lines / 2;

        group.bench_with_input(
            BenchmarkId::new("visible_only", total_lines),
            &cache,
            |b, cache| {
                b.iter(|| {
                    let visible_start = scroll_offset.saturating_sub(2);
                    let visible_end = (scroll_offset + visible_height + 5).min(cache.lines.len());

                    let lines: Vec<Line> = cache.lines[visible_start..visible_end]
                        .iter()
                        .enumerate()
                        .map(|(rel_idx, cached)| {
                            let abs_idx = visible_start + rel_idx;
                            let is_selected = abs_idx == scroll_offset;
                            let spans: Vec<_> = cached
                                .spans
                                .iter()
                                .map(|span| {
                                    Span::styled(
                                        cache.resolve(span.content).to_string(),
                                        span.style,
                                    )
                                })
                                .collect();
                            if is_selected {
                                Line::from(spans)
                                    .style(Style::default().add_modifier(Modifier::REVERSED))
                            } else {
                                Line::from(spans)
                            }
                        })
                        .collect();
                    black_box(lines)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_build_diff_cache,
    bench_build_diff_cache_no_highlight,
    bench_selected_line_rendering,
    bench_visible_range_processing,
    // Tree-sitter highlighting
    bench_highlighter_tree_sitter_rust,
    bench_highlighter_tree_sitter_haskell,
    bench_highlighter_tree_sitter_vue,
    // Archive
    bench_archive_selected_line,
    bench_archive_visible_range,
);
criterion_main!(benches);
