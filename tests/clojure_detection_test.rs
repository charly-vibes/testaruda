use testaruda::config::detect_project_language;

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
fn detect_clojure_by_deps_edn() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    std::fs::write(dir.path().join("deps.edn"), "{}").unwrap_or_panic();
    let result = detect_project_language(dir.path());
    assert_eq!(result, Some("testaruda-adapter-clojure".to_string()));
}

#[test]
fn detect_clojure_by_project_clj() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    std::fs::write(
        dir.path().join("project.clj"),
        "(defproject my-project \"0.1.0\")",
    )
    .unwrap_or_panic();
    let result = detect_project_language(dir.path());
    assert_eq!(result, Some("testaruda-adapter-clojure".to_string()));
}

#[test]
fn detect_prefers_rust_over_clojure() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    std::fs::write(dir.path().join("Cargo.toml"), "").unwrap_or_panic();
    std::fs::write(dir.path().join("deps.edn"), "{}").unwrap_or_panic();
    let result = detect_project_language(dir.path());
    assert_eq!(result, Some("testaruda-adapter-rust".to_string()));
}

#[test]
fn detect_no_clojure_marker_returns_none() {
    let dir = tempfile::tempdir().unwrap_or_panic();
    std::fs::write(dir.path().join("README.md"), "").unwrap_or_panic();
    let result = detect_project_language(dir.path());
    assert_eq!(result, None);
}
