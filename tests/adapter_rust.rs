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

/// Parse a static-deps response and return (edges, unresolved).
///
/// static-deps responses are flat envelopes (top-level edges/unresolved,
/// matching the engine's StaticDepsResponse deserialization) — unlike
/// discover/ingest which wrap in "result".
fn parse_static_deps(resp: &str) -> (Vec<(String, String)>, Vec<String>) {
    let parsed: serde_json::Value =
        serde_json::from_str(resp).expect("static-deps response should be valid JSON");
    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "static-deps failed: {resp}"
    );
    let edges = parsed["edges"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|e| {
            Some((
                e["from"].as_str()?.to_string(),
                e["to"].as_str()?.to_string(),
            ))
        })
        .collect();
    let unresolved = parsed["unresolved"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|u| u.as_str().map(String::from))
        .collect();
    (edges, unresolved)
}

/// Fotos-like fixture (testaruda-khn7): a Tauri-style repo whose Rust crate
/// lives at `src-tauri/` — NOT the cwd `src/`. `ai/ocr.rs` carries inline
/// `#[cfg(test)]` tests; `commands/mod.rs` imports it via `use crate::ai::ocr`.
fn create_nested_crate_fixture(dir: &std::path::Path) {
    let src = dir.join("src-tauri/src");
    std::fs::create_dir_all(src.join("ai")).unwrap();
    std::fs::create_dir_all(src.join("commands")).unwrap();
    std::fs::write(
        dir.join("src-tauri/Cargo.toml"),
        r#"[package]
name = "fotos"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src-tauri/src/lib.rs"),
        "pub mod ai;\npub mod commands;\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src-tauri/src/ai/ocr.rs"),
        r#"pub fn ocr(path: &str) -> String { path.to_string() }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_test() {
        assert!(true);
    }
}
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src-tauri/src/commands/mod.rs"),
        r#"use crate::ai::ocr::ocr;

pub fn run(path: &str) -> String { ocr(path) }

#[cfg(test)]
mod tests {
    #[test]
    fn run_test() {
        assert!(true);
    }
}
"#,
    )
    .unwrap();
}

#[test]
fn rust_adapter_inline_self_edge_outside_src_root() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_nested_crate_fixture(dir.path());

    let mut child = spawn_adapter(dir.path());
    let resp = send_command(
        &mut child,
        r#"{"command":"static-deps","params":{"changed_files":["src-tauri/src/ai/ocr.rs"]}}"#,
    );
    let (edges, unresolved) = parse_static_deps(&resp);
    child.kill().ok();
    child.wait().ok();

    // The changed file carries inline tests: it must self-link, never be
    // unresolved (before the fix it was unresolved → engine over-selected 66/66).
    assert!(
        !unresolved.contains(&"src-tauri/src/ai/ocr.rs".to_string()),
        "inline-test file must not be unresolved, got: {unresolved:?}"
    );
    assert!(
        edges
            .iter()
            .any(|(from, to)| { to == "src-tauri/src/ai/ocr.rs" && from.contains("ocr_test") }),
        "changed file's own inline test must be selected, got edges: {edges:?}"
    );
}

#[test]
fn rust_adapter_crate_import_resolves_against_package_src_root() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_nested_crate_fixture(dir.path());

    let mut child = spawn_adapter(dir.path());
    let resp = send_command(
        &mut child,
        r#"{"command":"static-deps","params":{"changed_files":["src-tauri/src/ai/ocr.rs"]}}"#,
    );
    let (edges, _) = parse_static_deps(&resp);
    child.kill().ok();
    child.wait().ok();

    // `use crate::ai::ocr` in commands/mod.rs must resolve against the
    // package src root (src-tauri/src), so commands' inline tests depend
    // on the changed file too.
    assert!(
        edges
            .iter()
            .any(|(from, to)| { to == "src-tauri/src/ai/ocr.rs" && from.contains("run_test") }),
        "crate::-rooted importer's tests must depend on changed file, got edges: {edges:?}"
    );
}

/// Prefix-collision fixture (testaruda-u1bv): two tests whose names collide
/// under cargo's substring filter semantics ("foo" matches "foo_bar" too).
fn create_collision_fixture(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "collision-fixture"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        r#"#[test]
fn foo() {
    assert!(true);
}

#[test]
fn foo_bar() {
    assert!(true);
}

#[test]
fn unrelated() {
    assert!(true);
}
"#,
    )
    .unwrap();
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

/// Pin the run-args filter contract (testaruda-u1bv): the adapter emits bare
/// short test names as cargo substring filters, NOT --exact. Exact matching
/// requires cargo's full test path (`tests::foo`), but node_id-derived names
/// are short (`foo`) — a --exact flip would silently select ZERO tests. If
/// this test fails, someone changed the contract; re-verify short-name
/// matching before accepting it.
#[test]
fn rust_adapter_run_args_uses_substring_filters_not_exact() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_rust_fixture_with_hidden_worktree(dir.path());

    let mut child = spawn_adapter(dir.path());
    let resp = send_command(
        &mut child,
        r#"{"command":"run-args","params":{"selected":["src/lib.rs::real_unit_test(Test)"]}}"#,
    );
    child.kill().ok();
    child.wait().ok();

    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("run-args response should be valid JSON");
    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "run-args failed: {resp}"
    );
    let args = parsed["result"]["runner_args"]
        .as_array()
        .expect("runner_args array");
    let args: Vec<&str> = args.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        args.contains(&"real_unit_test"),
        "bare short name must be a filter arg: {args:?}"
    );
    assert!(
        !args.contains(&"--exact"),
        "run-args must NOT use --exact: short names don't match cargo's full \
         test paths under exact matching — that would select 0 tests: {args:?}"
    );
}

/// Characterization (testaruda-u1bv): cargo substring filters over-run on
/// prefix collisions — selecting `foo` also runs `foo_bar`. Recall-safe by
/// design (never misses a test; may run extra ones). Pinned so a cargo
/// semantics change surfaces here instead of silently altering selection.
#[test]
fn rust_adapter_run_args_prefix_collision_over_runs() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_collision_fixture(dir.path());

    let output = Command::new("cargo")
        .current_dir(dir.path())
        .args(["test", "--", "foo"])
        .output()
        .expect("cargo test failed to run");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cargo test failed: {stdout}");
    assert!(
        stdout.contains("foo ... ok") || stdout.contains("test foo ... ok"),
        "selected test must run: {stdout}"
    );
    assert!(
        stdout.contains("test foo_bar ... ok"),
        "prefix-colliding test over-runs under substring filters \
         (recall-safe over-selection): {stdout}"
    );
    assert!(
        !stdout.contains("test unrelated"),
        "unrelated test must NOT run: {stdout}"
    );
}

/// Characterization (testaruda-u1bv): a stale selected name that no longer
/// matches any test makes cargo run 0 tests and exit 0 — nothing is recorded,
/// the stale item simply stays in the always-run set (SAFE-007) until it is
/// pruned by the next discover. Pinned: a cargo change to this exit contract
/// would silently turn stale selections into failures.
#[test]
fn rust_adapter_run_args_stale_name_zero_matches_exits_zero() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_collision_fixture(dir.path());

    let output = Command::new("cargo")
        .current_dir(dir.path())
        .args(["test", "--", "deleted_test_name"])
        .output()
        .expect("cargo test failed to run");

    assert!(
        output.status.success(),
        "0 matching filters must exit 0 (not an error), got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("running 0 tests"),
        "0 matches must run 0 tests: {stdout}"
    );
}
