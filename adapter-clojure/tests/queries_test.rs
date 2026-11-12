use std::sync::OnceLock;
use streaming_iterator::StreamingIterator;

fn tree_sitter_language() -> &'static tree_sitter::Language {
    static LANG: OnceLock<tree_sitter::Language> = OnceLock::new();
    LANG.get_or_init(|| tree_sitter_clojure::LANGUAGE.into())
}

fn parse(source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(tree_sitter_language()).unwrap();
    parser.parse(source, None).unwrap()
}

fn make_query(source: &str) -> tree_sitter::Query {
    tree_sitter::Query::new(tree_sitter_language(), source).unwrap()
}

fn captures(query: &tree_sitter::Query, tree: &tree_sitter::Tree, src: &[u8]) -> Vec<String> {
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), src);
    let mut names = Vec::new();
    while let Some(m) = matches.next() {
        for cap in m.captures {
            let name = query.capture_names()[cap.index as usize].to_string();
            names.push(name);
        }
    }
    names
}
// ── discover.scm ────────────────────────────────────────────────────────────

/// (description, source, required captures, forbidden captures)
#[test]
fn discover_query_contract() {
    let q = make_query(include_str!("../queries/discover.scm"));
    let cases: [(&str, &str, &[&str], &[&str]); 6] = [
        (
            "simple deftest yields item and name",
            "(deftest my-test (is (= 1 1)))",
            &["test_item", "test_name"],
            &[],
        ),
        (
            "deftest- private yields item and name",
            "(deftest- private-test (is (= 1 1)))",
            &["test_item", "test_name"],
            &[],
        ),
        (
            "defn is not a test",
            "(defn my-fn [x] (inc x))",
            &[],
            &["test_item"],
        ),
        (
            "comment block still matches — tree-sitter-clojure has no comment node; (comment ...) is a regular list, so the inner deftest matches syntactically. Filtering by test-path happens at the adapter level, not in the query. This row documents that behavior.",
            "(comment (deftest should-be-ignored (is (= 1 1))))",
            &["test_item"],
            &[],
        ),
        (
            "deftest inside a string must not match",
            "(def s \"(deftest fake-test)\")",
            &[],
            &["test_item"],
        ),
        (
            "read-cond discard: at minimum kept-test must be found (#_ discards syntactically; the inner form may still match depending on grammar treatment of #_)",
            "(#_(deftest discarded-test (is (= 1 1))) (deftest kept-test (is (= 2 2))))",
            &["test_name"],
            &[],
        ),
    ];
    for (desc, src, required, forbidden) in cases {
        let tree = parse(src);
        let caps = captures(&q, &tree, src.as_bytes());
        for r in required {
            assert!(
                caps.contains(&r.to_string()),
                "{desc}: expected {r:?}, got: {caps:?}"
            );
        }
        for f in forbidden {
            assert!(
                !caps.contains(&f.to_string()),
                "{desc}: {f:?} must not fire, got: {caps:?}"
            );
        }
    }
}

// ── ns.scm ──────────────────────────────────────────────────────────────────

#[test]
fn ns_query_contract() {
    let q = make_query(include_str!("../queries/ns.scm"));
    let cases: [(&str, &str, &[&str], &[&str]); 3] = [
        (
            "simple ns yields namespace_name",
            "(ns my-project.core)",
            &["namespace_name"],
            &[],
        ),
        (
            "ns with docstring metadata and requires yields namespace_name",
            "(ns ^{:doc \"A docstring\"} my-project.core\n  (:require [clojure.string :as str]))",
            &["namespace_name"],
            &[],
        ),
        (
            "ns symbol inside a string must not match",
            "(def some-ns \"(ns fake.ns)\")",
            &[],
            &["namespace_name"],
        ),
    ];
    for (desc, src, required, forbidden) in cases {
        let tree = parse(src);
        let caps = captures(&q, &tree, src.as_bytes());
        for r in required {
            assert!(
                caps.contains(&r.to_string()),
                "{desc}: expected {r:?}, got: {caps:?}"
            );
        }
        for f in forbidden {
            assert!(
                !caps.contains(&f.to_string()),
                "{desc}: {f:?} must not fire, got: {caps:?}"
            );
        }
    }
    // the in-string case must yield NO captures at all
    let tree = parse("(def some-ns \"(ns fake.ns)\")");
    let caps = captures(&q, &tree, "(def some-ns \"(ns fake.ns)\")".as_bytes());
    assert!(
        caps.is_empty(),
        "in-string ns must produce no captures: {caps:?}"
    );
}

// ── deps.scm ────────────────────────────────────────────────────────────────

#[test]
fn deps_query_contract() {
    let q = make_query(include_str!("../queries/deps.scm"));
    let cases: [(&str, &str, &[&str], &[&str]); 10] = [
        (
            "require with vector yields form and entry",
            "(ns my-project.core\n  (:require [clojure.string :as str]))",
            &["dep_form", "dep_entry"],
            &[],
        ),
        (
            "bare-symbol require yields entry",
            "(ns my-project.core\n  (:require clojure.string))",
            &["dep_entry"],
            &[],
        ),
        (
            ":use yields entry",
            "(ns my-project.core\n  (:use clojure.test))",
            &["dep_entry"],
            &[],
        ),
        (
            ":import yields entry",
            "(ns my-project.core\n  (:import java.util.Date))",
            &["dep_entry"],
            &[],
        ),
        (
            "multiple requires each yield an entry",
            "(ns my-project.core\n  (:require [clojure.string :as str]\n             [clojure.java.io :as io]))",
            &["dep_entry"],
            &[],
        ),
        (
            ":refer require yields entry",
            "(ns my-project.test\n  (:require [clojure.test :refer [deftest is]]))",
            &["dep_entry"],
            &[],
        ),
        (
            ":refer :all require yields entry",
            "(ns my-project.test\n  (:require [clojure.test :refer :all]))",
            &["dep_entry"],
            &[],
        ),
        (
            "line comments are tokens, not CST nodes — nothing matches",
            ";; (:require [some.lib])\n(ns my-project.core)",
            &[],
            &["dep_entry"],
        ),
        (
            "read-cond discard: #_ is dis_expr — the wrapped form IS still parsed as a child and WILL match; test-path filtering at the adapter level handles real filtering. This row documents the syntactic behavior.",
            "(#_(:require [discarded.lib]))",
            &["dep_entry"],
            &[],
        ),
        (
            "require mention inside a string must not match",
            "(ns my-project.core\n  (str \":require [some.lib]\"))",
            &[],
            &["dep_entry"],
        ),
    ];
    for (desc, src, required, forbidden) in cases {
        let tree = parse(src);
        let caps = captures(&q, &tree, src.as_bytes());
        for r in required {
            assert!(
                caps.contains(&r.to_string()),
                "{desc}: expected {r:?}, got: {caps:?}"
            );
        }
        for f in forbidden {
            assert!(
                !caps.contains(&f.to_string()),
                "{desc}: {f:?} must not fire, got: {caps:?}"
            );
        }
    }
}

#[test]
fn queries_compile() {
    // All three .scm queries must compile against the bundled grammar.
    for (name, scm) in [
        ("deps", include_str!("../queries/deps.scm")),
        ("ns", include_str!("../queries/ns.scm")),
        ("discover", include_str!("../queries/discover.scm")),
    ] {
        let q = make_query(scm);
        assert!(
            !q.capture_names().is_empty(),
            "{name}: query has no captures"
        );
    }
}

#[test]
fn deps_finds_toplevel_require() {
    // Babashka-script style (xcb6): top-level (require '[ns :as alias]) with
    // SYMBOL head and quoted entries. Two quoted vectors + one bare quoted
    // symbol must each produce a dep_entry capture.
    let q = make_query(include_str!("../queries/deps.scm"));
    let src = "(require '[clojure.test :refer [deftest]]\n         '[finanzas.store :as store])\n(require plain.ns)";
    let tree = parse(src);
    let caps = captures(&q, &tree, src.as_bytes());
    let entry_count = caps.iter().filter(|c| c.as_str() == "dep_entry").count();
    assert_eq!(
        entry_count, 3,
        "expected 3 dep_entry captures from top-level require, got: {caps:?}"
    );
}
