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
/// Takes `{"files": ["path/to/file.clj", ...]}` and returns edges from
/// test files to source files based on `:require/:use/:import` analysis.
fn cmd_static_deps(cmd: &serde_json::Value) -> serde_json::Value {
    let files = match cmd["args"]["files"].as_array() {
        Some(f) => f,
        None => return json_err("missing args.files"),
    };

    // Phase 1: Parse every changed file to get its namespace name and deps.
    let mut changed_namespaces: HashMap<String, String> = HashMap::new(); // ns → file
    let mut any_clojure = false;

    for file_val in files {
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
            Err(_) => continue, // skip unreadable files
        };
        let tree = query::parse(&content);

        // Get the namespace name
        let ns_query = query::compile_query(include_str!("../queries/ns.scm"));
        let ns_caps = query::run_query(&ns_query, &tree, content.as_bytes());
        let namespace = ns_caps
            .iter()
            .find(|c| c.name == "namespace_name")
            .map(|c| c.text.clone());

        if let Some(ref ns) = &namespace {
            changed_namespaces.insert(ns.clone(), file_path.to_string());
            // Extract deps from this file
            let deps_query = query::compile_query(include_str!("../queries/deps.scm"));
            let dep_caps = query::run_query(&deps_query, &tree, content.as_bytes());
            extract_dep_namespaces_from_caps(&dep_caps, &content);
        }
    }

    if !any_clojure {
        return json_ok(serde_json::json!({"edges": []}));
    }

    // Phase 2: Scan the project for test files and parse their deps.
    // For each test file, check if its deps include any changed namespace.
    let mut edges: Vec<serde_json::Value> = Vec::new();
    let mut edge_set: HashSet<(String, String)> = HashSet::new();

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
                && !p.ends_with("/.clj")
        })
    {
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path);

        if files.iter().any(|f| f.as_str() == Some(clean_path)) {
            continue; // skip changed files themselves
        }

        let content = match std::fs::read_to_string(entry.path()) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let tree = query::parse(&content);
        let deps_query = query::compile_query(include_str!("../queries/deps.scm"));
        let dep_caps = query::run_query(&deps_query, &tree, content.as_bytes());
        let test_deps = extract_dep_namespaces_from_caps(&dep_caps, &content);

        for dep_ns in &test_deps {
            if let Some(source_file) = changed_namespaces.get(dep_ns) {
                let from = clean_path.to_string();
                let to = source_file.clone();
                let edge_key = (from.clone(), to.clone());
                if edge_set.insert(edge_key) {
                    edges.push(serde_json::json!({
                        "from": from,
                        "to": to,
                        "weight": 1_000_000,
                        "origin": "static"
                    }));
                }
            }
        }
    }

    json_ok(serde_json::json!({"edges": edges}))
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
fn cmd_fingerprint(cmd: &serde_json::Value) -> serde_json::Value {
    let file_path = match cmd["args"]["file"].as_str() {
        Some(f) => f,
        None => return json_err("missing args.file"),
    };

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return json_err(&format!("cannot read {}: {}", file_path, e)),
    };

    let hash = blake3::hash(content.as_bytes());
    json_ok(serde_json::json!({
        "fingerprints": {
            file_path: hash.to_hex().to_string()
        }
    }))
}

/// Run-args: build CLI args for the test runner (TIA-ADAPT-002).
fn cmd_run_args(cmd: &serde_json::Value) -> serde_json::Value {
    let files = match cmd["args"]["files"].as_array() {
        Some(f) => f,
        None => return json_err("missing args.files"),
    };

    // Use project config to determine runner
    let config = project::ProjectConfig::detect(std::path::Path::new("."));

    // Extract test names from node_ids (format: <namespace>::<test-name>(Test))
    let mut test_names: Vec<String> = Vec::new();
    for file_val in files {
        let node_id = file_val.as_str().unwrap_or("");
        // Format: my-project.core-test::test-greet(Test) → test-greet
        if let Some(test_part) = node_id.split("::").nth(1) {
            let name = test_part.trim_end_matches("(Test)");
            test_names.push(name.to_string());
        } else {
            // Fallback: treat file as namespace
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
fn cmd_ingest(cmd: &serde_json::Value) -> serde_json::Value {
    let collection_path = match cmd["args"]["collection_path"].as_str() {
        Some(path) => path.to_string(),
        None => "target/test-results.xml".to_string(),
    };

    let content = match std::fs::read_to_string(&collection_path) {
        Ok(c) => c,
        Err(_) => {
            // If no JUnit XML, try Leiningen-style stdout from the args
            let stdout = cmd["args"]["stdout"].as_str().unwrap_or("");
            if stdout.is_empty() {
                return json_ok(serde_json::json!({
                    "edges": [],
                    "results": []
                }));
            }
            return parse_lein_stdout(stdout);
        }
    };

    parse_junit_xml(&content)
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

/// Parse Leiningen-style test output from stdout.
fn parse_lein_stdout(stdout: &str) -> serde_json::Value {
    let mut results: Vec<serde_json::Value> = Vec::new();
    let edges: Vec<serde_json::Value> = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        // Leiningen output: "ERROR in test-name (namespace.clj:42)"
        if let Some(rest) = trimmed.strip_prefix("ERROR in ") {
            #[allow(clippy::manual_pattern_char_comparison)]
            let parts: Vec<&str> = rest
                .splitn(3, |c| c == '(' || c == ')' || c == ':')
                .collect();
            if parts.len() >= 2 {
                let test_name = parts[0].trim().to_string();
                results.push(serde_json::json!({
                    "test_id": test_name,
                    "passed": false,
                    "time": 0.0,
                    "failure_message": ""
                }));
            }
        }
        // "FAIL in test-name (namespace.clj:42)"
        else if let Some(rest) = trimmed.strip_prefix("FAIL in ") {
            #[allow(clippy::manual_pattern_char_comparison)]
            let parts: Vec<&str> = rest
                .splitn(3, |c| c == '(' || c == ')' || c == ':')
                .collect();
            if parts.len() >= 2 {
                let test_name = parts[0].trim().to_string();
                results.push(serde_json::json!({
                    "test_id": test_name,
                    "passed": false,
                    "time": 0.0,
                    "failure_message": ""
                }));
            }
        }
        // "OK test-name"
        else if let Some(rest) = trimmed.strip_prefix("OK ") {
            results.push(serde_json::json!({
                "test_id": rest.trim().to_string(),
                "passed": true,
                "time": 0.0,
                "failure_message": ""
            }));
        }
    }

    json_ok(serde_json::json!({
        "edges": edges,
        "results": results
    }))
}
