//! Rust adapter integration tests (testaruda-n338).
//!
//! Verifies the Rust adapter's discover command against a synthetic crate
//! fixture. Tests are conditional on the Rust adapter binary being available
//! on PATH.
//!
//! Coverage here focuses on discovery hygiene: the adapter must not scan
//! hidden directories (`.claude/worktrees/agent-*/` from agent sessions, dot
//! configs, etc.). Real-world impact before the fix: a single-crate repo with
//! an agent worktree discovered 9x its real test count (dont: 9127 vs 1001).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Path to the freshly-built Rust adapter binary. Uses CARGO_BIN_EXE_ (not
/// PATH) so tests always exercise the current workspace code — a stale
/// installed binary would silently test old behavior (testaruda-wpil gotcha).
fn adapter_path() -> &'static str {
    env!("CARGO_BIN_EXE_testaruda-adapter-rust")
}

/// Create a minimal single-crate Rust fixture with a hidden agent worktree
/// that mirrors the real-world pollution seen in dont (.claude/worktrees).
fn create_rust_fixture_with_hidden_worktree(dir: &std::path::Path) {
    // Root crate: one unit test + one integration test.
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "hidden-fixture"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 { a + b }

#[test]
fn real_unit_test() {
    assert_eq!(add(1, 2), 3);
}
"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(
        dir.join("tests/integration.rs"),
        r#"#[test]
fn real_integration_test() {
    assert!(true);
}
"#,
    )
    .unwrap();

    // Hidden agent worktree: full copy of the source tree, as agent harnesses
    // create under .claude/worktrees/agent-*/.
    let wt = dir.join(".claude/worktrees/agent-a252e688");
    std::fs::create_dir_all(wt.join("src")).unwrap();
    std::fs::create_dir_all(wt.join("tests")).unwrap();
    std::fs::write(
        wt.join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 { a + b }

#[test]
fn real_unit_test() {
    assert_eq!(add(1, 2), 3);
}

#[tokio::test]
async fn shadowed_tokio_test() {
    assert!(true);
}
"#,
    )
    .unwrap();
    std::fs::write(
        wt.join("tests/integration.rs"),
        "#[test]\nfn other_test() {}\n",
    )
    .unwrap();
}

/// Spawn the Rust adapter rooted at the given directory and return a child
/// process handle. The child gets an explicit cwd — never rely on the harness
/// process cwd (testaruda-pzh6).
fn spawn_adapter(dir: &std::path::Path) -> std::process::Child {
    Command::new(adapter_path())
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Rust adapter")
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

/// Extract node_ids from a discover response.
fn parse_discover_node_ids(resp: &str) -> Vec<String> {
    let parsed: serde_json::Value =
        serde_json::from_str(resp).expect("discover response should be valid JSON");
    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "discover failed: {resp}"
    );
    parsed["result"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|t| t["node_id"].as_str().map(|s| s.to_string()))
        .collect()
}

#[test]
fn rust_adapter_discover_excludes_hidden_dirs() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_rust_fixture_with_hidden_worktree(dir.path());

    let mut child = spawn_adapter(dir.path());
    let resp = send_command(&mut child, r#"{"command":"discover"}"#);
    let node_ids = parse_discover_node_ids(&resp);
    child.kill().ok();
    child.wait().ok();

    let real = ["real_unit_test", "real_integration_test"];
    for name in real {
        assert!(
            node_ids.iter().any(|id| id.contains(name)),
            "should discover {name}, got: {node_ids:?}"
        );
    }

    // Nothing from the hidden worktree may leak into discovery.
    for id in &node_ids {
        assert!(
            !id.contains(".claude"),
            "hidden-dir test leaked into discovery: {id}"
        );
        assert!(
            !id.contains("shadowed_tokio_test") && !id.contains("other_test"),
            "worktree-only test leaked into discovery: {id}"
        );
    }
    assert_eq!(
        node_ids.len(),
        2,
        "expected exactly the 2 real tests, got: {node_ids:?}"
    );
}
