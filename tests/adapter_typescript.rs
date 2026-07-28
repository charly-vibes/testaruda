//! TypeScript adapter scaffolding tests (testaruda-xq1).
//!
//! Verifies the adapter-typescript crate compiles and the binary is on PATH.

use std::io::{BufRead, Write};
use std::process::Command;

/// Check if the TypeScript adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-typescript")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The adapter-typescript crate directory must exist with expected structure.
#[test]
fn adapter_typescript_crate_exists() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = manifest_dir.join("adapter-typescript");

    assert!(
        crate_dir.exists(),
        "adapter-typescript/ directory not found"
    );

    let cargo_toml = crate_dir.join("Cargo.toml");
    assert!(
        cargo_toml.exists(),
        "adapter-typescript/Cargo.toml not found"
    );

    let main_rs = crate_dir.join("src/main.rs");
    assert!(main_rs.exists(), "adapter-typescript/src/main.rs not found");

    let queries_dir = crate_dir.join("queries");
    assert!(
        queries_dir.exists(),
        "adapter-typescript/queries/ not found"
    );

    // Verify workspace membership
    let root_toml =
        std::fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("root Cargo.toml readable");
    assert!(
        root_toml.contains("adapter-typescript"),
        "adapter-typescript not in workspace members"
    );
}

/// The adapter must respond to a handshake with valid JSON.
#[test]
fn adapter_typescript_handshake() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-typescript not on PATH — skipping");
        return;
    }

    let mut child = Command::new("testaruda-adapter-typescript")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn TypeScript adapter");

    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, r#"{{"command":"handshake"}}"#).unwrap();
    stdin.flush().unwrap();

    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp = line.trim().to_string();

    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "handshake should succeed: {}",
        resp
    );

    let result = &parsed["result"];
    assert_eq!(
        result["name"], "typescript-adapter",
        "adapter name mismatch"
    );
    assert_eq!(result["protocol"], 1, "protocol version mismatch");
    assert!(
        result["languages"]
            .as_array()
            .map(|a| a.contains(&serde_json::json!("typescript")))
            .unwrap_or(false),
        "should declare typescript language: {}",
        resp
    );
    assert_eq!(result["granularity"], "file", "should be file-level");

    // Clean up: wait for child to avoid zombie process
    let _ = child.wait();
}

/// Path to the TypeScript fixture project.
fn fixture_path() -> std::path::PathBuf {
    let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/typescript");
    p
}

/// Spawn the TypeScript adapter and return a child process handle.
fn spawn_adapter() -> std::process::Child {
    Command::new("testaruda-adapter-typescript")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn TypeScript adapter")
}

/// Send a JSON command to an adapter subprocess and return the response line.
fn send_command(child: &mut std::process::Child, cmd: &str) -> String {
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{}", cmd).unwrap();
    stdin.flush().unwrap();

    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    line.trim().to_string()
}

#[test]
fn adapter_typescript_discover_fixture() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-typescript not on PATH — skipping");
        return;
    }

    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixture_path()).unwrap();

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"discover"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "discover should succeed: {:?}",
        parsed
    );

    let tests = parsed["result"].as_array().unwrap();
    assert!(!tests.is_empty(), "should find test files in fixture");

    // Should find at least user.test.ts and greeting.test.ts
    let files: Vec<&str> = tests.iter().filter_map(|t| t["file"].as_str()).collect();
    assert!(
        files.iter().any(|f| f.contains("user.test.ts")),
        "should find user.test.ts: {:?}",
        files
    );
    assert!(
        files.iter().any(|f| f.contains("greeting.test.ts")),
        "should find greeting.test.ts: {:?}",
        files
    );

    let _ = child.wait();
    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn adapter_typescript_static_deps_fixture() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-typescript not on PATH — skipping");
        return;
    }

    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixture_path()).unwrap();

    let mut child = spawn_adapter();
    let cmd = serde_json::json!({
        "command": "static-deps",
        "params": {
            "changed_files": ["src/user.ts", "src/greeting.ts"]
        }
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "static-deps should succeed: {:?}",
        parsed
    );

    let edges = parsed["edges"].as_array().unwrap();
    assert!(
        !edges.is_empty(),
        "should find edges for changed source files: {:?}",
        parsed
    );

    // Verify edges have correct format
    for edge in edges {
        assert!(edge["from"].as_str().is_some(), "edge should have 'from'");
        assert!(edge["to"].as_str().is_some(), "edge should have 'to'");
        assert_eq!(edge["weight"], 1_000_000);
        assert_eq!(edge["origin"], "static");
    }

    let _ = child.wait();
    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn adapter_typescript_full_pipeline() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-typescript not on PATH — skipping");
        return;
    }

    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixture_path()).unwrap();

    let mut child = spawn_adapter();

    // 1. Handshake
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));

    // 2. Discover
    let resp = send_command(&mut child, r#"{"command":"discover"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));
    let tests = parsed["result"].as_array().unwrap();
    assert!(!tests.is_empty(), "discover should find tests");

    // 3. Static deps
    let cmd = serde_json::json!({
        "command": "static-deps",
        "params": {"changed_files": ["src/user.ts"]}
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));

    // 4. Fingerprint
    let cmd = serde_json::json!({
        "command": "fingerprint",
        "path": "src/user.ts"
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("fingerprint response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));
    assert!(
        parsed["result"]["fingerprint"].as_str().is_some(),
        "fingerprint should return a hash"
    );

    // 5. Run args
    let cmd = serde_json::json!({
        "command": "run-args",
        "params": {"selected": ["test.ts::suite::test"]}
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("run-args response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));

    // 6. Ingest
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="test.ts" file="test.ts">
    <testcase classname="test.ts" name="should pass" file="test.ts" time="0.001">
    </testcase>
  </testsuite>
</testsuites>"#;
    let cmd = serde_json::json!({
        "command": "ingest",
        "params": {"run_output": xml}
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("ingest response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));

    let _ = child.wait();
    std::env::set_current_dir(orig).unwrap();
}

#[test]
fn adapter_typescript_static_deps_seeded_fault() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-typescript not on PATH — skipping");
        return;
    }

    let orig = std::env::current_dir().unwrap();
    std::env::set_current_dir(fixture_path()).unwrap();

    let mut child = spawn_adapter();

    // First, discover all tests
    let resp = send_command(&mut child, r#"{"command":"discover"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");
    let tests = parsed["result"].as_array().unwrap();

    // Find test IDs for user.test.ts and greeting.test.ts
    let user_test_ids: Vec<&str> = tests
        .iter()
        .filter(|t| {
            t["file"]
                .as_str()
                .is_some_and(|f| f.contains("user.test.ts"))
        })
        .filter_map(|t| t["node_id"].as_str())
        .collect();

    let greeting_test_ids: Vec<&str> = tests
        .iter()
        .filter(|t| {
            t["file"]
                .as_str()
                .is_some_and(|f| f.contains("greeting.test.ts"))
        })
        .filter_map(|t| t["node_id"].as_str())
        .collect();

    assert!(!user_test_ids.is_empty(), "should have user.test.ts tests");
    assert!(
        !greeting_test_ids.is_empty(),
        "should have greeting.test.ts tests"
    );

    // Now, check static-deps for src/user.ts change
    let cmd = serde_json::json!({
        "command": "static-deps",
        "params": {"changed_files": ["src/user.ts"]}
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");
    let edges = parsed["edges"].as_array().unwrap();

    // user.test.ts tests should be selected (they import from src/user.ts)
    let affected_users: Vec<&str> = edges
        .iter()
        .filter_map(|e| e["from"].as_str())
        .filter(|from| user_test_ids.iter().any(|id| from.contains(id)))
        .collect();

    assert!(
        !affected_users.is_empty(),
        "src/user.ts change should affect user.test.ts tests"
    );

    let _ = child.wait();
    std::env::set_current_dir(orig).unwrap();
}
