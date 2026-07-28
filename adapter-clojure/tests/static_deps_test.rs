use assert_cmd::Command;
use std::sync::OnceLock;

/// Create a fixture project directory with Clojure source and test files.
fn fixture_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // Create src/ and test/ directories
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("test")).unwrap();

    // src/core.clj — namespace my-project.core, requires clojure.string
    std::fs::write(
        root.join("src/core.clj"),
        "(ns my-project.core\n  (:require [clojure.string :as str]))\n\n(defn greet [name]\n  (str \"Hello, \" name))\n",
    )
    .unwrap();

    // src/utils.clj — namespace my-project.utils
    std::fs::write(
        root.join("src/utils.clj"),
        "(ns my-project.utils)\n\n(defn add [a b] (+ a b))\n",
    )
    .unwrap();

    // test/core_test.clj — tests that require my-project.core
    std::fs::write(
        root.join("test/core_test.clj"),
        "(ns my-project.core-test\n  (:require [clojure.test :refer [deftest is]]\n            [my-project.core :as core]))\n\n(deftest test-greet\n  (is (= \"Hello, World\" (core/greet \"World\"))))\n",
    )
    .unwrap();

    // test/utils_test.clj — tests that require my-project.utils
    std::fs::write(
        root.join("test/utils_test.clj"),
        "(ns my-project.utils-test\n  (:require [clojure.test :refer [deftest is]]\n            [my-project.utils :as utils]))\n\n(deftest test-add\n  (is (= 3 (utils/add 1 2))))\n",
    )
    .unwrap();

    // A non-test file in test/ (no deftest, for discovery filtering)
    std::fs::write(
        root.join("test/support.clj"),
        "(ns my-project.support\n  (:require [my-project.core :as core]))\n",
    )
    .unwrap();

    // A .rs file that should be ignored
    std::fs::write(root.join("src/other.rs"), "fn main() {}\n").unwrap();

    dir
}

/// Run the adapter binary with a JSON command, returning the parsed response.
fn send_command(cmd: &str, work_dir: &std::path::Path) -> serde_json::Value {
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap();
    let assert = binary
        .current_dir(work_dir)
        .write_stdin(cmd)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    serde_json::from_str(stdout.trim()).unwrap()
}

#[test]
fn static_deps_edges_for_changed_source() {
    let project = fixture_project();
    let root = project.path();

    let resp = send_command(
        r#"{"command":"static-deps","args":{"files":["src/core.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["result"]["edges"].as_array().unwrap();

    // Should find at least one edge: test/core_test.clj → src/core.clj
    let core_edge = edges.iter().find(|e| {
        e["from"]
            .as_str()
            .unwrap_or("")
            .contains("test/core_test.clj")
            && e["to"].as_str().unwrap_or("").contains("src/core.clj")
    });
    assert!(
        core_edge.is_some(),
        "expected edge test/core_test.clj → src/core.clj, got edges: {edges:?}"
    );

    // Verify the edge has correct weight and origin
    if let Some(edge) = core_edge {
        assert_eq!(edge["weight"], 1_000_000);
        assert_eq!(edge["origin"], "static");
    }
}

#[test]
fn static_deps_ignores_non_clojure_files() {
    let project = fixture_project();
    let root = project.path();

    let resp = send_command(
        r#"{"command":"static-deps","args":{"files":["src/other.rs"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["result"]["edges"].as_array().unwrap();
    assert!(
        edges.is_empty(),
        "no edges expected for non-Clojure file, got: {edges:?}"
    );
}

#[test]
fn static_deps_deduplicates_edges() {
    let project = fixture_project();
    let root = project.path();

    // Create a test that requires my-project.core multiple times
    std::fs::write(
        root.join("test/dup_test.clj"),
        "(ns my-project.dup-test\n  (:require [clojure.test :refer [deftest is]]\n            [my-project.core :as a]\n            [my-project.core :as b]))\n\n(deftest test-dup\n  (is (= 1 1)))\n",
    )
    .unwrap();

    let resp = send_command(
        r#"{"command":"static-deps","args":{"files":["src/core.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["result"]["edges"].as_array().unwrap();

    // Count edges from dup_test.clj → src/core.clj — should be exactly 1
    let dup_edges: Vec<&serde_json::Value> = edges
        .iter()
        .filter(|e| {
            e["from"].as_str().unwrap_or("").contains("dup_test.clj")
                && e["to"].as_str().unwrap_or("").contains("src/core.clj")
        })
        .collect();
    assert_eq!(
        dup_edges.len(),
        1,
        "expected exactly 1 deduplicated edge, got {dup_edges:?}"
    );
}

#[test]
fn static_deps_unresolved_require_still_emits_edge() {
    let project = fixture_project();
    let root = project.path();

    // Add a test that requires both a changed namespace and an unresolved one
    std::fs::write(
        root.join("test/external_test.clj"),
        "(ns my-project.external-test\n  (:require [clojure.test :refer [deftest is]]\n            [my-project.core :as core]\n            [external.lib :as ext]))\n\n(deftest test-ext\n  (is true))\n",
    )
    .unwrap();

    let resp = send_command(
        r#"{"command":"static-deps","args":{"files":["src/core.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["result"]["edges"].as_array().unwrap();

    // Edge from external_test.clj to src/core.clj should exist (because it depends on core)
    let ext_edge = edges.iter().find(|e| {
        e["from"]
            .as_str()
            .unwrap_or("")
            .contains("external_test.clj")
            && e["to"].as_str().unwrap_or("").contains("src/core.clj")
    });
    assert!(
        ext_edge.is_some(),
        "expected edge even with unresolved require, got: {edges:?}"
    );
}

#[test]
fn static_deps_handles_multiple_source_files() {
    let project = fixture_project();
    let root = project.path();

    let resp = send_command(
        r#"{"command":"static-deps","args":{"files":["src/core.clj", "src/utils.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["result"]["edges"].as_array().unwrap();

    // Should have edges for both source files
    let core_edges: Vec<&serde_json::Value> = edges
        .iter()
        .filter(|e| e["to"].as_str().unwrap_or("").contains("src/core.clj"))
        .collect();
    assert!(
        !core_edges.is_empty(),
        "expected edges to src/core.clj, got: {edges:?}"
    );

    let utils_edges: Vec<&serde_json::Value> = edges
        .iter()
        .filter(|e| e["to"].as_str().unwrap_or("").contains("src/utils.clj"))
        .collect();
    assert!(
        !utils_edges.is_empty(),
        "expected edges to src/utils.clj, got: {edges:?}"
    );
}

#[test]
fn handshake_still_works() {
    let project = fixture_project();
    let root = project.path();
    let resp = send_command(r#"{"command":"handshake"}"#, root);
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["result"]["languages"][0], "clojure");
}
