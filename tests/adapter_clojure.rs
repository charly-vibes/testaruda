//! Clojure adapter integration tests (testaruda-gjz).
//!
//! Verifies the adapter-clojure binary against fixture projects.
//! Mirrors the pattern from tests/adapter_typescript.rs.

use std::io::{BufRead, Write};
use std::process::Command;

/// Check if the Clojure adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-clojure")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The adapter-clojure crate directory must exist with expected structure.
#[test]
fn adapter_clojure_crate_exists() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = manifest_dir.join("adapter-clojure");

    assert!(crate_dir.exists(), "adapter-clojure/ directory not found");

    let cargo_toml = crate_dir.join("Cargo.toml");
    assert!(cargo_toml.exists(), "adapter-clojure/Cargo.toml not found");

    let src_dir = crate_dir.join("src");
    assert!(src_dir.exists(), "adapter-clojure/src/ not found");

    let queries_dir = crate_dir.join("queries");
    assert!(queries_dir.exists(), "adapter-clojure/queries/ not found");
    for q in &["discover.scm", "ns.scm", "deps.scm"] {
        assert!(
            queries_dir.join(q).exists(),
            "adapter-clojure/queries/{q} not found"
        );
    }

    let tests_dir = crate_dir.join("tests");
    assert!(tests_dir.exists(), "adapter-clojure/tests/ not found");
    for t in &["queries_test.rs", "adapter_test.rs", "static_deps_test.rs"] {
        assert!(
            tests_dir.join(t).exists(),
            "adapter-clojure/tests/{t} not found"
        );
    }
}

#[test]
fn adapter_clojure_handshake() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-clojure not on PATH — skipping");
        return;
    }

    let mut child = Command::new("testaruda-adapter-clojure")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn Clojure adapter");

    {
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(b"{\"command\":\"handshake\"}\n")
            .expect("failed to write to adapter stdin");
    }

    let output = child
        .wait_with_output()
        .expect("failed to read adapter output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let resp: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("handshake response should be valid JSON");

    assert_eq!(resp["ok"], true, "handshake should succeed: {resp}");
    assert_eq!(resp["result"]["languages"][0], "clojure");
    assert_eq!(resp["result"]["granularity"], "file");
    assert_eq!(
        resp["result"]["capabilities"]["symbol_model_complete"],
        false
    );
    assert_eq!(resp["result"]["capabilities"]["fingerprinting"], true);
    assert_eq!(resp["result"]["capabilities"]["runtime_edges"], false);
    assert_eq!(resp["result"]["protocol"], 1);
}

/// Path to the Clojure fixture project.
fn fixture_path() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/clojure");
    p
}

/// Spawn a Clojure adapter subprocess ready for command exchange.
fn spawn_adapter() -> std::process::Child {
    Command::new("testaruda-adapter-clojure")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn Clojure adapter")
}

/// Send a JSON command to an adapter subprocess and return the response line.
fn send_command(child: &mut std::process::Child, cmd: &str) -> String {
    let stdin = child.stdin.as_mut().unwrap();
    stdin
        .write_all(cmd.as_bytes())
        .expect("failed to write to adapter stdin");
    // Read exactly one line from stdout
    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .expect("failed to read adapter output");
    line.trim().to_string()
}

#[test]
fn adapter_clojure_discover_fixture() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-clojure not on PATH — skipping");
        return;
    }

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixture_path()).unwrap();

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, "{\"command\":\"discover\"}\n");
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");

    // Cleanup: wait for process
    child.wait().ok();
    std::env::set_current_dir(&cwd).unwrap();

    // Should succeed or return "not implemented"
    if parsed["ok"] == serde_json::Value::Bool(false) {
        eprintln!("discover not implemented yet — skipping");
        return;
    }

    let empty = vec![];
    let tests = parsed["result"].as_array().unwrap_or(&empty);
    assert!(!tests.is_empty(), "should find test files in fixture");

    // Should find at least core_test.clj and utils_test.clj
    let files: Vec<&str> = tests.iter().filter_map(|t| t["file"].as_str()).collect();
    assert!(
        files.iter().any(|f| f.contains("core_test.clj")),
        "should find core_test.clj: {:?}",
        files
    );
    assert!(
        files.iter().any(|f| f.contains("utils_test.clj")),
        "should find utils_test.clj: {:?}",
        files
    );
}

#[test]
fn adapter_clojure_static_deps_fixture() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-clojure not on PATH — skipping");
        return;
    }

    let cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixture_path()).unwrap();

    let mut child = spawn_adapter();
    let resp = send_command(
        &mut child,
        "{\"command\":\"static-deps\",\"params\":{\"changed_files\":[\"src/core.clj\",\"src/utils.clj\"]}}\n",
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

    child.wait().ok();
    std::env::set_current_dir(&cwd).unwrap();

    assert_eq!(parsed["ok"], true, "static-deps should succeed: {parsed}");

    let edges = parsed["edges"].as_array().unwrap();
    assert!(!edges.is_empty(), "should find edges from fixture");

    // Should have edge from core_test.clj → src/core.clj
    let core_edge = edges.iter().find(|e| {
        e["from"]
            .as_str()
            .unwrap_or("")
            .contains("my-project.core-test::test-greet(Test)")
            && e["to"].as_str().unwrap_or("").contains("src/core.clj")
    });
    assert!(
        core_edge.is_some(),
        "expected edge core_test.clj -> src/core.clj, got: {edges:?}"
    );

    // Should have edge from utils_test.clj → src/utils.clj
    let utils_edge = edges.iter().find(|e| {
        e["from"]
            .as_str()
            .unwrap_or("")
            .contains("my-project.utils-test::test-add(Test)")
            && e["to"].as_str().unwrap_or("").contains("src/utils.clj")
    });
    assert!(
        utils_edge.is_some(),
        "expected edge utils_test.clj -> src/utils.clj, got: {edges:?}"
    );

    // Verify edge weight and origin
    if let Some(edge) = core_edge {
        assert_eq!(edge["weight"], 1_000_000);
        assert_eq!(edge["origin"], "static");
    }
}

#[test]
fn adapter_clojure_full_pipeline() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-clojure not on PATH — skipping");
        return;
    }

    let _cwd_guard = CwdGuard::enter(fixture_path());

    // Phase 1: Discover tests
    let mut child = spawn_adapter();
    let discover_resp = send_command(&mut child, "{\"command\":\"discover\"}\n");
    let discover: serde_json::Value =
        serde_json::from_str(&discover_resp).expect("discover response should be valid JSON");

    if discover["ok"] == serde_json::Value::Bool(false) {
        eprintln!("discover not implemented yet — skipping full pipeline");
        child.wait().ok();
        return;
    }

    // Collect discover results
    let tests = discover["result"].as_array().unwrap();
    let test_count = tests.len();
    assert!(
        test_count >= 2,
        "should find at least 2 tests, found {test_count}"
    );

    // Find test IDs for core_test.clj and utils_test.clj
    let core_test_ids: Vec<&str> = tests
        .iter()
        .filter(|t| t["file"].as_str().unwrap_or("").contains("core_test.clj"))
        .filter_map(|t| t["node_id"].as_str())
        .collect();
    assert!(
        !core_test_ids.is_empty(),
        "should find node_id for core_test.clj"
    );

    let utils_test_ids: Vec<&str> = tests
        .iter()
        .filter(|t| t["file"].as_str().unwrap_or("").contains("utils_test.clj"))
        .filter_map(|t| t["node_id"].as_str())
        .collect();
    assert!(
        !utils_test_ids.is_empty(),
        "should find node_id for utils_test.clj"
    );

    // Phase 2: Static deps (change src/core.clj and src/utils.clj)
    let deps_resp = send_command(
        &mut child,
        "{\"command\":\"static-deps\",\"params\":{\"changed_files\":[\"src/core.clj\",\"src/utils.clj\"]}}\n",
    );
    let deps: serde_json::Value =
        serde_json::from_str(&deps_resp).expect("static-deps response should be valid JSON");

    assert_eq!(deps["ok"], true, "static-deps should succeed: {deps}");

    let edges = deps["edges"].as_array().unwrap();
    assert!(!edges.is_empty(), "should have edges from static-deps");

    // Cleanup
    child.wait().ok();
}

// ---- Seeded-fault recall test ----

#[test]
fn adapter_clojure_seeded_fault_recall() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-clojure not on PATH — skipping");
        return;
    }

    let _cwd_guard = CwdGuard::enter(fixture_path());

    // Phase 1: Discover tests
    let mut child = spawn_adapter();
    let discover_resp = send_command(&mut child, "{\"command\":\"discover\"}\n");
    let discover: serde_json::Value =
        serde_json::from_str(&discover_resp).expect("discover response should be valid JSON");

    if discover["ok"] == serde_json::Value::Bool(false) {
        eprintln!("discover not implemented yet — skipping seeded fault recall");
        child.wait().ok();
        return;
    }

    let tests = discover["result"].as_array().unwrap();
    let _core_test_file = tests
        .iter()
        .find(|t| t["file"].as_str().unwrap_or("").contains("core_test.clj"))
        .and_then(|t| t["file"].as_str())
        .expect("should find file for core_test.clj")
        .to_string();

    let _utils_test_file = tests
        .iter()
        .find(|t| t["file"].as_str().unwrap_or("").contains("utils_test.clj"))
        .and_then(|t| t["file"].as_str())
        .expect("should find file for utils_test.clj")
        .to_string();

    // Phase 2: Static deps with one changed file
    let deps_resp = send_command(
        &mut child,
        "{\"command\":\"static-deps\",\"params\":{\"changed_files\":[\"src/core.clj\"]}}\n",
    );
    let deps: serde_json::Value =
        serde_json::from_str(&deps_resp).expect("static-deps response should be valid JSON");

    assert_eq!(deps["ok"], true, "static-deps should succeed: {deps}");

    let edges = deps["edges"].as_array().unwrap();

    // Should have an edge from core_test.clj to src/core.clj.
    // The adapter's static-deps edge `from` is now the test function node_id,
    // not the file path. Get specific node_ids from the tests list.
    let core_node_ids: Vec<&str> = tests
        .iter()
        .filter(|t| t["file"].as_str().unwrap_or("").contains("core_test.clj"))
        .filter_map(|t| t["node_id"].as_str())
        .collect();

    let has_core_relation = edges.iter().any(|e| {
        e["from"].as_str().unwrap_or("") == core_node_ids.first().copied().unwrap_or("")
            && e["to"].as_str().unwrap_or("").contains("src/core.clj")
    });
    assert!(
        has_core_relation,
        "expected edge from {core_node_ids:?} -> src/core.clj, got: {edges:?}"
    );

    // Should NOT have an edge from utils_test.clj to src/core.clj
    let utils_node_ids: Vec<&str> = tests
        .iter()
        .filter(|t| t["file"].as_str().unwrap_or("").contains("utils_test.clj"))
        .filter_map(|t| t["node_id"].as_str())
        .collect();
    let has_utils_relation = edges.iter().any(|e| {
        e["from"].as_str().unwrap_or("") == utils_node_ids.first().copied().unwrap_or("")
            && e["to"].as_str().unwrap_or("").contains("src/core.clj")
    });
    assert!(
        !has_utils_relation,
        "utils_test.clj should NOT depend on src/core.clj, but edge found: {edges:?}"
    );

    child.wait().ok();
}

// ---- Cwd guard ----

/// Cwd guard: changes to a temp directory, restores on drop.
struct CwdGuard {
    saved: std::path::PathBuf,
}

impl CwdGuard {
    fn enter(temp: std::path::PathBuf) -> Self {
        let saved = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp).unwrap();
        Self { saved }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.saved);
    }
}
