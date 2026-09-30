//! Store runner-id resolution integration tests (testaruda-l0ts).
//!
//! `resolve_test_item_id` must resolve python-style runner ids
//! (`<file>::<test>`) against file-granular discover node_ids, without
//! disturbing the Rust-flavored tolerant forms. Lives here (not in the
//! store.rs unit suite) to stay under the 5000-line file gate.

use testaruda::Store;

/// Insert a test item and return its id.
fn insert_item(store: &Store, node_id: &str) -> u32 {
    store
        .conn()
        .execute(
            "INSERT INTO test_items (component, adapter, node_id) VALUES ('default', 'test', ?1)",
            [node_id],
        )
        .unwrap();
    store
        .conn()
        .query_row(
            "SELECT id FROM test_items WHERE node_id = ?1",
            [node_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn resolve_test_item_id_python_file_level_result() {
    // adapter-python discover is file-granular: node_ids are test file
    // paths. Run results arrive as "<file>::<test>" and must resolve to
    // the file-level item (round-3 finding, bichos).
    let project = tempfile::tempdir().unwrap();
    let store = Store::open(project.path().to_path_buf()).unwrap();
    store.initialize().unwrap();
    let expected = insert_item(&store, "tests/test_cli.py");

    assert_eq!(
        store.resolve_test_item_id("tests/test_cli.py::test_version_flag"),
        Some(expected)
    );
    // Non-file-looking prefixes (Rust module paths) must not match.
    assert_eq!(store.resolve_test_item_id("rules::some_test"), None);
}

#[test]
fn resolve_test_item_id_rust_forms_still_resolve() {
    // The file-prefix rule must not break the Rust tolerant forms
    // (testaruda-1m3i / a6gw regressions).
    let project = tempfile::tempdir().unwrap();
    let store = Store::open(project.path().to_path_buf()).unwrap();
    store.initialize().unwrap();
    let src_item = insert_item(&store, "src::lib::always_passes(Test)");
    let file_item = insert_item(&store, "tests/test_cli.py");

    // Exact node_id.
    assert_eq!(
        store.resolve_test_item_id("src::lib::always_passes(Test)"),
        Some(src_item)
    );
    // src:: path variant for a bare cargo id.
    assert_eq!(
        store.resolve_test_item_id("lib::always_passes"),
        Some(src_item)
    );
    // File-prefix rule for python ids.
    assert_eq!(
        store.resolve_test_item_id("tests/test_cli.py::test_version_flag"),
        Some(file_item)
    );
}
