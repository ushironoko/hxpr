//! UI rendering benchmarks for octorus TUI.
//!
//! These benchmarks measure the performance of:
//! - Diff cache building (with and without syntax highlighting)
//! - Selected line rendering (current vs improved approach)

mod common;

use std::collections::HashSet;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;

use common::{generate_comment_lines, generate_diff_patch};
use octorus::build_diff_cache;

/// Benchmark diff cache building with syntax highlighting.
///
/// Tests various diff sizes: 100, 500, 1000, 5000 lines.
fn bench_build_diff_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_cache/build_cache");

    for line_count in [100, 500, 1000, 5000] {
        let patch = generate_diff_patch(line_count);
        let comment_lines = generate_comment_lines(line_count, 0.05); // 5% comment density

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &(patch, comment_lines),
            |b, (patch, comments)| {
                b.iter(|| {
                    black_box(build_diff_cache(
                        black_box(patch),
                        black_box("test.rs"),
                        black_box("base16-ocean.dark"),
                        black_box(comments),
                    ))
                });
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
        let comment_lines = generate_comment_lines(line_count, 0.05);

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &(patch, comment_lines),
            |b, (patch, comments)| {
                b.iter(|| {
                    // Use an unknown extension to skip syntax highlighting
                    black_box(build_diff_cache(
                        black_box(patch),
                        black_box("file.unknown_ext"),
                        black_box("base16-ocean.dark"),
                        black_box(comments),
                    ))
                });
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
        let empty_comments: HashSet<usize> = HashSet::new();
        let cached_lines =
            build_diff_cache(&patch, "test.rs", "base16-ocean.dark", &empty_comments);

        // Benchmark current approach: clone each span and add REVERSED
        group.bench_with_input(
            BenchmarkId::new("span_clone", line_count),
            &cached_lines,
            |b, cached_lines| {
                b.iter(|| {
                    let lines: Vec<Line> = cached_lines
                        .iter()
                        .enumerate()
                        .map(|(i, cached)| {
                            let is_selected = i == line_count / 2; // Select middle line
                            if is_selected {
                                let spans: Vec<_> = cached
                                    .spans
                                    .iter()
                                    .map(|span| {
                                        ratatui::text::Span::styled(
                                            span.content.clone(),
                                            span.style.add_modifier(Modifier::REVERSED),
                                        )
                                    })
                                    .collect();
                                Line::from(spans)
                            } else {
                                Line::from(cached.spans.clone())
                            }
                        })
                        .collect();
                    black_box(lines)
                });
            },
        );

        // Benchmark improved approach: use Line::style()
        group.bench_with_input(
            BenchmarkId::new("line_style", line_count),
            &cached_lines,
            |b, cached_lines| {
                b.iter(|| {
                    let lines: Vec<Line> = cached_lines
                        .iter()
                        .enumerate()
                        .map(|(i, cached)| {
                            let is_selected = i == line_count / 2;
                            if is_selected {
                                Line::from(cached.spans.clone())
                                    .style(Style::default().add_modifier(Modifier::REVERSED))
                            } else {
                                Line::from(cached.spans.clone())
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

/// Benchmark visible range processing optimization.
///
/// Compares processing all lines vs only visible lines.
fn bench_visible_range_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("visible_range");

    for total_lines in [1000, 5000] {
        let patch = generate_diff_patch(total_lines);
        let empty_comments: HashSet<usize> = HashSet::new();
        let cached_lines =
            build_diff_cache(&patch, "test.rs", "base16-ocean.dark", &empty_comments);

        let visible_height = 50_usize;
        let scroll_offset = total_lines / 2; // Scroll to middle

        // Process all lines (current approach)
        group.bench_with_input(
            BenchmarkId::new("all_lines", total_lines),
            &cached_lines,
            |b, cached_lines| {
                b.iter(|| {
                    let lines: Vec<Line> = cached_lines
                        .iter()
                        .enumerate()
                        .map(|(i, cached)| {
                            let is_selected = i == scroll_offset;
                            if is_selected {
                                Line::from(cached.spans.clone())
                                    .style(Style::default().add_modifier(Modifier::REVERSED))
                            } else {
                                Line::from(cached.spans.clone())
                            }
                        })
                        .collect();
                    black_box(lines)
                });
            },
        );

        // Process only visible range (optimized approach)
        group.bench_with_input(
            BenchmarkId::new("visible_only", total_lines),
            &cached_lines,
            |b, cached_lines| {
                b.iter(|| {
                    let visible_start = scroll_offset.saturating_sub(2);
                    let visible_end = (scroll_offset + visible_height + 5).min(cached_lines.len());

                    let lines: Vec<Line> = cached_lines[visible_start..visible_end]
                        .iter()
                        .enumerate()
                        .map(|(rel_idx, cached)| {
                            let abs_idx = visible_start + rel_idx;
                            let is_selected = abs_idx == scroll_offset;
                            if is_selected {
                                Line::from(cached.spans.clone())
                                    .style(Style::default().add_modifier(Modifier::REVERSED))
                            } else {
                                Line::from(cached.spans.clone())
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
);
criterion_main!(benches);
