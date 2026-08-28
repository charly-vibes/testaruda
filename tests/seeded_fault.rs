//! Seeded-fault recall test (TIA-VER-004).
//!
//! Verifies the soundness invariant (TIA-SAFE-001): for every seeded
//! regression, the fault-revealing test is selected. Follows the worked
//! example pattern from Appendix G of the SRS.
//!
//! Note: tests MUST run single-threaded (they change the process cwd).

use std::path::Path;

use genesis::fixture::Fixture;
use testaruda::adapter::DepEdge;
use testaruda::ChangeSet;
use testaruda::Origin;
use testaruda::Selector;
use testaruda::Store;
use testaruda::ONE;

/// Create a seeded graph matching the Appendix G worked example.
///
/// Content units (files):
///   src/session.py, src/invoice.py, src/totp.py, src/helpers.py
///
/// Test items:
///   test_session  (100) — static deps: session.py, helpers.py
///   test_invoice  (101) — static deps: invoice.py, helpers.py
///   test_totp     (102) — runtime  dep: session.py (TIA-RUN-002)
///   test_helpers  (103) — static deps: helpers.py
///
/// Files are expected to already exist on disk (created via Fixture).
fn setup_graph(store: &Store, root: &Path) -> (Vec<u32>, [u32; 4]) {
    store.initialize().unwrap();

    let paths = [
        "src/session.py",
        "src/invoice.py",
        "src/totp.py",
        "src/helpers.py",
    ];
    let nodes = [
        "src::session::test_login(Test)",
        "src::invoice::test_create(Test)",
        "src::totp::test_generate(Test)",
        "src::helpers::test_fmt(Test)",
    ];

    let conn = store.conn();

    // Insert content units
    let mut cids: Vec<u32> = Vec::new();
    for p in &paths {
        let fp = Store::compute_fingerprint(&root.join(p)).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', ?1, NULL, 'source', ?2)",
            rusqlite::params![p, fp],
        )
        .unwrap();
        let id: u32 = conn
            .query_row(
                "SELECT id FROM content_units WHERE component='default' AND path=?1",
                rusqlite::params![p],
                |row| row.get(0),
            )
            .unwrap();
        cids.push(id);
    }

    // Insert test items
    let mut tids: [u32; 4] = [0; 4];
    for (i, n) in nodes.iter().enumerate() {
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'rust-adapter', ?1)",
            rusqlite::params![n],
        ).unwrap();
        tids[i] = conn
            .query_row(
                "SELECT id FROM test_items WHERE component='default' AND node_id=?1",
                rusqlite::params![n],
                |row| row.get(0),
            )
            .unwrap();
    }

    // Insert edges
    let edges = vec![
        DepEdge {
            from: nodes[0].into(),
            to: paths[0].into(),
            weight: ONE,
            origin: "static".into(),
        },
        DepEdge {
            from: nodes[0].into(),
            to: paths[3].into(),
            weight: ONE,
            origin: "static".into(),
        },
        DepEdge {
            from: nodes[1].into(),
            to: paths[1].into(),
            weight: ONE,
            origin: "static".into(),
        },
        DepEdge {
            from: nodes[1].into(),
            to: paths[3].into(),
            weight: ONE,
            origin: "static".into(),
        },
        DepEdge {
            from: nodes[2].into(),
            to: paths[0].into(),
            weight: ONE,
            origin: "runtime".into(),
        },
        DepEdge {
            from: nodes[3].into(),
            to: paths[3].into(),
            weight: ONE,
            origin: "static".into(),
        },
    ];
    store.store_static_deps("rust-adapter", &edges).unwrap();

    // Seed a passed run for all tests so no-history always-run doesn't
    // interfere with the precision assertions (TIA-SAFE-007 always-run is
    // tested directly in store-level tests)
    for &tid in &tids {
        conn.execute(
            "INSERT OR IGNORE INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)\
             VALUES (?1, 'seed-run', 'passed', 50, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();
    }

    (cids, tids)
}

fn run_test<F>(f: F)
where
    F: FnOnce(&Store, &Path, &[u32], &[u32; 4]),
{
    let fixture = Fixture::new()
        .with_file("src/session.py", "def login(): pass")
        .with_file("src/invoice.py", "def create(): pass")
        .with_file("src/totp.py", "def generate(): pass")
        .with_file("src/helpers.py", "def fmt(): pass")
        .with_file("testaruda.toml", "")
        .build()
        .unwrap();
    let root = fixture.root();

    let store = Store::open(root.join(".testaruda")).unwrap();
    let (cids, tids) = setup_graph(&store, root);
    f(&store, root, &cids, &tids);
}

#[test]
fn test_seeded_fault_soundness_and_precision() {
    // Run all scenarios in a single test to avoid cwd conflicts
    run_test(|store, root, _cids, tids| {
        // Scenario 1: change session.py → test_session (static) + test_totp (runtime)
        {
            std::fs::write(
                root.join("src/session.py"),
                b"def login(): return True  # modified",
            )
            .unwrap();
            let delta = ChangeSet {
                files: vec!["src/session.py".to_string()],
                base: None,
                head: None,
            };
            let sel = Selector::select(store, &delta).unwrap();
            let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();

            assert!(
                ids.contains(&tids[0]),
                "test_session — static dep on session.py"
            );
            assert!(
                ids.contains(&tids[2]),
                "test_totp — runtime dep on session.py"
            );
            assert!(!ids.contains(&tids[1]), "test_invoice — not affected");
            assert!(!ids.contains(&tids[3]), "test_helpers — not affected");
            assert_eq!(sel.selected_count, 2);
            assert_eq!(sel.changed_count, 1);
        }

        // Scenario 2: change helpers.py → all except test_totp
        {
            std::fs::write(root.join("src/helpers.py"), b"def fmt(x): return str(x)").unwrap();
            let delta = ChangeSet {
                files: vec!["src/helpers.py".to_string()],
                base: None,
                head: None,
            };
            let sel = Selector::select(store, &delta).unwrap();
            let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();

            assert!(ids.contains(&tids[0]), "test_session");
            assert!(ids.contains(&tids[1]), "test_invoice");
            assert!(!ids.contains(&tids[2]), "test_totp — no dep on helpers.py");
            assert!(ids.contains(&tids[3]), "test_helpers");
            assert_eq!(sel.selected_count, 3);
        }

        // Scenario 3: change session.py + invoice.py → 3 tests
        {
            std::fs::write(root.join("src/session.py"), b"def login(): return 42").unwrap();
            std::fs::write(root.join("src/invoice.py"), b"def create(x): return x").unwrap();
            let delta = ChangeSet {
                files: vec!["src/session.py".to_string(), "src/invoice.py".to_string()],
                base: None,
                head: None,
            };
            let sel = Selector::select(store, &delta).unwrap();
            let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();

            assert!(ids.contains(&tids[0]), "test_session");
            assert!(ids.contains(&tids[1]), "test_invoice");
            assert!(ids.contains(&tids[2]), "test_totp — runtime dep");
            assert!(!ids.contains(&tids[3]), "test_helpers — unaffected");
            assert_eq!(sel.selected_count, 3);
        }

        // Scenario 4: runtime witness verification
        {
            std::fs::write(
                root.join("src/session.py"),
                b"def login(x): pass  # signature change",
            )
            .unwrap();
            let delta = ChangeSet {
                files: vec!["src/session.py".to_string()],
                base: None,
                head: None,
            };
            let sel = Selector::select(store, &delta).unwrap();
            let totp = sel.tests.iter().find(|t| t.id == tids[2]).unwrap();

            let has_runtime = totp
                .witness
                .as_ref()
                .map(|w| w.iter().any(|e| e.origin == Origin::Runtime))
                .unwrap_or(false);
            assert!(has_runtime, "test_totp witness should include runtime edge");
        }
    });
}
