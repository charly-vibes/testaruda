use assert_cmd::Command;
use tempfile;

/// Helper: send a JSON command to the adapter binary and return the parsed response.
fn send_command(cmd: &str) -> serde_json::Value {
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap();
    let assert = binary.write_stdin(cmd).assert().success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    serde_json::from_str(stdout.trim()).unwrap()
}

#[test]
fn handshake_returns_ok() {
    let resp = send_command(r#"{"command":"handshake"}"#);
    assert_eq!(resp["ok"], true, "handshake should succeed: {resp}");
}

#[test]
fn handshake_languages_is_clojure() {
    let resp = send_command(r#"{"command":"handshake"}"#);
    let langs = resp["result"]["languages"].as_array().unwrap();
    let clojure_names: Vec<&str> = langs.iter().filter_map(|v| v.as_str()).collect();
    assert!(clojure_names.contains(&"clojure"), "got: {langs:?}");
}

#[test]
fn handshake_granularity_is_file() {
    let resp = send_command(r#"{"command":"handshake"}"#);
    assert_eq!(
        resp["result"]["granularity"], "file",
        "granularity should be file: {resp}"
    );
}

#[test]
fn handshake_capabilities_correct() {
    let resp = send_command(r#"{"command":"handshake"}"#);
    let caps = &resp["result"]["capabilities"];
    assert_eq!(caps["symbol_model_complete"], false);
    assert_eq!(caps["fingerprinting"], true);
    assert_eq!(caps["runtime_edges"], false);
}

#[test]
fn handshake_protocol_version() {
    let resp = send_command(r#"{"command":"handshake"}"#);
    assert_eq!(resp["result"]["protocol"], 1, "protocol should be 1");
}

#[test]
fn handshake_has_name_and_version() {
    let resp = send_command(r#"{"command":"handshake"}"#);
    assert!(resp["result"]["name"].is_string(), "name missing: {resp}");
    assert!(
        resp["result"]["version"].is_string(),
        "version missing: {resp}"
    );
}

#[test]
fn unknown_command_returns_error() {
    let resp = send_command(r#"{"command":"bogus"}"#);
    assert_eq!(resp["ok"], false, "bogus command should fail: {resp}");
    assert!(
        resp["error"].as_str().unwrap_or("").contains("unknown"),
        "should mention 'unknown': {resp}"
    );
}

#[test]
fn invalid_json_returns_error() {
    let resp = send_command("not json");
    assert_eq!(resp["ok"], false, "invalid json should fail: {resp}");
    assert!(
        resp["error"].as_str().unwrap_or("").contains("invalid"),
        "should mention 'invalid': {resp}"
    );
}

#[test]
fn discover_returns_results() {
    let resp = send_command(r#"{"command":"discover"}"#);
    assert_eq!(resp["ok"], true, "discover should succeed: {resp}");
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
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("test.clj"), "(ns foo)").unwrap();
    let mut binary = Command::cargo_bin("testaruda-adapter-clojure").unwrap();
    let assert = binary
        .current_dir(dir.path())
        .write_stdin(r#"{"command":"fingerprint","params":{"files":["test.clj"]}}"#)
        .assert()
        .success();
    let output = assert.get_output();
    let stdout = std::str::from_utf8(&output.stdout).unwrap();
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(resp["ok"], true, "fingerprint should succeed: {resp}");
    let fps = resp["result"]["fingerprints"].as_array().unwrap();
    assert_eq!(fps.len(), 1, "should have 1 fingerprint");
    assert_eq!(fps[0]["file"], "test.clj", "should be for test.clj");
    let hash = fps[0]["fingerprint"].as_str().unwrap();
    assert_eq!(hash.len(), 64, "blake3 hash should be 64 hex chars");
}

#[test]
fn run_args_returns_results() {
    let resp =
        send_command(r#"{"command":"run-args","params":{"selected":["test/core_test.clj"]}}"#);
    assert_eq!(resp["ok"], true, "run-args should succeed: {resp}");
    assert!(resp["result"]["args"].is_array(), "should have args array");
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
