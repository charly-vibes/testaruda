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

/// Static-deps: analyze imports in changed files and map to test items (TIA-ADAPT-005).
fn cmd_static_deps(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let changed_files: Vec<String> = match params["changed_files"].as_array() {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        None => return json_err("missing 'params.changed_files'"),
    };

    let mut unresolved = Vec::new();
    let mut edges = Vec::new();

    // Collect all test nodes mapped by file
    let discover_all = cmd_discover();
    let tests_by_file = if let Some(results) = discover_all["result"].as_array() {
        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for t in results {
            if let (Some(node_id), Some(file)) = (t["node_id"].as_str(), t["file"].as_str()) {
                map.entry(file.to_string())
                    .or_default()
                    .push(node_id.to_string());
            }
        }
        map
    } else {
        std::collections::HashMap::new()
    };

    for file in &changed_files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => {
                unresolved.push(file.clone());
                continue;
            }
        };

        // Find test items in this file
        let test_ids = tests_by_file.get(file).cloned().unwrap_or_default();

        // Parse use statements to find dependencies
        let dependencies = parse_rust_imports(&content);

        for test_id in &test_ids {
            for _dep in &dependencies {
                edges.push(serde_json::json!({
                    "from": test_id,
                    "to": file,
                    "weight": 1_000_000,
                    "origin": "static",
                }));
            }
        }
    }

    // Candidate tests: all discovered test node IDs
    let candidates: Vec<String> = tests_by_file.values().flat_map(|v| v.clone()).collect();

    // Return flat response (no `result` wrapper) — core expects top-level fields
    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": [],
    })
}

/// Parse `use` statements from Rust source code (simplified).
fn parse_rust_imports(content: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") {
            // Extract the crate path: "use crate::module::item;" → "crate::module::item"
            let path = trimmed
                .strip_prefix("use ")
                .and_then(|s| s.strip_suffix(';'))
                .map(|s| s.trim());
            if let Some(p) = path {
                // Only take the crate/module prefix (first 2 segments)
                let parts: Vec<&str> = p.split("::").collect();
                let dep = if parts.len() >= 2 {
                    format!("{}::{}", parts[0], parts[1])
                } else {
                    p.to_string()
                };
                deps.push(dep);
            }
        }
    }
    deps
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

    let mut per_test_results: Vec<serde_json::Value> = Vec::new();
    let runtime_edges: Vec<serde_json::Value> = Vec::new();

    // Parse cargo test output for test results
    // Lines look like: "test test_name ... ok" or "test test_name ... FAILED"
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

    // If no standard test output found, try to parse as JSON-line format
    if per_test_results.is_empty() {
        for line in run_output.lines() {
            let trimmed = line.trim();
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if val.get("test_id").is_some() && val.get("outcome").is_some() {
                    per_test_results.push(val);
                }
            }
        }
    }

    json_ok(serde_json::json!({
        "runtime_edges": runtime_edges,
        "per_test_results": per_test_results,
        "external_inputs": [],
    }))
}
