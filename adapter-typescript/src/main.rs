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
/// Returns a list of (capture_name, row, column) tuples for each captured node.
pub fn run_query_with_lang(
    query_source: &str,
    tree: &tree_sitter::Tree,
    source: &str,
    lang: &tree_sitter::Language,
) -> Vec<(String, usize, usize)> {
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
            results.push((name, start.row, start.column));
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
/// (Stub for scaffolding — will be implemented with tree-sitter queries.)
fn cmd_discover(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "tests": [],
        "warnings": ["discover not yet implemented"]
    }))
}

/// Static deps: extract import/require expressions from changed files.
/// (Stub for scaffolding — will be implemented with tree-sitter queries.)
fn cmd_static_deps(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "edges": [],
        "warnings": ["static-deps not yet implemented"]
    }))
}

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
    ) -> Vec<(String, usize, usize)> {
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
            results.iter().any(|(n, _, _)| n == "test_declaration"),
            "should capture describe block: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _)| n == "test_name"),
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
            .filter(|(n, _, _)| n == "test_name")
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
            .filter(|(n, _, _)| n == "test_declaration")
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
            .filter(|(n, _, _)| n == "test_declaration")
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
            .filter(|(n, _, _)| n == "test_declaration")
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
            .filter(|(n, _, _)| n == "test_name")
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
            results.iter().any(|(n, _, _)| n == "import_source"),
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
            .filter(|(n, _, _)| n == "import_source")
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
            .filter(|(n, _, _)| n == "import_source")
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
            .filter(|(n, _, _)| n == "import_source")
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
            .filter(|(n, _, _)| n == "require_source")
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
            .filter(|(n, _, _)| n == "dynamic_import_source")
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
            .filter(|(n, _, _)| n == "import_source")
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
            results.iter().any(|(n, _, _)| n == "export_name"),
            "should capture export name: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _)| n == "export_decl"),
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
            results.iter().any(|(n, _, _)| n == "export_name"),
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
            results.iter().any(|(n, _, _)| n == "export_name"),
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
            .filter(|(n, _, _)| n == "export_name")
            .map(|_| "")
            .collect();
        assert_eq!(
            names.len(),
            2,
            "should capture both export names: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _)| n == "export_clause"),
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
            results.iter().any(|(n, _, _)| n == "export_default"),
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
            results.iter().any(|(n, _, _)| n == "re_export_stmt"),
            "should capture re-export statement: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _)| n == "re_export_source"),
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
            results.iter().any(|(n, _, _)| n == "re_export_name"),
            "should capture re-export namespace name: {:?}",
            results
        );
        assert!(
            results.iter().any(|(n, _, _)| n == "re_export_source"),
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
            results.iter().any(|(n, _, _)| n == "test_declaration"),
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
}
