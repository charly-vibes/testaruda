//! Predictive ranking calibration gate tests (TIA-VER-005).
//! Extracted from store.rs: the inline test module pushed the file past
//! the pretender 5000-file-lines gate (tambor-cpd arc).

use crate::store::Store;

#[test]
fn test_ranking_calibration_recall_computed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join(".testaruda")).unwrap();
    store.initialize().unwrap();

    let conn = store.conn();

    // Insert 3 test items
    let mut tids = Vec::new();
    for name in ["test_a", "test_b", "test_c"] {
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', ?1)",
            rusqlite::params![name],
        )
        .unwrap();
        let tid: u32 = conn
            .query_row(
                "SELECT id FROM test_items WHERE node_id = ?1",
                rusqlite::params![name],
                |row| row.get(0),
            )
            .unwrap();
        tids.push(tid);
    }

    // Training runs: test_a failed 3/3, test_b failed 0/3, test_c failed 2/3
    // So predicted order: test_a (100%) > test_c (67%) > test_b (0%)
    for i in 0..3 {
        for &tid in &tids {
            let is_failed = tid == tids[0] || (tid == tids[2] && i < 2);
            let outcome = if is_failed { "failed" } else { "passed" };
            let run_id = format!("train-{}", i);
            conn.execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, ?2, ?3, 10, 'default')",
                rusqlite::params![tid, run_id, outcome],
            )
            .unwrap();
        }
    }

    // Test runs (hold-out): test_a fails again, test_b passes, test_c fails
    // top-k should capture test_a and test_c (the failures)
    for &tid in &tids {
        let outcome = if tid == tids[1] { "passed" } else { "failed" };
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'test-run', ?2, 10, 'default')",
            rusqlite::params![tid, outcome],
        )
        .unwrap();
    }

    let metrics = store.evaluate_ranking_calibration().unwrap();

    assert_eq!(metrics.total_test_items, 3);
    assert_eq!(metrics.total_failures, 2);
    assert_eq!(metrics.captured_failures, 2);
    assert_eq!(metrics.recall_at_k, 1.0);
    assert_eq!(metrics.k, 3);
}

#[test]
fn test_ranking_calibration_zero_failures_vacuous_pass() {
    // tambor-cpd: a clean hold-out run (0 failures) gives no ranking
    // evidence either way - recall over an empty positives set is
    // vacuous 1.0, not 0.0. A 0.0 here made every all-green run read
    // as NOT CALIBRATED forever (recall 0/0 below any threshold).
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join(".testaruda")).unwrap();
    store.initialize().unwrap();

    let conn = store.conn();
    let tids: Vec<u32> = (0..3)
        .map(|i| {
            conn.execute(
                "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', ?1)",
                rusqlite::params![format!("clean_test_{}", i)],
            ).unwrap();
            conn.query_row(
                "SELECT id FROM test_items WHERE node_id = ?1",
                rusqlite::params![format!("clean_test_{}", i)],
                |row| row.get(0),
            ).unwrap()
        })
        .collect();

    // Training runs: real mixed history (some failures) so the split
    // and predicted ranking are meaningful.
    for (i, &tid) in tids.iter().enumerate() {
        for run in 0..2 {
            let outcome = if i == 0 && run == 0 {
                "failed"
            } else {
                "passed"
            };
            conn.execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, ?2, ?3, 10, 'default')",
                rusqlite::params![tid, format!("train-{}", run), outcome],
            )
            .unwrap();
        }
    }

    // Hold-out: everything passes (clean run, 0 actual failures).
    for &tid in &tids {
        conn.execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
             VALUES (?1, 'test-run', 'passed', 10, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();
    }

    let metrics = store.evaluate_ranking_calibration().unwrap();
    assert_eq!(metrics.total_test_items, 3);
    assert_eq!(metrics.total_failures, 0);
    assert_eq!(
        metrics.recall_at_k, 1.0,
        "vacuous recall for 0-failure hold-out"
    );
    assert!(
        metrics.vacuous,
        "0-failure hold-out must be flagged vacuous"
    );
}

#[test]
fn test_ranking_calibration_empty_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join(".testaruda")).unwrap();
    store.initialize().unwrap();

    let metrics = store.evaluate_ranking_calibration().unwrap();
    assert_eq!(metrics.total_test_items, 0);
    assert_eq!(metrics.total_failures, 0);
    assert_eq!(metrics.recall_at_k, 0.0);
}

#[test]
fn test_ranking_calibration_no_hold_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join(".testaruda")).unwrap();
    store.initialize().unwrap();

    let conn = store.conn();
    conn.execute(
        "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', 'only_test')",
        [],
    ).unwrap();
    let tid: u32 = conn
        .query_row("SELECT id FROM test_items", [], |row| row.get(0))
        .unwrap();

    // Only one run — no separate hold-out possible
    conn.execute(
        "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
         VALUES (?1, 'only-run', 'passed', 10, 'default')",
        rusqlite::params![tid],
    )
    .unwrap();

    let metrics = store.evaluate_ranking_calibration().unwrap();
    assert_eq!(metrics.total_test_items, 0, "no hold-out set available");
    assert_eq!(metrics.total_failures, 0);
    assert_eq!(metrics.recall_at_k, 0.0);
}
