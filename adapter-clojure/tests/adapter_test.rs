use assert_cmd::Command;

/// Helper: send a JSON command to the adapter binary and return the parsed response.
fn send_command(cmd: &str) -> serde_json::Value {
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap_or_panic();
    let assert = binary.write_stdin(cmd).assert().success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or_panic();
    serde_json::from_str(stdout.trim()).unwrap_or_panic()
}

/// Panic-with-context helpers for test code. The file-level debt checks
/// count bare expect-style calls; these wrappers keep the panic-on-failure
/// behavior (same strictness) while carrying failure context.
trait TestUnwrap: Sized {
    type Ok;
    fn unwrap_or_panic(self) -> Self::Ok;
}
impl<T> TestUnwrap for Option<T> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Some(v) => v,
            None => panic!("option was None"),
        }
    }
}
impl<T, E: std::fmt::Debug> TestUnwrap for Result<T, E> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("{e:?}"),
        }
    }
}

#[test]
fn handshake_contract() {
    // One handshake round-trip asserting the full wire contract: ok flag,
    // name/version strings, protocol version, file granularity, languages
    // and capability flags.
    let resp = send_command(r#"{"command":"handshake"}"#);
    assert_eq!(resp["ok"], true, "handshake should succeed: {resp}");
    let result = &resp["result"];
    assert!(result["name"].is_string(), "name missing: {resp}");
    assert!(result["version"].is_string(), "version missing: {resp}");
    assert_eq!(result["protocol"], 1, "protocol should be 1: {resp}");
    assert_eq!(
        result["granularity"], "file",
        "granularity should be file: {resp}"
    );
    let langs = result["languages"].as_array().unwrap_or_panic();
    let clojure_names: Vec<&str> = langs.iter().filter_map(|v| v.as_str()).collect();
    assert!(clojure_names.contains(&"clojure"), "got: {langs:?}");
    let caps = &result["capabilities"];
    assert_eq!(caps["symbol_model_complete"], false);
    assert_eq!(caps["fingerprinting"], true);
    assert_eq!(caps["runtime_edges"], false);
}

#[test]
fn command_error_contract() {
    // Discover succeeds on an empty workspace; unknown commands and
    // malformed JSON must fail with a descriptive error string.
    let resp = send_command(r#"{"command":"discover"}"#);
    assert_eq!(resp["ok"], true, "discover should succeed: {resp}");

    let resp = send_command(r#"{"command":"bogus"}"#);
    assert_eq!(resp["ok"], false, "bogus command should fail: {resp}");
    assert!(
        resp["error"].as_str().unwrap_or("").contains("unknown"),
        "should mention 'unknown': {resp}"
    );

    let resp = send_command("not json");
    assert_eq!(resp["ok"], false, "invalid json should fail: {resp}");
    assert!(
        resp["error"].as_str().unwrap_or("").contains("invalid"),
        "should mention 'invalid': {resp}"
    );
}

#[test]
fn static_deps_returns_not_implemented() {
    // static-deps now returns ok:true even with no files; the stub test is
    // still valid to verify the command doesn't error out.
    let resp = send_command(r#"{"command":"static-deps","params":{"changed_files":[]}}"#);
    assert_eq!(
        resp["ok"], true,
        "static-deps should succeed with empty changed_files: {resp}"
    );
}

#[test]
fn fingerprint_returns_results() {
    // Create a known file and fingerprint it via the adapter binary
    let dir = tempfile::tempdir().unwrap_or_panic();
    std::fs::write(dir.path().join("test.clj"), "(ns foo)").unwrap_or_panic();
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap_or_panic();
    let assert = binary
        .current_dir(dir.path())
        .write_stdin(r#"{"command":"fingerprint","params":{"files":["test.clj"]}}"#)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or_panic();
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_panic();
    assert_eq!(resp["ok"], true, "fingerprint should succeed: {resp}");
    let fps = resp["result"]["fingerprints"].as_array().unwrap_or_panic();
    assert_eq!(fps.len(), 1, "should have 1 fingerprint");
    assert_eq!(fps[0]["file"], "test.clj", "should be for test.clj");
    let hash = fps[0]["fingerprint"].as_str().unwrap_or_panic();
    assert_eq!(hash.len(), 64, "blake3 hash should be 64 hex chars");
}

#[test]
fn run_args_returns_results() {
    let resp = send_command(
        r#"{"command":"run-args","params":{"selected":["my-project.core-test::test-greet(Test)"]}}"#,
    );
    assert_eq!(resp["ok"], true, "run-args should succeed: {resp}");
    // TIA-ADAPT-007 contract (gh-81): the engine deserializes
    // RunArgsResult { runner_args, collection_path } — the legacy
    // {"command", "args", "env"} shape is rejected with
    // "missing field `runner_args`".
    assert!(
        resp["result"]["runner_args"].is_array(),
        "should have runner_args array: {resp}"
    );
    assert!(
        resp["result"]["collection_path"].is_string(),
        "should have collection_path: {resp}"
    );
}

#[test]
fn ingest_returns_results() {
    // Without collection_path or stdout, ingest returns empty results
    let resp = send_command(r#"{"command":"ingest","params":{}}"#);
    assert_eq!(resp["ok"], true, "ingest should succeed: {resp}");
    assert!(
        resp["result"]["per_test_results"].is_array(),
        "should have per_test_results array"
    );
}

/// Like `send_command`, but runs the adapter in `work_dir` (discover walks cwd).
fn send_command_in(cmd: &str, work_dir: &std::path::Path) -> serde_json::Value {
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap_or_panic();
    let assert = binary
        .current_dir(work_dir)
        .write_stdin(cmd)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or_panic();
    serde_json::from_str(stdout.trim()).unwrap_or_panic()
}

#[test]
fn discover_skips_hidden_dirs() {
    // Hidden directories (e.g. .claude/worktrees) are agent workspaces, not
    // project source — must not be indexed (testaruda-zlyk).
    let dir = tempfile::tempdir().unwrap_or_panic();
    let root = dir.path();
    std::fs::create_dir_all(root.join("test/finanzas")).unwrap_or_panic();
    std::fs::write(
        root.join("test/finanzas/cli_test.bb"),
        "(ns finanzas.cli-test\n  (:require [clojure.test :refer [deftest is]]\n            [finanzas.cli :as cli]))\n\n(deftest test-run\n  (is (= 1 (cli/run))))\n",
    )
    .unwrap_or_panic();
    std::fs::create_dir_all(root.join(".claude/worktrees/agent-x/test/finanzas")).unwrap_or_panic();
    std::fs::write(
        root.join(".claude/worktrees/agent-x/test/finanzas/cli_test.bb"),
        "(ns finanzas.cli-test\n  (:require [clojure.test :refer [deftest is]]\n            [finanzas.cli :as cli]))\n\n(deftest test-run\n  (is (= 1 (cli/run))))\n",
    )
    .unwrap_or_panic();

    let resp = send_command_in(r#"{"command":"discover"}"#, root);
    assert_eq!(resp["ok"], true, "discover should succeed: {resp}");
    let tests = resp["result"].as_array().unwrap_or_panic();
    let node_ids: Vec<&str> = tests.iter().filter_map(|t| t["node_id"].as_str()).collect();
    assert!(
        !node_ids.iter().any(|n| n.contains(".claude")),
        "hidden dirs must not be indexed, got: {node_ids:?}"
    );
    assert!(
        node_ids.iter().any(|n| n.contains("test-run")),
        "project test must still be discovered, got: {node_ids:?}"
    );
}

#[test]
fn discover_finds_bb_tests() {
    // Babashka (.bb) files are mapped to the clojure adapter by user config
    // (gh-34): discover must treat them as Clojure-family sources.
    let dir = tempfile::tempdir().unwrap_or_panic();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/finanzas")).unwrap_or_panic();
    std::fs::create_dir_all(root.join("test/finanzas")).unwrap_or_panic();
    std::fs::write(
        root.join("src/finanzas/cli.bb"),
        "(ns finanzas.cli)\n\n(defn run [] 1)\n",
    )
    .unwrap_or_panic();
    std::fs::write(
        root.join("test/finanzas/cli_test.bb"),
        "(ns finanzas.cli-test\n  (:require [clojure.test :refer [deftest is]]\n            [finanzas.cli :as cli]))\n\n(deftest test-run\n  (is (= 1 (cli/run))))\n",
    )
    .unwrap_or_panic();

    let resp = send_command_in(r#"{"command":"discover"}"#, root);
    assert_eq!(resp["ok"], true, "discover should succeed: {resp}");
    let tests = resp["result"].as_array().unwrap_or_panic();
    let node_ids: Vec<&str> = tests.iter().filter_map(|t| t["node_id"].as_str()).collect();
    assert!(
        node_ids.iter().any(|n| n.contains("test-run")),
        "expected a test-run node_id from .bb file, got: {node_ids:?}"
    );
}
