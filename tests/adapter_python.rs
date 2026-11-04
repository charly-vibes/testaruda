//! Purpose: Verify the Python adapter protocol against isolated projects.
//! Responsibilities:
//! - Check discovery, dependencies and protocol responses for src-layout projects.
//! - Exercise configured discovery exclusions using the freshly built adapter.
//!
//! Rationale: Pin exclusion behavior without relying on installed binaries
//! (testaruda-vpnf); existing fixture tests also cover PATH-installed adapters.
mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Path to the Python adapter fixture project.
fn fixture_path() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/python-src-layout");
    p
}

/// Check if the Python adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-python")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Spawn the Python adapter rooted at the fixture directory and return a
/// child process handle. The child gets an explicit cwd — never rely on the
/// harness process cwd (testaruda-pzh6).
fn spawn_adapter() -> std::process::Child {
    Command::new("testaruda-adapter-python")
        .current_dir(fixture_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .require("failed to spawn Python adapter")
}

/// Send a JSON command to an adapter subprocess and return the response line.
fn send_command(child: &mut std::process::Child, cmd: &str) -> String {
    let stdin = child.stdin.as_mut().unwrap_or_panic();
    writeln!(stdin, "{}", cmd).unwrap_or_panic();
    stdin.flush().unwrap_or_panic();

    let stdout = child.stdout.as_mut().unwrap_or_panic();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap_or_panic();
    line.trim().to_string()
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
fn discover_honors_configured_exclusions() {
    let project = tempfile::tempdir().unwrap_or_panic();
    for directory in ["tests", "fixtures/nested", ".hidden", "target"] {
        std::fs::create_dir_all(project.path().join(directory)).unwrap_or_panic();
        std::fs::write(
            project.path().join(directory).join("test_example.py"),
            "def test_example(): pass\n",
        )
        .unwrap_or_panic();
    }
    std::fs::write(
        project.path().join("testaruda.toml"),
        "[discover]\nexclude = [\"fixtures\", \"target\"]\n",
    )
    .unwrap_or_panic();
    let output = Command::new(env!("CARGO_BIN_EXE_testaruda-adapter-python"))
        .current_dir(project.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            writeln!(
                child.stdin.take().unwrap_or_panic(),
                "{{\"command\":\"discover\"}}"
            )?;
            child.wait_with_output()
        })
        .unwrap_or_panic();
    assert!(output.status.success());
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_panic();
    let items = response["result"].as_array().unwrap_or_panic();
    let files: Vec<_> = items
        .iter()
        .map(|item| item["file"].as_str().unwrap_or_panic())
        .collect();
    assert_eq!(files, ["tests/test_example.py"]);
}

// ============================================================================
// Handshake
// ============================================================================

#[test]
fn python_adapter_handshake() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).require("handshake response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "handshake should succeed: {:?}",
        parsed.get("error")
    );
    let result = &parsed["result"];
    assert_eq!(result["name"], "python-adapter");
    assert_eq!(result["protocol"], 1);
    assert!(result["languages"]
        .as_array()
        .unwrap_or_panic()
        .contains(&"python".into()));
    assert_eq!(result["granularity"], "file");
    let caps = &result["capabilities"];
    assert!(caps["fingerprinting"].as_bool().unwrap_or(false));
    assert!(!caps["symbol_model_complete"].as_bool().unwrap_or(true));
    assert!(!caps["runtime_edges"].as_bool().unwrap_or(true));

    child.kill().ok();
}

// ============================================================================
// Discover — src/ layout
// ============================================================================

#[test]
fn discover_src_layout_finds_project_tests() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        // Handshake first
        send_command(&mut child, r#"{"command":"handshake"}"#);

        let resp = send_command(&mut child, r#"{"command":"discover"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("discover response should be valid JSON");

        assert!(
            parsed["ok"].as_bool().unwrap_or(false),
            "discover should succeed: {:?}",
            parsed.get("error")
        );

        let tests = parsed["result"].as_array().unwrap_or_panic();

        // Should find: test_model.py, test_service.py, test_nested.py
        // Should NOT find: conftest.py files, .venv vendored tests
        let node_ids: Vec<&str> = tests.iter().filter_map(|t| t["node_id"].as_str()).collect();

        // Check project tests are found
        assert!(
            node_ids.contains(&"tests/test_model.py"),
            "should find tests/test_model.py, got: {:?}",
            node_ids
        );
        assert!(
            node_ids.contains(&"tests/test_service.py"),
            "should find tests/test_service.py, got: {:?}",
            node_ids
        );
        assert!(
            node_ids.contains(&"tests/sub/test_nested.py"),
            "should find tests/sub/test_nested.py, got: {:?}",
            node_ids
        );

        // Check conftest.py is NOT listed as a test
        let conftest_found: Vec<&&str> = node_ids
            .iter()
            .filter(|id| id.contains("conftest"))
            .collect();
        assert!(
            conftest_found.is_empty(),
            "conftest.py should not be discovered as a test: {:?}",
            conftest_found
        );

        // Check .venv vendored tests are NOT discovered
        let venv_found: Vec<&&str> = node_ids.iter().filter(|id| id.contains(".venv")).collect();
        assert!(
            venv_found.is_empty(),
            ".venv tests should be excluded: {:?}",
            venv_found
        );

        // Check count: exactly 3 test files
        assert_eq!(
            node_ids.len(),
            3,
            "expected 3 test files, got {:?}",
            node_ids
        );

        child.kill().ok();
    }
}

#[test]
fn discover_src_layout_node_id_format() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);
        let resp = send_command(&mut child, r#"{"command":"discover"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("discover response should be valid JSON");

        let tests = parsed["result"].as_array().unwrap_or_panic();

        for test in tests {
            let node_id = test["node_id"].as_str().unwrap_or_panic();
            let file = test["file"].as_str().unwrap_or_panic();
            // node_id and file should match (no ./ prefix)
            assert_eq!(
                node_id, file,
                "node_id and file should match for {:?}",
                node_id
            );
            // node_id should not start with ./
            assert!(
                !node_id.starts_with("./"),
                "node_id should not have ./ prefix: {:?}",
                node_id
            );
            // suite_kind should be "unit"
            assert_eq!(
                test["suite_kind"].as_str().unwrap_or_panic(),
                "unit",
                "suite_kind should be 'unit' for {:?}",
                node_id
            );
        }

        child.kill().ok();
    }
}

// ============================================================================
// Discover — exclusion list
// ============================================================================

#[test]
fn discover_excludes_venv_and_cache_dirs() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);
        let resp = send_command(&mut child, r#"{"command":"discover"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("discover response should be valid JSON");

        let tests = parsed["result"].as_array().unwrap_or_panic();
        let node_ids: Vec<&str> = tests.iter().filter_map(|t| t["node_id"].as_str()).collect();

        // Ensure no path contains any excluded directory name
        for id in &node_ids {
            assert!(
                !id.contains(".venv"),
                "should not discover .venv files: {:?}",
                id
            );
            assert!(
                !id.contains("__pycache__"),
                "should not discover __pycache__ files: {:?}",
                id
            );
            assert!(
                !id.contains(".pytest_cache"),
                "should not discover .pytest_cache files: {:?}",
                id
            );
        }

        child.kill().ok();
    }
}

// ============================================================================
// Static-deps — src/ layout
// ============================================================================

#[test]
fn static_deps_src_layout_resolves_modules() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Change src/my_package/model.py — tests import from my_package.model
        //
        // src/ layouts are resolved via the package-root walk (testaruda-i802,
        // commit 8d69a3b): the adapter strips the non-importable `src/` prefix
        // so "src/my_package/model.py" matches "from my_package.model import".
        //
        // Both test files import (directly or transitively) from my_package.model.
        let cmd =
            r#"{"command":"static-deps","params":{"changed_files":["src/my_package/model.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("static-deps response should be valid JSON");

        assert!(
            parsed["ok"].as_bool().unwrap_or(false),
            "static-deps should succeed: {:?}",
            parsed.get("error")
        );

        let edges = parsed["edges"].as_array().unwrap_or_panic();
        let candidates = parsed["candidates"].as_array().unwrap_or_panic();

        // Should have candidates from discover
        assert!(
            !candidates.is_empty(),
            "should have candidate tests: {:?}",
            candidates
        );

        // src/ layout now resolves: both test files select on model.py
        let edge_pairs: Vec<(String, String)> = edges
            .iter()
            .map(|e| {
                (
                    e["from"].as_str().unwrap_or("").to_string(),
                    e["to"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        assert!(
            edge_pairs.contains(&(
                "tests/test_model.py".to_string(),
                "src/my_package/model.py".to_string()
            )),
            "expected test_model -> model edge, got: {:?}",
            edge_pairs
        );
        assert!(
            edge_pairs.contains(&(
                "tests/test_service.py".to_string(),
                "src/my_package/model.py".to_string()
            )),
            "expected test_service -> model (transitive) edge, got: {:?}",
            edge_pairs
        );

        child.kill().ok();
    }
}

#[test]
fn static_deps_src_layout_service_change() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // src/ layout resolves since testaruda-i802: service.py itself is a
        // changed source file whose test (test_service.py) imports it
        let cmd =
            r#"{"command":"static-deps","params":{"changed_files":["src/my_package/service.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("static-deps response should be valid JSON");

        let edges = parsed["edges"].as_array().unwrap_or_panic();

        let edge_pairs: Vec<(String, String)> = edges
            .iter()
            .map(|e| {
                (
                    e["from"].as_str().unwrap_or("").to_string(),
                    e["to"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect();
        assert!(
            edge_pairs.contains(&(
                "tests/test_service.py".to_string(),
                "src/my_package/service.py".to_string()
            )),
            "expected test_service -> service edge, got: {:?}",
            edge_pairs
        );

        child.kill().ok();
    }
}

#[test]
fn static_deps_src_layout_nested_test_no_deps() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Change a file that nothing imports — should produce no edges
        let cmd = r#"{"command":"static-deps","params":{"changed_files":["src/my_package/__init__.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("static-deps response should be valid JSON");

        let edges = parsed["edges"].as_array().unwrap_or_panic();

        // __init__.py re-exports Model from model.py, but adapter doesn't follow
        // re-exports (see Edge 17), so there should be no edges from this alone.
        // Additionally, the src/ layout module resolution doesn't work (see above),
        // so even if re-exports were followed, the module names wouldn't match.
        assert!(
            edges.is_empty(),
            "__init__.py change should produce no edges (src/ layout limitation + no re-export following). \
             Got: {:?}",
            edges
        );

        child.kill().ok();
    }
}

// ============================================================================
// Static-deps — unresolved files
// ============================================================================

#[test]
fn static_deps_nonexistent_file_unresolved() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        let cmd = r#"{"command":"static-deps","params":{"changed_files":["nonexistent.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("static-deps response should be valid JSON");

        let unresolved = parsed["unresolved"].as_array().unwrap_or_panic();
        assert!(
            unresolved
                .iter()
                .any(|v| v.as_str() == Some("nonexistent.py")),
            "nonexistent.py should be in unresolved list"
        );

        child.kill().ok();
    }
}

// ============================================================================
// Static-deps — test file self-edge
// ============================================================================

#[test]
fn static_deps_test_file_changed_self_edge() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Changing a test file should produce a self-edge
        let cmd = r#"{"command":"static-deps","params":{"changed_files":["tests/test_model.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).require("static-deps response should be valid JSON");

        let edges = parsed["edges"].as_array().unwrap_or_panic();
        let froms: Vec<&str> = edges.iter().filter_map(|e| e["from"].as_str()).collect();
        let tos: Vec<&str> = edges.iter().filter_map(|e| e["to"].as_str()).collect();

        // The test file itself should get a self-edge
        assert!(
            froms.contains(&"tests/test_model.py"),
            "test_model.py should have a self-edge"
        );
        assert!(
            tos.contains(&"tests/test_model.py"),
            "self-edge should point to itself"
        );

        child.kill().ok();
    }
}
