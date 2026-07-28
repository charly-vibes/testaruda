//! Import path resolution for the TypeScript adapter.
//!
//! Handles:
//! - Relative path resolution (`./foo`, `../bar`) with extension fallback
//! - `tsconfig.json` path alias resolution (`@/*`, `~/*`, `@components/*`)
//! - Barrel file resolution (directory → `index.ts`/`index.tsx`)
//! - Non-code import resolution (CSS, images → emit edges)
//! - Circular import safety via visited-set
//! - Re-export following (one hop)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Configuration parsed from `tsconfig.json` (or `jsconfig.json`).
#[derive(Debug, Default, Clone)]
pub struct TsConfig {
    /// `compilerOptions.baseUrl` — relative to the project root.
    pub base_url: PathBuf,
    /// `compilerOptions.paths` — list of (prefix, [replacements]) pairs.
    pub paths: Vec<(String, Vec<String>)>,
}

/// Result of resolving a single import path.
#[derive(Debug, Clone)]
pub struct ResolvedImport {
    /// The absolute path to the resolved file (if found on disk).
    pub resolved: Option<PathBuf>,
    /// Whether this was a non-code import (CSS, image, etc.)
    #[allow(dead_code)]
    pub is_non_code: bool,
}

/// Import path resolver backed by an optional `tsconfig.json`.
pub struct ImportResolver {
    base_dir: PathBuf,
    tsconfig: Option<TsConfig>,
}

// ---------------------------------------------------------------------------
// Non-code file extensions that should resolve but produce non-code edges.
// ---------------------------------------------------------------------------
const NON_CODE_EXTENSIONS: &[&str] = &[
    "css", "scss", "less", "sass", "png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "bmp",
    "woff", "woff2", "ttf", "eot", "json", "xml", "yaml", "yml", "md", "txt", "mp4", "webm", "ogg",
];

// ---------------------------------------------------------------------------
// File extensions to try during resolution, in order.
// ---------------------------------------------------------------------------
const TS_EXTENSIONS: &[&str] = &["ts", "tsx", "mts", "cts"];

impl ImportResolver {
    /// Create a new resolver rooted at `base_dir` (the project root).
    ///
    /// Automatically loads `tsconfig.json` (or `jsconfig.json` as fallback).
    pub fn new(base_dir: &Path) -> Self {
        let tsconfig = Self::load_tsconfig(&base_dir.join("tsconfig.json"))
            .or_else(|| Self::load_tsconfig(&base_dir.join("jsconfig.json")));

        if tsconfig.is_none() {
            eprintln!(
                "[adapter-typescript] WARNING: tsconfig.json not found — path-alias resolution disabled"
            );
        }

        Self {
            base_dir: base_dir
                .canonicalize()
                .unwrap_or_else(|_| base_dir.to_path_buf()),
            tsconfig,
        }
    }

    /// Return whether a `tsconfig.json` was found and loaded.
    #[allow(dead_code)]
    pub fn has_tsconfig(&self) -> bool {
        self.tsconfig.is_some()
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Resolve an import path to a concrete file on disk.
    ///
    /// `source_dir` is the directory containing the importing file.
    pub fn resolve(&self, source_dir: &Path, import_path: &str) -> ResolvedImport {
        let trimmed = import_path.trim();

        // 1. Check for tsconfig path alias first (e.g., @/components/Button)
        if let Some(tsconfig) = &self.tsconfig {
            if let Some(resolved) = self.resolve_alias(trimmed, tsconfig) {
                // Even if the resolved file doesn't exist on disk, return it
                // so the caller can check existence vs non-code vs unresolved
                if resolved.exists() {
                    let non_code = is_non_code_extension(&resolved);
                    return ResolvedImport {
                        resolved: Some(resolved),
                        is_non_code: non_code,
                    };
                }
            }
        }

        // 2. Relative imports (starting with ./ or ../)
        if trimmed.starts_with("./") || trimmed.starts_with("../") {
            if let Some(resolved) = self.resolve_relative(source_dir, trimmed) {
                let non_code = is_non_code_extension(&resolved);
                return ResolvedImport {
                    resolved: Some(resolved),
                    is_non_code: non_code,
                };
            }
            // Relative path that doesn't resolve — try as non-code
            let candidate = source_dir.join(trimmed);
            if candidate.exists() {
                return ResolvedImport {
                    resolved: Some(candidate),
                    is_non_code: true,
                };
            }
            return ResolvedImport {
                resolved: None,
                is_non_code: false,
            };
        }

        // 3. Absolute-style import starting with / — resolve against base_dir
        if trimmed.starts_with('/') {
            let candidate = self
                .base_dir
                .join(trimmed.strip_prefix('/').unwrap_or(trimmed));
            if let Some(resolved) = Self::try_resolve_path(&candidate) {
                let non_code = is_non_code_extension(&resolved);
                return ResolvedImport {
                    resolved: Some(resolved),
                    is_non_code: non_code,
                };
            }
            return ResolvedImport {
                resolved: None,
                is_non_code: false,
            };
        }

        // 4. Anything else (package imports like "lodash", "@angular/core") — external
        ResolvedImport {
            resolved: None,
            is_non_code: false,
        }
    }

    /// Resolve a single import path, but also follow re-exports one level deep.
    ///
    /// Returns the **set** of leaf-level source files that the import ultimately
    /// resolves to (after following one hop of re-exports from barrel files).
    ///
    /// `visited` is used for circular import safety.
    pub fn resolve_with_re_exports(
        &self,
        source_dir: &Path,
        import_path: &str,
        visited: &mut HashSet<PathBuf>,
    ) -> Vec<PathBuf> {
        let resolved = self.resolve(source_dir, import_path);
        let Some(path) = resolved.resolved else {
            return Vec::new();
        };

        // Already visited — circular import guard
        if !visited.insert(path.clone()) {
            return Vec::new();
        }

        // Check if the resolved file is a barrel (index) file with re-exports
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let is_barrel = file_name.starts_with("index.");

        if !is_barrel {
            // Not a barrel file — return as-is
            return vec![path];
        }

        // Follow re-exports one hop
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return vec![path], // Can't read — return barrel itself
        };

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("ts");
        let re_exports = extract_re_export_sources(&content, ext);
        let parent = path.parent().unwrap_or(&self.base_dir);

        let mut results = Vec::new();
        for re_export in re_exports {
            let sub_resolved = self.resolve(parent, &re_export);
            if let Some(sub_path) = sub_resolved.resolved {
                // Follow one more level only if it's another barrel
                let sub_name = sub_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if sub_name.starts_with("index.") && !visited.contains(&sub_path) {
                    visited.insert(sub_path.clone());
                    results.push(sub_path);
                } else {
                    results.push(sub_path);
                }
            }
        }

        if results.is_empty() {
            // No re-exports found — return the barrel file itself
            vec![path]
        } else {
            results
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Resolve a relative import path by trying extensions and barrel files.
    fn resolve_relative(&self, source_dir: &Path, import_path: &str) -> Option<PathBuf> {
        let candidate = source_dir.join(import_path);
        Self::try_resolve_path(&candidate)
    }

    /// Try to resolve a path by checking:
    /// 1. Exact match
    /// 2. With each TS extension appended
    /// 3. As a directory with barrel file
    /// 4. As a directory with each TS extension on the barrel
    fn try_resolve_path(candidate: &Path) -> Option<PathBuf> {
        // 1. Exact match (file exists)
        if candidate.is_file() {
            return Some(candidate.to_path_buf());
        }

        // 2. Try appending each extension
        if let Some(stem) = candidate.to_str() {
            for ext in TS_EXTENSIONS {
                let with_ext = format!("{}.{}", stem, ext);
                let p = PathBuf::from(&with_ext);
                if p.is_file() {
                    return Some(p);
                }
            }

            // Also try the path as-is if it already has a non-TS extension (e.g., .css)
            if candidate.extension().is_some() && candidate.is_file() {
                return Some(candidate.to_path_buf());
            }
        }

        // 3. As a directory — look for index.ts/index.tsx
        if candidate.is_dir() {
            return Self::resolve_barrel(candidate);
        }

        // 4. Try as a directory by taking the stem (for when import is "./foo" and foo is a dir)
        let as_dir = PathBuf::from(candidate.to_str().unwrap_or(""));
        if as_dir.is_dir() {
            return Self::resolve_barrel(&as_dir);
        }

        None
    }

    /// Resolve a barrel file: directory → `<dir>/index.ts`/`<dir>/index.tsx`.
    fn resolve_barrel(dir: &Path) -> Option<PathBuf> {
        for ext in TS_EXTENSIONS {
            let barrel = dir.join(format!("index.{}", ext));
            if barrel.is_file() {
                return Some(barrel);
            }
        }
        None
    }

    /// Resolve a tsconfig path alias (e.g., `@/components/Button`).
    fn resolve_alias(&self, import_path: &str, tsconfig: &TsConfig) -> Option<PathBuf> {
        for (prefix, replacements) in &tsconfig.paths {
            // Wildcard pattern: "@/*" -> "src/*"
            if let Some(prefix_wild) = prefix.strip_suffix("/*") {
                if let Some(rest) = import_path.strip_prefix(prefix_wild) {
                    // rest starts with '/', strip it
                    let rest = rest.strip_prefix('/').unwrap_or(rest);
                    for replacement in replacements {
                        let replaced = replacement.replace("/*", &format!("/{}", rest));
                        let candidate = self.base_dir.join(&tsconfig.base_url).join(&replaced);
                        // Remove trailing "/*" if replacement didn't have wildcard
                        let clean = if replaced.ends_with("/*") {
                            candidate.parent().unwrap_or(&candidate).to_path_buf()
                        } else {
                            candidate
                        };
                        if let Some(resolved) = Self::try_resolve_path(&clean) {
                            return Some(resolved);
                        }
                    }
                }
            } else {
                // Non-wildcard pattern: "@utils" -> ["src/utils.ts"]
                if import_path == prefix {
                    for replacement in replacements {
                        let candidate = self.base_dir.join(&tsconfig.base_url).join(replacement);
                        if let Some(resolved) = Self::try_resolve_path(&candidate) {
                            return Some(resolved);
                        }
                    }
                }
            }
        }
        None
    }

    /// Load and parse `tsconfig.json`, stripping JSON comments.
    fn load_tsconfig(path: &Path) -> Option<TsConfig> {
        let content = std::fs::read_to_string(path).ok()?;
        let clean = Self::strip_json_comments(&content);
        let parsed: serde_json::Value = serde_json::from_str(&clean).ok()?;

        let compiler_options = parsed.get("compilerOptions")?;

        let base_url = compiler_options
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or(".")
            .to_string();

        let mut tsconfig = TsConfig {
            base_url: PathBuf::from(base_url),
            paths: Vec::new(),
        };

        if let Some(paths_obj) = compiler_options.get("paths").and_then(|v| v.as_object()) {
            for (key, value) in paths_obj {
                if let Some(replacements) = value.as_array() {
                    let reps: Vec<String> = replacements
                        .iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                    if !reps.is_empty() {
                        tsconfig.paths.push((key.clone(), reps));
                    }
                }
            }
        }

        Some(tsconfig)
    }

    /// Strip C-style comments (`//` and `/* */`) from a JSON string.
    ///
    /// Respects string boundaries so that `/*` inside a JSON string value
    /// (e.g., `"@/*"`) is NOT treated as a block comment opening.
    fn strip_json_comments(input: &str) -> String {
        let mut result = String::with_capacity(input.len());
        let bytes = input.as_bytes();
        let mut i = 0;
        let len = bytes.len();
        let mut in_string = false;

        while i < len {
            let c = bytes[i];

            // Track string state (handle escaped quotes)
            if c == b'"' && !in_string {
                in_string = true;
                result.push(c as char);
                i += 1;
                continue;
            }

            if in_string {
                result.push(c as char);
                // Handle escape sequences
                if c == b'\\' {
                    // Push the next character too
                    if i + 1 < len {
                        i += 1;
                        result.push(bytes[i] as char);
                    }
                    i += 1;
                    continue;
                }
                if c == b'"' {
                    in_string = false;
                }
                i += 1;
                continue;
            }

            // Not inside a string — check for comments
            // Line comment: //
            if i + 1 < len && c == b'/' && bytes[i + 1] == b'/' {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Block comment: /* */
            if i + 1 < len && c == b'/' && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                if i + 1 < len {
                    i += 2; // skip */
                }
                continue;
            }

            result.push(c as char);
            i += 1;
        }

        result
    }
}

/// Check if a file path has a non-code extension (CSS, image, etc.).
fn is_non_code_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| NON_CODE_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

/// Extract re-export sources from a TypeScript source file.
///
/// Returns the source paths from `export { X } from "./module"` and
/// `export * from "./module"` statements.
fn extract_re_export_sources(source: &str, ext: &str) -> Vec<String> {
    use streaming_iterator::StreamingIterator;

    let lang = match crate::grammar_for_extension(ext) {
        Some(l) => l,
        None => return Vec::new(),
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let query_source = r#"; Re-export source: any string child of export_statement source
(export_statement
  (string (string_fragment) @re_export_source))
"#;

    let query = match tree_sitter::Query::new(&lang, query_source) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut sources = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let name = query.capture_names()[capture.index as usize].to_string();
            if name == "re_export_source" {
                let text = capture
                    .node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                if !sources.contains(&text) {
                    sources.push(text);
                }
            }
        }
    }

    sources
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[allow(dead_code)]
    fn setup_resolver() -> ImportResolver {
        // Create a temporary directory simulating a TypeScript project
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();

        // Create src/ directory structure
        std::fs::create_dir_all(base.join("src/components")).unwrap();
        std::fs::create_dir_all(base.join("src/models")).unwrap();
        std::fs::create_dir_all(base.join("src/styles")).unwrap();
        std::fs::create_dir_all(base.join("tests")).unwrap();

        // Create source files
        let mut f = std::fs::File::create(base.join("src/calculator.ts")).unwrap();
        writeln!(
            f,
            "export function add(a: number, b: number) {{ return a + b; }}"
        )
        .unwrap();
        drop(f);

        let mut f = std::fs::File::create(base.join("src/utils.ts")).unwrap();
        writeln!(f, "export function helper() {{}}").unwrap();
        drop(f);

        let mut f = std::fs::File::create(base.join("src/components/Button.tsx")).unwrap();
        writeln!(f, "export const Button = () => null;").unwrap();
        drop(f);

        // Create barrel file
        let mut f = std::fs::File::create(base.join("src/models/index.ts")).unwrap();
        writeln!(f, "export {{ User }} from \"./user\";").unwrap();
        drop(f);

        let mut f = std::fs::File::create(base.join("src/models/user.ts")).unwrap();
        writeln!(f, "export interface User {{ name: string; age: number; }}").unwrap();
        drop(f);

        // Create a style file
        let mut f = std::fs::File::create(base.join("src/styles/button.css")).unwrap();
        writeln!(f, ".btn {{ color: red; }}").unwrap();
        drop(f);

        // Create test files
        let mut f = std::fs::File::create(base.join("tests/calculator.test.ts")).unwrap();
        writeln!(
            f,
            "import {{ add }} from \"../src/calculator\";\nimport \"./styles.css\";"
        )
        .unwrap();
        drop(f);

        let resolver = ImportResolver::new(base.as_ref());
        // Keep the temp dir alive by returning it along with resolver
        let _ = dir;
        resolver
    }

    // -----------------------------------------------------------------------
    // strip_json_comments tests
    // -----------------------------------------------------------------------

    #[test]
    fn strip_json_comments_no_comments() {
        let input = r#"{"compilerOptions": {"baseUrl": "."}}"#;
        let result = ImportResolver::strip_json_comments(input);
        assert_eq!(result, input);
    }

    #[test]
    fn strip_json_comments_line_comments() {
        let input = "{\n  // base URL for path resolution\n  \"compilerOptions\": {\n    \"baseUrl\": \".\" // relative to project\n  }\n}\n";
        let result = ImportResolver::strip_json_comments(input);
        // Whitespace before/after comment lines is preserved; that's fine for JSON
        assert!(!result.contains("//"), "should strip line comments");
        assert!(
            result.contains("\"compilerOptions\""),
            "should preserve JSON keys"
        );
        assert!(
            result.contains("\"baseUrl\""),
            "should preserve JSON values"
        );
    }

    #[test]
    fn strip_json_comments_block_comments() {
        let input = "{\n  /* compiler options */\n  \"compilerOptions\": {\n    /* base URL */\n    \"baseUrl\": \".\"\n  }\n}\n";
        let result = ImportResolver::strip_json_comments(input);
        assert!(!result.contains("/*"), "should strip block comments");
        assert!(
            result.contains("\"compilerOptions\""),
            "should preserve JSON keys"
        );
        assert!(
            result.contains("\"baseUrl\""),
            "should preserve JSON values"
        );
    }

    #[test]
    fn strip_json_comments_mixed() {
        let input = "// leading comment\n{\n  // line\n  /* block */\n  \"key\": \"value\" // trailing\n}\n";
        let result = ImportResolver::strip_json_comments(input);
        assert!(!result.contains("//"), "should strip line comments");
        assert!(!result.contains("/*"), "should strip block comments");
    }

    #[test]
    fn strip_json_comments_inside_string() {
        // Paths with /* (like @/*) should NOT be treated as comments
        let input = r#"{"paths": {"@/*": ["src/*"]}}"#;
        let result = ImportResolver::strip_json_comments(input);
        assert_eq!(result, input, "should preserve /* inside strings");

        // Paths with // inside strings should NOT be treated as comments
        let input2 = r#"{"url": "http://example.com"}"#;
        let result2 = ImportResolver::strip_json_comments(input2);
        assert_eq!(result2, input2, "should preserve // inside strings");
    }

    // -----------------------------------------------------------------------
    // load_tsconfig tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_tsconfig_with_paths() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig_path = dir.path().join("tsconfig.json");

        let content = r#"{
            "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                    "@/*": ["src/*"],
                    "~/*": ["src/*"],
                    "@components/*": ["src/components/*"]
                }
            }
        }"#;
        let mut f = std::fs::File::create(&tsconfig_path).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let tsconfig = ImportResolver::load_tsconfig(&tsconfig_path);
        assert!(tsconfig.is_some(), "should parse tsconfig.json");

        let tc = tsconfig.unwrap();
        assert_eq!(tc.base_url, PathBuf::from("."));
        assert_eq!(tc.paths.len(), 3);

        // Check by key (serde_json Map sorts alphabetically by default)
        let at_star = tc.paths.iter().find(|(k, _)| k == "@/*");
        assert!(at_star.is_some(), "should have @/* path");
        assert_eq!(at_star.unwrap().1, vec!["src/*"]);

        let at_components = tc.paths.iter().find(|(k, _)| k == "@components/*");
        assert!(at_components.is_some(), "should have @components/* path");

        let tilde_star = tc.paths.iter().find(|(k, _)| k == "~/*");
        assert!(tilde_star.is_some(), "should have ~/* path");
        assert_eq!(tilde_star.unwrap().1, vec!["src/*"]);
    }

    #[test]
    fn load_tsconfig_with_comments() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig_path = dir.path().join("tsconfig.json");

        let content = "// TypeScript config\n{\n  /* compiler options */\n  \"compilerOptions\": {\n    // base URL\n    \"baseUrl\": \".\",\n    \"paths\": {\n      \"@/*\": [\"src/*\"] // wildcard\n    }\n  }\n}\n";
        let mut f = std::fs::File::create(&tsconfig_path).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let tsconfig = ImportResolver::load_tsconfig(&tsconfig_path);
        assert!(
            tsconfig.is_some(),
            "should parse tsconfig.json with comments"
        );
    }

    #[test]
    fn load_tsconfig_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let bad_path = dir.path().join("nonexistent.json");
        let tsconfig = ImportResolver::load_tsconfig(&bad_path);
        assert!(tsconfig.is_none(), "should return None for missing file");
    }

    #[test]
    fn load_tsconfig_missing_compiler_options() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig_path = dir.path().join("tsconfig.json");
        let content = r#"{"extends": "./base.json"}"#;
        let mut f = std::fs::File::create(&tsconfig_path).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let tsconfig = ImportResolver::load_tsconfig(&tsconfig_path);
        assert!(
            tsconfig.is_none(),
            "should return None without compilerOptions"
        );
    }

    #[test]
    fn load_tsconfig_empty_paths() {
        let dir = tempfile::tempdir().unwrap();
        let tsconfig_path = dir.path().join("tsconfig.json");
        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {}}}"#;
        let mut f = std::fs::File::create(&tsconfig_path).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let tsconfig = ImportResolver::load_tsconfig(&tsconfig_path);
        assert!(tsconfig.is_some());
        assert!(tsconfig.unwrap().paths.is_empty());
    }

    // -----------------------------------------------------------------------
    // try_extensions / try_resolve_path tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_relative_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("foo.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./foo.ts");
        assert!(result.resolved.is_some(), "should resolve exact file");
        assert!(!result.is_non_code);
    }

    #[test]
    fn resolve_relative_extensionless() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("foo.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./foo");
        assert!(
            result.resolved.is_some(),
            "should resolve extensionless import"
        );
    }

    #[test]
    fn resolve_relative_barrel() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("models")).unwrap();
        let mut f = std::fs::File::create(base.join("models/index.ts")).unwrap();
        writeln!(f, "export {{ User }} from \"./user\";").unwrap();
        drop(f);
        let mut f = std::fs::File::create(base.join("models/user.ts")).unwrap();
        writeln!(f, "export interface User {{}}").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./models");
        assert!(
            result.resolved.is_some(),
            "should resolve to models/index.ts"
        );
        let resolved = result.resolved.unwrap();
        assert!(
            resolved.ends_with("models/index.ts") || resolved.ends_with("models\\index.ts"),
            "should be index.ts: {:?}",
            resolved
        );
    }

    #[test]
    fn resolve_relative_extension_fallback_order() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // Create .tsx but import .ts — should find .tsx
        let mut f = std::fs::File::create(base.join("component.tsx")).unwrap();
        writeln!(f, "export const C = () => null;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./component");
        assert!(result.resolved.is_some(), "should resolve to component.tsx");
        let resolved = result.resolved.as_ref().unwrap();
        assert!(
            resolved.to_string_lossy().ends_with("component.tsx"),
            "should prefer .tsx: {:?}",
            resolved
        );
    }

    #[test]
    fn resolve_non_code_css() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("styles.css")).unwrap();
        writeln!(f, "body {{}}").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./styles.css");
        assert!(result.resolved.is_some(), "should resolve CSS as non-code");
        assert!(result.is_non_code, "CSS should be marked non-code");
    }

    #[test]
    fn resolve_non_code_image() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let f = std::fs::File::create(base.join("logo.png")).unwrap();
        // Just create an empty file
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./logo.png");
        assert!(result.resolved.is_some(), "should resolve PNG");
        assert!(result.is_non_code, "PNG should be marked non-code");
    }

    #[test]
    fn resolve_package_import_external() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = ImportResolver::new(dir.path());
        let result = resolver.resolve(dir.path(), "lodash");
        assert!(
            result.resolved.is_none(),
            "package imports should not resolve"
        );
    }

    #[test]
    fn resolve_scoped_package_external() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = ImportResolver::new(dir.path());
        let result = resolver.resolve(dir.path(), "@angular/core");
        assert!(
            result.resolved.is_none(),
            "scoped packages should not resolve"
        );
    }

    #[test]
    fn resolve_nonexistent_relative() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = ImportResolver::new(dir.path());
        let result = resolver.resolve(dir.path(), "./nonexistent");
        assert!(result.resolved.is_none(), "nonexistent should not resolve");
    }

    #[test]
    fn resolve_relative_updir() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::create_dir_all(base.join("tests")).unwrap();

        let mut f = std::fs::File::create(base.join("src/utils.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(&base.join("tests"), "../src/utils");
        assert!(result.resolved.is_some(), "should resolve ../src/utils");
    }

    // -----------------------------------------------------------------------
    // tsconfig path alias resolution tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_alias_at_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src/components")).unwrap();
        let mut f = std::fs::File::create(base.join("src/components/Button.ts")).unwrap();
        writeln!(f, "export const Button = () => null;").unwrap();
        drop(f);

        // Write tsconfig.json
        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["src/*"]}}}"#;
        let mut f = std::fs::File::create(base.join("tsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        assert!(resolver.has_tsconfig(), "tsconfig should be loaded");

        let result = resolver.resolve(base, "@/components/Button");
        assert!(
            result.resolved.is_some(),
            "should resolve @ alias: {:?}",
            result.resolved
        );
        let resolved = result.resolved.unwrap();
        assert!(
            resolved
                .to_string_lossy()
                .ends_with("src/components/Button.ts")
                || resolved
                    .to_string_lossy()
                    .ends_with("src/components/Button.tsx"),
            "should resolve to Button.ts(x): {:?}",
            resolved
        );
    }

    #[test]
    fn resolve_alias_tilde_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src")).unwrap();
        let mut f = std::fs::File::create(base.join("src/utils.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        drop(f);

        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {"~/*": ["src/*"]}}}"#;
        let mut f = std::fs::File::create(base.join("tsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "~/utils");
        assert!(
            result.resolved.is_some(),
            "should resolve ~ alias: {:?}",
            result.resolved
        );
    }

    #[test]
    fn resolve_alias_explicit_component_path() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src/components")).unwrap();
        let mut f = std::fs::File::create(base.join("src/components/Button.tsx")).unwrap();
        writeln!(f, "export const Button = () => null;").unwrap();
        drop(f);

        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@components/*": ["src/components/*"]}}}"#;
        let mut f = std::fs::File::create(base.join("tsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "@components/Button");
        assert!(
            result.resolved.is_some(),
            "should resolve @components alias"
        );
    }

    #[test]
    fn resolve_alias_non_wildcard() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("utils.ts")).unwrap();
        writeln!(f, "export const util = 1;").unwrap();
        drop(f);

        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@utils": ["utils.ts"]}}}"#;
        let mut f = std::fs::File::create(base.join("tsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "@utils");
        assert!(
            result.resolved.is_some(),
            "should resolve non-wildcard alias @utils"
        );
    }

    #[test]
    fn resolve_no_tsconfig_alias_fails() {
        let dir = tempfile::tempdir().unwrap();
        let resolver = ImportResolver::new(dir.path());
        assert!(!resolver.has_tsconfig(), "no tsconfig should be loaded");

        let result = resolver.resolve(dir.path(), "@/components/Button");
        assert!(
            result.resolved.is_none(),
            "without tsconfig, @ alias should not resolve"
        );
    }

    #[test]
    fn resolve_alias_base_url() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src")).unwrap();
        let mut f = std::fs::File::create(base.join("src/app.ts")).unwrap();
        writeln!(f, "export const app = 1;").unwrap();
        drop(f);

        // baseUrl = "src", paths = {"@/*": ["./*"]}
        let content = r#"{"compilerOptions": {"baseUrl": "src", "paths": {"@/*": ["./*"]}}}"#;
        let mut f = std::fs::File::create(base.join("tsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "@/app");
        assert!(
            result.resolved.is_some(),
            "should resolve with baseUrl='src': {:?}",
            result.resolved
        );
    }

    // -----------------------------------------------------------------------
    // Re-export resolution tests
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_with_re_exports_one_hop() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // Create a barrel file re-exporting from a leaf module
        std::fs::create_dir_all(base.join("src")).unwrap();
        let mut f = std::fs::File::create(base.join("src/index.ts")).unwrap();
        writeln!(f, "export {{ coreFn }} from \"./core\";").unwrap();
        drop(f);

        let mut f = std::fs::File::create(base.join("src/core.ts")).unwrap();
        writeln!(f, "export function coreFn() {{ return 42; }}").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let mut visited = HashSet::new();

        let results = resolver.resolve_with_re_exports(base, "./src", &mut visited);
        assert!(!results.is_empty(), "should resolve through re-exports");
        let has_core = results.iter().any(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| n == "core.ts" || n == "core.tsx")
        });
        assert!(
            has_core,
            "should include src/core.ts in results: {:?}",
            results
        );
    }

    #[test]
    fn resolve_with_re_exports_circular_safety() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // Create circular re-exports: a.ts -> b.ts -> a.ts
        std::fs::create_dir_all(base.join("src")).unwrap();
        let mut f = std::fs::File::create(base.join("src/index.ts")).unwrap();
        writeln!(f, "export {{ a }} from \"./a\";").unwrap();
        drop(f);

        let mut f = std::fs::File::create(base.join("src/a.ts")).unwrap();
        writeln!(f, "export {{ b }} from \"./b\";\nexport const a = 1;").unwrap();
        drop(f);

        let mut f = std::fs::File::create(base.join("src/b.ts")).unwrap();
        writeln!(f, "export {{ a }} from \"./a\";\nexport const b = 2;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let mut visited = HashSet::new();

        // Should not hang or overflow
        let results = resolver.resolve_with_re_exports(base, "./src", &mut visited);
        assert!(!results.is_empty(), "should still produce results");
        // Should not loop infinitely (test timeout would catch this)
    }

    #[test]
    fn resolve_non_barrel_no_re_export_follow() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("utils.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let mut visited = HashSet::new();
        let results = resolver.resolve_with_re_exports(base, "./utils", &mut visited);
        assert_eq!(
            results.len(),
            1,
            "non-barrel file should return itself: {:?}",
            results
        );
    }

    // -----------------------------------------------------------------------
    // integration: static-deps scenarios
    // -----------------------------------------------------------------------

    #[test]
    fn resolver_returns_correct_edges() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::create_dir_all(base.join("tests")).unwrap();

        // Source file
        let mut f = std::fs::File::create(base.join("src/calculator.ts")).unwrap();
        writeln!(
            f,
            "export function add(a: number, b: number) {{ return a + b; }}"
        )
        .unwrap();
        drop(f);

        // Test file that imports it
        let mut f = std::fs::File::create(base.join("tests/calc.test.ts")).unwrap();
        writeln!(
            f,
            "import {{ add }} from \"../src/calculator\";\ndescribe(\"calc\", () => {{ it(\"adds\", () => {{}}); }});"
        )
        .unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);

        // Resolve the test's import path relative to the test's directory
        let test_dir = base.join("tests");
        let resolved = resolver.resolve(&test_dir, "../src/calculator");
        assert!(
            resolved.resolved.is_some(),
            "should resolve test import to source file"
        );

        let resolved_path = resolved.resolved.unwrap();
        assert!(
            resolved_path
                .to_string_lossy()
                .ends_with("src/calculator.ts")
                || resolved_path
                    .to_string_lossy()
                    .ends_with("src/calculator.tsx"),
            "should resolve to calculator.ts: {:?}",
            resolved_path
        );
    }

    // -----------------------------------------------------------------------
    // Non-code import edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_non_code_scss() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("styles.scss")).unwrap();
        writeln!(f, "$primary: blue;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./styles.scss");
        assert!(result.resolved.is_some());
        assert!(result.is_non_code);
    }

    #[test]
    fn resolve_non_code_svg() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("icon.svg")).unwrap();
        writeln!(f, "<svg></svg>").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./icon.svg");
        assert!(result.resolved.is_some());
        assert!(result.is_non_code);
    }

    // -----------------------------------------------------------------------
    // tsconfig with jsconfig.json fallback
    // -----------------------------------------------------------------------

    #[test]
    fn load_jsconfig_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        // Write jsconfig.json (not tsconfig.json)
        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["src/*"]}}}"#;
        let mut f = std::fs::File::create(base.join("jsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        assert!(
            resolver.has_tsconfig(),
            "should load jsconfig.json as fallback"
        );
    }

    // -----------------------------------------------------------------------
    // extract_re_export_sources tests
    // -----------------------------------------------------------------------

    #[test]
    fn extract_re_exports_from_source() {
        let source = r#"export { Button } from "./components/Button";
export * from "./helpers";
export * as Utils from "./utils";
"#;
        let sources = extract_re_export_sources(source, "ts");
        assert_eq!(
            sources.len(),
            3,
            "should find 3 re-export sources: {:?}",
            sources
        );
        assert!(sources.contains(&"./components/Button".to_string()));
        assert!(sources.contains(&"./helpers".to_string()));
        assert!(sources.contains(&"./utils".to_string()));
    }

    #[test]
    fn extract_re_exports_no_re_exports() {
        let source = "export const x = 1;\nexport function foo() {}\n";
        let sources = extract_re_export_sources(source, "ts");
        assert_eq!(sources.len(), 0, "should find no re-exports");
    }

    #[test]
    fn extract_re_exports_unsupported_ext() {
        let source = "export { X } from \"./module\";\n";
        let sources = extract_re_export_sources(source, "js");
        assert_eq!(sources.len(), 0, "unsupported ext returns empty");
    }

    #[test]
    fn resolve_alias_components_path() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        std::fs::create_dir_all(base.join("src/components")).unwrap();
        let mut f = std::fs::File::create(base.join("src/components/Button.tsx")).unwrap();
        writeln!(f, "export const Button = () => null;").unwrap();
        drop(f);

        let content = r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@components/*": ["src/components/*"]}}}"#;
        let mut f = std::fs::File::create(base.join("tsconfig.json")).unwrap();
        write!(f, "{}", content).unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "@components/Button");
        assert!(
            result.resolved.is_some(),
            "should resolve @components/Button"
        );
    }

    #[test]
    fn resolve_relative_with_hyphen() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let mut f = std::fs::File::create(base.join("my-component.ts")).unwrap();
        writeln!(f, "export const x = 1;").unwrap();
        drop(f);

        let resolver = ImportResolver::new(base);
        let result = resolver.resolve(base, "./my-component");
        assert!(
            result.resolved.is_some(),
            "should resolve hyphenated name: {:?}",
            result.resolved
        );
    }
}
