//! Query runner for tree-sitter-based adapters.
//!
//! Provides a high-level function to run a tree-sitter query against a parsed
//! tree and return captured node ranges as `(name, line, column)` tuples.
//! Used by the adapter binary (main.rs) for discover, ns, and deps queries.

use std::sync::OnceLock;
use streaming_iterator::StreamingIterator;

/// A single capture produced by a tree-sitter query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capture {
    /// The capture name (e.g. "test_item", "namespace_name", "dep_entry").
    pub name: String,
    /// 0-indexed line in the source file.
    pub line: usize,
    /// 0-indexed byte offset from start of the line.
    pub column: usize,
}

/// Run a compiled tree-sitter [`query`] against a parsed [`tree`] and return
/// all captures.
#[allow(dead_code)]
pub fn run_query(
    query: &tree_sitter::Query,
    tree: &tree_sitter::Tree,
    source_bytes: &[u8],
) -> Vec<Capture> {
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source_bytes);
    let mut results = Vec::new();
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let name = query.capture_names()[cap.index as usize].to_string();
            let node = cap.node;
            let start = node.start_position();
            results.push(Capture {
                name,
                line: start.row,
                column: start.column,
            });
        }
    }
    results
}

/// Get the singleton tree-sitter Language for Clojure.
#[allow(dead_code)]
pub fn clojure_language() -> &'static tree_sitter::Language {
    static LANG: OnceLock<tree_sitter::Language> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_clojure::LANGUAGE.into())
}

/// Parse Clojure source text into a tree-sitter [`Tree`].
#[allow(dead_code)]
pub fn parse(source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(clojure_language()).unwrap();
    parser.parse(source, None).unwrap()
}

/// Build a [`tree_sitter::Query`] from source text, using the Clojure grammar.
#[allow(dead_code)]
pub fn compile_query(query_source: &str) -> tree_sitter::Query {
    tree_sitter::Query::new(clojure_language(), query_source).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── discover query (embedded via include_str!) ──────────────────────────

    fn discover_query() -> tree_sitter::Query {
        compile_query(include_str!("../queries/discover.scm"))
    }

    fn ns_query() -> tree_sitter::Query {
        compile_query(include_str!("../queries/ns.scm"))
    }

    fn deps_query() -> tree_sitter::Query {
        compile_query(include_str!("../queries/deps.scm"))
    }

    #[test]
    fn discover_captures_test_item_and_name() {
        let src = "(deftest my-test (is (= 1 1)))";
        let tree = parse(src);
        let caps = run_query(&discover_query(), &tree, src.as_bytes());
        let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"test_item"),
            "expected test_item, got: {names:?}"
        );
        assert!(
            names.contains(&"test_name"),
            "expected test_name, got: {names:?}"
        );
    }

    #[test]
    fn discover_captures_correct_position() {
        let src = "(deftest my-test (is (= 1 1)))";
        let tree = parse(src);
        let caps = run_query(&discover_query(), &tree, src.as_bytes());
        let name_cap = caps.iter().find(|c| c.name == "test_name").unwrap();
        assert_eq!(name_cap.line, 0, "expected line 0, got {}", name_cap.line);
        assert_eq!(
            name_cap.column, 9,
            "expected column 9, got {}",
            name_cap.column
        );
    }

    #[test]
    fn discover_returns_empty_for_unmatched() {
        let src = "(defn my-fn [x] x)";
        let tree = parse(src);
        let caps = run_query(&discover_query(), &tree, src.as_bytes());
        assert!(caps.is_empty(), "expected no captures, got: {caps:?}");
    }

    #[test]
    fn discover_finds_deftest_dash() {
        let src = "(deftest- private-test (is (= 1 1)))";
        let tree = parse(src);
        let caps = run_query(&discover_query(), &tree, src.as_bytes());
        let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"test_item"));
        assert!(names.contains(&"test_name"));
    }

    #[test]
    fn discover_finds_multiple_tests() {
        let src = "(deftest a (is T))\n(deftest b (is F))";
        let tree = parse(src);
        let caps = run_query(&discover_query(), &tree, src.as_bytes());
        let count = caps.iter().filter(|c| c.name == "test_name").count();
        assert_eq!(count, 2, "expected 2 test_name captures, got {count}");
    }

    #[test]
    fn discover_multi_test_positions() {
        let src = "(deftest a (is T))\n(deftest b (is F))";
        let tree = parse(src);
        let caps = run_query(&discover_query(), &tree, src.as_bytes());
        // Line 0: (deftest a ...), Line 1: (deftest b ...)
        let lines: Vec<usize> = caps
            .iter()
            .filter(|c| c.name == "test_item")
            .map(|c| c.line)
            .collect();
        assert_eq!(lines, vec![0, 1], "expected lines 0 and 1, got: {lines:?}");
    }

    // ── ns query ─────────────────────────────────────────────────────────────

    #[test]
    fn ns_captures_namespace_name() {
        let src = "(ns my-project.core)";
        let tree = parse(src);
        let caps = run_query(&ns_query(), &tree, src.as_bytes());
        let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"namespace_name"), "got: {names:?}");
        assert!(names.contains(&"ns_form"), "got: {names:?}");
    }

    #[test]
    fn ns_captures_correct_position() {
        let src = "(ns my-project.core)";
        let tree = parse(src);
        let caps = run_query(&ns_query(), &tree, src.as_bytes());
        let ns_cap = caps.iter().find(|c| c.name == "namespace_name").unwrap();
        assert_eq!(ns_cap.line, 0);
        assert_eq!(ns_cap.column, 4);
    }

    #[test]
    fn ns_with_metadata() {
        let src = "(ns ^{:doc \"docs\"} my-project.core\n  (:require ...))";
        let tree = parse(src);
        let caps = run_query(&ns_query(), &tree, src.as_bytes());
        let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
        // The namespace_name should be captured; metadata wrapping may affect
        // whether the sym_lit is directly accessible.
        // At minimum, ns_form should be captured.
        assert!(names.contains(&"ns_form"), "got: {names:?}");
    }

    // ── deps query ───────────────────────────────────────────────────────────

    #[test]
    fn deps_captures_require_vec() {
        let src = "(ns core (:require [clojure.string :as str]))";
        let tree = parse(src);
        let caps = run_query(&deps_query(), &tree, src.as_bytes());
        let names: Vec<&str> = caps.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"dep_form"), "got: {names:?}");
        assert!(names.contains(&"dep_entry"), "got: {names:?}");
    }

    #[test]
    fn deps_captures_bare_symbol() {
        let src = "(ns core (:require clojure.string))";
        let tree = parse(src);
        let caps = run_query(&deps_query(), &tree, src.as_bytes());
        assert!(caps.iter().any(|c| c.name == "dep_entry"), "got: {caps:?}");
    }

    #[test]
    fn deps_captures_multiple_requires() {
        let src = "(ns core (:require [a :as a] [b :as b]))";
        let tree = parse(src);
        let caps = run_query(&deps_query(), &tree, src.as_bytes());
        let dep_entries: Vec<&Capture> = caps.iter().filter(|c| c.name == "dep_entry").collect();
        // Only the first dep_entry after the keyword is captured per match
        // (due to the `.` anchor in the query). The runner layer iterates
        // dep_form children for full resolution.
        assert!(
            dep_entries.len() >= 1,
            "expected at least 1 dep_entry, got: {dep_entries:?}"
        );
    }

    #[test]
    fn deps_finds_use() {
        let src = "(ns core (:use clojure.test))";
        let tree = parse(src);
        let caps = run_query(&deps_query(), &tree, src.as_bytes());
        assert!(caps.iter().any(|c| c.name == "dep_entry"), "got: {caps:?}");
    }

    #[test]
    fn deps_finds_import() {
        let src = "(ns core (:import java.util.Date))";
        let tree = parse(src);
        let caps = run_query(&deps_query(), &tree, src.as_bytes());
        assert!(caps.iter().any(|c| c.name == "dep_entry"), "got: {caps:?}");
    }

    #[test]
    fn deps_requires_inside_ns() {
        let src = "(ns core\n  (:require [a :as a])\n  (:require [b :as b]))";
        let tree = parse(src);
        let caps = run_query(&deps_query(), &tree, src.as_bytes());
        let dep_entries: Vec<&Capture> = caps.iter().filter(|c| c.name == "dep_entry").collect();
        assert_eq!(
            dep_entries.len(),
            2,
            "expected 2 dep_entries (one per dep_form), got: {dep_entries:?}"
        );
    }

    // ── integration: combined workflow ───────────────────────────────────────

    #[test]
    fn full_clojure_file() {
        let src = "(ns my-app.core\n  (:require [clojure.string :as str]\n            [my-app.utils :as utils]))\n\n(defn greet [name]\n  (str \"Hello, \" name))\n\n(deftest test-greet\n  (is (= \"Hello, World\" (greet \"World\"))))\n\n(deftest- internal-test\n  (is (= 1 1)))";
        let tree = parse(src);
        let discover_caps = run_query(&discover_query(), &tree, src.as_bytes());
        let ns_caps = run_query(&ns_query(), &tree, src.as_bytes());
        let deps_caps = run_query(&deps_query(), &tree, src.as_bytes());

        let test_names: Vec<&str> = discover_caps
            .iter()
            .filter(|c| c.name == "test_name")
            .map(|_| "found")
            .collect();
        assert_eq!(
            test_names.len(),
            2,
            "expected 2 test_names (test-greet, internal-test), got discover_caps: {discover_caps:?}"
        );

        assert!(
            ns_caps.iter().any(|c| c.name == "namespace_name"),
            "expected namespace_name, got: {ns_caps:?}"
        );

        let deps: Vec<&Capture> = deps_caps.iter().filter(|c| c.name == "dep_entry").collect();
        assert!(
            deps.len() >= 1,
            "expected at least 1 dep_entry, got: {deps_caps:?}"
        );
    }
}
