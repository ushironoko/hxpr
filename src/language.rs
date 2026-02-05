//! Centralized language support definitions.
//!
//! This module provides a single source of truth for all language-specific
//! information used throughout the codebase, including:
//! - File extension to language mapping
//! - Tree-sitter language and highlight queries
//! - Definition keyword prefixes for Go to Definition
//! - Common keywords to exclude from symbol candidates

use std::sync::LazyLock;
use tree_sitter::Language;

/// Supported languages for tree-sitter based syntax highlighting and symbol analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    Rust,
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Go,
    Python,
    // New languages
    Ruby,
    Zig,
    C,
    /// C++
    Cpp,
    Java,
    CSharp,
    // Phase 1: Additional languages
    Lua,
    Bash,
    Php,
    Swift,
    // Kotlin is excluded: highlights.scm uses #lua-match? which is not tree-sitter compatible
    Haskell,
    // Svelte is excluded: tree-sitter-svelte-ng requires injection for <script>/<style> content
    // which octorus doesn't support. Svelte falls back to syntect which provides better highlighting.
    // Phase 2: MoonBit (path dependency)
    MoonBit,
    // Phase 3: Svelte (with injection support)
    Svelte,
    // Phase 3: CSS (for injection support in Svelte/Vue)
    Css,
}

/// Combined TypeScript highlights query (JavaScript base + TypeScript-specific).
///
/// TypeScript's HIGHLIGHTS_QUERY only contains TypeScript-specific patterns
/// (abstract, declare, enum, etc.) and expects to inherit JavaScript patterns.
/// We combine them here for complete highlighting.
static TYPESCRIPT_COMBINED_QUERY: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}\n{}",
        tree_sitter_javascript::HIGHLIGHT_QUERY,
        tree_sitter_typescript::HIGHLIGHTS_QUERY
    )
});

/// Combined C++ highlights query (C base + C++-specific).
///
/// C++'s HIGHLIGHT_QUERY only contains C++-specific patterns (class, virtual, etc.)
/// and expects to inherit C patterns. We combine them here for complete highlighting.
static CPP_COMBINED_QUERY: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{}\n{}",
        tree_sitter_c::HIGHLIGHT_QUERY,
        tree_sitter_cpp::HIGHLIGHT_QUERY
    )
});

/// Combined Svelte highlights query (HTML base + Svelte-specific).
///
/// Svelte's HIGHLIGHTS_QUERY uses `; inherits: html` which requires combining
/// with HTML patterns for complete highlighting.
static SVELTE_COMBINED_QUERY: LazyLock<String> = LazyLock::new(|| {
    // Filter out "; inherits:" lines from Svelte query
    let svelte_query: String = tree_sitter_svelte_ng::HIGHLIGHTS_QUERY
        .lines()
        .filter(|line| !line.trim().starts_with("; inherits:"))
        .collect::<Vec<_>>()
        .join("\n");

    format!("{}\n{}", tree_sitter_html::HIGHLIGHTS_QUERY, svelte_query)
});

/// C# highlights query (bundled as tree-sitter-c-sharp doesn't export it).
const CSHARP_HIGHLIGHTS_QUERY: &str = include_str!("queries/c_sharp/highlights.scm");

/// All definition prefixes from all supported languages, deduplicated.
static ALL_DEFINITION_PREFIXES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut prefixes = Vec::new();
    for lang in SupportedLanguage::all() {
        for prefix in lang.definition_prefixes() {
            if !prefixes.contains(prefix) {
                prefixes.push(*prefix);
            }
        }
    }
    prefixes
});

/// All keywords from all supported languages, deduplicated.
static ALL_KEYWORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let mut keywords = Vec::new();
    for lang in SupportedLanguage::all() {
        for keyword in lang.keywords() {
            if !keywords.contains(keyword) {
                keywords.push(*keyword);
            }
        }
    }
    keywords
});

impl SupportedLanguage {
    /// Create a SupportedLanguage from a file extension.
    ///
    /// Returns `None` if the extension is not supported.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::TypeScriptReact),
            "js" => Some(Self::JavaScript),
            "jsx" => Some(Self::JavaScriptReact),
            "go" => Some(Self::Go),
            "py" => Some(Self::Python),
            // Ruby
            "rb" | "rake" | "gemspec" => Some(Self::Ruby),
            // Zig
            "zig" => Some(Self::Zig),
            // C (including .h - plain C headers commonly use C11/gnu extensions)
            "c" | "h" => Some(Self::C),
            // C++ (explicit C++ headers and source files)
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(Self::Cpp),
            // Java
            "java" => Some(Self::Java),
            // C#
            "cs" => Some(Self::CSharp),
            // Phase 1: Additional languages
            "lua" => Some(Self::Lua),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "php" => Some(Self::Php),
            "swift" => Some(Self::Swift),
            "hs" | "lhs" => Some(Self::Haskell),
            // Phase 2: MoonBit
            "mbt" => Some(Self::MoonBit),
            // Phase 3: Svelte
            "svelte" => Some(Self::Svelte),
            // Phase 3: CSS (for injection support)
            "css" => Some(Self::Css),
            _ => None,
        }
    }

    /// Check if the given file extension is supported.
    pub fn is_supported(ext: &str) -> bool {
        Self::from_extension(ext).is_some()
    }

    /// Get the default file extension for this language.
    ///
    /// This is used to look up the parser from the parser pool.
    pub fn default_extension(&self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::TypeScript => "ts",
            Self::TypeScriptReact => "tsx",
            Self::JavaScript => "js",
            Self::JavaScriptReact => "jsx",
            Self::Go => "go",
            Self::Python => "py",
            Self::Ruby => "rb",
            Self::Zig => "zig",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::CSharp => "cs",
            Self::Lua => "lua",
            Self::Bash => "sh",
            Self::Php => "php",
            Self::Swift => "swift",
            Self::Haskell => "hs",
            Self::MoonBit => "mbt",
            Self::Svelte => "svelte",
            Self::Css => "css",
        }
    }

    /// Get the tree-sitter Language for this language.
    pub fn ts_language(&self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::TypeScriptReact => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript | Self::JavaScriptReact => tree_sitter_javascript::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            // Phase 1: Additional languages
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Self::Swift => tree_sitter_swift::LANGUAGE.into(),
            Self::Haskell => tree_sitter_haskell::LANGUAGE.into(),
            // Phase 2: MoonBit
            Self::MoonBit => tree_sitter_moonbit::LANGUAGE.into(),
            // Phase 3: Svelte
            Self::Svelte => tree_sitter_svelte_ng::LANGUAGE.into(),
            // Phase 3: CSS (for injection support)
            Self::Css => tree_sitter_css::LANGUAGE.into(),
        }
    }

    /// Get the highlights query for this language.
    ///
    /// For TypeScript/TSX, this returns a combined query including JavaScript
    /// patterns, since tree-sitter-typescript's query only contains TS-specific
    /// additions.
    ///
    /// For C++, this returns a combined query including C patterns, since
    /// tree-sitter-cpp's query only contains C++-specific additions.
    ///
    /// For C#, we bundle our own highlights query as the upstream crate
    /// doesn't export it.
    pub fn highlights_query(&self) -> &'static str {
        match self {
            Self::Rust => tree_sitter_rust::HIGHLIGHTS_QUERY,
            Self::TypeScript | Self::TypeScriptReact => {
                // SAFETY: LazyLock ensures the string is initialized once and lives
                // for the duration of the program
                TYPESCRIPT_COMBINED_QUERY.as_str()
            }
            Self::JavaScript | Self::JavaScriptReact => tree_sitter_javascript::HIGHLIGHT_QUERY,
            Self::Go => tree_sitter_go::HIGHLIGHTS_QUERY,
            Self::Python => tree_sitter_python::HIGHLIGHTS_QUERY,
            Self::Ruby => tree_sitter_ruby::HIGHLIGHTS_QUERY,
            Self::Zig => tree_sitter_zig::HIGHLIGHTS_QUERY,
            Self::C => tree_sitter_c::HIGHLIGHT_QUERY,
            Self::Cpp => {
                // SAFETY: LazyLock ensures the string is initialized once and lives
                // for the duration of the program
                CPP_COMBINED_QUERY.as_str()
            }
            Self::Java => tree_sitter_java::HIGHLIGHTS_QUERY,
            Self::CSharp => CSHARP_HIGHLIGHTS_QUERY,
            // Phase 1: Additional languages
            Self::Lua => tree_sitter_lua::HIGHLIGHTS_QUERY,
            Self::Bash => tree_sitter_bash::HIGHLIGHT_QUERY, // Note: singular HIGHLIGHT_QUERY
            Self::Php => tree_sitter_php::HIGHLIGHTS_QUERY,
            Self::Swift => tree_sitter_swift::HIGHLIGHTS_QUERY,
            Self::Haskell => tree_sitter_haskell::HIGHLIGHTS_QUERY,
            // Phase 2: MoonBit
            Self::MoonBit => tree_sitter_moonbit::HIGHLIGHTS_QUERY,
            // Phase 3: Svelte (combined with HTML)
            Self::Svelte => SVELTE_COMBINED_QUERY.as_str(),
            // Phase 3: CSS (for injection support)
            Self::Css => tree_sitter_css::HIGHLIGHTS_QUERY,
        }
    }

    /// Get the definition keyword prefixes for this language.
    ///
    /// Each entry is a keyword (including trailing space) that precedes a symbol
    /// name in a definition context.
    pub fn definition_prefixes(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "fn ",
                "pub fn ",
                "pub(crate) fn ",
                "pub(super) fn ",
                "struct ",
                "pub struct ",
                "enum ",
                "pub enum ",
                "trait ",
                "pub trait ",
                "type ",
                "pub type ",
                "const ",
                "pub const ",
                "static ",
                "pub static ",
                "mod ",
                "pub mod ",
                "impl ",
                "impl<",
            ],
            Self::TypeScript | Self::TypeScriptReact | Self::JavaScript | Self::JavaScriptReact => {
                &[
                    "function ",
                    "export function ",
                    "class ",
                    "interface ",
                    "export class ",
                    "export interface ",
                    "export type ",
                    "export enum ",
                    "export const ",
                ]
            }
            Self::Python => &["def ", "class "],
            Self::Go => &["func ", "type ", "var "],
            Self::Ruby => &[
                "def ",
                "class ",
                "module ",
                "attr_reader ",
                "attr_writer ",
                "attr_accessor ",
            ],
            Self::Zig => &[
                "fn ", "pub fn ", "const ", "var ", "struct ", "enum ", "union ",
            ],
            Self::C => &["void ", "int ", "char ", "struct ", "enum ", "typedef "],
            Self::Cpp => &[
                "void ",
                "int ",
                "char ",
                "struct ",
                "enum ",
                "typedef ",
                "class ",
                "namespace ",
                "template ",
                "virtual ",
            ],
            Self::Java => &[
                "public ",
                "private ",
                "protected ",
                "class ",
                "interface ",
                "enum ",
                "void ",
            ],
            Self::CSharp => &[
                "public ",
                "private ",
                "protected ",
                "class ",
                "interface ",
                "struct ",
                "enum ",
                "void ",
            ],
            // Phase 1: Additional languages
            Self::Lua => &["function ", "local function ", "local "],
            Self::Bash => &["function ", "alias "],
            Self::Php => &[
                "function ",
                "public function ",
                "private function ",
                "protected function ",
                "class ",
                "interface ",
                "trait ",
            ],
            Self::Swift => &[
                "func ",
                "class ",
                "struct ",
                "enum ",
                "protocol ",
                "extension ",
                "var ",
                "let ",
            ],
            Self::Haskell => &["data ", "newtype ", "type ", "class ", "instance "],
            // Phase 2: MoonBit
            Self::MoonBit => &[
                "fn ",
                "pub fn ",
                "priv fn ",
                "struct ",
                "pub struct ",
                "enum ",
                "pub enum ",
                "type ",
                "pub type ",
                "trait ",
                "pub trait ",
                "let ",
            ],
            // Phase 3: Svelte (uses JS/TS for script, so similar to JS)
            Self::Svelte => &[
                "function ",
                "export function ",
                "class ",
                "export const ",
                "export let ",
            ],
            // Phase 3: CSS (for injection support)
            Self::Css => &[],
        }
    }

    /// Get common keywords for this language that should be excluded from
    /// symbol popup candidates.
    pub fn keywords(&self) -> &'static [&'static str] {
        match self {
            Self::Rust => &[
                "fn", "pub", "let", "mut", "const", "static", "struct", "enum", "trait", "impl",
                "mod", "use", "crate", "self", "super", "where", "for", "in", "if", "else",
                "match", "return", "break", "continue", "loop", "while", "as", "ref", "move",
                "async", "await", "dyn", "type", "true", "false", "Some", "None", "Ok", "Err",
                "Self",
            ],
            Self::TypeScript | Self::TypeScriptReact | Self::JavaScript | Self::JavaScriptReact => {
                &[
                    "function",
                    "class",
                    "interface",
                    "export",
                    "import",
                    "from",
                    "default",
                    "const",
                    "let",
                    "var",
                    "new",
                    "this",
                    "typeof",
                    "instanceof",
                    "void",
                    "null",
                    "undefined",
                    "try",
                    "catch",
                    "throw",
                    "finally",
                    "yield",
                    "delete",
                    "switch",
                    "case",
                    "if",
                    "else",
                    "for",
                    "while",
                    "return",
                    "break",
                    "continue",
                    "true",
                    "false",
                    "async",
                    "await",
                ]
            }
            Self::Python => &[
                "def", "class", "pass", "raise", "with", "lambda", "global", "nonlocal", "assert",
                "del", "not", "and", "or", "is", "elif", "except", "if", "else", "for", "while",
                "return", "break", "continue", "import", "from", "try", "except", "finally",
                "True", "False", "None",
            ],
            Self::Go => &[
                "func",
                "package",
                "defer",
                "go",
                "select",
                "chan",
                "fallthrough",
                "range",
                "map",
                "type",
                "var",
                "const",
                "struct",
                "interface",
                "if",
                "else",
                "for",
                "switch",
                "case",
                "return",
                "break",
                "continue",
                "import",
                "true",
                "false",
                "nil",
            ],
            Self::Ruby => &[
                "def",
                "class",
                "module",
                "end",
                "if",
                "else",
                "elsif",
                "unless",
                "case",
                "when",
                "while",
                "until",
                "for",
                "do",
                "begin",
                "rescue",
                "ensure",
                "raise",
                "return",
                "break",
                "next",
                "redo",
                "retry",
                "yield",
                "self",
                "super",
                "nil",
                "true",
                "false",
                "and",
                "or",
                "not",
                "in",
                "then",
                "alias",
                "defined",
                "BEGIN",
                "END",
                "__FILE__",
                "__LINE__",
                "__ENCODING__",
                "attr_reader",
                "attr_writer",
                "attr_accessor",
                "private",
                "protected",
                "public",
                "require",
                "require_relative",
                "include",
                "extend",
                "prepend",
            ],
            Self::Zig => &[
                "fn",
                "pub",
                "const",
                "var",
                "struct",
                "enum",
                "union",
                "if",
                "else",
                "switch",
                "while",
                "for",
                "break",
                "continue",
                "return",
                "defer",
                "errdefer",
                "unreachable",
                "try",
                "catch",
                "orelse",
                "and",
                "or",
                "comptime",
                "inline",
                "noalias",
                "volatile",
                "extern",
                "export",
                "align",
                "packed",
                "linksection",
                "threadlocal",
                "allowzero",
                "anytype",
                "anyframe",
                "null",
                "undefined",
                "true",
                "false",
                "error",
                "test",
            ],
            Self::C => &[
                "void", "int", "char", "short", "long", "float", "double", "signed", "unsigned",
                "struct", "union", "enum", "typedef", "const", "static", "extern", "register",
                "volatile", "auto", "inline", "restrict", "if", "else", "switch", "case",
                "default", "while", "do", "for", "break", "continue", "return", "goto", "sizeof",
                "NULL", "true", "false",
            ],
            Self::Cpp => &[
                "void",
                "int",
                "char",
                "short",
                "long",
                "float",
                "double",
                "signed",
                "unsigned",
                "struct",
                "union",
                "enum",
                "typedef",
                "const",
                "static",
                "extern",
                "register",
                "volatile",
                "auto",
                "inline",
                "restrict",
                "if",
                "else",
                "switch",
                "case",
                "default",
                "while",
                "do",
                "for",
                "break",
                "continue",
                "return",
                "goto",
                "sizeof",
                "NULL",
                "true",
                "false",
                "class",
                "public",
                "private",
                "protected",
                "virtual",
                "override",
                "final",
                "new",
                "delete",
                "this",
                "throw",
                "try",
                "catch",
                "template",
                "typename",
                "namespace",
                "using",
                "operator",
                "friend",
                "explicit",
                "mutable",
                "constexpr",
                "nullptr",
                "noexcept",
                "decltype",
                "static_cast",
                "dynamic_cast",
                "const_cast",
                "reinterpret_cast",
                "co_await",
                "co_yield",
                "co_return",
                "concept",
                "requires",
                "export",
                "import",
                "module",
            ],
            Self::Java => &[
                "public",
                "private",
                "protected",
                "class",
                "interface",
                "enum",
                "extends",
                "implements",
                "abstract",
                "final",
                "static",
                "void",
                "int",
                "long",
                "short",
                "byte",
                "char",
                "float",
                "double",
                "boolean",
                "if",
                "else",
                "switch",
                "case",
                "default",
                "while",
                "do",
                "for",
                "break",
                "continue",
                "return",
                "throw",
                "try",
                "catch",
                "finally",
                "new",
                "this",
                "super",
                "null",
                "true",
                "false",
                "instanceof",
                "import",
                "package",
                "native",
                "synchronized",
                "transient",
                "volatile",
                "strictfp",
                "assert",
                "throws",
                "var",
                "record",
                "sealed",
                "permits",
                "non-sealed",
                "yield",
            ],
            Self::CSharp => &[
                "public",
                "private",
                "protected",
                "internal",
                "class",
                "interface",
                "struct",
                "enum",
                "namespace",
                "using",
                "abstract",
                "sealed",
                "static",
                "readonly",
                "const",
                "virtual",
                "override",
                "new",
                "void",
                "int",
                "long",
                "short",
                "byte",
                "char",
                "float",
                "double",
                "decimal",
                "bool",
                "string",
                "object",
                "var",
                "dynamic",
                "if",
                "else",
                "switch",
                "case",
                "default",
                "while",
                "do",
                "for",
                "foreach",
                "in",
                "break",
                "continue",
                "return",
                "throw",
                "try",
                "catch",
                "finally",
                "null",
                "true",
                "false",
                "this",
                "base",
                "typeof",
                "sizeof",
                "is",
                "as",
                "ref",
                "out",
                "params",
                "async",
                "await",
                "lock",
                "yield",
                "get",
                "set",
                "init",
                "value",
                "where",
                "when",
                "record",
                "with",
            ],
            // Phase 1: Additional languages
            Self::Lua => &[
                "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto",
                "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true",
                "until", "while",
            ],
            Self::Bash => &[
                "if", "then", "else", "elif", "fi", "case", "esac", "for", "while", "until", "do",
                "done", "in", "function", "select", "time", "coproc", "local", "return", "exit",
                "break", "continue", "declare", "typeset", "export", "readonly", "unset", "shift",
                "source", "alias", "true", "false",
            ],
            Self::Php => &[
                "abstract",
                "and",
                "array",
                "as",
                "break",
                "callable",
                "case",
                "catch",
                "class",
                "clone",
                "const",
                "continue",
                "declare",
                "default",
                "die",
                "do",
                "echo",
                "else",
                "elseif",
                "empty",
                "enddeclare",
                "endfor",
                "endforeach",
                "endif",
                "endswitch",
                "endwhile",
                "eval",
                "exit",
                "extends",
                "final",
                "finally",
                "fn",
                "for",
                "foreach",
                "function",
                "global",
                "goto",
                "if",
                "implements",
                "include",
                "include_once",
                "instanceof",
                "insteadof",
                "interface",
                "isset",
                "list",
                "match",
                "namespace",
                "new",
                "or",
                "print",
                "private",
                "protected",
                "public",
                "readonly",
                "require",
                "require_once",
                "return",
                "static",
                "switch",
                "throw",
                "trait",
                "try",
                "unset",
                "use",
                "var",
                "while",
                "xor",
                "yield",
                "true",
                "false",
                "null",
                "self",
                "parent",
            ],
            Self::Swift => &[
                "associatedtype",
                "class",
                "deinit",
                "enum",
                "extension",
                "fileprivate",
                "func",
                "import",
                "init",
                "inout",
                "internal",
                "let",
                "open",
                "operator",
                "private",
                "protocol",
                "public",
                "rethrows",
                "static",
                "struct",
                "subscript",
                "typealias",
                "var",
                "break",
                "case",
                "continue",
                "default",
                "defer",
                "do",
                "else",
                "fallthrough",
                "for",
                "guard",
                "if",
                "in",
                "repeat",
                "return",
                "switch",
                "where",
                "while",
                "as",
                "catch",
                "is",
                "nil",
                "super",
                "self",
                "Self",
                "throw",
                "throws",
                "true",
                "false",
                "try",
                "async",
                "await",
                "actor",
                "get",
                "set",
                "init",
                "value",
            ],
            Self::Haskell => &[
                "case",
                "class",
                "data",
                "default",
                "deriving",
                "do",
                "else",
                "forall",
                "foreign",
                "hiding",
                "if",
                "import",
                "in",
                "infix",
                "infixl",
                "infixr",
                "instance",
                "let",
                "mdo",
                "module",
                "newtype",
                "of",
                "proc",
                "qualified",
                "rec",
                "then",
                "type",
                "where",
                "True",
                "False",
            ],
            // Phase 2: MoonBit
            Self::MoonBit => &[
                "fn", "pub", "priv", "let", "mut", "const", "struct", "enum", "type", "trait",
                "impl", "derive", "test", "if", "else", "match", "for", "while", "loop", "break",
                "continue", "return", "try", "catch", "throw", "raise", "true", "false", "not",
                "and", "or", "self", "Self", "init",
            ],
            // Phase 3: Svelte (uses JS/TS keywords + Svelte-specific)
            Self::Svelte => &[
                "export",
                "let",
                "const",
                "function",
                "class",
                "if",
                "else",
                "each",
                "await",
                "then",
                "catch",
                "as",
                "key",
                "html",
                "debug",
                "snippet",
                "render",
                "true",
                "false",
                "null",
                "undefined",
            ],
            // Phase 3: CSS (for injection support)
            Self::Css => &[
                "important",
                "inherit",
                "initial",
                "unset",
                "none",
                "auto",
                "block",
                "inline",
                "flex",
                "grid",
                "absolute",
                "relative",
                "fixed",
                "sticky",
                "static",
                "hidden",
                "visible",
                "solid",
                "dotted",
                "dashed",
                "transparent",
                "currentColor",
            ],
        }
    }

    /// Get all definition prefixes from all supported languages.
    ///
    /// Returns a deduplicated list of prefixes.
    pub fn all_definition_prefixes() -> &'static [&'static str] {
        &ALL_DEFINITION_PREFIXES
    }

    /// Get all keywords from all supported languages.
    ///
    /// Returns a deduplicated list of keywords.
    pub fn all_keywords() -> &'static [&'static str] {
        &ALL_KEYWORDS
    }

    /// Iterate over all supported languages.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::Rust,
            Self::TypeScript,
            Self::TypeScriptReact,
            Self::JavaScript,
            Self::JavaScriptReact,
            Self::Go,
            Self::Python,
            Self::Ruby,
            Self::Zig,
            Self::C,
            Self::Cpp,
            Self::Java,
            Self::CSharp,
            // Phase 1: Additional languages
            Self::Lua,
            Self::Bash,
            Self::Php,
            Self::Swift,
            Self::Haskell,
            // Phase 2: MoonBit
            Self::MoonBit,
            // Phase 3: Svelte
            Self::Svelte,
            // Phase 3: CSS (for injection)
            Self::Css,
        ]
        .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_extension_supported() {
        assert_eq!(
            SupportedLanguage::from_extension("rs"),
            Some(SupportedLanguage::Rust)
        );
        assert_eq!(
            SupportedLanguage::from_extension("ts"),
            Some(SupportedLanguage::TypeScript)
        );
        assert_eq!(
            SupportedLanguage::from_extension("tsx"),
            Some(SupportedLanguage::TypeScriptReact)
        );
        assert_eq!(
            SupportedLanguage::from_extension("js"),
            Some(SupportedLanguage::JavaScript)
        );
        assert_eq!(
            SupportedLanguage::from_extension("jsx"),
            Some(SupportedLanguage::JavaScriptReact)
        );
        assert_eq!(
            SupportedLanguage::from_extension("go"),
            Some(SupportedLanguage::Go)
        );
        assert_eq!(
            SupportedLanguage::from_extension("py"),
            Some(SupportedLanguage::Python)
        );
    }

    #[test]
    fn test_from_extension_new_languages() {
        // Ruby
        assert_eq!(
            SupportedLanguage::from_extension("rb"),
            Some(SupportedLanguage::Ruby)
        );
        assert_eq!(
            SupportedLanguage::from_extension("rake"),
            Some(SupportedLanguage::Ruby)
        );
        assert_eq!(
            SupportedLanguage::from_extension("gemspec"),
            Some(SupportedLanguage::Ruby)
        );

        // Zig
        assert_eq!(
            SupportedLanguage::from_extension("zig"),
            Some(SupportedLanguage::Zig)
        );

        // C
        assert_eq!(
            SupportedLanguage::from_extension("c"),
            Some(SupportedLanguage::C)
        );

        // C header files (.h) are treated as C (supports C11/gnu extensions)
        assert_eq!(
            SupportedLanguage::from_extension("h"),
            Some(SupportedLanguage::C)
        );

        // C++ (explicit C++ headers and source files)
        assert_eq!(
            SupportedLanguage::from_extension("cpp"),
            Some(SupportedLanguage::Cpp)
        );
        assert_eq!(
            SupportedLanguage::from_extension("cc"),
            Some(SupportedLanguage::Cpp)
        );
        assert_eq!(
            SupportedLanguage::from_extension("cxx"),
            Some(SupportedLanguage::Cpp)
        );
        assert_eq!(
            SupportedLanguage::from_extension("hpp"),
            Some(SupportedLanguage::Cpp)
        );
        assert_eq!(
            SupportedLanguage::from_extension("hxx"),
            Some(SupportedLanguage::Cpp)
        );

        // Java
        assert_eq!(
            SupportedLanguage::from_extension("java"),
            Some(SupportedLanguage::Java)
        );

        // C#
        assert_eq!(
            SupportedLanguage::from_extension("cs"),
            Some(SupportedLanguage::CSharp)
        );

        // Phase 1: Additional languages
        assert_eq!(
            SupportedLanguage::from_extension("lua"),
            Some(SupportedLanguage::Lua)
        );
        assert_eq!(
            SupportedLanguage::from_extension("sh"),
            Some(SupportedLanguage::Bash)
        );
        assert_eq!(
            SupportedLanguage::from_extension("bash"),
            Some(SupportedLanguage::Bash)
        );
        assert_eq!(
            SupportedLanguage::from_extension("zsh"),
            Some(SupportedLanguage::Bash)
        );
        assert_eq!(
            SupportedLanguage::from_extension("php"),
            Some(SupportedLanguage::Php)
        );
        assert_eq!(
            SupportedLanguage::from_extension("swift"),
            Some(SupportedLanguage::Swift)
        );
        assert_eq!(
            SupportedLanguage::from_extension("hs"),
            Some(SupportedLanguage::Haskell)
        );
        assert_eq!(
            SupportedLanguage::from_extension("lhs"),
            Some(SupportedLanguage::Haskell)
        );
        // Phase 3: Svelte is now supported with tree-sitter
        assert_eq!(
            SupportedLanguage::from_extension("svelte"),
            Some(SupportedLanguage::Svelte)
        );
    }

    #[test]
    fn test_from_extension_unsupported() {
        assert_eq!(SupportedLanguage::from_extension("vue"), None);
        assert_eq!(SupportedLanguage::from_extension("yaml"), None);
        assert_eq!(SupportedLanguage::from_extension("md"), None);
        assert_eq!(SupportedLanguage::from_extension("toml"), None);
        assert_eq!(SupportedLanguage::from_extension(""), None);
    }

    #[test]
    fn test_is_supported() {
        assert!(SupportedLanguage::is_supported("rs"));
        assert!(SupportedLanguage::is_supported("ts"));
        assert!(SupportedLanguage::is_supported("tsx"));
        assert!(SupportedLanguage::is_supported("js"));
        assert!(SupportedLanguage::is_supported("jsx"));
        assert!(SupportedLanguage::is_supported("go"));
        assert!(SupportedLanguage::is_supported("py"));

        // New languages
        assert!(SupportedLanguage::is_supported("rb"));
        assert!(SupportedLanguage::is_supported("zig"));
        assert!(SupportedLanguage::is_supported("c"));
        assert!(SupportedLanguage::is_supported("cpp"));
        // .h is now treated as C (not C++)
        assert!(SupportedLanguage::is_supported("h"));
        assert!(SupportedLanguage::is_supported("java"));
        assert!(SupportedLanguage::is_supported("cs"));

        // Phase 1 languages
        assert!(SupportedLanguage::is_supported("lua"));
        assert!(SupportedLanguage::is_supported("sh"));
        assert!(SupportedLanguage::is_supported("php"));
        assert!(SupportedLanguage::is_supported("swift"));
        assert!(SupportedLanguage::is_supported("hs"));

        // Phase 3: Svelte is now supported
        assert!(SupportedLanguage::is_supported("svelte"));
        // Vue is not yet supported (requires similar injection support)
        assert!(!SupportedLanguage::is_supported("vue"));
        // Phase 2: MoonBit is now supported
        assert!(SupportedLanguage::is_supported("mbt"));
        // Phase 3: Svelte is now supported
        assert!(SupportedLanguage::is_supported("svelte"));
        assert!(!SupportedLanguage::is_supported("yaml"));
        assert!(!SupportedLanguage::is_supported("md"));
    }

    #[test]
    fn test_ts_language_is_valid() {
        // Verify each language can be converted to a tree-sitter Language
        for lang in SupportedLanguage::all() {
            let ts_lang = lang.ts_language();
            // Just ensure it doesn't panic
            assert!(ts_lang.abi_version() > 0);
        }
    }

    #[test]
    fn test_highlights_query_not_empty() {
        for lang in SupportedLanguage::all() {
            let query = lang.highlights_query();
            assert!(
                !query.is_empty(),
                "{:?} should have a non-empty highlights query",
                lang
            );
        }
    }

    #[test]
    fn test_highlights_query_parses_correctly() {
        // Verify each highlights query can be parsed by tree-sitter
        for lang in SupportedLanguage::all() {
            let ts_lang = lang.ts_language();
            let query_str = lang.highlights_query();
            let result = tree_sitter::Query::new(&ts_lang, query_str);
            assert!(
                result.is_ok(),
                "{:?} highlights query should parse correctly: {:?}",
                lang,
                result.err()
            );
        }
    }

    #[test]
    fn test_typescript_combined_query() {
        let query = SupportedLanguage::TypeScript.highlights_query();
        // Should contain JavaScript patterns
        assert!(
            query.contains("function"),
            "TypeScript query should include JavaScript patterns"
        );
        // Should also contain TypeScript-specific patterns
        // Note: exact content depends on tree-sitter-typescript version
        assert!(
            query.len() > tree_sitter_javascript::HIGHLIGHT_QUERY.len(),
            "Combined query should be longer than JavaScript-only query"
        );
    }

    #[test]
    fn test_cpp_combined_query() {
        let query = SupportedLanguage::Cpp.highlights_query();
        // Should contain C patterns (like struct)
        assert!(
            query.contains("struct"),
            "C++ query should include C patterns"
        );
        // Should also contain C++-specific patterns
        assert!(
            query.len() > tree_sitter_c::HIGHLIGHT_QUERY.len(),
            "Combined C++ query should be longer than C-only query"
        );
    }

    #[test]
    fn test_definition_prefixes_not_empty() {
        for lang in SupportedLanguage::all() {
            // CSS is injection-only and has no definition prefixes
            if lang == SupportedLanguage::Css {
                continue;
            }
            let prefixes = lang.definition_prefixes();
            assert!(
                !prefixes.is_empty(),
                "{:?} should have definition prefixes",
                lang
            );
        }
    }

    #[test]
    fn test_keywords_not_empty() {
        for lang in SupportedLanguage::all() {
            let keywords = lang.keywords();
            assert!(!keywords.is_empty(), "{:?} should have keywords", lang);
        }
    }

    #[test]
    fn test_all_definition_prefixes_deduplicated() {
        let prefixes = SupportedLanguage::all_definition_prefixes();
        let mut seen = std::collections::HashSet::new();
        for prefix in prefixes {
            assert!(seen.insert(*prefix), "Duplicate prefix found: {}", prefix);
        }
    }

    #[test]
    fn test_all_keywords_deduplicated() {
        let keywords = SupportedLanguage::all_keywords();
        let mut seen = std::collections::HashSet::new();
        for keyword in keywords {
            assert!(
                seen.insert(*keyword),
                "Duplicate keyword found: {}",
                keyword
            );
        }
    }

    #[test]
    fn test_all_definition_prefixes_contains_expected() {
        let prefixes = SupportedLanguage::all_definition_prefixes();
        // Check a few expected prefixes from different languages
        assert!(prefixes.contains(&"fn "), "Should contain Rust 'fn '");
        assert!(
            prefixes.contains(&"function "),
            "Should contain JS 'function '"
        );
        assert!(
            prefixes.contains(&"def "),
            "Should contain Python/Ruby 'def '"
        );
        assert!(prefixes.contains(&"func "), "Should contain Go 'func '");
        // New languages
        assert!(
            prefixes.contains(&"module "),
            "Should contain Ruby 'module '"
        );
        assert!(
            prefixes.contains(&"class "),
            "Should contain C++/Java/C# 'class '"
        );
    }

    #[test]
    fn test_all_keywords_contains_expected() {
        let keywords = SupportedLanguage::all_keywords();
        // Check a few expected keywords from different languages
        assert!(keywords.contains(&"fn"), "Should contain Rust 'fn'");
        assert!(
            keywords.contains(&"function"),
            "Should contain JS 'function'"
        );
        assert!(
            keywords.contains(&"def"),
            "Should contain Python/Ruby 'def'"
        );
        assert!(keywords.contains(&"func"), "Should contain Go 'func'");
        // New languages
        assert!(keywords.contains(&"module"), "Should contain Ruby 'module'");
        assert!(
            keywords.contains(&"namespace"),
            "Should contain C++/C# 'namespace'"
        );
        assert!(keywords.contains(&"trait"), "Should contain Rust 'trait'");
    }

    #[test]
    fn test_all_iterator() {
        let langs: Vec<_> = SupportedLanguage::all().collect();
        assert_eq!(langs.len(), 21);
        assert!(langs.contains(&SupportedLanguage::Rust));
        assert!(langs.contains(&SupportedLanguage::TypeScript));
        assert!(langs.contains(&SupportedLanguage::TypeScriptReact));
        assert!(langs.contains(&SupportedLanguage::JavaScript));
        assert!(langs.contains(&SupportedLanguage::JavaScriptReact));
        assert!(langs.contains(&SupportedLanguage::Go));
        assert!(langs.contains(&SupportedLanguage::Python));
        // New languages
        assert!(langs.contains(&SupportedLanguage::Ruby));
        assert!(langs.contains(&SupportedLanguage::Zig));
        assert!(langs.contains(&SupportedLanguage::C));
        assert!(langs.contains(&SupportedLanguage::Cpp));
        assert!(langs.contains(&SupportedLanguage::Java));
        assert!(langs.contains(&SupportedLanguage::CSharp));
    }

    #[test]
    fn test_language_hash_eq() {
        use std::collections::HashMap;

        let mut map: HashMap<SupportedLanguage, &str> = HashMap::new();
        map.insert(SupportedLanguage::Rust, "rust");
        map.insert(SupportedLanguage::TypeScript, "typescript");
        map.insert(SupportedLanguage::Ruby, "ruby");

        assert_eq!(map.get(&SupportedLanguage::Rust), Some(&"rust"));
        assert_eq!(map.get(&SupportedLanguage::TypeScript), Some(&"typescript"));
        assert_eq!(map.get(&SupportedLanguage::Ruby), Some(&"ruby"));
        assert_eq!(map.get(&SupportedLanguage::Python), None);
    }
}
