//! testaruda-adapter-python — Reference adapter for Python projects.
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
        "name": "python-adapter",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["python"],
        "granularity": "file",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}

/// Discover: find test_*.py and *_test.py files (TIA-ADAPT-004).
fn cmd_discover() -> serde_json::Value {
    let mut tests = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        let fname = entry.file_name().to_string_lossy().to_string();
        if fname.starts_with("test_") && fname.ends_with(".py") || fname.ends_with("_test.py") {
            tests.push(serde_json::json!({
                "node_id": path,
                "suite_kind": "unit",
                "file": path,
            }));
        }
    }

    json_ok(serde_json::Value::Array(tests))
}

/// Static-deps: analyze imports in changed Python files and map to test items (TIA-ADAPT-005).
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

        let test_ids = tests_by_file.get(file).cloned().unwrap_or_default();
        let imports = parse_python_imports(&content);

        for test_id in &test_ids {
            for _imp in &imports {
                edges.push(serde_json::json!({
                    "from": test_id,
                    "to": file,
                    "weight": 1_000_000,
                    "origin": "static",
                }));
            }
        }
    }

    let candidates: Vec<String> = tests_by_file.values().flat_map(|v| v.clone()).collect();

    // Return flat response with `ok:true` at top level
    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": [],
    })
}

/// Parse `import` and `from ... import` statements from Python source.
fn parse_python_imports(content: &str) -> Vec<String> {
    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            let module = trimmed
                .strip_prefix("import ")
                .map(|s| s.split(" as ").next().unwrap_or(s).trim().to_string());
            if let Some(m) = module {
                // Take only the top-level module
                let top = m.split('.').next().unwrap_or(&m).to_string();
                deps.push(top);
            }
        } else if trimmed.starts_with("from ") {
            // from foo.bar import baz → dependency on foo
            let module = trimmed
                .strip_prefix("from ")
                .and_then(|s| s.split(" import ").next())
                .map(|s| s.trim().to_string());
            if let Some(m) = module {
                let top = m.split('.').next().unwrap_or(&m).to_string();
                deps.push(top);
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
            Err(e) => return json_err(&format!("cannot read {}: {}", file, e)),
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

    // Build pytest args: pass selected test file paths directly
    let runner_args: Vec<String> = std::iter::once("pytest".to_string())
        .chain(selected.iter().cloned())
        .chain(std::iter::once("-v".to_string()))
        .chain(std::iter::once("--junitxml=target/test-results.xml".to_string()))
        .collect();

    let collection_path = "target/test-results.xml".to_string();

    json_ok(serde_json::json!({
        "runner_args": runner_args,
        "collection_path": collection_path,
    }))
}

/// Ingest: parse pytest output and return runtime edges and results (TIA-ADAPT-008).
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

    // Parse pytest output for test results
    // Lines look like: "test_file.py::test_name PASSED" or "FAILED"
    for line in run_output.lines() {
        let trimmed = line.trim();

        if trimmed.ends_with(" PASSED") {
            let test_id = trimmed
                .strip_suffix(" PASSED")
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !test_id.is_empty() {
                per_test_results.push(serde_json::json!({
                    "test_id": test_id,
                    "outcome": "passed",
                }));
            }
        } else if trimmed.ends_with(" FAILED") {
            let test_id = trimmed
                .strip_suffix(" FAILED")
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !test_id.is_empty() {
                per_test_results.push(serde_json::json!({
                    "test_id": test_id,
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
