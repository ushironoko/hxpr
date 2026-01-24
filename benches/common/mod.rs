//! Common utilities for benchmarks.
//!
//! Provides test data generators with fixed seeds for reproducibility.

#![allow(dead_code)]

use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha8Rng;

/// Fixed seed for reproducible benchmark data
const SEED: u64 = 42;

/// Create a seeded RNG for reproducible test data
pub fn seeded_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(SEED)
}

/// Generate a realistic diff patch with the specified number of lines.
///
/// Creates a mix of:
/// - Hunk headers (@@ ... @@)
/// - Added lines (+)
/// - Removed lines (-)
/// - Context lines (space)
pub fn generate_diff_patch(line_count: usize) -> String {
    let mut rng = seeded_rng();
    let mut lines = Vec::with_capacity(line_count);
    let mut current_line = 1u32;

    // Start with a hunk header
    lines.push(format!("@@ -1,{} +1,{} @@", line_count / 2, line_count / 2));

    for i in 1..line_count {
        // Every 50 lines, add a new hunk header
        if i % 50 == 0 {
            current_line += 50;
            lines.push(format!(
                "@@ -{},{} +{},{} @@",
                current_line, 30, current_line, 30
            ));
            continue;
        }

        let line_type: u8 = rng.random_range(0..10);
        let content = generate_code_line(&mut rng, i);

        match line_type {
            0..=1 => lines.push(format!("+{}", content)), // 20% added
            2..=3 => lines.push(format!("-{}", content)), // 20% removed
            _ => lines.push(format!(" {}", content)),     // 60% context
        }
    }

    lines.join("\n")
}

/// Generate a line of realistic Rust-like code
fn generate_code_line(rng: &mut ChaCha8Rng, line_num: usize) -> String {
    let templates = [
        "    let x = value.unwrap_or_default();",
        "    fn process_data(input: &str) -> Result<String> {",
        "    }",
        "    if condition { return Ok(()); }",
        "    for item in items.iter() {",
        "    match result {",
        "        Ok(v) => v,",
        "        Err(e) => return Err(e),",
        "    use std::collections::HashMap;",
        "    pub struct Config {",
        "        field: String,",
        "    impl Default for Config {",
        "    #[derive(Debug, Clone)]",
        "    /// Documentation comment",
        "    // Regular comment",
        "    assert_eq!(expected, actual);",
        "    println!(\"Debug: {}\", value);",
        "    self.inner.lock().unwrap()",
        "    async fn fetch_data() -> Result<Vec<u8>> {",
        "    .map(|x| x * 2)",
    ];

    let idx = rng.random_range(0..templates.len());
    format!("{} // line {}", templates[idx], line_num)
}

/// Generate a simple patch without complex syntax (for baseline comparisons)
pub fn generate_simple_patch(line_count: usize) -> String {
    let mut lines = Vec::with_capacity(line_count);

    lines.push("@@ -1,100 +1,100 @@".to_string());

    for i in 1..line_count {
        if i % 50 == 0 {
            lines.push(format!("@@ -{},{} +{},{} @@", i, 30, i, 30));
            continue;
        }

        let prefix = match i % 5 {
            0 => "+",
            1 => "-",
            _ => " ",
        };
        lines.push(format!("{}simple line {}", prefix, i));
    }

    lines.join("\n")
}

/// Generate a set of comment line indices
pub fn generate_comment_lines(
    total_lines: usize,
    comment_density: f64,
) -> std::collections::HashSet<usize> {
    let mut rng = seeded_rng();
    let mut comment_lines = std::collections::HashSet::new();

    for i in 0..total_lines {
        if rng.random::<f64>() < comment_density {
            comment_lines.insert(i);
        }
    }

    comment_lines
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_generate_diff_patch_length() {
        let patch = super::generate_diff_patch(100);
        let line_count = patch.lines().count();
        assert_eq!(line_count, 100);
    }

    #[test]
    fn test_generate_diff_patch_reproducible() {
        let patch1 = super::generate_diff_patch(50);
        let patch2 = super::generate_diff_patch(50);
        assert_eq!(patch1, patch2);
    }

    #[test]
    fn test_generate_comment_lines() {
        let comments = super::generate_comment_lines(100, 0.1);
        // With 10% density on 100 lines, expect roughly 10 comments
        assert!(comments.len() >= 5 && comments.len() <= 20);
    }
}
