//! exec: uncalibrated-store warning tests (gh-26 / testaruda-n5b4).
//! Extracted from store.rs to stay under the 5000-line file gate.

use super::*;
use crate::test_util::TestUnwrap;

#[test]
fn test_run_history_is_empty_on_fresh_store() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(dir.path().join(".testaruda")).unwrap_or_panic();
    store.initialize().unwrap_or_panic();
    assert!(store.run_history_is_empty().unwrap_or_panic());
}

#[test]
fn test_run_history_is_empty_false_after_run() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(dir.path().join(".testaruda")).unwrap_or_panic();
    store.initialize().unwrap_or_panic();
    let conn = store.conn();
    conn.execute(
        "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 't1')",
        [],
    )
    .unwrap_or_panic();
    let tid: u32 = conn
        .query_row("SELECT id FROM test_items", [], |row| row.get(0))
        .unwrap_or_panic();
    conn.execute(
        "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
         VALUES (?1, 'r1', 'passed', 10, 'default')",
        rusqlite::params![tid],
    )
    .unwrap_or_panic();
    assert!(!store.run_history_is_empty().unwrap_or_panic());
}
