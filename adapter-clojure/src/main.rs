//! testaruda-adapter-clojure — JSON-over-stdin/stdout adapter for Clojure.
//!
//! Reads JSON commands from stdin, responds on stdout.
//! Protocol: single JSON line → single JSON line response.
//!
//! Supports the 6 adapter protocol commands (TIA-ADAPT-001):
//! - handshake: declare capabilities
//! - discover: enumerate tests via tree-sitter queries
//! - static-deps: extract :require/:use/:import dependencies
//! - fingerprint: blake3 hash of file contents
//! - run-args: build runner CLI args (deps.edn vs project.clj)
//! - ingest: parse JUnit XML or stdout for results
//!
//! **Status:** handshake + static-deps implemented. All other commands return
//! "not implemented" errors (see testaruda-kw6, testaruda-cch, testaruda-fjj).

#[allow(dead_code)]
mod project;
mod query;

use std::collections::{HashMap, HashSet};
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
        println!("{out}");
        std::io::stdout().flush().ok();
    }
}

fn handle_command(input: &str) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return json_err(&format!("invalid JSON: {e}")),
    };

    let command = cmd["command"].as_str().unwrap_or("");
    match command {
        "handshake" => cmd_handshake(),
        "discover" => cmd_discover(),
        "static-deps" => cmd_static_deps(&cmd),
        "fingerprint" => cmd_fingerprint(&cmd),
        "run-args" => cmd_run_args(&cmd),
        "ingest" => cmd_ingest(&cmd),
        _ => json_err(&format!("unknown command: {command}")),
    }
}

fn json_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "result": result})
}

fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": msg})
}

/// Handshake: declare capabilities (TIA-ADAPT-017).
fn cmd_handshake() -> serde_json::Value {
    json_ok(serde_json::json!({
        "name": "testaruda-adapter-clojure",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["clojure"],
        "granularity": "file",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}

/// Static-deps: extract dependency edges from changed files (TIA-ADAPT-019).
///
/// Takes `{"params": {"changed_files": ["path/to/file.clj", ...]}}` and returns
/// edges from test files to source files based on `:require/:use/:import`
/// analysis. Returns flat format matching `StaticDepsResponse`:
/// `candidates`, `edges`, `unresolved`, `symbol_edges` at top level.
///
/// Strategy:
/// 1. Parse changed files to extract their namespace → file mapping.
/// 2. Scan test files (in `test/` or `tests/` dirs), parse their `:require`
///    declarations, and find which changed namespaces they depend on.
/// 3. Return edges from test items → changed source files.
fn cmd_static_deps(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let changed_files = match params["changed_files"].as_array() {
        Some(f) => f,
        None => return json_err("missing 'params.changed_files'"),
    };

    // Phase 1: Parse changed files to get namespace → file mapping.
    let mut changed_ns_to_file: HashMap<String, String> = HashMap::new();
    let mut any_clojure = false;

    for file_val in changed_files {
        let file_path = match file_val.as_str() {
            Some(s) => s,
            None => continue,
        };
        if !file_path.ends_with(".clj")
            && !file_path.ends_with(".cljs")
            && !file_path.ends_with(".cljc")
        {
            continue;
        }
        any_clojure = true;

        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let tree = query::parse(&content);

        let ns_query = query::compile_query(include_str!("../queries/ns.scm"));
        let ns_caps = query::run_query(&ns_query, &tree, content.as_bytes());
        if let Some(ns) = ns_caps
            .iter()
            .find(|c| c.name == "namespace_name")
            .map(|c| c.text.clone())
        {
            changed_ns_to_file.insert(ns, file_path.to_string());
        }
    }

    if !any_clojure {
        return serde_json::json!({
            "ok": true,
            "candidates": [],
            "edges": [],
            "unresolved": [],
            "symbol_edges": []
        });
    }

    // Phase 2: Scan test files only, parse their deps, match against changed
    // namespaces. Build candidates (all test node IDs) simultaneously.
    let mut edges: Vec<serde_json::Value> = Vec::new();
    let mut edge_set: HashSet<(String, String)> = HashSet::new();
    let mut candidates: Vec<String> = Vec::new();
    let mut candidates_set: HashSet<String> = HashSet::new();

    let ns_q = query::compile_query(include_str!("../queries/ns.scm"));
    let deps_q = query::compile_query(include_str!("../queries/deps.scm"));
    let discover_q = query::compile_query(include_str!("../queries/discover.scm"));

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.file_type().is_file()
                && (p.ends_with(".clj") || p.ends_with(".cljs") || p.ends_with(".cljc"))
                && !p.contains("/target/")
                && !p.contains("/.git/")
                && !p.contains("/.flatpak-builder/")
        })
    {
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path);

        // Only scan test files for dependency edges
        if !clean_path.starts_with("test/")
            && !clean_path.starts_with("tests/")
            && !clean_path.contains("/test/")
        {
            continue;
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let tree = query::parse(&content);

        // Get namespace name
        let ns_caps = query::run_query(&ns_q, &tree, content.as_bytes());
        let namespace = ns_caps
            .iter()
            .find(|c| c.name == "namespace_name")
            .map(|c| c.text.clone())
            .unwrap_or_default();

        // Discover test function names (same logic as cmd_discover)
        let discover_caps = query::run_query(&discover_q, &tree, content.as_bytes());
        let test_names: Vec<&str> = discover_caps
            .iter()
            .filter(|c| c.name == "test_name")
            .map(|c| c.text.as_str())
            .collect();

        // Build node_ids matching cmd_discover format
        let test_node_ids: Vec<String> = if test_names.is_empty() {
            // No explicit test names — emit edge from file path as fallback
            vec![clean_path.to_string()]
        } else {
            test_names
                .iter()
                .map(|name| {
                    if namespace.is_empty() {
                        format!("{}::{}(Test)", clean_path.replace('/', "::"), name)
                    } else {
                        format!("{}::{}(Test)", namespace, name)
                    }
                })
                .collect()
        };

        // Collect unique candidates
        for node_id in &test_node_ids {
            if candidates_set.insert(node_id.clone()) {
                candidates.push(node_id.clone());
            }
        }

        // Parse deps and match against changed namespaces
        let dep_caps = query::run_query(&deps_q, &tree, content.as_bytes());
        let test_deps = extract_dep_namespaces_from_caps(&dep_caps, &content);

        for dep_ns in &test_deps {
            if let Some(source_file) = changed_ns_to_file.get(dep_ns) {
                let to = source_file.clone();
                for node_id in &test_node_ids {
                    let edge_key = (node_id.clone(), to.clone());
                    if edge_set.insert(edge_key) {
                        edges.push(serde_json::json!({
                            "from": node_id,
                            "to": to,
                            "weight": 1_000_000,
                            "origin": "static"
                        }));
                    }
                }
            }
        }
    }

    // Compute unresolved: changed files we couldn't find namespace for
    let mut unresolved: Vec<String> = Vec::new();
    for file_val in changed_files {
        let file_path = match file_val.as_str() {
            Some(s) => s,
            None => continue,
        };
        if !changed_ns_to_file.values().any(|v| v == file_path) {
            unresolved.push(file_path.to_string());
        }
    }

    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": []
    })
}

/// Extract dependency namespace names from tree-sitter capture results.
fn extract_dep_namespaces_from_caps(caps: &[query::Capture], _content: &str) -> Vec<String> {
    let mut namespaces = Vec::new();

    for cap in caps.iter().filter(|c| c.name == "dep_entry") {
        let ns = query::extract_namespace_from_dep_entry(&cap.text);
        if !ns.is_empty() {
            namespaces.push(ns);
        }
    }
    namespaces
}

/// Discover: enumerate tests by scanning .clj files and running the discover
/// query to find deftest/deftest- forms (TIA-ADAPT-018).
fn cmd_discover() -> serde_json::Value {
    let mut tests: Vec<serde_json::Value> = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.file_type().is_file()
                && (p.ends_with(".clj") || p.ends_with(".cljs") || p.ends_with(".cljc"))
                && !p.contains("/target/")
                && !p.contains("/.git/")
                && !p.contains("/.flatpak-builder/")
        })
    {
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path);

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let tree = query::parse(&content);
        let discover_q = query::compile_query(include_str!("../queries/discover.scm"));
        let caps = query::run_query(&discover_q, &tree, content.as_bytes());

        // Get the namespace from the file (using ns query)
        let ns_query = query::compile_query(include_str!("../queries/ns.scm"));
        let ns_caps = query::run_query(&ns_query, &tree, content.as_bytes());
        let namespace = ns_caps
            .iter()
            .find(|c| c.name == "namespace_name")
            .map(|c| c.text.clone())
            .unwrap_or_default();

        // For each test_name capture, create a test item
        let test_names: Vec<&str> = caps
            .iter()
            .filter(|c| c.name == "test_name")
            .map(|c| c.text.as_str())
            .collect();

        for test_name in test_names {
            let node_id = if namespace.is_empty() {
                format!("{}::{}(Test)", clean_path.replace('/', "::"), test_name)
            } else {
                format!("{}::{}(Test)", namespace, test_name)
            };

            tests.push(serde_json::json!({
                "node_id": node_id,
                "suite_kind": "unit",
                "file": clean_path,
            }));
        }
    }

    json_ok(serde_json::json!(tests))
}

/// Fingerprint: blake3 hash of file contents (TIA-ADAPT-002).
///
/// Standard protocol: `{"command":"fingerprint","params":{"files":[...]}}`.
/// Returns array of `{"file": path, "fingerprint": hex_hash}`.
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
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => return json_err(&format!("cannot read {}: {}", file, e)),
        };
        let hash = blake3::hash(content.as_bytes());
        fingerprints.push(serde_json::json!({
            "file": file,
            "fingerprint": hash.to_hex().to_string()
        }));
    }

    json_ok(serde_json::json!({
        "fingerprints": fingerprints
    }))
}

/// Run-args: build CLI args for the test runner (TIA-ADAPT-002).
///
/// Standard protocol: `{"command":"run-args","params":{"selected":[...]}}`.
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

    // Use project config to determine runner
    let config = project::ProjectConfig::detect(std::path::Path::new("."));

    // Extract test names from node_ids (format: <namespace>::<test-name>(Test))
    let mut test_names: Vec<String> = Vec::new();
    for node_id in &selected {
        // Format: my-project.core-test::test-greet(Test) → test-greet
        if let Some(test_part) = node_id.split("::").nth(1) {
            let name = test_part.trim_end_matches("(Test)");
            test_names.push(name.to_string());
        } else {
            // Fallback: treat node_id as namespace
            test_names.push(node_id.to_string());
        }
    }

    let runner = config
        .as_ref()
        .map(|c| &c.runner)
        .unwrap_or(&project::TestRunner::Default);

    let args: Vec<String> = match runner {
        project::TestRunner::Leiningen => {
            // lein test :only namespace/test-name
            let mut args = vec!["test".to_string(), ":only".to_string()];
            for name in &test_names {
                args.push(format!("{}/{}", name, name)); // simplified
            }
            args
        }
        project::TestRunner::Kaocha => {
            // clojure -M:test --focus namespace
            let mut args = vec!["-M:test".to_string(), "--focus".to_string()];
            for name in &test_names {
                args.push(name.clone());
            }
            args
        }
        project::TestRunner::Cognitect | project::TestRunner::Default => {
            // clojure -M:test -n namespace
            let mut args = vec!["-M:test".to_string(), "-n".to_string()];
            for name in &test_names {
                args.push(name.clone());
            }
            args
        }
    };

    json_ok(serde_json::json!({
        "command": "clojure",
        "args": args,
        "env": {}
    }))
}

/// Ingest: parse JUnit XML output from Cognitect runner or Leiningen stdout
/// and return runtime edges and per-test results (TIA-ADAPT-002).
/// Ingest: parse test runner output and return runtime edges + per-test results.
///
/// Standard protocol: `{"command":"ingest","params":{"run_output":"..."}}`.
fn cmd_ingest(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];

    // Try run_output as a string (newline-delimited JSON lines)
    if let Some(run_output) = params["run_output"].as_str() {
        if run_output.is_empty() {
            return json_ok(serde_json::json!({
                "per_test_results": [],
                "runtime_edges": [],
                "external_inputs": []
            }));
        }

        // Parse each line as a JSON test result
        let mut results = Vec::new();
        for line in run_output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<serde_json::Value>(trimmed) {
                let test_id = entry["test_id"].as_str().unwrap_or("").to_string();
                let outcome = entry["outcome"].as_str().unwrap_or("passed").to_string();
                let duration_ms = entry["duration_ms"].as_i64().unwrap_or(0);
                results.push(serde_json::json!({
                    "test_id": test_id,
                    "outcome": outcome,
                    "duration_ms": duration_ms
                }));
            }
        }

        return json_ok(serde_json::json!({
            "per_test_results": results,
            "runtime_edges": [],
            "external_inputs": []
        }));
    }

    // Fallback: try JUnit XML from target/test-results.xml
    let collection_path = params["collection_path"]
        .as_str()
        .unwrap_or("target/test-results.xml");
    match std::fs::read_to_string(collection_path) {
        Ok(content) => parse_junit_xml(&content),
        Err(_) => json_ok(serde_json::json!({
            "per_test_results": [],
            "runtime_edges": [],
            "external_inputs": []
        })),
    }
}

/// Parse JUnit XML content and return runtime edges and per-test results.
fn parse_junit_xml(content: &str) -> serde_json::Value {
    let edges: Vec<serde_json::Value> = Vec::new();
    let mut results: Vec<serde_json::Value> = Vec::new();

    // Simple line-based JUnit XML parser
    let mut in_testcase = false;
    let mut test_name = String::new();
    let mut class_name = String::new();
    let mut test_time = 0.0f64;
    let mut passed = true;
    let mut failure_message = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("<testcase") {
            in_testcase = true;
            test_name = extract_attr(trimmed, "name").unwrap_or_default();
            class_name = extract_attr(trimmed, "classname").unwrap_or_default();
            test_time = extract_attr(trimmed, "time")
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(0.0);
            passed = true;
            failure_message.clear();
        } else if trimmed.starts_with("<failure") {
            passed = false;
            failure_message = extract_attr(trimmed, "message").unwrap_or_default();
        } else if trimmed.starts_with("</testcase>") && in_testcase {
            let node_id = if class_name.is_empty() {
                test_name.clone()
            } else {
                format!("{}::{}", class_name, test_name)
            };

            results.push(serde_json::json!({
                "test_id": node_id,
                "passed": passed,
                "time": test_time,
                "failure_message": if passed { "" } else { &failure_message }
            }));

            in_testcase = false;
        } else if trimmed.starts_with("<testsuite") {
            // Extract suite attributes if needed
        }
    }

    json_ok(serde_json::json!({
        "edges": edges,
        "results": results
    }))
}

/// Extract an XML attribute value by name from a tag string.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let search = format!("{}=\"", attr);
    let start = tag.find(&search)? + search.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}
