//! testaruda-adapter-rust — Reference adapter for Rust projects.
//!
//! Reads JSON commands from stdin, responds on stdout.
//! Protocol: single JSON line → single JSON line response.

use std::io::{BufRead, BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_command(&trimmed);
        let out = serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_string());
        println!("{}", out);
        std::io::stdout().flush().ok();
    }
}

fn handle_command(input: &str) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return json_err(&format!("invalid JSON: {}", e)),
    };

    let command = cmd["command"].as_str().unwrap_or("");
    match command {
        "handshake" => cmd_handshake(),
        "discover" => cmd_discover(),
        "static-deps" => cmd_static_deps(&cmd),
        "fingerprint" => cmd_fingerprint(&cmd),
        "run-args" => cmd_run_args(&cmd),
        "ingest" => cmd_ingest(&cmd),
        _ => json_err(&format!("unknown command: {}", command)),
    }
}

fn json_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "result": result})
}

fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": msg})
}

/// Handshake: declare capabilities (TIA-ADAPT-002).
fn cmd_handshake() -> serde_json::Value {
    json_ok(serde_json::json!({
        "name": "rust-adapter",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["rust"],
        "granularity": "symbol",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}

/// Discover: enumerate tests by scanning for #[test] in files (TIA-ADAPT-004).
fn cmd_discover() -> serde_json::Value {
    let mut tests = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.file_type().is_file()
                && e.path().to_string_lossy().ends_with(".rs")
                && !p.contains("/target/")
                && !p.contains("/.flatpak-builder/")
        })
    {
        let path = entry.path().to_string_lossy().to_string();
        // Strip ./ prefix for clean node_id generation
        let clean_path = path.strip_prefix("./").unwrap_or(&path);
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lines: Vec<&str> = content.lines().collect();

        for i in 0..lines.len() {
            let trimmed = lines[i].trim();

            let is_test_attr = trimmed == "#[test]" || trimmed.starts_with("#[tokio::test]");

            if is_test_attr && i + 1 < lines.len() {
                let fn_line = lines[i + 1].trim();
                let name = fn_line
                    .strip_prefix("fn ")
                    .and_then(|s| s.split(['(', '{', ' ']).next())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                if !name.is_empty() && !name.starts_with('_') {
                    let node_id = format!(
                        "{}::{}(Test)",
                        clean_path.replace('/', "::").replace(".rs", ""),
                        name
                    );
                    tests.push(serde_json::json!({
                        "node_id": node_id,
                        "suite_kind": "unit",
                        "file": clean_path,
                    }));
                }
            }
        }
    }

    json_ok(serde_json::Value::Array(tests))
}

/// Static-deps: analyze test file imports and map changed source files to dependent
/// tests (TIA-ADAPT-005).
///
/// Strategy:
/// 1. Discover all tests (files with `#[test]`/`#[tokio::test]`) and group by file.
/// 2. For each test file, parse `use` statements to find which source files it
///    imports from.
/// 3. Build a reverse map: `source_file → [test_ids]` that depend on it.
/// 4. For each changed file, return edges to its dependent tests.
///
/// This handles three common Rust test patterns:
///
/// - **Inline tests**: `#[cfg(test)] mod tests { use super::*; #[test] fn ... }`
///   Tests inside `src/*.rs` depend on their own file.
/// - **Integration tests** (`tests/*.rs`): `use crate_name::item;` — resolves the
///   crate name from `Cargo.toml` to find `lib.rs` / `main.rs`.
/// - **Module tests**: `use crate::module::item;` — resolves `crate::module` to
///   `src/module.rs` or `src/module/mod.rs`.
fn cmd_static_deps(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let changed_files: Vec<String> = match params["changed_files"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.changed_files'"),
    };

    // 1. Discover all tests and read Cargo.toml
    let all_tests = discover_all_tests();
    let tests_by_file = group_tests_by_file(&all_tests);
    let cargo_info = parse_cargo_toml();

    // 2. Build reverse map: source file → test IDs that depend on it
    let source_to_tests = build_source_to_test_map(&tests_by_file, &cargo_info);

    // 3. For each changed file, find dependent test edges
    let mut edges = Vec::new();
    let mut unresolved = Vec::new();

    for file in &changed_files {
        if let Some(test_ids) = source_to_tests.get(file) {
            for test_id in test_ids {
                edges.push(serde_json::json!({
                    "from": test_id,
                    "to": file,
                    "weight": 1_000_000,
                    "origin": "static",
                }));
            }
        } else {
            unresolved.push(file.clone());
        }
    }

    // All test IDs as candidates
    let candidates: Vec<String> = tests_by_file.values().flat_map(|v| v.clone()).collect();

    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": [],
    })
}

// ── Cargo.toml parsing ───────────────────────────────────────────────────────

/// Info extracted from a project's `Cargo.toml` for resolving import paths.
struct CargoInfo {
    /// Package name, e.g. `"recall_fixture"` or `"bat"`.
    crate_name: String,
    /// Path to `lib.rs` if a `[lib]` section exists (default `src/lib.rs`).
    lib_path: String,
    /// Path to `main.rs` for crates with no lib (default `src/main.rs`).
    main_path: Option<String>,
}

/// Parse the project's `Cargo.toml` to extract crate name and lib path.
///
/// Uses `.get()` instead of `[]` indexing to avoid panics when sections
/// are missing (toml 0.8 panics on missing key access via `[]`).
fn parse_cargo_toml() -> Option<CargoInfo> {
    let content = std::fs::read_to_string("Cargo.toml").ok()?;
    let parsed: toml::Value = toml::from_str(&content).ok()?;

    let crate_name = parsed.get("package")?.get("name")?.as_str()?.to_string();

    // Resolve [lib] path: explicit path or default src/lib.rs
    let lib_path = parsed
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or("src/lib.rs")
        .to_string();

    // Resolve [[bin]] path for crates with no lib. Handle both
    // `[bin]` (table) and `[[bin]]` (array of tables) formats.
    let main_path = parsed
        .get("bin")
        .and_then(|bin| {
            bin.as_array()
                .and_then(|arr| arr.first())
                .and_then(|first| first.get("path"))
                .and_then(|p| p.as_str())
                .or_else(|| bin.as_str())
        })
        .map(|s| s.to_string());

    Some(CargoInfo {
        crate_name,
        lib_path,
        main_path,
    })
}

// ── Test discovery ───────────────────────────────────────────────────────────

/// Discover all test items across the project, returning raw data used by both
/// `cmd_discover` and `cmd_static_deps`.
fn discover_all_tests() -> Vec<serde_json::Value> {
    let mut tests = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.file_type().is_file()
                && p.ends_with(".rs")
                && !p.contains("/target/")
                && !p.contains("/.flatpak-builder/")
        })
    {
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path).to_string();
        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let lines: Vec<&str> = content.lines().collect();

        for i in 0..lines.len() {
            let trimmed = lines[i].trim();
            let is_test_attr = trimmed == "#[test]" || trimmed.starts_with("#[tokio::test]");

            if is_test_attr && i + 1 < lines.len() {
                let fn_line = lines[i + 1].trim();
                let name = fn_line
                    .strip_prefix("fn ")
                    .and_then(|s| s.split(['(', '{', ' ']).next())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                if !name.is_empty() && !name.starts_with('_') {
                    let node_id = format!(
                        "{}::{}(Test)",
                        clean_path.replace('/', "::").replace(".rs", ""),
                        name
                    );
                    tests.push(serde_json::json!({
                        "node_id": node_id,
                        "suite_kind": "unit",
                        "file": clean_path,
                    }));
                }
            }
        }
    }

    tests
}

/// Group discovered tests by their source file.
fn group_tests_by_file(
    tests: &[serde_json::Value],
) -> std::collections::HashMap<String, Vec<String>> {
    let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for t in tests {
        if let (Some(node_id), Some(file)) = (t["node_id"].as_str(), t["file"].as_str()) {
            map.entry(file.to_string())
                .or_default()
                .push(node_id.to_string());
        }
    }
    map
}

// ── Reverse dependency mapping ───────────────────────────────────────────────

/// Build a map from source file path → test IDs that depend on it.
///
/// For each test file, parses its `use` statements and resolves them to concrete
/// filesystem paths. Also handles the common inline-test pattern where tests
/// inside `src/` files depend on their own file.
fn build_source_to_test_map(
    tests_by_file: &std::collections::HashMap<String, Vec<String>>,
    cargo_info: &Option<CargoInfo>,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut source_to_tests: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for (test_file, test_ids) in tests_by_file {
        let content = match std::fs::read_to_string(test_file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let source_deps = resolve_test_file_deps(test_file, &content, cargo_info);

        for source_file in &source_deps {
            let entry = source_to_tests.entry(source_file.clone()).or_default();
            for test_id in test_ids {
                entry.push(test_id.clone());
            }
        }
    }

    source_to_tests
}

/// Resolve the source files a test file depends on, given its content and
/// Cargo.toml info.
///
/// Covers three patterns:
/// - **Inline tests** (`src/*.rs`): tests in source files depend on the same file.
/// - **`use crate::module::item`**: resolves `crate::module` to `src/module.rs`.
/// - **`use crate_name::item`** (integration tests): resolves crate name to lib.rs.
fn resolve_test_file_deps(
    test_file: &str,
    content: &str,
    cargo_info: &Option<CargoInfo>,
) -> Vec<String> {
    let mut deps: Vec<String> = Vec::new();

    // Pattern 1: inline tests — test file is also a source file.
    // Tests inside src/*.rs with #[cfg(test)] mod tests { use super::*; ... }
    // depend on their own source file.
    if test_file.starts_with("src/") || test_file.starts_with("tests/") {
        deps.push(test_file.to_string());
    }

    // Pattern 2: `use crate::module::item;` — resolve to source file path.
    for line in content.lines() {
        let trimmed = line.trim();

        if let Some(path) = trimmed
            .strip_prefix("use crate::")
            .and_then(|s| s.strip_suffix(';'))
            .map(|s| s.trim())
        {
            let parts: Vec<&str> = path.split("::").collect();
            let file_deps = resolve_crate_module_to_file(&parts);
            deps.extend(file_deps);
        }

        // Pattern 3: `use super::*;` or `use super::item;` in inline test mods.
        if trimmed.starts_with("use super::") || trimmed == "use super::*;" {
            // For inline tests, super::* refers to the parent module (the file itself)
            if test_file.starts_with("src/") || test_file.starts_with("tests/") {
                deps.push(test_file.to_string());
            }
        }
    }

    // Pattern 4: integration test uses crate name directly.
    // e.g., `use recall_fixture::add;` in tests/integration_test.rs
    if let Some(info) = cargo_info {
        let crate_use_prefix = format!("use {}::", info.crate_name);
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(&crate_use_prefix) {
                deps.push(info.lib_path.clone());
                if let Some(main_path) = &info.main_path {
                    deps.push(main_path.clone());
                }
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    deps.retain(|d| seen.insert(d.clone()));

    deps
}

/// Resolve a `crate::module::submodule::...` path to filesystem paths.
///
/// Tries progressively shorter path prefixes:
/// - `crate::foo::bar::Baz` → `src/foo/bar/baz.rs`, `src/foo/bar.rs`, `src/foo.rs`
/// - Only returns paths that actually exist on disk.
fn resolve_crate_module_to_file(parts: &[&str]) -> Vec<String> {
    if parts.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();

    // The last part might be a named item, not a module. Try progressively
    // shorter paths to find the actual module file.
    for len in 1..=parts.len() {
        let segments: Vec<&str> = parts[..len].to_vec();
        let rel = segments.join("/");

        let as_file = format!("src/{}.rs", rel);
        if std::path::Path::new(&as_file).is_file() {
            candidates.push(as_file);
        }

        let as_mod = format!("src/{}/mod.rs", rel);
        if std::path::Path::new(&as_mod).is_file() {
            candidates.push(as_mod);
        }
    }

    candidates
}

/// Fingerprint: compute blake3 hashes for files (TIA-ADAPT-006).
fn cmd_fingerprint(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let files: Vec<String> = match params["files"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.files'"),
    };

    let mut fingerprints = Vec::new();
    for file in &files {
        let content = match std::fs::read(file) {
            Ok(c) => c,
            Err(e) => {
                return json_err(&format!("cannot read {}: {}", file, e));
            }
        };
        let hash = blake3::hash(&content);
        fingerprints.push(serde_json::json!({
            "file": file,
            "fingerprint": hash.to_hex().to_string(),
            "symbol": null,
        }));
    }

    serde_json::json!({"ok": true, "fingerprints": fingerprints})
}

/// Run-args: return native runner arguments for the selected test set (TIA-ADAPT-007).
/// Does NOT execute the tests.
fn cmd_run_args(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let selected: Vec<String> = match params["selected"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.selected'"),
    };

    if selected.is_empty() {
        return json_err("no tests selected");
    }

    // Build cargo test args: extract test names from node_ids like
    // "crate::module::test_name(Test)"
    let mut test_names: Vec<String> = Vec::new();
    for node_id in &selected {
        if let Some(name) = node_id.rsplit_once("::") {
            let clean = name.1.strip_suffix("(Test)").unwrap_or(name.1);
            test_names.push(clean.to_string());
        } else {
            test_names.push(node_id.clone());
        }
    }

    let mut runner_args = vec!["cargo", "test", "--"];
    runner_args.extend(test_names.iter().map(|s| s.as_str()));

    // Collection path for JUnit-style results
    let collection_path = "target/test-results.xml".to_string();

    json_ok(serde_json::json!({
        "runner_args": runner_args,
        "collection_path": collection_path,
    }))
}

/// Ingest: parse test runner output and return runtime edges and results (TIA-ADAPT-008).
fn cmd_ingest(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let run_output = match params["run_output"].as_str() {
        Some(s) => s,
        None => return json_err("missing 'params.run_output'"),
    };

    if run_output.is_empty() {
        return json_err("empty run output");
    }

    let per_test_results =
        parse_cargo_test_output(run_output).unwrap_or_else(|| parse_json_lines(run_output));

    json_ok(serde_json::json!({
        "runtime_edges": Vec::<serde_json::Value>::new(),
        "per_test_results": per_test_results,
        "external_inputs": [],
    }))
}

/// Parse cargo test output: lines like "test test_name ... ok" or "FAILED".
fn parse_cargo_test_output(run_output: &str) -> Option<Vec<serde_json::Value>> {
    let mut per_test_results: Vec<serde_json::Value> = Vec::new();
    for line in run_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("test ") && trimmed.ends_with(" ok") {
            let test_name = trimmed
                .strip_prefix("test ")
                .and_then(|s| s.strip_suffix(" ok"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !test_name.is_empty() {
                per_test_results.push(serde_json::json!({
                    "test_id": test_name,
                    "outcome": "passed",
                }));
            }
        } else if trimmed.starts_with("test ") && trimmed.ends_with(" FAILED") {
            let test_name = trimmed
                .strip_prefix("test ")
                .and_then(|s| s.strip_suffix(" FAILED"))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !test_name.is_empty() {
                per_test_results.push(serde_json::json!({
                    "test_id": test_name,
                    "outcome": "failed",
                }));
            }
        }
    }
    if per_test_results.is_empty() {
        None
    } else {
        Some(per_test_results)
    }
}

/// Parse JSON-lines format where each line is a JSON object with test_id + outcome.
fn parse_json_lines(run_output: &str) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    for line in run_output.lines() {
        let trimmed = line.trim();
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if val.get("test_id").is_some() && val.get("outcome").is_some() {
                results.push(val);
            }
        }
    }
    results
}
