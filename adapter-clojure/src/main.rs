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
        "version": env!("CARGO_PKG_VERSION"),
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
    let changed_ns_to_file = collect_changed_namespaces(changed_files);
    if changed_ns_to_file.is_empty() {
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
        // Hidden dirs are agent/editor worktrees, not project source (testaruda-n338/zlyk)
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.file_type().is_file()
                && is_clojure_source(&p)
                && !p.contains("/target/")
                && !p.contains("/.git/")
                && !p.contains("/.flatpak-builder/")
        })
    {
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path);
        if !is_test_file_path(clean_path) {
            continue;
        }
        process_test_file(
            entry.path(),
            clean_path,
            &ns_q,
            &deps_q,
            &discover_q,
            &changed_ns_to_file,
            &mut candidates,
            &mut candidates_set,
            &mut edges,
            &mut edge_set,
        );
    }

    let unresolved = unresolved_changed_files(changed_files, &changed_ns_to_file);

    serde_json::json!({
        "ok": true,
        "candidates": candidates,
        "edges": edges,
        "unresolved": unresolved,
        "symbol_edges": []
    })
}

/// Phase 1: map each changed clojure file's namespace to its path. Empty
/// when no changed file is a clojure-family source.
fn collect_changed_namespaces(changed_files: &[serde_json::Value]) -> HashMap<String, String> {
    let mut changed_ns_to_file: HashMap<String, String> = HashMap::new();
    for file_val in changed_files {
        let file_path = match file_val.as_str() {
            Some(s) => s,
            None => continue,
        };
        if !is_clojure_source(file_path) {
            continue;
        }
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let tree = query::parse(&content);

        let ns_query = query::compile_query(include_str!("../queries/ns.scm"));
        let ns_caps = query::run_query(&ns_query, &tree, content.as_bytes());
        let ns = ns_caps
            .iter()
            .find(|c| c.name == "namespace_name")
            .map(|c| c.text.clone())
            .unwrap_or_else(|| infer_namespace_from_path(file_path));
        changed_ns_to_file.insert(ns, file_path.to_string());
    }
    changed_ns_to_file
}

/// Only test files carry dependency edges to source.
fn is_test_file_path(clean_path: &str) -> bool {
    clean_path.starts_with("test/")
        || clean_path.starts_with("tests/")
        || clean_path.contains("/test/")
}

/// Phase 2 per-file work: discover test node_ids (candidates), parse the
/// file's deps and emit edges to changed source files.
#[allow(clippy::too_many_arguments)]
fn process_test_file(
    entry_path: &std::path::Path,
    clean_path: &str,
    ns_q: &tree_sitter::Query,
    deps_q: &tree_sitter::Query,
    discover_q: &tree_sitter::Query,
    changed_ns_to_file: &HashMap<String, String>,
    candidates: &mut Vec<String>,
    candidates_set: &mut HashSet<String>,
    edges: &mut Vec<serde_json::Value>,
    edge_set: &mut HashSet<(String, String)>,
) {
    let content = match std::fs::read_to_string(entry_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let tree = query::parse(&content);

    // Get namespace name
    let ns_caps = query::run_query(ns_q, &tree, content.as_bytes());
    let namespace = ns_caps
        .iter()
        .find(|c| c.name == "namespace_name")
        .map(|c| c.text.clone())
        .unwrap_or_default();

    // Discover test function names (same logic as cmd_discover)
    let discover_caps = query::run_query(discover_q, &tree, content.as_bytes());
    let test_names: Vec<&str> = discover_caps
        .iter()
        .filter(|c| c.name == "test_name")
        .map(|c| c.text.as_str())
        .collect();

    let test_node_ids = build_test_node_ids(&namespace, &test_names, clean_path);

    // Collect unique candidates
    for node_id in &test_node_ids {
        if candidates_set.insert(node_id.clone()) {
            candidates.push(node_id.clone());
        }
    }

    // Parse deps and match against changed namespaces
    let dep_caps = query::run_query(deps_q, &tree, content.as_bytes());
    let test_deps = extract_dep_namespaces_from_caps(&dep_caps, &content);
    push_dep_edges(
        &test_deps,
        &test_node_ids,
        changed_ns_to_file,
        edges,
        edge_set,
    );
}

/// Build node_ids matching cmd_discover format. Without explicit test
/// names, emit an edge from the file path as fallback.
fn build_test_node_ids(namespace: &str, test_names: &[&str], clean_path: &str) -> Vec<String> {
    if test_names.is_empty() {
        return vec![clean_path.to_string()];
    }
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
}

/// Emit static edges for test deps that hit a changed source namespace.
fn push_dep_edges(
    test_deps: &[String],
    test_node_ids: &[String],
    changed_ns_to_file: &HashMap<String, String>,
    edges: &mut Vec<serde_json::Value>,
    edge_set: &mut HashSet<(String, String)>,
) {
    for dep_ns in test_deps {
        let Some(to) = changed_ns_to_file.get(dep_ns) else {
            continue;
        };
        for node_id in test_node_ids {
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

/// Changed files we couldn't find a namespace for.
fn unresolved_changed_files(
    changed_files: &[serde_json::Value],
    changed_ns_to_file: &HashMap<String, String>,
) -> Vec<String> {
    let mut unresolved = Vec::new();
    for file_val in changed_files {
        let file_path = match file_val.as_str() {
            Some(s) => s,
            None => continue,
        };
        if !changed_ns_to_file.values().any(|v| v == file_path) {
            unresolved.push(file_path.to_string());
        }
    }
    unresolved
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

/// Infer a namespace from a source path using the Clojure/babashka file
/// convention: the source-root prefix (`src/`, `test/`, `lib/`) is dropped,
/// remaining segments become ns segments, underscores become dashes
/// (`src/finanzas/my_ns.bb` → `finanzas.my-ns`). Fallback for ns-less
/// babashka-script-style files (t79n).
fn infer_namespace_from_path(path: &str) -> String {
    let cleaned = path
        .strip_prefix("./")
        .unwrap_or(path)
        .trim_start_matches("src/")
        .trim_start_matches("test/")
        .trim_start_matches("lib/");
    let stem = cleaned
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(cleaned);
    stem.replace('_', "-").replace('/', ".")
}

/// Clojure-family source extensions. Includes babashka `.bb` (gh-34): the
/// discover/static-deps requests carry no extension config, so the adapter
/// self-detects the full dialect family.
fn is_clojure_source(path: &str) -> bool {
    path.ends_with(".clj")
        || path.ends_with(".cljs")
        || path.ends_with(".cljc")
        || path.ends_with(".bb")
}

/// Discover: enumerate tests by scanning .clj files and running the discover
/// query to find deftest/deftest- forms (TIA-ADAPT-018).
fn cmd_discover() -> serde_json::Value {
    let mut tests: Vec<serde_json::Value> = Vec::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        // Hidden dirs are agent/editor worktrees, not project source (testaruda-n338/zlyk)
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            e.file_type().is_file()
                && is_clojure_source(&p)
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
/// Parse a node_id into `(namespace, test_name)`.
///
/// Format: `<namespace>::<test-name>(Test)` — ns-less files use the
/// path-derived namespace (see [`infer_namespace_from_path`]), so the `::`
/// separator is always present; the fallback treats the whole id as the
/// namespace with no test name.
fn parse_node_id(node_id: &str) -> (String, String) {
    let body = node_id.strip_suffix("(Test)").unwrap_or(node_id);
    if body.matches("::").count() > 1 {
        // ns-less script-file id (t79n): the file path is `::`-joined and
        // the test name is the final segment, e.g.
        // `test::finanzas::cli_test.bb::test-foo(Test)` → path
        // `test/finanzas/cli_test.bb`, name `test-foo`.
        let (path, name) = body.rsplit_once("::").unwrap_or((body, ""));
        return (path.replace("::", "/"), name.to_string());
    }
    match body.split_once("::") {
        Some((ns, name)) => (ns.to_string(), name.to_string()),
        None => (body.to_string(), String::new()),
    }
}

/// True when the node_id's namespace segment is a file path encoded with
/// `::` separators — the ns-less script-file fallback (t79n), e.g.
/// `test::finanzas::cli_test.bb`. Real namespaces use dots and never end
/// with a clojure-family file extension.
fn is_source_file_path(ns: &str) -> bool {
    ns.ends_with(".bb") || ns.ends_with(".clj") || ns.ends_with(".cljs") || ns.ends_with(".cljc")
}

/// Build the run-args response (TIA-ADAPT-007, gh-81).
///
/// The engine (src/adapter.rs `RunArgsResult`) expects `{"runner_args":
/// [...], "collection_path": ...}` — the legacy `{"command", "args",
/// "env"}` shape fails deserialization with "missing field `runner_args`".
///
/// `collection_path` is `""` for every clojure-family runner: Cognitect/
/// Leiningen/babashka.test-runner report to stdout (and the engine falls
/// back to combined stdout/stderr for ingest); Kaocha's junit-xml plugin
/// is opt-in, so nothing is declared unconditionally.
fn build_run_args_response(
    config: &project::ProjectConfig,
    selected: &[String],
) -> serde_json::Value {
    let (namespaces, testable_ids) = parse_selection(selected);

    let args: Vec<String> = match &config.runner {
        project::TestRunner::Leiningen => {
            let mut args = vec!["lein".to_string(), "test".to_string(), ":only".to_string()];
            args.extend(testable_ids);
            args
        }
        project::TestRunner::Kaocha => {
            let mut args = vec![
                "clojure".to_string(),
                "-M:test".to_string(),
                "--focus".to_string(),
            ];
            args.extend(testable_ids);
            args
        }
        project::TestRunner::Babashka => build_babashka_args(config, &namespaces),
        project::TestRunner::Cognitect | project::TestRunner::Default => {
            let mut args = vec![
                "clojure".to_string(),
                "-M:test".to_string(),
                "-n".to_string(),
            ];
            args.extend(namespaces);
            args
        }
    };

    json_ok(serde_json::json!({
        "runner_args": args,
        "collection_path": ""
    }))
}

/// Split selected node_ids into deduped namespaces for `-n`-style
/// selectors and full testable ids for `:only`/`--focus` selectors.
fn parse_selection(selected: &[String]) -> (Vec<String>, Vec<String>) {
    let mut namespaces: Vec<String> = Vec::new();
    let mut testable_ids: Vec<String> = Vec::new();
    for node_id in selected {
        let (ns, name) = parse_node_id(node_id);
        if !namespaces.contains(&ns) {
            namespaces.push(ns.clone());
        }
        testable_ids.push(format!("{}/{}", ns, name));
    }
    (namespaces, testable_ids)
}

/// Build runner args for a babashka project.
///
/// Namespace-style ids (`finanzas.cli-test`) and script-file ids
/// (`test::finanzas::cli_test.bb::name`) need different runners: bb test
/// namespaces load via require; ns-less .bb scripts are self-running
/// executables that call (System/exit) themselves, so they must run as
/// `bb <file>` subprocesses.
fn build_babashka_args(config: &project::ProjectConfig, namespaces: &[String]) -> Vec<String> {
    if let Some(args) = babashka_test_runner_args(config, namespaces) {
        return args;
    }

    let mut script_files: Vec<String> = Vec::new();
    let mut ns_only: Vec<String> = Vec::new();
    for ns in namespaces {
        if is_source_file_path(ns) {
            if !script_files.contains(ns) {
                script_files.push(ns.clone());
            }
        } else if !ns_only.contains(ns) {
            ns_only.push(ns.clone());
        }
    }

    let mut script = String::new();
    let mut needs_classpath = false;
    if !ns_only.is_empty() {
        // Inline clojure.test runner: bb has clojure.test built in, so a
        // classpath of test roots + source roots plus an -e script runs
        // the selected namespaces without any project-side test deps.
        needs_classpath = true;
        script.push_str(&clojure_test_script(&ns_only));
    }
    if !script_files.is_empty() {
        script.push_str(&babashka_script_file_shell(&script_files));
    }

    let mut args = vec!["bb".to_string()];
    if needs_classpath {
        args.push("--classpath".to_string());
        args.push(babashka_classpath(config));
    }
    args.push("-e".to_string());
    args.push(script);
    args
}

/// `bb --main babashka.test-runner` args when bb.edn declares the dep —
/// returns None otherwise (the runner is NOT bundled with bb).
fn babashka_test_runner_args(
    config: &project::ProjectConfig,
    namespaces: &[String],
) -> Option<Vec<String>> {
    if !config.declares_test_runner || namespaces.is_empty() {
        return None;
    }
    let mut args = vec![
        "bb".to_string(),
        "--main".to_string(),
        "babashka.test-runner".to_string(),
        "-n".to_string(),
    ];
    args.extend(namespaces.to_vec());
    Some(args)
}

/// Inline clojure.test script: require the namespaces, run their tests.
fn clojure_test_script(namespaces: &[String]) -> String {
    let requires: Vec<String> = namespaces.iter().map(|ns| format!("'{ns}")).collect();
    format!(
        "(require {}) (clojure.test/run-tests {})",
        requires.join(" "),
        requires.join(" ")
    )
}

/// Script-file shell: run each ns-less .bb script as a `bb <file>` child
/// process with inherited output, failing when any child fails.
fn babashka_script_file_shell(script_files: &[String]) -> String {
    // Clojure map literals contain braces, so assemble the script with
    // push_str-style literals instead of format! placeholders.
    let mut script = String::new();
    let quoted_files: Vec<String> = script_files.iter().map(|p| format!("\"{p}\"")).collect();
    script.push_str("(require '[babashka.process :as p]) ");
    script.push_str("(let [fail (atom false)] ");
    script.push_str(&format!("(doseq [f [{}] ] ", quoted_files.join(" ")));
    // (when-not (zero? (:exit (p/shell ...))) (reset! ...))
    script.push_str(concat!(
        "(println \"\u{25b8} bb\" f) ",
        "(when-not (zero? (:exit ",
        "(p/shell {:continue true :out :inherit :err :inherit} \"bb\" f))) ",
        "(reset! fail true)) ",
        "(when @fail (System/exit 1)))"
    ));
    script.push(')');
    script
}

/// Classpath for bb `-e` runs: test roots first, then source roots.
fn babashka_classpath(config: &project::ProjectConfig) -> String {
    let mut classpath: Vec<&String> = config.test_paths.iter().collect();
    for p in &config.source_paths {
        if !classpath.contains(&p) {
            classpath.push(p);
        }
    }
    classpath
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(":")
}

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

    let config = project::ProjectConfig::detect(std::path::Path::new("."))
        .unwrap_or_else(project::ProjectConfig::default);

    build_run_args_response(&config, &selected)
}

/// Ingest: parse test runner output and return per-test results.
///
/// The engine always feeds ingest a single string (TIA-ADAPT-008):
/// either the freshly-rewritten `collection_path` file or the runner's
/// combined stdout/stderr. This adapter accepts, in order:
///
/// 1. JUnit XML (a runner configured with a junit reporter),
/// 2. newline-delimited JSON test results (legacy ingest convention),
/// 3. plain clojure.test text output (`FAIL in (name) (file:line)`) —
///    the actual output of Cognitect/Leiningen/babashka.test-runner runs.
///
/// Pass-only text names no individual tests, so it yields no results; the
/// engine keeps un-ingested items always_run, which is recall-safe
/// (SAFE-007). Failures ARE individually attributable and keep warm
/// selection effective.
fn cmd_ingest(cmd: &serde_json::Value) -> serde_json::Value {
    let params = &cmd["params"];
    let run_output = params["run_output"].as_str().unwrap_or("");
    if run_output.trim().is_empty() {
        return json_ok(serde_json::json!({
            "per_test_results": [],
            "runtime_edges": [],
            "external_inputs": []
        }));
    }

    // 1. JUnit XML — detectable by leading `<`
    if run_output.trim_start().starts_with('<') {
        return json_ok(parse_junit_xml(run_output));
    }

    // 2. Newline-delimited JSON test results
    let mut jsonl: Vec<serde_json::Value> = Vec::new();
    for line in run_output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<serde_json::Value>(trimmed) {
            let test_id = entry["test_id"].as_str().unwrap_or("").to_string();
            let outcome = entry["outcome"].as_str().unwrap_or("passed").to_string();
            let duration_ms = entry["duration_ms"].as_i64().unwrap_or(0);
            jsonl.push(serde_json::json!({
                "test_id": test_id,
                "outcome": outcome,
                "duration_ms": duration_ms
            }));
        }
    }
    if !jsonl.is_empty() {
        return json_ok(serde_json::json!({
            "per_test_results": jsonl,
            "runtime_edges": [],
            "external_inputs": []
        }));
    }

    // 3. clojure.test text output
    json_ok(parse_clojure_test_output(run_output))
}

/// Parse plain clojure.test output into per-test results.
///
/// Recognizes `FAIL in (name) (path:line)` / `ERROR in (name)` blocks and
/// attributes each failure to `namespace::name(Test)` using the
/// path-derived namespace convention (`infer_namespace_from_path`). Only
/// failures are emitted — passes are not individually named by clojure.test.
fn parse_clojure_test_output(content: &str) -> serde_json::Value {
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut current: Option<(String, String, Vec<String>)> = None; // (test_id, outcome, detail lines)

    let flush = |current: &mut Option<(String, String, Vec<String>)>,
                 results: &mut Vec<serde_json::Value>| {
        if let Some((test_id, outcome, detail)) = current.take() {
            results.push(serde_json::json!({
                "test_id": test_id,
                "outcome": outcome,
                "duration_ms": 0,
                "error_text": detail.join("\n")
            }));
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("FAIL in (")
            .or_else(|| trimmed.strip_prefix("ERROR in ("))
        {
            // Both FAIL and ERROR are failures for the engine: anything
            // other than outcome "failed" counts as a pass (src/store.rs
            // run-history), so an ERROR reported as "error" would be
            // unsound — map both to "failed".
            let outcome = "failed";
            flush(&mut current, &mut results);
            let name = rest.split(")").next().unwrap_or("");
            // Optional location: `(path/to/file.clj:42)` after the name paren
            let file = trimmed
                .rsplit_once(") (")
                .and_then(|(_, loc)| loc.strip_suffix(")"))
                .and_then(|loc| loc.rsplit_once(':').map(|(f, _)| f.to_string()));
            let test_id = match file {
                Some(f) => format!("{}::{}(Test)", infer_namespace_from_path(&f), name),
                None => name.to_string(),
            };
            current = Some((test_id, outcome.to_string(), Vec::new()));
        } else if trimmed.starts_with("Ran ")
            || trimmed.starts_with("Testing ")
            || (trimmed.is_empty() && current.is_none())
        {
            // summary / section-header lines — stop collecting detail
            if trimmed.starts_with("Ran ") || trimmed.starts_with("Testing ") {
                flush(&mut current, &mut results);
            }
            continue;
        } else if current.is_some() && !trimmed.is_empty() {
            if let Some((_, _, detail)) = current.as_mut() {
                detail.push(line.to_string());
            }
        }
    }
    flush(&mut current, &mut results);

    serde_json::json!({
        "per_test_results": results,
        "runtime_edges": [],
        "external_inputs": []
    })
}

/// Parse JUnit XML content and return per-test results.
///
/// Response shape follows the engine's `IngestResult`/`TestRunResult`
/// (TIA-ADAPT-008): `per_test_results` entries carry `test_id`, `outcome`,
/// `duration_ms`, `error_text` — the previous `edges`/`results`/`passed`
/// keys were never read by the engine.
fn parse_junit_xml(content: &str) -> serde_json::Value {
    let results = parse_junit_cases(content);

    serde_json::json!({
        "per_test_results": results,
        "runtime_edges": [],
        "external_inputs": []
    })
}

/// Accumulated state of the `<testcase>` currently being parsed.
struct JunitCase {
    test_name: String,
    class_name: String,
    test_time: f64,
    passed: bool,
    failure_message: String,
}

/// Line-based JUnit XML parser: collect one result per `<testcase>`.
fn parse_junit_cases(content: &str) -> Vec<serde_json::Value> {
    let mut results: Vec<serde_json::Value> = Vec::new();
    let mut case = JunitCase {
        test_name: String::new(),
        class_name: String::new(),
        test_time: 0.0,
        passed: true,
        failure_message: String::new(),
    };
    let mut in_testcase = false;
    let mut in_failure = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("<testcase") {
            in_testcase = true;
            in_failure = false;
            case = JunitCase {
                test_name: extract_attr(trimmed, "name").unwrap_or_default(),
                class_name: extract_attr(trimmed, "classname").unwrap_or_default(),
                test_time: extract_attr(trimmed, "time")
                    .and_then(|t| t.parse::<f64>().ok())
                    .unwrap_or(0.0),
                passed: true,
                failure_message: String::new(),
            };
        } else if trimmed.starts_with("<failure") {
            in_failure = junit_failure_open(trimmed, &mut case);
        } else if trimmed.starts_with("</failure") {
            in_failure = false;
        } else if in_failure {
            junit_failure_body(trimmed, &mut case.failure_message);
        } else if trimmed.starts_with("</testcase>") && in_testcase {
            emit_junit_result(&case, &mut results);
            in_testcase = false;
        }
        // `<testsuite>` attributes are not needed per-test
    }
    results
}

/// Handle the `<failure ...>` open tag: capture the message attribute and
/// any same-line body (`<failure msg="...">boom</failure>`). Returns
/// whether the failure body continues on following lines.
fn junit_failure_open(trimmed: &str, case: &mut JunitCase) -> bool {
    case.passed = false;
    case.failure_message = extract_attr(trimmed, "message").unwrap_or_default();
    let same_line_body = trimmed.ends_with("</failure>");
    if let Some(start) = trimmed.find('>') {
        if start + 1 < trimmed.len() {
            let body = trimmed[start + 1..].trim_end_matches("</failure>");
            junit_failure_body(body.trim(), &mut case.failure_message);
        }
    }
    !same_line_body
}

/// Append failure body text (may be multi-line CDATA/plain text).
fn junit_failure_body(body: &str, failure_message: &mut String) {
    if body.is_empty() {
        return;
    }
    if !failure_message.is_empty() {
        failure_message.push('\n');
    }
    failure_message.push_str(body);
}

/// Emit one finished `<testcase>` as an engine-shaped TestRunResult.
fn emit_junit_result(case: &JunitCase, results: &mut Vec<serde_json::Value>) {
    let node_id = if case.class_name.is_empty() {
        case.test_name.clone()
    } else {
        format!("{}::{}", case.class_name, case.test_name)
    };
    results.push(serde_json::json!({
        "test_id": node_id,
        "outcome": if case.passed { "passed" } else { "failed" },
        "duration_ms": (case.test_time * 1000.0).round() as u64,
        "error_text": if case.passed { "" } else { &case.failure_message }
    }));
}

/// Extract an XML attribute value by name from a tag string.
fn extract_attr(tag: &str, attr: &str) -> Option<String> {
    let search = format!("{}=\"", attr);
    let start = tag.find(&search)? + search.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── run-args contract (gh-81, TIA-ADAPT-007) ─────────────────────────────

    fn runner_config(runner: project::TestRunner) -> project::ProjectConfig {
        project::ProjectConfig {
            runner,
            ..project::ProjectConfig::default()
        }
    }

    #[test]
    fn run_args_new_contract_shape() {
        let resp = build_run_args_response(
            &runner_config(project::TestRunner::Cognitect),
            &["finanzas.core-test::test-greet(Test)".to_string()],
        );
        let result = &resp["result"];
        assert!(
            result["runner_args"].is_array(),
            "runner_args missing: {resp}"
        );
        assert!(
            result["collection_path"].is_string(),
            "collection_path missing: {resp}"
        );
        assert_eq!(
            result["runner_args"],
            serde_json::json!(["clojure", "-M:test", "-n", "finanzas.core-test"])
        );
        assert_eq!(result["collection_path"], serde_json::json!(""));
    }

    #[test]
    fn run_args_babashka_declared_test_runner() {
        let config = project::ProjectConfig {
            runner: project::TestRunner::Babashka,
            declares_test_runner: true,
            ..project::ProjectConfig::default()
        };
        let resp =
            build_run_args_response(&config, &["finanzas.cli-test::test-main(Test)".to_string()]);
        let result = &resp["result"];
        assert_eq!(
            result["runner_args"],
            serde_json::json!([
                "bb",
                "--main",
                "babashka.test-runner",
                "-n",
                "finanzas.cli-test"
            ])
        );
    }

    #[test]
    fn run_args_babashka_script_files_run_per_file() {
        // ns-less .bb script files (t79n) are self-running executables that
        // call (System/exit) — they must run as `bb <file>` subprocesses,
        // not be require'd into a shared namespace.
        let config = runner_config(project::TestRunner::Babashka);
        let resp = build_run_args_response(
            &config,
            &[
                "test::finanzas::cli_test.bb::help-lists-all-subcommands(Test)".to_string(),
                "test::finanzas::cli_test.bb::unknown-subcommand-exits-2(Test)".to_string(),
            ],
        );
        let result = &resp["result"];
        let args = result["runner_args"].as_array().unwrap();
        assert_eq!(args[0], "bb");
        // No classpath needed: script files run as child bb processes that
        // resolve bb.edn themselves.
        assert_eq!(args[1], "-e");
        let script = args[2].as_str().unwrap();
        assert_eq!(
            script.matches("test/finanzas/cli_test.bb").count(),
            1,
            "dedupe: {script}"
        );
        assert!(script.contains("p/shell"), "got: {script}");
        assert!(script.contains(":inherit"), "got: {script}");
        assert!(script.contains("System/exit"), "got: {script}");
    }

    #[test]
    fn run_args_babashka_mixed_files_and_namespaces() {
        let config = project::ProjectConfig {
            runner: project::TestRunner::Babashka,
            source_paths: vec!["src".to_string()],
            ..project::ProjectConfig::default()
        };
        let resp = build_run_args_response(
            &config,
            &[
                "test::finanzas::cli_test.bb::test-a(Test)".to_string(),
                "finanzas.core-test::test-greet(Test)".to_string(),
            ],
        );
        let result = &resp["result"];
        let args = result["runner_args"].as_array().unwrap();
        assert_eq!(args[0], "bb");
        assert_eq!(args[1], "--classpath");
        assert_eq!(args[2], "test:src");
        assert_eq!(args[3], "-e");
        let script = args[4].as_str().unwrap();
        assert!(
            script.contains("run-tests 'finanzas.core-test"),
            "got: {script}"
        );
        assert!(
            script.contains("test/finanzas/cli_test.bb"),
            "got: {script}"
        );
    }

    #[test]
    fn is_source_file_path_detects_script_ids() {
        assert!(is_source_file_path("test/finanzas/cli_test.bb"));
        assert!(is_source_file_path("cli_test.clj"));
        assert!(!is_source_file_path("finanzas.cli-test"));
        assert!(!is_source_file_path("finanzas.core-test"));
    }

    #[test]
    fn parse_node_id_reconstructs_script_file_path() {
        assert_eq!(
            parse_node_id("test::finanzas::cli_test.bb::test-foo(Test)"),
            (
                "test/finanzas/cli_test.bb".to_string(),
                "test-foo".to_string()
            )
        );
    }

    #[test]
    fn run_args_babashka_inline_runner_when_dep_undeclared() {
        // babashka.test-runner is NOT bundled with bb (verified live on
        // finanzas, bb 1.12.218) — without a declared dep, run an inline
        // clojure.test script over the selected namespaces instead.
        let config = project::ProjectConfig {
            runner: project::TestRunner::Babashka,
            source_paths: vec!["src".to_string()],
            test_paths: vec!["test".to_string()],
            ..project::ProjectConfig::default()
        };
        let resp = build_run_args_response(
            &config,
            &[
                "finanzas.cli-test::test-main(Test)".to_string(),
                "finanzas.cli-test::test-parse(Test)".to_string(),
                "finanzas.core-test::test-greet(Test)".to_string(),
            ],
        );
        let result = &resp["result"];
        let args = result["runner_args"].as_array().unwrap();
        assert_eq!(args[0], "bb");
        assert_eq!(args[1], "--classpath");
        assert_eq!(args[2], "test:src");
        assert_eq!(args[3], "-e");
        let script = args[4].as_str().unwrap();
        assert!(
            script.contains("(require 'finanzas.cli-test 'finanzas.core-test)"),
            "got: {script}"
        );
        assert!(
            script.contains("(clojure.test/run-tests 'finanzas.cli-test 'finanzas.core-test)"),
            "got: {script}"
        );
    }

    #[test]
    fn run_args_dedupes_namespaces() {
        let resp = build_run_args_response(
            &runner_config(project::TestRunner::Cognitect),
            &[
                "finanzas.core-test::test-a(Test)".to_string(),
                "finanzas.core-test::test-b(Test)".to_string(),
                "finanzas.cli-test::test-c(Test)".to_string(),
            ],
        );
        let result = &resp["result"];
        assert_eq!(
            result["runner_args"],
            serde_json::json!([
                "clojure",
                "-M:test",
                "-n",
                "finanzas.core-test",
                "finanzas.cli-test"
            ])
        );
    }

    #[test]
    fn run_args_kaocha_focus() {
        let resp = build_run_args_response(
            &runner_config(project::TestRunner::Kaocha),
            &["finanzas.core-test::test-greet(Test)".to_string()],
        );
        let result = &resp["result"];
        assert_eq!(
            result["runner_args"],
            serde_json::json!([
                "clojure",
                "-M:test",
                "--focus",
                "finanzas.core-test/test-greet"
            ])
        );
    }

    #[test]
    fn run_args_leiningen_uses_ns_over_name() {
        let resp = build_run_args_response(
            &runner_config(project::TestRunner::Leiningen),
            &["finanzas.core-test::test-greet(Test)".to_string()],
        );
        let result = &resp["result"];
        assert_eq!(
            result["runner_args"],
            serde_json::json!(["lein", "test", ":only", "finanzas.core-test/test-greet"])
        );
    }

    #[test]
    fn parse_node_id_extracts_ns_and_name() {
        assert_eq!(
            parse_node_id("finanzas.core-test::test-greet(Test)"),
            ("finanzas.core-test".to_string(), "test-greet".to_string())
        );
    }

    // ── ingest: clojure.test text output (gh-81) ─────────────────────────────

    #[test]
    fn ingest_clojure_test_failures_from_text() {
        let output = concat!(
            "Testing finanzas.cli-test\n\n",
            "FAIL in (test-main) (test/finanzas/cli_test.bb:12)\n",
            "expected: (= 0 (cli/run []))\n",
            "  actual: (not (= 0 nil))\n\n",
            "ERROR in (test-parse) (test/finanzas/cli_test.bb:20)\n",
            "Uncaught exception, not in assertion.\n\n",
            "RAN 2 TESTS\n",
            "0 failures\n"
        );
        let resp = cmd_ingest(&serde_json::json!({
            "params": {"run_output": output}
        }));
        let results = resp["result"]["per_test_results"].as_array().unwrap();
        assert_eq!(results.len(), 2, "got: {resp}");
        assert_eq!(results[0]["test_id"], "finanzas.cli-test::test-main(Test)");
        assert_eq!(results[0]["outcome"], "failed");
        assert!(results[0]["error_text"]
            .as_str()
            .unwrap()
            .contains("expected:"));
        assert_eq!(results[1]["test_id"], "finanzas.cli-test::test-parse(Test)");
        assert_eq!(results[1]["outcome"], "failed");
        // Correct response keys per TIA-ADAPT-008 TestRunResult
        assert!(results[0].get("duration_ms").is_some());
    }

    #[test]
    fn ingest_pass_only_text_yields_no_results() {
        // Pass-only clojure.test output names no individual tests; an empty
        // result list is recall-safe (engine keeps un-ingested items always_run).
        let output = "Testing finanzas.cli-test\n\nRAN 6 TESTS\n0 failures\n";
        let resp = cmd_ingest(&serde_json::json!({
            "params": {"run_output": output}
        }));
        let results = resp["result"]["per_test_results"].as_array().unwrap();
        assert!(results.is_empty(), "got: {resp}");
    }

    #[test]
    fn ingest_junit_xml_uses_engine_result_shape() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="finanzas.core-test" tests="1" failures="1">
    <testcase name="test-greet" classname="finanzas.core-test" time="0.01">
      <failure message="expected: (= \"hi\" out)">boom</failure>
    </testcase>
  </testsuite>
</testsuites>"#;
        let resp = cmd_ingest(&serde_json::json!({
            "params": {"run_output": xml}
        }));
        let results = resp["result"]["per_test_results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "got: {resp}");
        assert_eq!(results[0]["test_id"], "finanzas.core-test::test-greet");
        assert_eq!(results[0]["outcome"], "failed");
        assert_eq!(results[0]["duration_ms"], 10);
        assert!(results[0]["error_text"].as_str().unwrap().contains("boom"));
    }

    #[test]
    fn ingest_jsonl_still_supported() {
        let resp = cmd_ingest(&serde_json::json!({
            "params": {"run_output": "{\"test_id\":\"a::b(Test)\",\"outcome\":\"passed\",\"duration_ms\":3}\n"}
        }));
        let results = resp["result"]["per_test_results"].as_array().unwrap();
        assert_eq!(results.len(), 1, "got: {resp}");
        assert_eq!(results[0]["test_id"], "a::b(Test)");
        assert_eq!(results[0]["outcome"], "passed");
    }
}
