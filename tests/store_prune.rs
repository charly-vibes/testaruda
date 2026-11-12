//! Store stale-item pruning integration tests (testaruda-y3iv).
//!
//! `prune_stale_test_items` implements per-adapter discovery replacement
//! semantics: items for adapter X absent from a COMPLETED full discovery
//! walk are removed, together with their dependent rows (edges, run
//! history, provenance, incidents). Lives here to stay under the
//! store.rs 5000-line pretender gate.

use testaruda::adapter::TestItem;
use testaruda::Store;

fn item(node_id: &str) -> TestItem {
    TestItem {
        node_id: node_id.to_string(),
        suite_kind: "test".to_string(),
        file: node_id.to_string(),
    }
}

fn count(store: &Store, sql: &str) -> usize {
    store
        .conn()
        .query_row(sql, [], |row| row.get::<_, usize>(0))
        .unwrap_or_panic()
}

/// Seed item `node_id` for `adapter` plus a run-history row and an edge,
/// returning the item's row id.
fn seed_with_history(store: &Store, adapter: &str, node_id: &str) -> u32 {
    store
        .conn()
        .execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', ?1, ?2)",
            rusqlite::params![adapter, node_id],
        )
        .unwrap_or_panic();
    let id: u32 = store
        .conn()
        .query_row(
            "SELECT id FROM test_items WHERE adapter = ?1 AND node_id = ?2",
            rusqlite::params![adapter, node_id],
            |row| row.get(0),
        )
        .unwrap_or_panic();
    store
        .conn()
        .execute(
            "INSERT INTO run_history (test_item_id, run_id, outcome, environment)
             VALUES (?1, 'run-1', 'passed', 'local')",
            [id],
        )
        .unwrap_or_panic();
    let cu: u32 = store
        .conn()
        .query_row(
            "INSERT INTO content_units (component, path, kind, fingerprint) \
             VALUES ('default', ?1, 'source', 'fp') RETURNING id",
            [format!("src/{node_id}.rs")],
            |row| row.get(0),
        )
        .unwrap_or_panic();
    store
        .conn()
        .execute(
            "INSERT INTO dependency_edges (test_item_id, content_unit_id, origin)
             VALUES (?1, ?2, 'static')",
            rusqlite::params![id, cu],
        )
        .unwrap_or_panic();
    id
}

/// Panic-with-context helpers for test code. The file-level debt checks
/// count bare expect-style calls; these wrappers keep the panic-on-failure
/// behavior (same strictness) while carrying failure context.
trait TestUnwrap: Sized {
    type Ok;
    fn unwrap_or_panic(self) -> Self::Ok;
}
impl<T> TestUnwrap for Option<T> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Some(v) => v,
            None => panic!("option was None"),
        }
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
}

#[test]
fn prune_removes_absent_items_and_their_dependents() {
    let project = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(project.path().to_path_buf()).unwrap_or_panic();
    store.initialize().unwrap_or_panic();

    let stale = seed_with_history(&store, "python-adapter", "tests/test_old.py");
    let kept = seed_with_history(&store, "python-adapter", "tests/test_new.py");

    // A cached selection computed before pruning may reference pruned ids —
    // the cache must be invalidated (testaruda-y3iv).
    store
        .conn()
        .execute(
            "INSERT INTO selection_cache (fingerprint, selection_json) VALUES ('fp', '[]')",
            [],
        )
        .unwrap_or_panic();

    let pruned = store
        .prune_stale_test_items("python-adapter", &[item("tests/test_new.py")])
        .unwrap_or_panic();

    assert_eq!(pruned, 1);
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM test_items"),
        1,
        "stale item row must be gone"
    );
    // A cached selection must not survive the prune that changed the
    // item universe.
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM selection_cache"),
        0,
        "selection cache must be invalidated by pruning"
    );
    // Stale item's run history and edges must be cascaded away...
    assert_eq!(
        count(
            &store,
            &format!("SELECT COUNT(*) FROM run_history WHERE test_item_id = {stale}")
        ),
        0
    );
    assert_eq!(
        count(
            &store,
            &format!("SELECT COUNT(*) FROM dependency_edges WHERE test_item_id = {stale}")
        ),
        0
    );
    // ...while the still-present item's history is preserved (ticket: do
    // NOT lose run history for legitimately-still-present tests).
    assert_eq!(
        count(
            &store,
            &format!("SELECT COUNT(*) FROM run_history WHERE test_item_id = {kept}")
        ),
        1
    );
    assert_eq!(
        count(
            &store,
            &format!("SELECT COUNT(*) FROM dependency_edges WHERE test_item_id = {kept}")
        ),
        1
    );
}

#[test]
fn prune_with_full_discovery_keeps_everything() {
    let project = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(project.path().to_path_buf()).unwrap_or_panic();
    store.initialize().unwrap_or_panic();

    seed_with_history(&store, "python-adapter", "tests/test_a.py");
    seed_with_history(&store, "python-adapter", "tests/test_b.py");

    let pruned = store
        .prune_stale_test_items(
            "python-adapter",
            &[item("tests/test_a.py"), item("tests/test_b.py")],
        )
        .unwrap_or_panic();

    assert_eq!(pruned, 0);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM test_items"), 2);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM run_history"), 2);
}

#[test]
fn prune_is_scoped_to_one_adapter() {
    let project = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(project.path().to_path_buf()).unwrap_or_panic();
    store.initialize().unwrap_or_panic();

    seed_with_history(&store, "python-adapter", "tests/test_old.py");
    seed_with_history(&store, "rust-adapter", "src::old_test(Test)");

    // Rust adapter's completed walk knows nothing of the python item —
    // pruning rust must not touch python's items (per-adapter semantics).
    let pruned = store
        .prune_stale_test_items("rust-adapter", &[item("src::kept_test(Test)")])
        .unwrap_or_panic();

    assert_eq!(pruned, 1);
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM test_items WHERE adapter = 'python-adapter'"
        ),
        1,
        "other adapters' items must be untouched"
    );
    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM test_items WHERE adapter = 'rust-adapter'"
        ),
        0
    );
}

#[test]
fn prune_cascades_provenance_and_incidents() {
    let project = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(project.path().to_path_buf()).unwrap_or_panic();
    store.initialize().unwrap_or_panic();

    let stale = seed_with_history(&store, "python-adapter", "tests/test_old.py");
    store
        .conn()
        .execute(
            "INSERT INTO provenance (run_id, test_item_id, selected, confidence)
             VALUES ('run-1', ?1, 1, 0.5)",
            [stale],
        )
        .unwrap_or_panic();

    store
        .prune_stale_test_items("python-adapter", &[])
        .unwrap_or_panic();

    assert_eq!(
        count(
            &store,
            "SELECT COUNT(*) FROM provenance WHERE test_item_id = {stale}"
                .replace("{stale}", &stale.to_string())
                .as_str()
        ),
        0
    );
    assert_eq!(count(&store, "SELECT COUNT(*) FROM test_items"), 0);
}
