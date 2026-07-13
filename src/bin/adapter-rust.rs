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
        let out = serde_json::to_string(&response).unwrap_or_else(|_| {
            r#"{"ok":false,"error":"serialization failed"}"#.to_string()
        });
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

            let is_test_attr = trimmed == "#[test]"
                || trimmed.starts_with("#[tokio::test]");

            if is_test_attr && i + 1 < lines.len() {
                let fn_line = lines[i + 1].trim();
                let name = fn_line
                    .strip_prefix("fn ")
                    .and_then(|s| s.split(|c| c == '(' || c == '{' || c == ' ').next())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                if !name.is_empty() && !name.starts_with('_') {
                    let node_id = format!("{}::{}(Test)",
                        clean_path.replace('/', "::").replace(".rs", ""), name);
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
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        None => return json_err("missing 'params.changed_files'"),
    };

    let mut unresolved = Vec::new();
    let mut edges = Vec::new();

    // Collect all test nodes mapped by file
    let discover_all = cmd_discover();
    let tests_by_file = if let Some(results) = discover_all["result"].as_array() {
        let mut map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for t in results {
            if let (Some(node_id), Some(file)) = (t["node_id"].as_str(), t["file"].as_str()) {
                map.entry(file.to_string()).or_default().push(node_id.to_string());
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
    let candidates: Vec<String> = tests_by_file.values()
        .flat_map(|v| v.clone())
        .collect();

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
        Some(arr) => arr.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
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
