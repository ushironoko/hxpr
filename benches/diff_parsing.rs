//! Diff parsing benchmarks for octorus.
//!
//! These benchmarks measure the performance of:
//! - Line classification (classify_line)
//! - Line info extraction (get_line_info)

mod common;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use common::{generate_diff_patch, generate_simple_patch};
use octorus::{classify_line, get_line_info};

/// Benchmark line classification.
///
/// Tests the classify_line function which determines line type (Added, Removed, Context, etc.)
/// and extracts content without the prefix.
fn bench_classify_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_parsing/classify_line");

    // Test different line types
    let test_lines = [
        ("header", "@@ -1,10 +1,12 @@"),
        ("meta_diff", "diff --git a/file.rs b/file.rs"),
        ("meta_plus", "+++ b/file.rs"),
        ("meta_minus", "--- a/file.rs"),
        ("added", "+    let x = foo();"),
        ("removed", "-    let y = bar();"),
        ("context", "     fn main() {"),
        ("context_long", "     let very_long_variable_name = some_function_with_many_arguments(arg1, arg2, arg3, arg4, arg5);"),
    ];

    for (name, line) in test_lines {
        group.bench_with_input(BenchmarkId::from_parameter(name), line, |b, line| {
            b.iter(|| black_box(classify_line(black_box(line))));
        });
    }

    group.finish();
}

/// Benchmark line classification on patch lines.
///
/// Measures classify_line performance when iterating over entire patches.
fn bench_classify_line_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_parsing/classify_line_batch");

    for line_count in [100, 500, 1000] {
        let patch = generate_diff_patch(line_count);
        let lines: Vec<&str> = patch.lines().collect();

        group.throughput(Throughput::Elements(line_count as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(line_count),
            &lines,
            |b, lines| {
                b.iter(|| {
                    for line in lines.iter() {
                        black_box(classify_line(black_box(line)));
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark get_line_info function.
///
/// Tests extracting line info at various positions in the patch.
fn bench_get_line_info(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_parsing/get_line_info");

    for line_count in [100, 500, 1000] {
        let patch = generate_diff_patch(line_count);

        // Test getting info at beginning, middle, and end
        for (position_name, position) in [
            ("start", 5_usize),
            ("middle", line_count / 2),
            ("end", line_count.saturating_sub(5)),
        ] {
            group.bench_with_input(
                BenchmarkId::new(format!("{}/{}", line_count, position_name), position),
                &(patch.clone(), position),
                |b, (patch, pos)| {
                    b.iter(|| black_box(get_line_info(black_box(patch), black_box(*pos))));
                },
            );
        }
    }

    group.finish();
}

/// Benchmark get_line_info with simple vs complex patches.
///
/// Compares performance on simple uniform patches vs realistic patches.
fn bench_get_line_info_complexity(c: &mut Criterion) {
    let mut group = c.benchmark_group("diff_parsing/get_line_info_complexity");

    let line_count = 1000_usize;
    let simple_patch = generate_simple_patch(line_count);
    let complex_patch = generate_diff_patch(line_count);
    let position = line_count / 2;

    group.bench_with_input(
        BenchmarkId::from_parameter("simple"),
        &simple_patch,
        |b, patch| {
            b.iter(|| black_box(get_line_info(black_box(patch), black_box(position))));
        },
    );

    group.bench_with_input(
        BenchmarkId::from_parameter("complex"),
        &complex_patch,
        |b, patch| {
            b.iter(|| black_box(get_line_info(black_box(patch), black_box(position))));
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    bench_classify_line,
    bench_classify_line_batch,
    bench_get_line_info,
    bench_get_line_info_complexity,
);
criterion_main!(benches);
