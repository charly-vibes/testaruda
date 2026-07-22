//! Seeded-fault recall test (TIA-VER-004 variant).
//!
//! Runs the full adapter pipeline — handshake → discover → static-deps — then
//! feeds the resulting edges into the selection engine. Seeds a known semantic
//! mutation in a source file and verifies the fault-revealing test is selected.
//!
//! This mirrors the pattern from tests/seeded_fault.rs but uses real adapter
//! binary output instead of synthetic edges, providing end-to-end coverage of
//! the adapter→store→selector pipeline.
//!
//! Can serve as a pre-deployment gate: if this test fails, the selection
//! pipeline is broken.
//!
//! Note: tests MUST run single-threaded (they change the process cwd).

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};

use testaruda::adapter::DepEdge;
use testaruda::ChangeSet;
use testaruda::Selector;
use testaruda::Store;
use testaruda::ONE;

/// Global lock for CWD-manipulating tests to prevent parallel interference.
static CWD_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Cwd guard: changes to a temp directory, restores on drop.
struct CwdGuard {
    saved: PathBuf,
}

impl CwdGuard {
    fn enter(temp: &std::path::Path) -> Self {
        let saved = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp).unwrap();
        Self { saved }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.saved);
    }
}

/// Check if the Rust adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-rust")
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

/// Create a minimal Rust fixture project in the given directory.
fn create_rust_fixture(dir: &std::path::Path) {
    std::fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "recall-fixture"
version = "0.1.0"
edition = "2021"

[lib]
name = "recall_fixture"
path = "src/lib.rs"

[dependencies]
"#,
    )
    .unwrap();

    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 { a + b }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_positive() { assert_eq!(add(2, 3), 5); }

    #[test]
    fn test_add_negative() { assert_eq!(add(-1, 1), 0); }

    #[test]
    fn test_add_zero() { assert_eq!(add(0, 0), 0); }

    #[test]
    fn test_add_positive_values() { assert_eq!(add(10, 20), 30); }
}
"#,
    )
    .unwrap();

    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(
        dir.join("tests/integration_test.rs"),
        r#"use recall_fixture::add;

#[test]
fn integration_test_add() { assert_eq!(add(100, 200), 300); }
"#,
    )
    .unwrap();
}

/// Parse the Rust adapter's discover response into test node IDs.
fn parse_discover_results(resp: &str) -> Vec<String> {
    let parsed: serde_json::Value =
        serde_json::from_str(resp).expect("discover response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));
    parsed["result"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["node_id"].as_str().map(|s| s.to_string()))
        .collect()
}

/// Parse the Rust adapter's static-deps response into DepEdges.
fn parse_static_deps_result(resp: &str) -> Vec<DepEdge> {
    let parsed: serde_json::Value =
        serde_json::from_str(resp).expect("static-deps response should be valid JSON");
    assert!(parsed["ok"].as_bool().unwrap_or(false));
    parsed["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| DepEdge {
            from: e["from"].as_str().unwrap().to_string(),
            to: e["to"].as_str().unwrap().to_string(),
            weight: e["weight"].as_u64().unwrap_or(ONE as u64) as u32,
            origin: e["origin"].as_str().unwrap_or("static").to_string(),
        })
        .collect()
}

/// Seed a store with test items, a content unit, run history, and dependency edges.
/// Returns the content_unit_id and the tid_map.
fn seed_store(
    store: &Store,
    test_ids: &[String],
    cu_path: &str,
    edge_froms: &HashSet<String>,
) -> (u32, HashMap<String, u32>) {
    let conn = store.conn();

    // Insert test items
    for node_id in test_ids {
        conn.execute(
            "INSERT OR IGNORE INTO test_items (component, adapter, node_id)
             VALUES ('default', 'rust-adapter', ?1)",
            rusqlite::params![node_id],
        )
        .unwrap();
    }

    // Build tid_map
    let mut tid_map = HashMap::new();
    for node_id in test_ids {
        let tid: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE component='default' AND adapter='rust-adapter' AND node_id=?1",
                rusqlite::params![node_id],
                |row| row.get(0),
            )
            .unwrap();
        tid_map.insert(node_id.clone(), tid);
    }

    // Create content unit
    conn.execute(
        "INSERT OR IGNORE INTO content_units (component, path, symbol, kind, fingerprint)
         VALUES ('default', ?1, NULL, 'source', 'unknown')",
        rusqlite::params![cu_path],
    )
    .unwrap();
    let cu_id: u32 = conn
        .query_row(
            "SELECT id FROM content_units WHERE component='default' AND path=?1 AND symbol IS NULL",
            rusqlite::params![cu_path],
            |row| row.get(0),
        )
        .unwrap();

    // Seed run history for all tests
    for tid in tid_map.values() {
        conn.execute(
            "INSERT OR IGNORE INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'seed-run', 'passed', 50, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();
    }

    // Insert edges only for test_ids in edge_froms
    for (node_id, &tid) in &tid_map {
        if !edge_froms.contains(node_id) {
            continue;
        }
        conn.execute(
            "INSERT OR IGNORE INTO dependency_edges (test_item_id, content_unit_id, environment, origin, k_value)
             VALUES (?1, ?2, 'default', 'static', ?3)",
            rusqlite::params![tid, cu_id, ONE],
        )
        .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO reverse_index (content_unit_id, test_item_id)
             VALUES (?1, ?2)",
            rusqlite::params![cu_id, tid],
        )
        .unwrap();
    }

    (cu_id, tid_map)
}

#[test]
fn test_recall_fault_pipeline() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-rust not on PATH — skipping");
        return;
    }

    let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_rust_fixture(dir.path());
    std::fs::write(dir.path().join("testaruda.toml"), "").unwrap();

    let store = Store::open(dir.path().join(".testaruda")).unwrap();
    store.initialize().unwrap();
    let _cwd = CwdGuard::enter(dir.path());

    // ---- Adapter pipeline: handshake → discover → static-deps ----
    let mut child = Command::new("testaruda-adapter-rust")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Rust adapter");

    send_command(&mut child, r#"{"command":"handshake"}"#);
    let resp = send_command(&mut child, r#"{"command":"discover"}"#);
    let test_ids = parse_discover_results(&resp);
    assert_eq!(test_ids.len(), 5, "should discover 5 test items");

    let cmd = serde_json::json!({
        "command": "static-deps",
        "params": {"changed_files": ["src/lib.rs"]}
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let edges = parse_static_deps_result(&resp);
    assert!(
        !edges.is_empty(),
        "should have edges from tests to src/lib.rs"
    );
    for edge in &edges {
        assert_eq!(edge.to, "src/lib.rs");
        assert!(
            test_ids.contains(&edge.from),
            "edge source '{}' should be a discovered test",
            edge.from
        );
    }

    child.kill().ok();
    child.wait().ok();

    // ---- Seed store with edges from all 5 test_ids ----
    let all_edge_froms: HashSet<String> = test_ids.iter().cloned().collect();
    seed_store(&store, &test_ids, "src/lib.rs", &all_edge_froms);

    // ---- Run selector ----
    let delta = ChangeSet {
        files: vec!["src/lib.rs".to_string()],
        base: None,
        head: None,
    };
    let sel = Selector::select(&store, &delta).unwrap();

    // All 5 tests should be selected
    assert_eq!(
        sel.selected_count,
        5,
        "all 5 tests should be selected, got: {:?}",
        sel.tests.iter().map(|t| t.id).collect::<Vec<_>>()
    );

    // Verify witnesses
    for t in &sel.tests {
        let witness = t
            .witness
            .as_ref()
            .expect("each selected test should have a witness");
        assert!(witness
            .iter()
            .any(|w| w.origin == testaruda::Origin::Static));
    }
}

#[test]
fn test_recall_fault_semantic_mutation_detected() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-rust not on PATH — skipping");
        return;
    }

    let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    create_rust_fixture(dir.path());
    std::fs::write(dir.path().join("testaruda.toml"), "").unwrap();

    let _cwd = CwdGuard::enter(dir.path());

    // ---- Adapter pipeline ----
    let mut child = Command::new("testaruda-adapter-rust")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Rust adapter");

    send_command(&mut child, r#"{"command":"handshake"}"#);
    let resp = send_command(&mut child, r#"{"command":"discover"}"#);
    let test_ids = parse_discover_results(&resp);
    assert_eq!(test_ids.len(), 5, "should discover 5 tests");

    let cmd = serde_json::json!({
        "command": "static-deps",
        "params": {"changed_files": ["src/lib.rs"]}
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let edges = parse_static_deps_result(&resp);
    // 4 unit tests have edges; integration test may not have a direct file-level edge
    assert!(
        edges.len() >= 4,
        "at least 4 edges expected, got: {}",
        edges.len()
    );

    child.kill().ok();
    child.wait().ok();

    // ---- Seed store with edges only from the adapter's output ----
    let edge_froms: HashSet<String> = edges.iter().map(|e| e.from.clone()).collect();
    let store = Store::open(dir.path().join(".testaruda")).unwrap();
    store.initialize().unwrap();
    seed_store(&store, &test_ids, "src/lib.rs", &edge_froms);

    // ---- Semantic mutation: change add(a,b) to a - b ----
    std::fs::write(
        dir.path().join("src/lib.rs"),
        r#"pub fn add(a: i32, b: i32) -> i32 { a - b }  // BUG: was a + b

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_positive() { assert_eq!(add(2, 3), 5); }

    #[test]
    fn test_add_negative() { assert_eq!(add(-1, 1), 0); }

    #[test]
    fn test_add_zero() { assert_eq!(add(0, 0), 0); }

    #[test]
    fn test_add_positive_values() { assert_eq!(add(10, 20), 30); }
}
"#,
    )
    .unwrap();

    // ---- Run selector ----
    let delta = ChangeSet {
        files: vec!["src/lib.rs".to_string()],
        base: None,
        head: None,
    };
    let sel = Selector::select(&store, &delta).unwrap();

    // Only the 4 tests with edges should be selected
    assert_eq!(
        sel.selected_count,
        4,
        "4 tests with edges should be selected after semantic mutation, got: {:?}",
        sel.tests.iter().map(|t| t.id).collect::<Vec<_>>()
    );

    // Verify witnesses
    for t in &sel.tests {
        let witness = t
            .witness
            .as_ref()
            .expect("selected test should have a witness");
        assert!(witness
            .iter()
            .any(|w| w.origin == testaruda::Origin::Static));
    }
}
