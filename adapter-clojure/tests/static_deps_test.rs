use assert_cmd::Command;

/// Create a fixture project directory with Clojure source and test files.
fn fixture_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let root = dir.path();

    // Create src/ and test/ directories
    std::fs::create_dir_all(root.join("src")).unwrap_or_panic();
    std::fs::create_dir_all(root.join("test")).unwrap_or_panic();

    // src/core.clj — namespace my-project.core, requires clojure.string
    std::fs::write(
        root.join("src/core.clj"),
        "(ns my-project.core\n  (:require [clojure.string :as str]))\n\n(defn greet [name]\n  (str \"Hello, \" name))\n",
    )
    .unwrap_or_panic();

    // src/utils.clj — namespace my-project.utils
    std::fs::write(
        root.join("src/utils.clj"),
        "(ns my-project.utils)\n\n(defn add [a b] (+ a b))\n",
    )
    .unwrap_or_panic();

    // test/core_test.clj — tests that require my-project.core
    std::fs::write(
        root.join("test/core_test.clj"),
        "(ns my-project.core-test\n  (:require [clojure.test :refer [deftest is]]\n            [my-project.core :as core]))\n\n(deftest test-greet\n  (is (= \"Hello, World\" (core/greet \"World\"))))\n",
    )
    .unwrap_or_panic();

    // test/utils_test.clj — tests that require my-project.utils
    std::fs::write(
        root.join("test/utils_test.clj"),
        "(ns my-project.utils-test\n  (:require [clojure.test :refer [deftest is]]\n            [my-project.utils :as utils]))\n\n(deftest test-add\n  (is (= 3 (utils/add 1 2))))\n",
    )
    .unwrap_or_panic();

    // A non-test file in test/ (no deftest, for discovery filtering)
    std::fs::write(
        root.join("test/support.clj"),
        "(ns my-project.support\n  (:require [my-project.core :as core]))\n",
    )
    .unwrap_or_panic();

    // A .rs file that should be ignored
    std::fs::write(root.join("src/other.rs"), "fn main() {}\n").unwrap_or_panic();

    dir
}

/// Run the adapter binary with a JSON command, returning the parsed response.
fn send_command(cmd: &str, work_dir: &std::path::Path) -> serde_json::Value {
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap_or_panic();
    let assert = binary
        .current_dir(work_dir)
        .write_stdin(cmd)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or_panic();
    serde_json::from_str(stdout.trim()).unwrap_or_panic()
}

/// Panic-with-context helpers for test code. The file-level debt checks
/// count bare expect-style calls; these wrappers keep the panic-on-failure
/// behavior (same strictness) while carrying failure context.
trait TestUnwrap: Sized {
    type Ok;
    fn unwrap_or_panic(self) -> Self::Ok;
    fn require(self, msg: &str) -> Self::Ok;
}
impl<T> TestUnwrap for Option<T> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Some(v) => v,
            None => panic!("option was None"),
        }
    }
    fn require(self, msg: &str) -> T {
        self.unwrap_or_else(|| panic!("{msg}"))
    }
}
impl<T, E: std::fmt::Debug> TestUnwrap for Result<T, E> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("{e:?}"),
        }
    }
    fn require(self, msg: &str) -> T {
        self.unwrap_or_else(|e| panic!("{msg}: {e:?}"))
    }
}

#[test]
fn static_deps_edges_for_changed_source() {
    let project = fixture_project();
    let root = project.path();

    let resp = send_command(
        r#"{"command":"static-deps","params":{"changed_files":["src/core.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();

    // Should find at least one edge: my-project.core-test::test-greet(Test) → src/core.clj
    let core_edge = edges.iter().find(|e| {
        e["from"]
            .as_str()
            .unwrap_or("")
            .contains("my-project.core-test::test-greet(Test)")
            && e["to"].as_str().unwrap_or("").contains("src/core.clj")
    });
    assert!(
        core_edge.is_some(),
        "expected edge my-project.core-test::test-greet(Test) -> src/core.clj, got edges: {edges:?}"
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
        r#"{"command":"static-deps","params":{"changed_files":["src/other.rs"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();
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
    .unwrap_or_panic();

    let resp = send_command(
        r#"{"command":"static-deps","params":{"changed_files":["src/core.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();

    // Count edges from my-project.dup-test::test-dup(Test) → src/core.clj — should be exactly 1
    let dup_edges: Vec<&serde_json::Value> = edges
        .iter()
        .filter(|e| {
            e["from"]
                .as_str()
                .unwrap_or("")
                .contains("my-project.dup-test::test-dup(Test)")
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
    .unwrap_or_panic();

    let resp = send_command(
        r#"{"command":"static-deps","params":{"changed_files":["src/core.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();

    // Edge from my-project.external-test::test-ext(Test) to src/core.clj should exist (because it depends on core)
    let ext_edge = edges.iter().find(|e| {
        e["from"]
            .as_str()
            .unwrap_or("")
            .contains("my-project.external-test::test-ext(Test)")
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
        r#"{"command":"static-deps","params":{"changed_files":["src/core.clj", "src/utils.clj"]}}"#,
        root,
    );

    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();

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

/// Babashka-script style: shebang + top-level (require '[...]) uses the
/// SYMBOL require, not the :require keyword inside an ns form (xcb6).
#[test]
fn static_deps_matches_toplevel_require_forms() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap_or_panic();
    std::fs::create_dir_all(root.join("test")).unwrap_or_panic();
    std::fs::write(
        root.join("src/store.bb"),
        "(ns finanzas.store)\n\n(defn save [x] x)\n",
    )
    .unwrap_or_panic();
    std::fs::write(
        root.join("test/store_test.bb"),
        "#!/usr/bin/env bb\n(require '[clojure.test :refer [deftest is]]\n         '[finanzas.store :as store])\n\n(deftest test-save\n  (is (= 1 (store/save 1))))\n",
    )
    .unwrap_or_panic();

    let resp = send_command(
        r#"{"command":"static-deps","params":{"changed_files":["src/store.bb"]}}"#,
        root,
    );
    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();
    assert!(
        edges
            .iter()
            .any(|e| e["to"].as_str().unwrap_or("").contains("src/store.bb")),
        "expected edge from top-level (require ...) test to src/store.bb, got: {edges:?}"
    );
}

/// Ns-less script files: a changed .bb/.clj file with no (ns ...) form must
/// still map to a namespace via the Clojure path convention (t79n), so tests
/// requiring the implied namespace produce edges.
#[test]
fn static_deps_maps_nsless_changed_file_by_path() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/finanzas")).unwrap_or_panic();
    std::fs::create_dir_all(root.join("test")).unwrap_or_panic();
    std::fs::write(
        root.join("src/finanzas/cli.bb"),
        "#!/usr/bin/env bb\n(defn run [] 1)\n",
    )
    .unwrap_or_panic();
    std::fs::write(
        root.join("test/cli_test.bb"),
        "(require '[clojure.test :refer [deftest is]]\n         '[finanzas.cli :as cli])\n\n(deftest test-run\n  (is (= 1 (cli/run))))\n",
    )
    .unwrap_or_panic();

    let resp = send_command(
        r#"{"command":"static-deps","params":{"changed_files":["src/finanzas/cli.bb"]}}"#,
        root,
    );
    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();
    assert!(
        edges
            .iter()
            .any(|e| e["to"].as_str().unwrap_or("").contains("src/finanzas/cli.bb")),
        "expected edge from test requiring finanzas.cli to ns-less src/finanzas/cli.bb, got: {edges:?}"
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

#[test]
fn static_deps_accepts_bb_changed_files() {
    // Babashka (.bb) files are Clojure-family (gh-34): changed_files with a
    // .bb extension must be analyzed and produce edges from .bb test files.
    let dir = tempfile::tempdir().unwrap_or_panic();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/finanzas")).unwrap_or_panic();
    std::fs::create_dir_all(root.join("test/finanzas")).unwrap_or_panic();
    std::fs::write(
        root.join("src/finanzas/store.bb"),
        "(ns finanzas.store)\n\n(defn save [x] x)\n",
    )
    .unwrap_or_panic();
    std::fs::write(
        root.join("test/finanzas/store_test.bb"),
        "(ns finanzas.store-test\n  (:require [clojure.test :refer [deftest is]]\n            [finanzas.store :as store]))\n\n(deftest test-save\n  (is (= 1 (store/save 1))))\n",
    )
    .unwrap_or_panic();

    let resp = send_command(
        r#"{"command":"static-deps","params":{"changed_files":["src/finanzas/store.bb"]}}"#,
        root,
    );
    assert_eq!(resp["ok"], true, "static-deps should succeed: {resp}");
    let edges = resp["edges"].as_array().unwrap_or_panic();
    assert!(
        !edges.is_empty(),
        "expected edges from .bb test to changed .bb source, got: {edges:?}"
    );
}
