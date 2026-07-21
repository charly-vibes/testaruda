//! Julia adapter integration tests.
//!
//! Verifies the Julia adapter protocol via subprocess spawning.
//! Tests are conditionally skipped when Julia or the adapter binary
//! is not available on PATH.
//!
//! The adapter lives in Testimonial.jl as a separate package — these
//! tests verify the wire protocol from testaruda's side, not the
//! adapter's internal logic (which is tested in Testimonial.jl's own
//! test suite).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Check if the Julia adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-julia")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if Julia itself is available.
fn julia_available() -> bool {
    Command::new("julia")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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

#[test]
fn julia_adapter_handshake() {
    if !julia_available() {
        eprintln!("Julia not available — skipping Julia adapter test");
        return;
    }
    if !adapter_available() {
        eprintln!("testaruda-adapter-julia not on PATH — skipping Julia adapter test");
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "handshake should succeed"
    );
    let result = &parsed["result"];
    assert_eq!(result["name"], "testimonial-adapter");
    assert_eq!(result["protocol"], 1);
    assert_eq!(result["languages"], serde_json::json!(["julia"]));
    assert_eq!(result["granularity"], "file");
    assert!(result["capabilities"]["fingerprinting"]
        .as_bool()
        .unwrap_or(false));
    assert!(result["capabilities"]["runtime_edges"]
        .as_bool()
        .unwrap_or(false));
    assert!(!result["capabilities"]["symbol_model_complete"]
        .as_bool()
        .unwrap_or(true));

    // Clean shutdown
    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_discover() {
    if !julia_available() || !adapter_available() {
        return;
    }

    // Create a temp dir with a test file
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let test_file = dir.path().join("test_foo.jl");
    std::fs::write(&test_file, r#"@testitem "my_test" begin @test 1==1 end"#)
        .expect("failed to write test file");

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        dir.path().display()
    );
    let resp = send_command(&mut child, &cmd);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "discover should succeed"
    );
    let nodes = parsed["result"]
        .as_array()
        .expect("result should be an array");
    assert_eq!(nodes.len(), 1, "should discover one @testitem");

    let node = &nodes[0];
    assert_eq!(node["suite_kind"], "ReTestItems.jl");
    let abs_file = std::fs::canonicalize(&test_file).unwrap();
    assert_eq!(node["file"], abs_file.to_string_lossy().as_ref());
    assert!(
        node["node_id"]
            .as_str()
            .unwrap_or("")
            .contains("test_foo.jl:"),
        "node_id should contain file:line format"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_fingerprint() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let test_file = dir.path().join("test.jl");
    std::fs::write(&test_file, "hello world").expect("failed to write test file");

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let cmd = format!(
        r#"{{"command":"fingerprint","params":{{"files":["{}"]}}}}"#,
        test_file.display()
    );
    let resp = send_command(&mut child, &cmd);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("fingerprint response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "fingerprint should succeed"
    );
    let fps = parsed["result"]["fingerprints"].as_array().unwrap();
    assert_eq!(fps.len(), 1);
    assert_eq!(fps[0]["file"], test_file.to_string_lossy().as_ref());
    // SHA-256 produces 64 hex chars
    let fp = fps[0]["fingerprint"].as_str().unwrap();
    assert_eq!(fp.len(), 64, "SHA-256 fingerprint should be 64 hex chars");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_run_args() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(
        &mut child,
        r#"{"command":"run-args","params":{"selected":["test_a.jl:item_1"]}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("run-args response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "run-args should succeed"
    );
    let result = &parsed["result"];
    assert!(
        result["runner_args"].is_array(),
        "runner_args should be an array"
    );
    assert!(
        result["collection_path"].is_string(),
        "collection_path should be a string"
    );

    // First arg should be "julia"
    let args = result["runner_args"].as_array().unwrap();
    assert_eq!(args[0], "julia", "runner should be julia");
    // Should contain ReTestItems reference
    let expr = args[3].as_str().unwrap_or("");
    assert!(
        expr.contains("ReTestItems"),
        "should call ReTestItems.runtests"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_static_deps_unresolved() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(
        &mut child,
        r#"{"command":"static-deps","params":{"changed_files":["src/foo.jl"]}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "static-deps should succeed"
    );
    let edges = &parsed["result"]["edges"];
    assert_eq!(
        edges["src/foo.jl"], "unresolved",
        "without prior ingest, changed files should be unresolved"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_unknown_command() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(&mut child, r#"{"command":"bogus"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("response should be valid JSON");

    assert!(
        !parsed["ok"].as_bool().unwrap_or(true),
        "unknown command should fail"
    );
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("unknown command"));

    child.kill().ok();
    child.wait().ok();
}
