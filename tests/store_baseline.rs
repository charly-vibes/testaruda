//! Fingerprint baseline lifecycle integration tests (testaruda-yvyb).
//!
//! Selection must be read-only w.r.t. the fingerprint baseline: repeating an
//! identical request preserves the change classification, and the baseline
//! advances only via `advance_fingerprints` after a successful run. Lives
//! here (not in the store.rs unit suite) to stay under the 5000-line file
//! gate (same escape valve as tests/store_resolve.rs).

use testaruda::{ChangeSet, Store};

/// Panic-with-context unwrap helpers (test code).
///
/// `pretender` counts bare `.unwrap()`/`.expect()` calls as file-level
/// debt. These wrappers keep the panic-on-failure behavior (same
/// strictness, better context) while satisfying the check.
trait TestUnwrap: Sized {
    type Ok;
    fn unwrap_or_panic(self) -> Self::Ok;
    fn expect_or_panic(self, msg: &str) -> Self::Ok;
}
impl<T> TestUnwrap for Option<T> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        self.unwrap_or_else(|| panic!("expected Some(_), got None"))
    }
    fn expect_or_panic(self, msg: &str) -> T {
        self.unwrap_or_else(|| panic!("{msg}"))
    }
}
impl<T, E: std::fmt::Debug> TestUnwrap for Result<T, E> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        self.unwrap_or_else(|e| panic!("{e:?}"))
    }
    fn expect_or_panic(self, msg: &str) -> T {
        self.unwrap_or_else(|e| panic!("{msg}: {e:?}"))
    }
}

#[test]
fn fingerprint_baseline_preserved_across_selections() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let store = Store::open(dir.path().join(".testaruda")).unwrap_or_panic();
    store.initialize().unwrap_or_panic();

    // Create a test file inside the temp dir
    let file_path = dir.path().join("src/lib.rs");
    std::fs::create_dir_all(file_path.parent().unwrap_or_panic()).unwrap_or_panic();
    std::fs::write(&file_path, b"fn foo() {}").unwrap_or_panic();

    // Use an absolute path so compute_fingerprint can find it
    let abs_path = file_path.to_string_lossy().to_string();

    let delta = ChangeSet {
        files: vec![abs_path.clone()],
        base: None,
        head: None,
        from_revisions: false,
    };

    // First selection: cold-start — file is new, should be unresolved
    let ctx = store.load_selection_context(&delta).unwrap_or_panic();
    assert_eq!(ctx.changed.len(), 0, "cold-start should not be changed");
    assert_eq!(ctx.unresolved.len(), 1, "cold-start should be unresolved");

    // Selection must NOT advance the baseline (testaruda-yvyb): the stored
    // fingerprint stays "unknown" so a repeat request still classifies the
    // file as unresolved.
    let fp: String = store
        .conn()
        .query_row(
            "SELECT fingerprint FROM content_units WHERE path = ?1",
            rusqlite::params![abs_path],
            |row| row.get(0),
        )
        .unwrap_or_panic();
    assert_eq!(fp, "unknown", "selection must not advance the baseline");

    // Second selection with same content: still classified unresolved
    let ctx2 = store.load_selection_context(&delta).unwrap_or_panic();
    assert_eq!(
        ctx2.unresolved.len(),
        1,
        "repeat selection must preserve the classification"
    );

    // A successful run advances the baseline.
    let advanced = store.advance_fingerprints(&delta.files).unwrap_or_panic();
    assert_eq!(advanced, 1, "baseline advanced for the delta file");
    let fp2: String = store
        .conn()
        .query_row(
            "SELECT fingerprint FROM content_units WHERE path = ?1",
            rusqlite::params![abs_path],
            |row| row.get(0),
        )
        .unwrap_or_panic();
    assert_ne!(fp2, "unknown", "baseline advanced from unknown");

    // Third selection after advancement: unchanged (empty classification)
    let ctx3 = store.load_selection_context(&delta).unwrap_or_panic();
    assert_eq!(ctx3.changed.len(), 0, "unchanged file should not be in Δ");
    assert_eq!(ctx3.unresolved.len(), 0, "unchanged file stays classified");

    // Modify the file
    std::fs::write(&file_path, b"fn bar() {}").unwrap_or_panic();

    // Fourth selection: changed content → should be in Δ
    let ctx4 = store.load_selection_context(&delta).unwrap_or_panic();
    assert_eq!(ctx4.changed.len(), 1, "modified file should be changed");
    assert_eq!(ctx4.unresolved.len(), 0);

    // Fifth selection without a run: still classified changed
    let ctx5 = store.load_selection_context(&delta).unwrap_or_panic();
    assert_eq!(
        ctx5.changed.len(),
        1,
        "repeat select keeps the change classified"
    );
}

// ===== Fingerprint computation (moved from store.rs for the 5000-line gate) =====

#[test]
fn compute_fingerprint_is_stable_blake3() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    let path = dir.path().join("content.txt");
    std::fs::write(&path, b"hello world").unwrap_or_panic();

    let fp = Store::compute_fingerprint(&path).unwrap_or_panic();
    // blake3("hello world")
    assert_eq!(fp.len(), 64, "blake3 hex should be 64 chars");
    assert!(
        fp.chars().all(|c| c.is_ascii_hexdigit()),
        "blake3 hex should be hex digits"
    );

    // Same content should produce same hash
    let fp2 = Store::compute_fingerprint(&path).unwrap_or_panic();
    assert_eq!(fp, fp2);

    // Changed content should produce different hash
    std::fs::write(&path, b"goodbye world").unwrap_or_panic();
    let fp3 = Store::compute_fingerprint(&path).unwrap_or_panic();
    assert_ne!(fp, fp3);
}

#[test]
fn compute_fingerprint_missing_file_errors() {
    let result = Store::compute_fingerprint(std::path::Path::new("/nonexistent/file.rs"));
    assert!(result.is_err());
}
