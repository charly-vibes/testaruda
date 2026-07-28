//! testaruda-adapter-typescript — Reference adapter for TypeScript projects.
//!
//! Reads JSON commands from stdin, responds on stdout.
//! Protocol: single JSON line → single JSON line response.
//!
//! Uses tree-sitter-typescript for parsing `.ts`/`.tsx`/`.mts`/`.cts` files,
//! extracting imports, test declarations, and exports via Scheme queries.

use std::io::{BufRead, BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_command(&trimmed);
        let out = serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_string());
        println!("{}", out);
        std::io::stdout().flush().ok();
    }
}

fn handle_command(input: &str) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return json_err(&format!("invalid JSON: {}", e)),
    };

    let command = cmd["command"].as_str().unwrap_or("");
    match command {
        "handshake" => cmd_handshake(),
        "discover" => cmd_discover(&cmd),
        "static-deps" => cmd_static_deps(&cmd),
        "fingerprint" => cmd_fingerprint(&cmd),
        "run-args" => cmd_run_args(&cmd),
        "ingest" => cmd_ingest(&cmd),
        _ => json_err(&format!("unknown command: {}", command)),
    }
}

fn json_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "result": result})
}

fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": msg})
}

/// Directories to exclude during file discovery.
const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    "dist",
    "build",
    ".next",
    ".git",
    "target",
    ".cache",
    ".nyc_output",
    "coverage",
    "__pycache__",
    ".venv",
];

/// Test file name suffixes for TypeScript projects.
fn is_test_file(name: &str) -> bool {
    name.ends_with(".test.ts")
        || name.ends_with(".spec.ts")
        || name.ends_with(".test.tsx")
        || name.ends_with(".spec.tsx")
        || name.ends_with(".test.mts")
        || name.ends_with(".spec.mts")
        || name.ends_with(".test.cts")
        || name.ends_with(".spec.cts")
}

/// Check if a path is inside a __tests__ or __test__ directory.
fn is_in_test_dir(path: &std::path::Path) -> bool {
    path.components().any(|c| {
        let name = c.as_os_str().to_string_lossy();
        name == "__tests__" || name == "__test__"
    })
}

/// Parse a TypeScript/TSX source file and return test items from discover.scm.
fn parse_test_file(path: &str, source: &str, ext: &str) -> Vec<serde_json::Value> {
    use streaming_iterator::StreamingIterator;

    let lang = match grammar_for_extension(ext) {
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

    let query_source = include_str!("../queries/discover.scm");
    let query = match tree_sitter::Query::new(&lang, query_source) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    // Build a stack of describe chains. Each match gives us test_declaration + test_name.
    // We track the current nesting depth by the position of each declaration.
    let mut tests = Vec::new();
    let mut chain: Vec<String> = Vec::new();
    let mut chain_depth: Vec<usize> = Vec::new(); // depth of each chain element

    while let Some(m) = matches.next() {
        let mut test_name: Option<String> = None;
        let mut depth: Option<usize> = None;

        for capture in m.captures {
            let name = query.capture_names()[capture.index as usize].to_string();
            match name.as_str() {
                "test_name" => {
                    let text = capture
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    test_name = Some(text);
                    depth = Some(capture.node.start_position().row);
                }
                "test_declaration" => {
                    // declaration marker — name is attached via test_name
                }
                _ => {}
            }
        }

        if let Some(name) = test_name {
            let row = depth.unwrap_or(0);

            // Pop chain elements that are deeper than current row
            while !chain_depth.is_empty() && *chain_depth.last().unwrap() >= row {
                chain.pop();
                chain_depth.pop();
            }

            // Push the current test name
            chain.push(name.clone());
            chain_depth.push(row);

            // Emit a test item
            let node_id = format!("{}::{}", path, chain.join("::"));
            tests.push(serde_json::json!({
                "node_id": node_id,
                "suite_kind": "unit",
                "file": path,
            }));
        }
    }

    tests
}

/// Select the tree-sitter grammar for a given file extension.
pub fn grammar_for_extension(ext: &str) -> Option<tree_sitter::Language> {
    match ext {
        "ts" | "mts" | "cts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        _ => None,
    }
}

/// Run a tree-sitter query against a parsed tree and return captured nodes.
///
/// Returns a list of (capture_name, row, column, text) tuples for each captured node.
pub fn run_query_with_lang(
    query_source: &str,
    tree: &tree_sitter::Tree,
    source: &str,
    lang: &tree_sitter::Language,
) -> Vec<(String, usize, usize, String)> {
    use streaming_iterator::StreamingIterator;

    let query = match tree_sitter::Query::new(lang, query_source) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut results = Vec::new();
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let name = query.capture_names()[capture.index as usize].to_string();
            let node = capture.node;
            let start = node.start_position();
            let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            results.push((name, start.row, start.column, text));
        }
    }
    results
}

/// Handshake: declare capabilities (TIA-ADAPT-020).
fn cmd_handshake() -> serde_json::Value {
    json_ok(serde_json::json!({
        "name": "typescript-adapter",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["typescript"],
        "granularity": "file",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}

/// Discover: find test files by convention and parse test declarations.
fn cmd_discover(_cmd: &serde_json::Value) -> serde_json::Value {
    let mut tests = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !EXCLUDED_DIRS.contains(&name.as_ref())
        })
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path).to_string();
        let fname = entry.file_name().to_string_lossy().to_string();

        // Determine extension
        let ext = match path.rsplit_once('.') {
            Some((_, e)) => e,
            None => continue,
        };

        let is_test_by_name = is_test_file(&fname) || is_in_test_dir(entry.path());

        if !is_test_by_name {
            continue;
        }

        // Read the file content
        let contents = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => {
                // Add file-level fallback
                tests.push(serde_json::json!({
                    "node_id": clean_path,
                    "suite_kind": "unit",
                    "file": clean_path,
                }));
                continue;
            }
        };

        // Parse with tree-sitter and discover test declarations
        let parsed = parse_test_file(&clean_path, &contents, ext);
        if parsed.is_empty() {
            // No test declarations found — add file-level fallback
            tests.push(serde_json::json!({
                "node_id": clean_path,
                "suite_kind": "unit",
                "file": clean_path,
            }));
        } else {
            tests.extend(parsed);
        }
    }

    json_ok(serde_json::Value::Array(tests))
}

/// Static deps: extract import/require expressions from changed files.
fn cmd_static_deps(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let changed_files: Vec<String> = match params["changed_files"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.changed_files'"),
    };

    let mut unresolved = Vec::new();
    let mut edges = Vec::new();

    // Discover all test items
    let discover_all = cmd_discover(&serde_json::json!({}));
    let test_files: Vec<(String, String)> = if let Some(results) = discover_all["result"].as_array()
    {
        results
            .iter()
            .filter_map(|t| {
                let node_id = t["node_id"].as_str()?;
                let file = t["file"].as_str()?;
                Some((node_id.to_string(), file.to_string()))
            })
            .collect()
    } else {
        Vec::new()
    };

    // Build a map: import path -> list of (test_node_id, test_file_path)
    // For each test file, parse its imports
    let mut import_to_tests: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for (node_id, file) in &test_files {
        if let Ok(content) = std::fs::read_to_string(file) {
            let ext = file.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            let imports = parse_imports_from_source(&content, ext);
            for imp in imports {
                import_to_tests
                    .entry(imp)
                    .or_default()
                    .push((node_id.clone(), file.clone()));
            }
        }
    }

    // For each changed file, check if any test file imports from it
    for file in &changed_files {
        // Derive the import path from the file path
        // e.g., "src/component.ts" -> "./component" or "../component"
        let file_stem = std::path::Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        // Check if any test file imports this file
        // Try matching by stem (simple heuristic)
        let matching_tests = import_to_tests.get(&file_stem).cloned().or_else(|| {
            // Also try matching by path without extension
            let path_no_ext = file
                .rsplit_once('.')
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            import_to_tests.get(&path_no_ext).cloned()
        });

        if let Some(tests) = matching_tests {
            for (test_node_id, _test_file) in &tests {
                edges.push(serde_json::json!({
                    "from": test_node_id,
                    "to": file,
                    "weight": 1_000_000,
                    "origin": "static",
                }));
            }
        } else {
            // If the changed file is itself a test file, create self-edge
            let is_test = test_files.iter().any(|(_, f)| f == file);
            if is_test {
                if let Some(node_id) = test_files.iter().find(|(_, f)| f == file).map(|(n, _)| n) {
                    edges.push(serde_json::json!({
                        "from": node_id,
                        "to": file,
                        "weight": 1_000_000,
                        "origin": "static",
                    }));
                }
            } else {
                unresolved.push(file.clone());
            }
        }
    }

    // Candidate tests: all discovered test node IDs
    let candidates: Vec<String> = test_files.iter().map(|(n, _)| n.clone()).collect();

    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": [],
    })
}

/// Parse import sources from a TypeScript source file using imports.scm.
fn parse_imports_from_source(source: &str, ext: &str) -> Vec<String> {
    use streaming_iterator::StreamingIterator;

    let lang = match grammar_for_extension(ext) {
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

    let query_source = include_str!("../queries/imports.scm");
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
            match name.as_str() {
                "import_source" | "require_source" | "dynamic_import_source" => {
                    let text = capture
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    if !sources.contains(&text) {
                        sources.push(text);
                    }
                }
                _ => {}
            }
        }
    }

    sources
}

/// Static deps: extract import/require expressions from changed files.
/// (Stub for scaffolding — will be implemented with tree-sitter queries.)
/// Fingerprint: compute blake3 hash of file contents.
fn cmd_fingerprint(cmd: &serde_json::Value) -> serde_json::Value {
    let path = match cmd["path"].as_str() {
        Some(p) => p,
        None => return json_err("missing 'path' field"),
    };

    let contents = match std::fs::read(path) {
        Ok(c) => c,
        Err(e) => return json_err(&format!("failed to read file: {}", e)),
    };

    let hash = blake3::hash(&contents);
    json_ok(serde_json::json!({
        "path": path,
        "fingerprint": hash.to_hex().to_string()
    }))
}

/// Run args: detect test runner and build command-line arguments.
/// (Stub for scaffolding — will be implemented with runner detection.)
fn cmd_run_args(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "runner": "vitest",
        "args": ["npx", "vitest", "run", "--reporter=junit"],
        "warnings": ["run-args not yet fully implemented"]
    }))
}

/// Ingest: parse test runner output (JUnit XML) into runtime edges.
/// (Stub for scaffolding — will be implemented with JUnit parsing.)
fn cmd_ingest(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "results": [],
        "edges": [],
        "warnings": ["ingest not yet implemented"]
    }))
}

#[cfg(test)]
mod tests {
    use super::{grammar_for_extension, run_query_with_lang};
    use tree_sitter::Parser;

    fn ts_language() -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn parse(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        parser.set_language(&ts_language()).unwrap();
        parser.parse(source, None).unwrap()
    }

    #[test]
    fn grammar_selector_ts() {
        let lang = grammar_for_extension("ts").unwrap();
        assert!(lang.node_kind_count() > 100);
    }

    #[test]
    fn grammar_selector_tsx() {
        let lang = grammar_for_extension("tsx").unwrap();
        assert!(lang.node_kind_count() > 0);
    }

    #[test]
    fn grammar_selector_mts_cts() {
        assert!(grammar_for_extension("mts").is_some());
        assert!(grammar_for_extension("cts").is_some());
    }

    #[test]
    fn grammar_selector_unsupported() {
        assert!(grammar_for_extension("js").is_none());
        assert!(grammar_for_extension("jsx").is_none());
        assert!(grammar_for_extension("py").is_none());
    }

    fn run_query_test(
        query_source: &str,
        tree: &tree_sitter::Tree,
        source: &str,
    ) -> Vec<(String, usize, usize, String)> {
        let lang = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        run_query_with_lang(query_source, tree, source, &lang)
    }

    // ========================================================================
    // discover.scm tests
    // ========================================================================

    const DISCOVER_QUERY: &str = include_str!("../queries/discover.scm");

    #[test]
    fn discover_describe_block() {
        let source =
            "describe(\"UserService\", () => {\n    it(\"returns user\", () => {});\n});\n";
        let tree = parse(source);
        let results = run_query_test(DISCOVER_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "test_declaration"),
            "should capture describe block: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _, _)| n == "test_name"),
            "should capture test name: {:?}",
            results
        );
    }

    #[test]
    fn discover_it_and_test() {
        let source = "it(\"should add\", () => { expect(1 + 1).toBe(2); });\ntest(\"should subtract\", () => { expect(2 - 1).toBe(1); });\n";
        let tree = parse(source);
        let results = run_query_test(DISCOVER_QUERY, &tree, source);

        let names: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "test_name")
            .map(|_| "")
            .collect();
        assert_eq!(names.len(), 2, "should find 2 test names: {:?}", results);
    }

    #[test]
    fn discover_each_parameterized() {
        let source = "describe.each([1, 2, 3])(\"number %d\", (n) => {\n    it(\"is positive\", () => { expect(n).toBeGreaterThan(0); });\n});\n";
        let tree = parse(source);
        let results = run_query_test(DISCOVER_QUERY, &tree, source);

        let decls: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "test_declaration")
            .map(|_| "")
            .collect();
        assert!(
            decls.len() >= 2,
            "should capture describe.each AND inner it: {:?}",
            results
        );
    }

    #[test]
    fn discover_skipped_not_captured() {
        let source = "describe.skip(\"skip suite\", () => {});\nit.skip(\"skip test\", () => {});\ntest.skip(\"skip test\", () => {});\n";
        let tree = parse(source);
        let results = run_query_test(DISCOVER_QUERY, &tree, source);

        let decls: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "test_declaration")
            .map(|_| "")
            .collect();
        assert_eq!(
            decls.len(),
            0,
            "should NOT capture .skip calls: {:?}",
            results
        );
    }

    #[test]
    fn discover_negative_non_test_functions() {
        let source = "function setup() {}\nconst x = compute(42);\nconsole.log(\"hello\");\n";
        let tree = parse(source);
        let results = run_query_test(DISCOVER_QUERY, &tree, source);

        let decls: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "test_declaration")
            .map(|_| "")
            .collect();
        assert_eq!(
            decls.len(),
            0,
            "should not capture non-test calls: {:?}",
            results
        );
    }

    #[test]
    fn discover_nested_describe() {
        let source = "describe(\"outer\", () => {\n    describe(\"inner\", () => {\n        it(\"works\", () => {});\n    });\n});\n";
        let tree = parse(source);
        let results = run_query_test(DISCOVER_QUERY, &tree, source);

        let names: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "test_name")
            .map(|_| "")
            .collect();
        assert_eq!(names.len(), 3, "should capture all 3 names: {:?}", results);
    }

    // ========================================================================
    // imports.scm tests
    // ========================================================================

    const IMPORTS_QUERY: &str = include_str!("../queries/imports.scm");

    #[test]
    fn imports_es_module_default() {
        let source = "import React from \"react\";\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "import_source"),
            "should capture import source: {:?}",
            results
        );
    }

    #[test]
    fn imports_named_and_namespace() {
        let source =
            "import { useState, useEffect } from \"react\";\nimport * as Lodash from \"lodash\";\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        let sources: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "import_source")
            .map(|_| "")
            .collect();
        assert_eq!(
            sources.len(),
            2,
            "should capture both sources: {:?}",
            results
        );
    }

    #[test]
    fn imports_relative_path() {
        let source = "import { Button } from \"./components/Button\";\nimport { greet } from \"../utils/helpers\";\nimport config from \"/absolute/path/config\";\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        let sources: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "import_source")
            .map(|_| "")
            .collect();
        assert_eq!(
            sources.len(),
            3,
            "should capture all 3 imports: {:?}",
            results
        );
    }

    #[test]
    fn imports_type_only() {
        let source = "import type { User } from \"./types\";\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        let sources: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "import_source")
            .map(|_| "")
            .collect();
        assert_eq!(
            sources.len(),
            1,
            "should capture type-only import: {:?}",
            results
        );
    }

    #[test]
    fn imports_require_call() {
        let source = "const fs = require(\"fs\");\nconst path = require(\"path\");\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        let sources: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "require_source")
            .map(|_| "")
            .collect();
        assert_eq!(
            sources.len(),
            2,
            "should capture require() calls: {:?}",
            results
        );
    }

    #[test]
    fn imports_dynamic_import() {
        let source = "const mod = import(\"./dynamic-module\");\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        let sources: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "dynamic_import_source")
            .map(|_| "")
            .collect();
        assert_eq!(
            sources.len(),
            1,
            "should capture dynamic import: {:?}",
            results
        );
    }

    #[test]
    fn imports_side_effect() {
        let source = "import \"./styles.css\";\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        let sources: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "import_source")
            .map(|_| "")
            .collect();
        assert_eq!(
            sources.len(),
            1,
            "should capture side-effect import: {:?}",
            results
        );
    }

    #[test]
    fn imports_negative_no_imports() {
        let source = "const x = 42;\nfunction foo() { return x; }\n";
        let tree = parse(source);
        let results = run_query_test(IMPORTS_QUERY, &tree, source);

        assert_eq!(
            results.len(),
            0,
            "should not capture non-import code: {:?}",
            results
        );
    }

    // ========================================================================
    // exports.scm tests
    // ========================================================================

    const EXPORTS_QUERY: &str = include_str!("../queries/exports.scm");

    #[test]
    fn exports_named_const() {
        let source = "export const PI = 3.14;\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "export_name"),
            "should capture export name: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _, _)| n == "export_decl"),
            "should capture export declaration: {:?}",
            results
        );
    }

    #[test]
    fn exports_function() {
        let source =
            "export function greet(name: string): string {\n    return `Hello, ${name}!`;\n}\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "export_name"),
            "should capture function export name: {:?}",
            results
        );
    }

    #[test]
    fn exports_class() {
        let source = "export class UserService {\n    constructor(private db: DB) {}\n}\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "export_name"),
            "should capture class export name: {:?}",
            results
        );
    }

    #[test]
    fn exports_export_list() {
        let source = "const foo = 1;\nconst bar = 2;\nexport { foo, bar };\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        let names: Vec<&str> = results
            .iter()
            .filter(|(n, _, _, _)| n == "export_name")
            .map(|_| "")
            .collect();
        assert_eq!(
            names.len(),
            2,
            "should capture both export names: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _, _)| n == "export_clause"),
            "should capture export clause: {:?}",
            results
        );
    }

    #[test]
    fn exports_default() {
        let source = "export default function() {\n    return 42;\n}\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "export_default"),
            "should capture default export: {:?}",
            results
        );
    }

    #[test]
    fn exports_wildcard_re_export() {
        let source = "export * from \"./helpers\";\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "re_export_stmt"),
            "should capture re-export statement: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _, _)| n == "re_export_source"),
            "should capture re-export source: {:?}",
            results
        );
    }

    #[test]
    fn exports_wildcard_as_re_export() {
        let source = "export * as Utils from \"./utils\";\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "re_export_name"),
            "should capture re-export namespace name: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _, _)| n == "re_export_source"),
            "should capture re-export source: {:?}",
            results
        );
    }

    #[test]
    fn exports_negative_no_exports() {
        let source = "const x = 42;\nfunction foo() { return x; }\n";
        let tree = parse(source);
        let results = run_query_test(EXPORTS_QUERY, &tree, source);

        assert_eq!(
            results.len(),
            0,
            "should not capture non-export code: {:?}",
            results
        );
    }

    #[test]
    fn run_query_with_lang_tsx() {
        let source = "describe(\"tsx test\", () => { it(\"works\", () => {}); });\n";
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter_typescript::LANGUAGE_TSX.into();
        parser.set_language(&lang).unwrap();
        let tree = parser.parse(source, None).unwrap();
        let results = run_query_with_lang(DISCOVER_QUERY, &tree, source, &lang);

        assert!(
            results.iter().any(|(n, _, _, _)| n == "test_declaration"),
            "should discover tests in TSX: {:?}",
            results
        );
    }

    #[test]
    fn run_query_with_lang_error_handling() {
        let source = "const x = 1;\n";
        let tree = parse(source);
        // Invalid query should return empty vec, not panic
        let results = run_query_with_lang("invalid syntax (((", &tree, source, &ts_language());
        assert_eq!(
            results.len(),
            0,
            "invalid query should return empty results"
        );
    }

    // ========================================================================
    // discover helper tests
    // ========================================================================

    #[test]
    fn is_test_file_various_patterns() {
        assert!(super::is_test_file("foo.test.ts"));
        assert!(super::is_test_file("foo.spec.ts"));
        assert!(super::is_test_file("foo.test.tsx"));
        assert!(super::is_test_file("foo.spec.tsx"));
        assert!(super::is_test_file("foo.test.mts"));
        assert!(super::is_test_file("foo.spec.mts"));
        assert!(super::is_test_file("foo.test.cts"));
        assert!(super::is_test_file("foo.spec.cts"));
        assert!(!super::is_test_file("foo.ts"));
        assert!(!super::is_test_file("foo.tsx"));
        assert!(!super::is_test_file("foo.utils.ts"));
    }

    #[test]
    fn is_in_test_dir_various_paths() {
        use std::path::Path;
        assert!(super::is_in_test_dir(Path::new("src/__tests__/foo.ts")));
        assert!(super::is_in_test_dir(Path::new("src/__test__/foo.ts")));
        assert!(super::is_in_test_dir(Path::new("__tests__/foo.ts")));
        assert!(!super::is_in_test_dir(Path::new("src/tests/foo.ts")));
        assert!(!super::is_in_test_dir(Path::new("src/foo.ts")));
    }

    #[test]
    fn parse_test_file_simple_describe() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.ts");
        let mut f = std::fs::File::create(&file_path).unwrap();
        write!(
            f,
            "describe(\"suite\", () => {{ it(\"test\", () => {{}}); }});\n"
        )
        .unwrap();
        drop(f);

        let path = file_path.to_string_lossy();
        let source = std::fs::read_to_string(&file_path).unwrap();
        let tests = super::parse_test_file(&path, &source, "ts");

        assert!(!tests.is_empty(), "should find tests in describe block");
        let has_test = tests.iter().any(|t| {
            t["node_id"]
                .as_str()
                .map_or(false, |id| id.contains("test"))
        });
        assert!(has_test, "should contain test name in node_id");
    }

    #[test]
    fn parse_test_file_no_tests() {
        let source = "const x = 1;\nfunction foo() { return x; }\n";
        let tests = super::parse_test_file("foo.ts", source, "ts");
        assert_eq!(tests.len(), 0, "no test declarations found");
    }

    #[test]
    fn parse_test_file_unsupported_extension() {
        let source = "const x = 1;\n";
        let tests = super::parse_test_file("foo.js", source, "js");
        assert_eq!(tests.len(), 0, "unsupported extension returns empty");
    }

    // ========================================================================
    // static-deps tests
    // ========================================================================

    #[test]
    fn parse_imports_es_module() {
        let source = "import React from \"react\";\nimport { useState } from \"react\";\n";
        let imports = super::parse_imports_from_source(source, "ts");
        assert_eq!(
            imports.len(),
            1,
            "both imports from 'react' should deduplicate: {:?}",
            imports
        );
        assert!(imports.contains(&"react".to_string()));
    }

    #[test]
    fn parse_imports_relative_and_package() {
        let source = "import { Button } from \"./components/Button\";\nimport fs from \"fs\";\nimport { greet } from \"../utils\";\n";
        let imports = super::parse_imports_from_source(source, "ts");
        assert_eq!(imports.len(), 3, "should find all 3 imports: {:?}", imports);
    }

    #[test]
    fn parse_imports_require_and_dynamic() {
        let source = "const x = require(\"./utils\");\nconst y = import(\"./dynamic\");\n";
        let imports = super::parse_imports_from_source(source, "ts");
        assert_eq!(
            imports.len(),
            2,
            "should find require and dynamic import: {:?}",
            imports
        );
    }

    #[test]
    fn parse_imports_no_imports() {
        let source = "const x = 1;\nfunction foo() { return x; }\n";
        let imports = super::parse_imports_from_source(source, "ts");
        assert_eq!(imports.len(), 0, "no imports found");
    }

    #[test]
    fn parse_imports_unsupported_extension() {
        let source = "import React from \"react\";\n";
        let imports = super::parse_imports_from_source(source, "js");
        assert_eq!(imports.len(), 0, "unsupported extension returns empty");
    }
}
