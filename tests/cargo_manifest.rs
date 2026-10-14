//! Purpose: Pin the Cargo.toml dependency-table structure.
//! Responsibilities:
//! - Assert every unconditional dependency is in the plain [dependencies]
//!   table, not swallowed by a target-specific one.
//!
//! Rationale: a `[target.'cfg(unix)'.dependencies]` table placed mid-list
//! silently claims every dependency that follows it — Linux/macOS keep
//! building (cfg(unix) matches) while Windows loses the crates and the
//! release build breaks (v0.5.4 first tag, testaruda release incident).
//! Platform-split bugs like this are invisible to a Linux-only gate, so the
//! manifest shape itself is the checkable invariant.

use std::path::Path;

#[test]
fn unconditional_dependencies_are_not_swallowed_by_target_tables() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let raw = std::fs::read_to_string(&manifest).expect("read Cargo.toml");
    let value: toml::Value = raw.parse().expect("parse Cargo.toml");

    let deps = value
        .get("dependencies")
        .and_then(|d| d.as_table())
        .expect("[dependencies] table must exist");

    // Every crate the sources `use` unconditionally. A missing entry here
    // means a target-specific table above it claimed it (see module rationale).
    for required in [
        "ascent",
        "blake3",
        "genesis-vibes",
        "glob",
        "miette",
        "rusqlite",
        "serde",
        "serde_json",
        "shell-words",
        "thiserror",
        "time",
        "toml",
        "walkdir",
    ] {
        assert!(
            deps.contains_key(required),
            "`{required}` is missing from [dependencies] — it was likely \
             swallowed by a target-specific table declared above it; move \
             that table after the last unconditional dependency"
        );
    }

    // The only unix-gated dependency is libc (testaruda-cd4u process-group kill).
    let unix = value
        .get("target")
        .and_then(|t| t.get("cfg(unix)"))
        .and_then(|t| t.get("dependencies"))
        .and_then(|d| d.as_table())
        .expect("[target.'cfg(unix)'.dependencies] table must exist");
    let unix_keys: Vec<&String> = unix.keys().collect();
    assert_eq!(
        unix_keys,
        vec!["libc"],
        "unexpected unix-gated dependencies: {unix_keys:?}"
    );
}
