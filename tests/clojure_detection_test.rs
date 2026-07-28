use std::path::Path;
use testaruda::config::detect_project_language;

#[test]
fn detect_clojure_by_deps_edn() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("deps.edn"), "{}").unwrap();
    let result = detect_project_language(dir.path());
    assert_eq!(result, Some("testaruda-adapter-clojure".to_string()));
}

#[test]
fn detect_clojure_by_project_clj() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("project.clj"),
        "(defproject my-project \"0.1.0\")",
    )
    .unwrap();
    let result = detect_project_language(dir.path());
    assert_eq!(result, Some("testaruda-adapter-clojure".to_string()));
}

#[test]
fn detect_prefers_rust_over_clojure() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Cargo.toml"), "").unwrap();
    std::fs::write(dir.path().join("deps.edn"), "{}").unwrap();
    let result = detect_project_language(dir.path());
    assert_eq!(result, Some("testaruda-adapter-rust".to_string()));
}

#[test]
fn detect_no_clojure_marker_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("README.md"), "").unwrap();
    let result = detect_project_language(dir.path());
    assert_eq!(result, None);
}
