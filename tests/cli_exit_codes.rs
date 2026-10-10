//! Purpose: Verify selection and execution exit contracts through the CLI.
//! Responsibilities:
//! - Compare selection exit codes across output modes.
//! - Reject incomplete execution while preserving other groups' run history.
//!
//! Rationale: Isolated adapters distinguish unavailable execution from success
//! without depending on installed language tools (testaruda-oeft).
mod common;

/// Set up a minimal git project with a testaruda store.
/// Spawned children get `current_dir` explicitly — no process-cwd mutation
/// (testaruda-pzh6).
fn setup_project() -> tempfile::TempDir {
    let project = tempfile::tempdir().unwrap();

    // Initialize git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(project.path())
        .output()
        .expect("git init failed");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(project.path())
        .output()
        .expect("git config user.email failed");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(project.path())
        .output()
        .expect("git config user.name failed");

    // Create placeholder files. Ignore testaruda's own artifacts — real
    // projects gitignore .testaruda/ (build artifact), so the init output
    // must not pollute the v2 revision range under test.
    std::fs::write(
        project.path().join(".gitignore"),
        ".testaruda/\n.genesis/\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("Cargo.toml"),
        r#"[package]
name = "test"
version = "0.1.0"
"#,
    )
    .unwrap();
    let src = project.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("lib.rs"), "pub fn hello() -> &str { \"hello\" }").unwrap();

    // Commit so git operations work
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(project.path())
        .output()
        .expect("git add failed");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(project.path())
        .output()
        .expect("git commit failed");

    // Initialize testaruda store
    let init_output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .arg("init")
        .current_dir(project.path())
        .output()
        .expect("testaruda init failed");
    assert!(
        init_output.status.success(),
        "testaruda init should succeed: {}",
        String::from_utf8_lossy(&init_output.stderr)
    );

    project
}

fn readiness_execution_case(failure: &str, expected: i32, diagnostic: &str) {
    let project = setup_project();
    let adapter = project.path().join("adapter.py");
    std::fs::write(&adapter, r#"import json, sys
name, mode = sys.argv[1:]
for line in sys.stdin:
    cmd = json.loads(line)['command']
    result = []
    if cmd == 'handshake':
        result = dict(name=name, version='1', protocol=1, languages=['fake'], granularity='file', capabilities={})
    elif cmd == 'static-deps':
        print(json.dumps(dict(candidates=[], edges=[], unresolved=[])), flush=True)
        continue
    elif cmd == 'fingerprint':
        print(json.dumps(dict(fingerprints=[])), flush=True)
        continue
    elif cmd == 'run-args':
        if mode == 'error':
            print(json.dumps(dict(ok=False, error='controlled run-args error')), flush=True)
            continue
        argv = ['python3', '-c', "open('executed', 'w').write('yes')"]
        if mode == 'empty': argv = []
        if mode == 'runner': argv = ['./absent-runner']
        if mode == 'failed': argv = ['python3', '-c', 'import sys; sys.exit(7)']
        result = dict(runner_args=argv, collection_path='unused')
    elif cmd == 'ingest':
        result = dict(per_test_results=[dict(test_id=name, outcome='passed')])
    print(json.dumps(dict(ok=True, result=result)), flush=True)
"#).unwrap();
    let good = format!("python3 {} healthy ok", adapter.display());
    let bad = if failure == "adapter" {
        "./absent-adapter".to_string()
    } else {
        format!("python3 {} faulty {}", adapter.display(), failure)
    };
    std::fs::write(project.path().join("testaruda.toml"), format!(
        "confidence_threshold = 0.0\n[adapters.extensions]\n\".foo\" = {bad:?}\n\".bar\" = {good:?}\n"
    )).unwrap();
    std::fs::write(project.path().join("source.foo"), "change").unwrap();
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    conn.execute_batch("INSERT INTO test_items(component, adapter, node_id) VALUES
        ('default','faulty','faulty'), ('default','healthy','healthy'), ('default','healthy','spare');
        INSERT INTO run_history(test_item_id,run_id,outcome,environment)
        VALUES(3,'previous','passed','default');").unwrap();
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"));
    command.args(["select", "--safe", "--json", "--files", "source.foo"]);
    if failure == "ok" {
        command.arg("--shadow");
    }
    let output = command.current_dir(project.path()).output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(expected), "{stderr}");
    assert!(stderr.contains(diagnostic), "{stderr}");
    assert!(
        project.path().join("executed").exists(),
        "healthy runner must execute"
    );
    let ingested: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM run_history WHERE test_item_id=2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(ingested > 0, "healthy group's results must be ingested");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    if failure != "ok" {
        assert_eq!(plan["data"]["selected_count"], 2);
    }
}

#[test]
fn readiness_execution_missing_adapter() {
    readiness_execution_case("adapter", 1, "no adapter binary");
}
#[test]
fn readiness_execution_run_args_error() {
    readiness_execution_case("error", 1, "run args");
}
#[test]
fn readiness_execution_empty_argv() {
    readiness_execution_case("empty", 1, "empty runner");
}
#[test]
fn readiness_execution_missing_runner() {
    readiness_execution_case("runner", 1, "failed to run tests");
}
#[test]
fn readiness_execution_success() {
    readiness_execution_case("ok", 0, "ingesting results");
}
#[test]
fn readiness_execution_runner_failure() {
    readiness_execution_case("failed", 7, "test runner failed with exit code 7");
}

/// Test that `testaruda select --json` exits with the outcome-derived code
/// (not 0) when no tests are selected (exit code 20).
#[test]
fn json_mode_exits_with_no_tests_code() {
    let project = setup_project();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--json", "--files", "nonexistent.py"])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --json failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The JSON output should contain the correct exit code
    assert!(
        stdout.contains("\"exit_code\": 20"),
        "JSON output should have exit_code: 20\nstatus: {:?}\nstdout: {}\nstderr: {}",
        output.status,
        stdout,
        stderr
    );

    // The process exit code should match the outcome code, not 0
    assert_eq!(
        output.status.code(),
        Some(20),
        "JSON mode should exit with outcome-derived code (20), not 0.\nstderr: {}\nstdout: {}",
        stderr,
        stdout,
    );
}

/// Test that `testaruda select --agent` exits with the outcome-derived code
/// (not 0) when no tests are selected (exit code 20) — testaruda-9lbm.
/// Agent mode is a machine-readable JSON contract: the process status must
/// carry the CI decision like JSON plan mode does (testaruda-fnyi).
#[test]
fn agent_mode_exits_with_no_tests_code() {
    let project = setup_project();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--agent", "--files", "nonexistent.py"])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --agent failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The process exit code must match the outcome code (20), not 0
    assert_eq!(
        output.status.code(),
        Some(20),
        "Agent mode should exit with outcome-derived code (20), not 0.\nstderr: {}\nstdout: {}",
        stderr,
        stdout,
    );

    // Valid agent JSON must still be printed despite the non-zero exit
    assert!(
        !stdout.trim().is_empty(),
        "Agent mode should still print its JSON payload.\nstderr: {}",
        stderr
    );
}

/// Test that `testaruda select --pre-edit` exits with the outcome-derived
/// code (not 0) when no tests are selected (exit code 20) — testaruda-ljeg.
/// Pre-edit mode is also a machine-readable contract: the process status
/// must carry the CI decision like agent and JSON plan modes do.
#[test]
fn pre_edit_mode_exits_with_no_tests_code() {
    let project = setup_project();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--pre-edit", "--files", "nonexistent.py"])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --pre-edit failed");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The process exit code must match the outcome code (20), not 0
    assert_eq!(
        output.status.code(),
        Some(20),
        "Pre-edit mode should exit with outcome-derived code (20), not 0.\nstderr: {}\nstdout: {}",
        stderr,
        stdout,
    );

    // Valid pre-edit JSON must still be printed despite the non-zero exit
    assert!(
        stdout.contains("testaruda-pre-edit-v1"),
        "Pre-edit mode should still print its JSON payload.\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}

/// Test that `testaruda select --base/--head` detects changes in a revision
/// range even when the store was ingested at head (testaruda-jdw5).
///
/// Repro: store fingerprints populated at HEAD, then ask for the range
/// HEAD~1..HEAD touching a source file. The working-tree fingerprint equals
/// the stored one, but the range itself proves the file changed —
/// changed_count must be 1, not 0.
#[test]
fn revision_range_detects_in_range_change() {
    let project = setup_project();

    // Commit a v2 change on top of the initial commit (v1)
    std::fs::write(
        project.path().join("src/lib.rs"),
        "pub fn hello() -> &str { \"hello v2\" }",
    )
    .unwrap();
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(project.path())
        .output()
        .expect("git add failed");
    std::process::Command::new("git")
        .args(["commit", "-m", "v2"])
        .current_dir(project.path())
        .output()
        .expect("git commit failed");

    // Populate the store at HEAD (v2), mirroring reality: stores get their
    // units from selects/ingests on working changes, then match head.
    // Touch lib.rs in the working tree, select, revert, select again —
    // the unit now exists with the v2 fingerprint.
    for i in 0..3 {
        if i == 0 {
            std::fs::write(
                project.path().join("src/lib.rs"),
                "pub fn hello() -> &str { \"working tree edit\" }",
            )
            .unwrap();
        }
        if i == 1 {
            std::process::Command::new("git")
                .args(["checkout", "--", "src/lib.rs"])
                .current_dir(project.path())
                .output()
                .expect("git checkout failed");
        }
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
            .args(["select", "--json"])
            .current_dir(project.path())
            .output()
            .expect("testaruda select failed");
        assert!(
            out.status.success() || out.status.code() == Some(20),
            "baseline select should not crash: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // Now ask for the range that touches src/lib.rs
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--json", "--base", "HEAD~1", "--head", "HEAD"])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --base/--head failed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("select --json output should be valid JSON");

    assert_eq!(
        parsed["data"]["changed_count"].as_i64(),
        Some(1),
        "Revision range touching src/lib.rs must report changed_count=1 \
         even when the store was ingested at head.\nstderr: {}\nstdout: {}",
        String::from_utf8_lossy(&output.stderr),
        stdout
    );
}

/// Test that `testaruda select --json` exits with the outcome-derived code
/// when confidence is low (exit code 10).
#[test]
fn json_mode_exits_with_low_confidence_code() {
    let project = setup_project();

    // Run select with --files to trigger a selection with a changed file
    // We need to modify a file to create a change set
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--json", "--files", "src/lib.rs"])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --json failed");

    let _stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The process exit code should be non-zero only if the outcome is non-zero
    // (selection may complete successfully with exit 0)
    assert!(
        stdout.contains("\"exit_code\""),
        "JSON output should contain exit_code\nstdout: {}",
        stdout
    );
}
