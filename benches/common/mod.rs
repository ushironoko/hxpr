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

/// Generate a realistic TypeScript/JavaScript diff patch.
pub fn generate_typescript_diff_patch(line_count: usize) -> String {
    let mut rng = seeded_rng();
    let mut lines = Vec::with_capacity(line_count);
    let mut current_line = 1u32;

    lines.push(format!("@@ -1,{} +1,{} @@", line_count / 2, line_count / 2));

    for i in 1..line_count {
        if i % 50 == 0 {
            current_line += 50;
            lines.push(format!(
                "@@ -{},{} +{},{} @@",
                current_line, 30, current_line, 30
            ));
            continue;
        }

        let line_type: u8 = rng.random_range(0..10);
        let content = generate_typescript_line(&mut rng, i);

        match line_type {
            0..=1 => lines.push(format!("+{}", content)),
            2..=3 => lines.push(format!("-{}", content)),
            _ => lines.push(format!(" {}", content)),
        }
    }

    lines.join("\n")
}

/// Generate a line of realistic TypeScript code
fn generate_typescript_line(rng: &mut ChaCha8Rng, line_num: usize) -> String {
    let templates = [
        "const value = data.map(x => x * 2);",
        "export function processData(input: string): Result<string> {",
        "}",
        "if (condition) { return ok(); }",
        "for (const item of items) {",
        "switch (result) {",
        "  case 'ok': return value;",
        "  default: throw new Error('unexpected');",
        "import { useState, useEffect } from 'react';",
        "interface Config {",
        "  field: string;",
        "class Component extends React.Component {",
        "@decorator",
        "/** Documentation comment */",
        "// Regular comment",
        "expect(result).toBe(expected);",
        "console.log('Debug:', value);",
        "await fetch('/api/data').then(r => r.json());",
        "async function fetchData(): Promise<number[]> {",
        ".filter(x => x > 0)",
    ];

    let idx = rng.random_range(0..templates.len());
    format!("{} // line {}", templates[idx], line_num)
}

/// Generate a realistic Vue SFC diff patch.
pub fn generate_vue_diff_patch(line_count: usize) -> String {
    let mut rng = seeded_rng();
    let mut lines = Vec::with_capacity(line_count);

    // Start with template section
    lines.push("@@ -1,50 +1,50 @@".to_string());
    lines.push(" <template>".to_string());

    let template_lines = line_count / 3;
    for i in 0..template_lines {
        let line_type: u8 = rng.random_range(0..10);
        let content = generate_vue_template_line(&mut rng, i);
        match line_type {
            0..=1 => lines.push(format!("+{}", content)),
            2..=3 => lines.push(format!("-{}", content)),
            _ => lines.push(format!(" {}", content)),
        }
    }
    lines.push(" </template>".to_string());

    // Script section
    lines.push(format!(
        "@@ -{},{} +{},{} @@",
        template_lines, 50, template_lines, 50
    ));
    lines.push(" <script lang=\"ts\">".to_string());

    let script_lines = line_count / 3;
    for i in 0..script_lines {
        let line_type: u8 = rng.random_range(0..10);
        let content = generate_typescript_line(&mut rng, i);
        match line_type {
            0..=1 => lines.push(format!("+{}", content)),
            2..=3 => lines.push(format!("-{}", content)),
            _ => lines.push(format!(" {}", content)),
        }
    }
    lines.push(" </script>".to_string());

    // Style section
    lines.push(format!(
        "@@ -{},{} +{},{} @@",
        template_lines + script_lines,
        30,
        template_lines + script_lines,
        30
    ));
    lines.push(" <style scoped>".to_string());

    let remaining = line_count.saturating_sub(lines.len());
    for i in 0..remaining {
        let line_type: u8 = rng.random_range(0..10);
        let content = generate_css_line(&mut rng, i);
        match line_type {
            0..=1 => lines.push(format!("+{}", content)),
            2..=3 => lines.push(format!("-{}", content)),
            _ => lines.push(format!(" {}", content)),
        }
    }
    lines.push(" </style>".to_string());

    lines.join("\n")
}

fn generate_vue_template_line(rng: &mut ChaCha8Rng, _line_num: usize) -> String {
    let templates = [
        "  <div class=\"container\">",
        "  </div>",
        "  <button @click=\"handleClick\">Click me</button>",
        "  <input v-model=\"value\" type=\"text\" />",
        "  <span>{{ message }}</span>",
        "  <component :is=\"dynamicComponent\" />",
        "  <ul><li v-for=\"item in items\" :key=\"item.id\">{{ item.name }}</li></ul>",
        "  <p v-if=\"show\">Conditional content</p>",
        "  <slot name=\"header\"></slot>",
        "  <template #footer>Footer content</template>",
    ];
    let idx = rng.random_range(0..templates.len());
    templates[idx].to_string()
}

fn generate_css_line(rng: &mut ChaCha8Rng, _line_num: usize) -> String {
    let templates = [
        ".container { display: flex; }",
        ".button { padding: 8px 16px; }",
        "  color: #333;",
        "  background-color: white;",
        "  border-radius: 4px;",
        "  font-size: 14px;",
        "@media (max-width: 768px) {",
        "}",
        ".active { opacity: 1; }",
        "  transition: all 0.3s ease;",
    ];
    let idx = rng.random_range(0..templates.len());
    templates[idx].to_string()
}

/// Generate a realistic Haskell diff patch.
///
/// Haskell has complex syntax features: type classes, GADTs, pattern matching,
/// do-notation, type-level programming, and significant whitespace.
pub fn generate_haskell_diff_patch(line_count: usize) -> String {
    let mut rng = seeded_rng();
    let mut lines = Vec::with_capacity(line_count);
    let mut current_line = 1u32;

    lines.push(format!("@@ -1,{} +1,{} @@", line_count / 2, line_count / 2));

    for i in 1..line_count {
        if i % 50 == 0 {
            current_line += 50;
            lines.push(format!(
                "@@ -{},{} +{},{} @@",
                current_line, 30, current_line, 30
            ));
            continue;
        }

        let line_type: u8 = rng.random_range(0..10);
        let content = generate_haskell_line(&mut rng, i);

        match line_type {
            0..=1 => lines.push(format!("+{}", content)),
            2..=3 => lines.push(format!("-{}", content)),
            _ => lines.push(format!(" {}", content)),
        }
    }

    lines.join("\n")
}

/// Generate a line of realistic Haskell code
fn generate_haskell_line(rng: &mut ChaCha8Rng, line_num: usize) -> String {
    let templates = [
        "module Main (main, processData) where",
        "import qualified Data.Map.Strict as Map",
        "import Control.Monad.Trans.State (StateT, get, put, evalStateT)",
        "data Tree a = Leaf | Node (Tree a) a (Tree a) deriving (Show, Eq, Functor)",
        "class (Monad m) => MonadReader r m | m -> r where",
        "  ask :: m r",
        "instance Functor Tree where",
        "  fmap f Leaf = Leaf",
        "  fmap f (Node l x r) = Node (fmap f l) (f x) (fmap f r)",
        "type family Elem (xs :: [k]) :: k where",
        "  Elem (x ': _) = x",
        "processData :: (MonadIO m, MonadReader Config m) => FilePath -> m [Result]",
        "processData path = do",
        "  contents <- liftIO $ readFile path",
        "  config <- ask",
        "  let parsed = parseWith (configParser config) contents",
        "  case parsed of",
        "    Left err -> throwError $ ParseError err",
        "    Right val -> pure $ transform val",
        "transform :: Value -> [Result]",
        "transform = \\case",
        "  Object fields -> concatMap transformField (Map.toList fields)",
        "  Array items  -> zipWith transformItem [0..] (toList items)",
        "  _            -> []",
        "  where",
        "    transformField (k, v) = [Result k (show v)]",
        "    transformItem i v = [Result (show i) (show v)]",
        "newtype AppM a = AppM { runAppM :: ReaderT Config (ExceptT AppError IO) a }",
        "  deriving (Functor, Applicative, Monad, MonadIO, MonadReader Config)",
        "{-# LANGUAGE GADTs, TypeFamilies, DataKinds, RankNTypes #-}",
        "-- | Main entry point with error handling",
        "main :: IO ()",
        "main = do",
        "  args <- getArgs",
        "  result <- runExceptT $ runReaderT (runAppM app) defaultConfig",
        "  either (hPutStrLn stderr . show) pure result",
        "fibonacci :: Integer -> Integer",
        "fibonacci n",
        "  | n <= 0    = 0",
        "  | n == 1    = 1",
        "  | otherwise = fibonacci (n - 1) + fibonacci (n - 2)",
    ];

    let idx = rng.random_range(0..templates.len());
    format!("{} -- line {}", templates[idx], line_num)
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
