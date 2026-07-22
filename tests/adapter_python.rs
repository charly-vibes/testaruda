//! Python adapter integration tests (testaruda-6yw).
//!
//! Verifies the Python adapter's discover and static-deps commands against
//! a synthetic src-layout project fixture. Tests are conditional on the
//! Python adapter binary being available on PATH.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};

/// Global lock for CWD-manipulating tests to prevent parallel interference.
static CWD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

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

/// Spawn the Python adapter and return a child process handle.
fn spawn_adapter() -> std::process::Child {
    Command::new("testaruda-adapter-python")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Python adapter")
}

/// Send a JSON command to an adapter subprocess and return the response line.
fn send_command(child: &mut std::process::Child, cmd: &str) -> String {
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{}", cmd).unwrap();
    stdin.flush().unwrap();

    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    line.trim().to_string()
}

/// Run `f` with the process CWD set to the fixture dir, then restore.
/// Handles poisoned mutex (from panicked prior tests) gracefully.
fn with_fixture_cwd<R>(f: impl FnOnce() -> R) -> R {
    let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let orig = std::env::current_dir().unwrap();
    let fixture = fixture_path();
    assert!(
        fixture.exists(),
        "fixture directory not found: {:?}",
        fixture
    );
    std::env::set_current_dir(&fixture).unwrap();
    let result = f();
    std::env::set_current_dir(&orig).unwrap();
    result
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
        serde_json::from_str(&resp).expect("handshake response should be valid JSON");

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
        .unwrap()
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

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        // Handshake first
        send_command(&mut child, r#"{"command":"handshake"}"#);

        let resp = send_command(&mut child, r#"{"command":"discover"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("discover response should be valid JSON");

        assert!(
            parsed["ok"].as_bool().unwrap_or(false),
            "discover should succeed: {:?}",
            parsed.get("error")
        );

        let tests = parsed["result"].as_array().unwrap();

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
    });
}

#[test]
fn discover_src_layout_node_id_format() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);
        let resp = send_command(&mut child, r#"{"command":"discover"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("discover response should be valid JSON");

        let tests = parsed["result"].as_array().unwrap();

        for test in tests {
            let node_id = test["node_id"].as_str().unwrap();
            let file = test["file"].as_str().unwrap();
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
                test["suite_kind"].as_str().unwrap(),
                "unit",
                "suite_kind should be 'unit' for {:?}",
                node_id
            );
        }

        child.kill().ok();
    });
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

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);
        let resp = send_command(&mut child, r#"{"command":"discover"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("discover response should be valid JSON");

        let tests = parsed["result"].as_array().unwrap();
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
    });
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

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Change src/my_package/model.py — tests import from my_package.model
        //
        // NOTE: The adapter currently resolves file paths to module names by
        // converting the on-disk path directly (e.g., "src/my_package/model.py"
        // → "src.my_package.model"). But tests import using the package name
        // without the "src." prefix ("from my_package.model import Model").
        //
        // This is a known limitation for src/ layouts (see Edge 1 in the edge
        // case catalog). The adapter would need to know which directories are
        // source roots and strip them from module resolution.
        //
        // The test verifies this current behavior: no edges are found.
        let cmd =
            r#"{"command":"static-deps","params":{"changed_files":["src/my_package/model.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

        assert!(
            parsed["ok"].as_bool().unwrap_or(false),
            "static-deps should succeed: {:?}",
            parsed.get("error")
        );

        let edges = parsed["edges"].as_array().unwrap();
        let candidates = parsed["candidates"].as_array().unwrap();

        // Should have candidates from discover
        assert!(
            !candidates.is_empty(),
            "should have candidate tests: {:?}",
            candidates
        );

        // Current limitation: no edges found because src/ prefix in file path
        // doesn't match the module name used in imports.
        // This assertion documents the current behavior. When the adapter is
        // fixed to handle src/ layouts, this test should be updated.
        assert!(
            edges.is_empty(),
            "src/ layout module resolution currently produces no edges (known limitation). \
             Got: {:?}",
            edges
        );

        child.kill().ok();
    });
}

#[test]
fn static_deps_src_layout_service_change() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Same limitation as src layout module resolution above
        let cmd =
            r#"{"command":"static-deps","params":{"changed_files":["src/my_package/service.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

        let edges = parsed["edges"].as_array().unwrap();

        // Current limitation: no edges found for src/ layout
        assert!(
            edges.is_empty(),
            "src/ layout module resolution currently produces no edges (known limitation). \
             Got: {:?}",
            edges
        );

        child.kill().ok();
    });
}

#[test]
fn static_deps_src_layout_nested_test_no_deps() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-python not on PATH — skipping");
        return;
    }

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Change a file that nothing imports — should produce no edges
        let cmd = r#"{"command":"static-deps","params":{"changed_files":["src/my_package/__init__.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

        let edges = parsed["edges"].as_array().unwrap();

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
    });
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

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        let cmd = r#"{"command":"static-deps","params":{"changed_files":["nonexistent.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

        let unresolved = parsed["unresolved"].as_array().unwrap();
        assert!(
            unresolved
                .iter()
                .any(|v| v.as_str() == Some("nonexistent.py")),
            "nonexistent.py should be in unresolved list"
        );

        child.kill().ok();
    });
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

    with_fixture_cwd(|| {
        let mut child = spawn_adapter();
        send_command(&mut child, r#"{"command":"handshake"}"#);

        // Changing a test file should produce a self-edge
        let cmd = r#"{"command":"static-deps","params":{"changed_files":["tests/test_model.py"]}}"#;
        let resp = send_command(&mut child, cmd);
        let parsed: serde_json::Value =
            serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

        let edges = parsed["edges"].as_array().unwrap();
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
    });
}
