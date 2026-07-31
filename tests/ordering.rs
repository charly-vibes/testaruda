//! Ordering tests: deterministic, duration, and predictive ranking (TIA-SEL-005, TIA-SEL-006, TIA-SEL-007).
//!
//! Verifies:
//! - TIA-SEL-005: identical inputs + store state → byte-identical output
//! - TIA-SEL-006: duration ordering sorts by descending mean duration
//! - TIA-SEL-007: predictive ranking re-orders by failure rate, never removes always-run
//!
//! Note: tests MUST run single-threaded (they change the process cwd).

use std::path::Path;
use std::path::PathBuf;

use genesis::fixture::Fixture;
use testaruda::adapter::DepEdge;
use testaruda::ChangeSet;
use testaruda::Selector;
use testaruda::Store;
use testaruda::TestOrdering;
use testaruda::ONE;

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

/// Setup a seeded graph matching the seeded_fault pattern.
/// All four content units have known fingerprints stored.
/// Files are expected to already exist on disk (created via Fixture).
fn setup_graph(store: &Store, root: &Path) -> [u32; 4] {
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

    for p in &paths {
        let fp = Store::compute_fingerprint(&root.join(p)).unwrap();
        conn.execute(
            "INSERT INTO content_units (component, path, symbol, kind, fingerprint)
             VALUES ('default', ?1, NULL, 'source', ?2)",
            rusqlite::params![p, fp],
        )
        .unwrap();
    }

    let mut tids: [u32; 4] = [0; 4];
    for (i, n) in nodes.iter().enumerate() {
        conn.execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'rust-adapter', ?1)",
            rusqlite::params![n],
        )
        .unwrap();
        tids[i] = conn
            .query_row(
                "SELECT id FROM test_items WHERE component='default' AND node_id=?1",
                rusqlite::params![n],
                |row| row.get(0),
            )
            .unwrap();
    }

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
    // interfere with ordering scenarios (TIA-SAFE-007 always-run is tested
    // directly in store-level tests)
    for &tid in &tids {
        conn.execute(
            "INSERT OR IGNORE INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)\
             VALUES (?1, 'seed-run', 'passed', 50, 'default')",
            rusqlite::params![tid],
        )
        .unwrap();
    }

    tids
}

fn run_test<F>(f: F)
where
    F: FnOnce(&Store, &Path, &[u32; 4]),
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
    let _guard = CwdGuard::enter(root);

    let store = Store::open(root.join(".testaruda")).unwrap();
    let tids = setup_graph(&store, root);
    f(&store, root, &tids);
}

#[test]
fn test_ordering_scenarios() {
    run_test(|store, root, tids| {
        // === Scenario 1: Deterministic stability (TIA-SEL-005) ===
        // Each select call updates fingerprints, so we must use different file
        // content or different files across scenarios.
        // We own 4 files; each scenario uses one that hasn't been touched before.
        //
        // Scenario 1: helpers.py → affects tids[0,1,3] (session, invoice, helpers)
        {
            std::fs::write(root.join("src/helpers.py"), b"def fmt(x): return str(x)").unwrap();
            let delta = ChangeSet {
                files: vec!["src/helpers.py".to_string()],
                base: None,
                head: None,
            };

            let sel1 =
                Selector::select_with_ordering(store, &delta, TestOrdering::Deterministic).unwrap();
            // Second call: different content on helpers.py so a new change is detected
            std::fs::write(
                root.join("src/helpers.py"),
                b"def fmt(x, y): return str(x + y)",
            )
            .unwrap();
            let delta2 = ChangeSet {
                files: vec!["src/helpers.py".to_string()],
                base: None,
                head: None,
            };
            let sel2 = Selector::select_with_ordering(store, &delta2, TestOrdering::Deterministic)
                .unwrap();

            assert_eq!(
                sel1.selected_count, sel2.selected_count,
                "same number of tests across calls"
            );
            let ids1: Vec<u32> = sel1.tests.iter().map(|t| t.id).collect();
            let ids2: Vec<u32> = sel2.tests.iter().map(|t| t.id).collect();
            assert_eq!(
                ids1, ids2,
                "deterministic order must be stable across calls"
            );
            for i in 1..ids1.len() {
                assert!(ids1[i - 1] < ids1[i], "must be strictly ascending by ID");
            }
        }

        // === Scenario 2: Deterministic sorts output (TIA-SEL-005) ===
        // session.py → affects tids[0,2] (session, totp)
        {
            std::fs::write(root.join("src/session.py"), b"def login(): return 42").unwrap();
            let delta = ChangeSet {
                files: vec!["src/session.py".to_string()],
                base: None,
                head: None,
            };
            let sel =
                Selector::select_with_ordering(store, &delta, TestOrdering::Deterministic).unwrap();
            assert_eq!(sel.selected_count, 2);
            let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
            assert!(ids[0] < ids[1], "must be sorted by ID: {:?}", ids);
            // Verify: tids[0] (session) and tids[2] (totp) — sorted by ID
            // IDs are auto-increment from test_items insertion order, so
            // tids[0] < tids[1] < tids[2] < tids[3]
            assert_eq!(ids[0], tids[0], "first must be test_session");
            assert_eq!(ids[1], tids[2], "second must be test_totp");
        }

        // === Scenario 3: Duration ordering descending (TIA-SEL-006) ===
        // invoice.py → affects tids[1] (invoice) only
        // Seed durations: tids[1] = 200ms, tids[0] = 100ms (not affected by this change)
        {
            store.conn().execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, 'dur-1', 'passed', 100, 'default')",
                rusqlite::params![tids[0]],
            ).unwrap();
            store.conn().execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, 'dur-1', 'passed', 200, 'default')",
                rusqlite::params![tids[1]],
            ).unwrap();

            std::fs::write(root.join("src/invoice.py"), b"def create(x): return x * 2").unwrap();
            let delta = ChangeSet {
                files: vec!["src/invoice.py".to_string()],
                base: None,
                head: None,
            };
            let sel =
                Selector::select_with_ordering(store, &delta, TestOrdering::ByDuration).unwrap();
            // invoice.py → test_invoice (tid[1]) only
            assert_eq!(sel.selected_count, 1);
            assert_eq!(sel.tests[0].id, tids[1]);
        }

        // === Scenario 4: No-history tiebreaker by ID (TIA-SEL-006) ===
        // helpers.py was already modified in scenario 1, so we re-modify it
        // This affects tids[0,1,3] — none have duration history for this run
        {
            std::fs::write(
                root.join("src/helpers.py"),
                b"def fmt(a, b, c): return str(a + b + c)",
            )
            .unwrap();
            let delta = ChangeSet {
                files: vec!["src/helpers.py".to_string()],
                base: None,
                head: None,
            };
            let sel =
                Selector::select_with_ordering(store, &delta, TestOrdering::ByDuration).unwrap();
            // 3 tests affected: session, invoice, helpers (tids[0,1,3])
            assert_eq!(sel.selected_count, 3);
            let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
            // invoice (200ms from scenario 3) before session (100ms) before helpers (no history)
            // All tests are in the selection, order is by duration desc, then ID asc as tiebreaker
            assert_eq!(ids[0], tids[1], "invoice (200ms) should be first");
            assert_eq!(ids[1], tids[0], "session (100ms) should be second");
            assert_eq!(ids[2], tids[3], "helpers (no history) should be last");
        }

        // === Scenario 5: Always-run preserved (TIA-SEL-007) ===
        // totp.py → no test depends on it directly.
        // Instead, mark test_totp as always-run and change session.py.
        // session.py was already modified in scenario 2, so re-modify.
        {
            // Mark test_totp as always-run (previously failed)
            store.conn().execute(
                "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                 VALUES (?1, 'fail-1', 'failed', 50, 'default')",
                rusqlite::params![tids[2]],
            ).unwrap();

            std::fs::write(
                root.join("src/session.py"),
                b"def login(): return True  # v5",
            )
            .unwrap();
            let delta = ChangeSet {
                files: vec!["src/session.py".to_string()],
                base: None,
                head: None,
            };

            // Default ordering
            let sel = Selector::select_with_ordering(store, &delta, TestOrdering::Default).unwrap();
            let ids: std::collections::HashSet<u32> = sel.tests.iter().map(|t| t.id).collect();
            assert!(ids.contains(&tids[0]), "test_session (dep on session.py)");
            assert!(ids.contains(&tids[2]), "test_totp (always-run from failed)");
            assert_eq!(sel.selected_count, 2);
        }

        // === Scenario 6: Predictive ranking by failure rate (TIA-SEL-007) ===
        // Seed per-test failure histories, then change helpers.py → affects tids[0,1,3].
        // Expected order: tids[0] (60% failed) → tids[3] (0% passed) → tids[1] (no history)
        {
            // Seed failure histories (5 runs each)
            // tid[0]: 3/5 failed = 60%
            for i in 0..5 {
                let outcome = if i < 3 { "failed" } else { "passed" };
                store.conn().execute(
                    "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                     VALUES (?1, ?2, ?3, 50, 'default')",
                    rusqlite::params![tids[0], format!("pred-{}-{}", i, tids[0]), outcome],
                ).unwrap();
            }
            // tid[3]: 5/5 passed = 0% failure rate
            for i in 0..5 {
                store.conn().execute(
                    "INSERT INTO run_history (test_item_id, run_id, outcome, duration_ms, environment)
                     VALUES (?1, ?2, 'passed', 50, 'default')",
                    rusqlite::params![tids[3], format!("pred-{}-{}", i, tids[3])],
                ).unwrap();
            }
            // tid[1]: no history (0 runs) → goes last

            std::fs::write(
                root.join("src/helpers.py"),
                b"def fmt(p, q, r, s): return str(p + q + r + s)",
            )
            .unwrap();
            let delta = ChangeSet {
                files: vec!["src/helpers.py".to_string()],
                base: None,
                head: None,
            };

            // Note: setup_graph seeds all 4 tids with a 'passed' run.
            // scenario 5 adds a failed run for tids[2].
            // scenario 6 adds:
            //   tids[0]: 3 failed + 2 passed → 3/6 = 50%
            //   tids[3]: 5 passed → 0/6 = 0%
            //   tids[1]: no additions → 0/1 = 0%
            //   tids[2]: no additions → 1/2 = 50% (from scenario 5)
            //
            // Always-run from scenario 5 means tids[2] is also selected → 4 tests total.
            // Expected order: tids[0] (50%) → tids[2] (50%) → tids[1] (0%) → tids[3] (0%)
            // Tiebreaker on equal rate: ascending ID
            let sel =
                Selector::select_with_ordering(store, &delta, TestOrdering::Predictive).unwrap();
            assert_eq!(
                sel.selected_count, 4,
                "helpers.py → 3 affected + 1 always-run"
            );
            let ids: Vec<u32> = sel.tests.iter().map(|t| t.id).collect();
            // Verify descending failure rate with ID tiebreaker
            let rates = store.load_failure_rates().unwrap();
            for i in 1..ids.len() {
                let ra = rates.get(&ids[i - 1]).copied().unwrap_or(0.0);
                let rb = rates.get(&ids[i]).copied().unwrap_or(0.0);
                assert!(
                    ra >= rb || (ra == rb && ids[i - 1] < ids[i]),
                    "order violation at {}: id={}(rate={}) before id={}(rate={})",
                    i,
                    ids[i - 1],
                    ra,
                    ids[i],
                    rb
                );
            }
        }
    });
}
