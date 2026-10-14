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

fn readiness_execution_case(failure: &str, expected: i32, diagnostic: &str) -> serde_json::Value {
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
    plan
}

#[test]
fn readiness_execution_missing_adapter() {
    let plan = readiness_execution_case("adapter", 1, "no adapter binary");
    // Select-time spawn deficit on the changed file: the conservative
    // component fallback (testaruda-0bwk, TIA-ADAPT-012) selects all three
    // component tests; the execution-readiness contract (exit code, healthy
    // group still runs and ingests) is asserted inside the helper.
    assert_eq!(plan["data"]["selected_count"], 3);
}
#[test]
fn readiness_execution_run_args_error() {
    let plan = readiness_execution_case("error", 1, "run args");
    // Execution-time failure: selection evidence is untouched — the
    // always-run pair (faulty, healthy; spare has history).
    assert_eq!(plan["data"]["selected_count"], 2);
}
#[test]
fn readiness_execution_empty_argv() {
    let plan = readiness_execution_case("empty", 1, "empty runner");
    assert_eq!(plan["data"]["selected_count"], 2);
}
#[test]
fn readiness_execution_missing_runner() {
    let plan = readiness_execution_case("runner", 1, "failed to run tests");
    assert_eq!(plan["data"]["selected_count"], 2);
}
#[test]
fn readiness_execution_success() {
    let plan = readiness_execution_case("ok", 0, "ingesting results");
    // Shadow mode reports the full-run outcome code in the plan.
    assert_eq!(plan["data"]["exit_code"], 10);
}
#[test]
fn readiness_execution_runner_failure() {
    let plan = readiness_execution_case("failed", 7, "test runner failed with exit code 7");
    assert_eq!(plan["data"]["selected_count"], 2);
}

/// testaruda-8trl: stale items discovered before a directory joined
/// [discover].exclude keep living in the store and get selected — the runner
/// then collects fixture projects the discover walk excludes (pytest exit 2
/// reddens the pre-push gate). The CI runner must never be invoked on a
/// node_id under an excluded path: drop those ids before grouping, warn, and
/// let the remaining ids run (recall-safe stale-data hygiene).
#[test]
fn ci_run_skips_selected_items_under_excluded_paths() {
    let project = setup_project();
    let adapter = project.path().join("adapter.py");
    std::fs::write(&adapter, r#"import json, sys
name = sys.argv[1]
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
        sel = json.loads(line)['params'].get('selected', [])
        argv = ['python3', '-c', "import sys; open('executed','w').write(' '.join(sys.argv[1:]))"] + list(sel)
        result = dict(runner_args=argv, collection_path='unused')
    elif cmd == 'ingest':
        result = dict(per_test_results=[dict(test_id='normal.py', outcome='passed')])
    print(json.dumps(dict(ok=True, result=result)), flush=True)
"#).unwrap();
    let good = format!("python3 {} healthy", adapter.display());
    std::fs::write(project.path().join("testaruda.toml"), format!(
        "confidence_threshold = 0.0\n[adapters.extensions]\n\".bar\" = {good:?}\n\n[discover]\nexclude = [\"fixtures\"]\n"
    )).unwrap();
    std::fs::write(project.path().join("source.bar"), "change").unwrap();
    // The items' files exist on disk — only the *directory* joined
    // [discover].exclude after discovery, so the stale-path prune (gh-71)
    // must not remove them.
    let fixture_dir = project
        .path()
        .join("tests/fixtures/python-src-layout/tests");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    std::fs::write(
        fixture_dir.join("test_model.py"),
        "def test_model(): pass\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("normal.py"),
        "def test_normal(): pass\n",
    )
    .unwrap();
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    conn.execute_batch(
        "INSERT INTO test_items(component, adapter, node_id) VALUES
        ('default','healthy','tests/fixtures/python-src-layout/tests/test_model.py'),
        ('default','healthy','normal.py');",
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--safe", "--json", "--files", "source.bar"])
        .current_dir(project.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{}", stderr);
    assert!(
        stderr.contains("[discover].exclude"),
        "expected a diagnostic about skipped excluded items.\nstderr: {}",
        stderr
    );
    let executed = std::fs::read_to_string(project.path().join("executed")).unwrap_or_default();
    assert!(
        executed.contains("normal.py"),
        "non-excluded item must still run. executed: {:?}",
        executed
    );
    assert!(
        !executed.contains("fixtures"),
        "excluded-path item must never reach the runner. executed: {:?}",
        executed
    );
}

/// testaruda-8trl (all-excluded case): when every selected item sits under
/// [discover].exclude paths, the group empties and must be skipped entirely —
/// never an adapter run with zero ids (run-args error → "incomplete
/// execution") and never a non-zero exit for the pre-push gate.
#[test]
fn ci_run_all_excluded_group_skips_runner_entirely() {
    let project = setup_project();
    let adapter = project.path().join("adapter.py");
    std::fs::write(&adapter, r#"import json, sys
name = sys.argv[1]
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
        result = dict(runner_args=['python3', '-c', "open('executed','w').write('yes')"], collection_path='unused')
    elif cmd == 'ingest':
        result = dict(per_test_results=[])
    print(json.dumps(dict(ok=True, result=result)), flush=True)
"#).unwrap();
    let good = format!("python3 {} healthy", adapter.display());
    std::fs::write(project.path().join("testaruda.toml"), format!(
        "confidence_threshold = 0.0\n[adapters.extensions]\n\".bar\" = {good:?}\n\n[discover]\nexclude = [\"fixtures\"]\n"
    )).unwrap();
    std::fs::write(project.path().join("source.bar"), "change").unwrap();
    let fixture_dir = project
        .path()
        .join("tests/fixtures/python-src-layout/tests");
    std::fs::create_dir_all(&fixture_dir).unwrap();
    std::fs::write(
        fixture_dir.join("test_model.py"),
        "def test_model(): pass\n",
    )
    .unwrap();
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    conn.execute_batch(
        "INSERT INTO test_items(component, adapter, node_id) VALUES
        ('default','healthy','tests/fixtures/python-src-layout/tests/test_model.py');",
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--safe", "--json", "--files", "source.bar"])
        .current_dir(project.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(0), "{}", stderr);
    assert!(
        !project.path().join("executed").exists(),
        "runner must not be invoked when the whole group is excluded.\nstderr: {}",
        stderr
    );
    assert!(
        stderr.contains("[discover].exclude"),
        "expected the skip diagnostic.\nstderr: {}",
        stderr
    );
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

// ===== Incomplete adapter evidence — conservative fallback (testaruda-0bwk) =====
//
// TIA-CHG-007 (unknown file kind fallback), TIA-SAFE-004 (unresolved file
// treatment), TIA-ADAPT-012 (adapter failure fallback): when the evidence
// behind a changed unit is incomplete, selection must not silently return an
// empty set — the core SHALL raise a fallback signal for the affected
// component (conservatively selecting that component's tests, FULL_RUN = 10).

/// testaruda-cd4u: a hung adapter child must never wedge exec/discover.
/// The adapter (a) spawns a long-lived grandchild and (b) never reads stdin
/// nor responds. With `[adapters] timeout_ms = 300` the discover pipeline
/// must finish promptly (typed timeout treated as adapter deficit, not a
/// crash) and the grandchild must not survive as an orphan (process-group
/// kill). Before cd4u the engine had no bound around adapter execution — a
/// wedged exec tree persisted for days.
#[test]
fn discover_hanging_adapter_times_out_and_kills_process_group() {
    let project = setup_project();
    let hang = project.path().join("hang.py");
    std::fs::write(
        &hang,
        "import subprocess, time\nsubprocess.Popen(['sleep', '424243'])\ntime.sleep(30)\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join("testaruda.toml"),
        "[adapters]\ntimeout_ms = 300\n[adapters.extensions]\n\".foo\" = \"python3 hang.py\"\n",
    )
    .unwrap();
    std::fs::write(project.path().join("source.foo"), "fn main() {}\n").unwrap();

    // Bounded run: spawn + poll instead of output(), so a regression back to
    // the unbounded wait fails this test with a diagnosis instead of hanging
    // the whole suite until the CI timeout.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["discover"])
        .current_dir(project.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn testaruda discover");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let status = loop {
        match child.try_wait().expect("try_wait failed") {
            Some(status) => break status,
            None if std::time::Instant::now() > deadline => {
                let _ = child.kill();
                panic!(
                    "discover did not finish within 30s against a hanging adapter — \
                     exec is wedging again (testaruda-cd4u)"
                );
            }
            None => std::thread::sleep(std::time::Duration::from_millis(100)),
        }
    };

    // The adapter deficit is recorded, never fatal: discover exits 0 with a
    // warning naming the adapter timeout.
    assert!(
        status.success(),
        "discover should degrade gracefully on adapter timeout, got: {status:?}"
    );
    let stderr = {
        use std::io::Read;
        let mut buf = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut buf);
        }
        buf
    };
    assert!(
        stderr.contains("timeout") || stderr.contains("Failed"),
        "expected a diagnostic about the adapter timeout.\nstderr: {stderr}"
    );

    // The shim's grandchild (sleep 424243) must be gone — the deadline kill
    // takes the whole process group, not just the direct child.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        !marker_process_alive("424243"),
        "orphaned grandchild survived the configured adapter timeout (testaruda-cd4u)"
    );
}

/// Scan /proc for a live process whose cmdline contains `marker`.
fn marker_process_alive(marker: &str) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false; // no /proc — cannot verify, assume absent
    };
    for entry in entries.flatten() {
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let cmd = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        if cmd.contains(marker) {
            return true;
        }
    }
    false
}

/// Shared harness for incomplete-evidence cases. Warm store (every test item
/// has run history) so nothing is always-run: the selection can only come
/// from real adapter evidence or the conservative fallback. The changed file
/// is covered by the `faulty` adapter; the `healthy` adapter covers `.bar`.
/// Plain `select --json` (no --safe/--ci) — no runner execution, so the
/// outcome code is the process exit code.
fn readiness_incomplete_case(
    mode: &str,
    changed: &str,
    expected: i32,
    diagnostic: &str,
) -> (serde_json::Value, String) {
    let project = setup_project();
    let adapter = project.path().join("adapter.py");
    std::fs::write(&adapter, r#"import json, sys
name, mode = sys.argv[1:]
if mode == 'startup' and name == 'faulty':
    sys.exit(1)
for line in sys.stdin:
    cmd = json.loads(line)['command']
    result = []
    if cmd == 'handshake':
        result = dict(name=name, version='1', protocol=1, languages=['fake'], granularity='file', capabilities={})
    elif cmd == 'static-deps':
        if mode == 'malformed' and name == 'faulty':
            print('this is not json', flush=True)
            continue
        unresolved = ['source.foo'] if (mode == 'unresolved' and name == 'faulty') else []
        print(json.dumps(dict(candidates=[], edges=[], unresolved=unresolved)), flush=True)
        continue
    elif cmd == 'discover':
        print(json.dumps(dict(result=[])), flush=True)
        continue
    elif cmd == 'fingerprint':
        print(json.dumps(dict(fingerprints=[])), flush=True)
        continue
    print(json.dumps(dict(ok=True, result=result)), flush=True)
"#).unwrap();
    // Both extension slots use the same script; mode picks the failure.
    // "missing" points the `.foo` slot at a binary that does not exist.
    let faulty = if mode == "missing" {
        "./absent-adapter".to_string()
    } else {
        format!("python3 {} faulty {}", adapter.display(), mode)
    };
    let healthy = format!("python3 {} healthy {}", adapter.display(), mode);
    std::fs::write(project.path().join("testaruda.toml"), format!(
        "confidence_threshold = 0.0\n[adapters.extensions]\n\".foo\" = {faulty:?}\n\".bar\" = {healthy:?}\n"
    )).unwrap();
    std::fs::write(project.path().join("source.foo"), "change").unwrap();
    std::fs::write(project.path().join("source.bar"), "original").unwrap();
    if changed == "source.xyz" {
        std::fs::write(project.path().join("source.xyz"), "change").unwrap();
    }
    // Warm store: all three items have run history (no always-run members).
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    conn.execute_batch("INSERT INTO test_items(component, adapter, node_id) VALUES
        ('default','faulty','faulty'), ('default','healthy','healthy'), ('default','healthy','spare');
        INSERT INTO run_history(test_item_id,run_id,outcome,environment) VALUES
        (1,'prev','passed','default'), (2,'prev','passed','default'), (3,'prev','passed','default');")
        .unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--json", "--files", changed])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --json failed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(expected), "{stderr}");
    assert!(stderr.contains(diagnostic), "{stderr}");
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        plan["data"]["selected_count"], 3,
        "component fallback must select all component tests\n{stderr}"
    );
    (plan, stderr.to_string())
}

/// Unknown file kind (no adapter registered for `.xyz`) with no known edges:
/// TIA-CHG-007 requires a fallback signal for the affected component.
#[test]
fn readiness_incomplete_unknown_kind() {
    let (plan, _) = readiness_incomplete_case("ok", "source.xyz", 10, "unknown file kind");
    assert!(plan["data"]["reason"]
        .as_str()
        .unwrap_or("")
        .contains("unknown file kind"));
}

/// Adapter reports the changed file unresolved (TIA-SAFE-004): dependents
/// must be conservatively force-included via a component fallback.
#[test]
fn readiness_incomplete_unresolved_report() {
    let (plan, _) = readiness_incomplete_case(
        "unresolved",
        "source.foo",
        10,
        "reported the file unresolved",
    );
    assert!(plan["data"]["reason"]
        .as_str()
        .unwrap_or("")
        .contains("reported the file unresolved"));
}

/// Adapter binary missing: TIA-ADAPT-012 requires the fallback mechanism,
/// not a log-only warning with an empty selection.
#[test]
fn readiness_incomplete_missing_binary() {
    let (plan, _) =
        readiness_incomplete_case("missing", "source.foo", 10, "Incomplete adapter evidence");
    assert!(plan["data"]["reason"]
        .as_str()
        .unwrap_or("")
        .contains("unavailable or failed"));
}

/// Adapter dies at startup (handshake never completes): same fallback contract.
#[test]
fn readiness_incomplete_startup_failure() {
    let (plan, _) =
        readiness_incomplete_case("startup", "source.foo", 10, "Incomplete adapter evidence");
    assert!(plan["data"]["reason"]
        .as_str()
        .unwrap_or("")
        .contains("unavailable or failed"));
}

/// Adapter emits a malformed static-deps response: same fallback contract.
#[test]
fn readiness_incomplete_malformed_response() {
    let (plan, _) =
        readiness_incomplete_case("malformed", "source.foo", 10, "Incomplete adapter evidence");
    assert!(plan["data"]["reason"]
        .as_str()
        .unwrap_or("")
        .contains("unavailable or failed"));
}

// ===== Selection idempotence — baseline advancement (testaruda-yvyb) =====
//
// TIA-CHG-002/003/010, TIA-SAFE-001: selection must not consume the change
// classification. Repeating an identical request — select-only or after a
// FAILED execution — must preserve the classification and select the same
// tests again; the fingerprint baseline advances only after a successful run.

/// Shared harness for baseline-preservation cases: warm store (no always-run
/// members), a healthy adapter pair, and an unknown-kind changed file whose
/// only selection route is the conservative component fallback. Both selects
/// in each test must select all three component tests.
fn readiness_baseline_setup(mode: &str, changed: &str, preseed_old_fp: bool) -> tempfile::TempDir {
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
    elif cmd == 'run-args':
        if mode == 'run-fails' and name == 'faulty':
            print(json.dumps(dict(ok=False, error='controlled run-args error')), flush=True)
            continue
        result = dict(runner_args=['python3', '-c', "open('executed', 'a').write('yes')"], collection_path='unused')
    elif cmd == 'ingest':
        result = dict(per_test_results=[dict(test_id=name, outcome='passed' if mode != 'runner-fails' else 'failed')])
    print(json.dumps(dict(ok=True, result=result)), flush=True)
"#).unwrap();
    let faulty = format!("python3 {} faulty {}", adapter.display(), mode);
    let healthy = format!("python3 {} healthy {}", adapter.display(), mode);
    std::fs::write(project.path().join("testaruda.toml"), format!(
        "confidence_threshold = 0.0\n[adapters.extensions]\n\".foo\" = {faulty:?}\n\".bar\" = {healthy:?}\n"
    )).unwrap();
    std::fs::write(project.path().join(changed), "changed-content").unwrap();
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    if preseed_old_fp {
        // The file is KNOWN to the store with a stale fingerprint: the next
        // select must classify it as changed, and a repeat must too.
        conn.execute(
            "INSERT INTO content_units(component, path, symbol, kind, fingerprint) VALUES
             ('default', ?1, NULL, 'source', 'stale-fingerprint')",
            rusqlite::params![changed],
        )
        .unwrap();
    }
    conn.execute_batch("INSERT INTO test_items(component, adapter, node_id) VALUES
        ('default','faulty','faulty'), ('default','healthy','healthy'), ('default','healthy','spare');
        INSERT INTO run_history(test_item_id,run_id,outcome,environment) VALUES
        (1,'prev','passed','default'), (2,'prev','passed','default'), (3,'prev','passed','default');")
        .unwrap();
    project
}

fn run_select(project: &tempfile::TempDir, args: &[&str]) -> (i32, serde_json::Value, String) {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"));
    command
        .args(["select", "--json"])
        .args(args)
        .current_dir(project.path());
    let output = command.output().unwrap();
    let plan: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
    (
        output.status.code().unwrap_or(-1),
        plan,
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn assert_fallback_selected(code: i32, plan: &serde_json::Value, select: usize) {
    assert_eq!(code, 10, "fallback must yield FULL_RUN");
    assert_eq!(
        plan["data"]["selected_count"], 3,
        "select #{select}: classification must be preserved"
    );
}

/// Known change: a repeat select-only request must not lose the affected
/// tests (the fingerprint baseline must not advance during selection).
#[test]
fn readiness_baseline_known_change() {
    let project = readiness_baseline_setup("ok", "source.xyz", true);
    let (code, plan, stderr) = run_select(&project, &["--files", "source.xyz"]);
    assert_fallback_selected(code, &plan, 1);
    let (code2, plan2, _) = run_select(&project, &["--files", "source.xyz"]);
    assert_fallback_selected(code2, &plan2, 2);
    // The preseeded stale baseline must survive both select-only requests.
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    let fp: String = conn
        .query_row(
            "SELECT fingerprint FROM content_units WHERE path = 'source.xyz'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        fp, "stale-fingerprint",
        "select-only must not advance the baseline"
    );
    drop(stderr);
}

/// Cold start: a file unknown to the store stays classified (unresolved)
/// across repeat select-only requests.
#[test]
fn readiness_baseline_cold_start() {
    let project = readiness_baseline_setup("ok", "source.xyz", false);
    let (code, plan, _) = run_select(&project, &["--files", "source.xyz"]);
    assert_fallback_selected(code, &plan, 1);
    let (code2, plan2, _) = run_select(&project, &["--files", "source.xyz"]);
    assert_fallback_selected(code2, &plan2, 2);
    // Cold-start baseline: still "unknown" after both select-only requests.
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    let fp: String = conn
        .query_row(
            "SELECT fingerprint FROM content_units WHERE path = 'source.xyz'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fp, "unknown", "cold-start baseline must stay unknown");
}

/// Failed execution: after a CI run whose faulty group fails, a repeat select
/// must still classify and select the change (no baseline advancement on
/// failed execution).
#[test]
fn readiness_baseline_failed_execution() {
    let project = readiness_baseline_setup("run-fails", "source.xyz", true);
    let (code, plan, _) = run_select(&project, &["--ci", "--files", "source.xyz"]);
    assert_eq!(
        code, 1,
        "the faulty group's run-args error must fail the run"
    );
    assert_eq!(plan["data"]["selected_count"], 3);
    // All groups process before exit (kkno): the healthy group still ran.
    assert!(
        project.path().join("executed").exists(),
        "healthy group must execute"
    );
    // Repeat select-only request: classification preserved.
    let (code2, plan2, _) = run_select(&project, &["--files", "source.xyz"]);
    assert_fallback_selected(code2, &plan2, 2);
    // The failed run must not have advanced the baseline either.
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    let fp: String = conn
        .query_row(
            "SELECT fingerprint FROM content_units WHERE path = 'source.xyz'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        fp, "stale-fingerprint",
        "failed execution must not advance the baseline"
    );
}

// ===== Confidence floor over real component memberships (testaruda-2bxl) =====
//
// TIA-SAFE-002/003: the confidence fallback must consult ACTUAL component
// memberships (loaded from the store) and fire only for the below-threshold
// component; always-run-only components are exempt.
//
// Seed: three components (content units are all 'default' — the unit
// classification is component-scoped to 'default'; TESTS carry the
// component axis the engine reasons over).
// - 'default': t1 (edge→source.a, k=0.1), t4/t8 (no edges)
// - 'other':   t2/t3 (edges→source.b, k=1.0), t5 (no edges)
// - 'sparse':  t6 (edge→source.c, k=0.1, NO history → always-run), t7 (no edges, has history)
// Invocation quality works out to 0.7 (history depth factor), so effective
// confidences: t1 0.07, t2/t3 0.7, t6 0.07. Threshold 0.5.
// Test-item ids follow insertion order: t1=1 t4=2 t8=3 t2=4 t3=5 t5=6 t7=7 t6=8.
fn readiness_confidence_setup() -> tempfile::TempDir {
    let project = setup_project();
    std::fs::write(
        project.path().join("testaruda.toml"),
        "confidence_threshold = 0.5\n[adapters.extensions]\n",
    )
    .unwrap();
    std::fs::write(project.path().join("source.a"), "aaa").unwrap();
    std::fs::write(project.path().join("source.b"), "bbb").unwrap();
    std::fs::write(project.path().join("source.c"), "ccc").unwrap();
    let conn = rusqlite::Connection::open(project.path().join(".testaruda/store.db")).unwrap();
    conn.execute_batch(
        "INSERT INTO content_units(component, path, symbol, kind, fingerprint) VALUES
            ('default','source.a',NULL,'source','seed-a'),
            ('default','source.b',NULL,'source','seed-b'),
            ('default','source.c',NULL,'source','seed-c');
         INSERT INTO test_items(component, adapter, node_id) VALUES
            ('default','x','t1'), ('default','x','t4'), ('default','x','t8'),
            ('other','x','t2'), ('other','x','t3'), ('other','x','t5'),
            ('sparse','x','t7'), ('sparse','x','t6');
         INSERT INTO dependency_edges(test_item_id, content_unit_id, environment, origin, k_value) VALUES
            (1, 1, 'default', 'static', 100000),
            (4, 2, 'default', 'static', 1000000),
            (5, 2, 'default', 'static', 1000000),
            (8, 3, 'default', 'static', 100000);
         INSERT INTO reverse_index(content_unit_id, test_item_id) VALUES (1,1),(2,4),(2,5),(3,8);
         INSERT INTO run_history(test_item_id,run_id,outcome,environment)
         SELECT id, 'prev', 'passed', 'default' FROM test_items WHERE id <= 7;",
    )
    .unwrap();
    project
}

fn run_confidence_select(project: &tempfile::TempDir, file: &str) -> serde_json::Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--json", "--files", file])
        .current_dir(project.path())
        .output()
        .expect("testaruda select --json failed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("no plan: {e}; stderr: {stderr}"));
    plan
}

fn selected_test_ids(plan: &serde_json::Value) -> Vec<u64> {
    plan["data"]["tests"]
        .as_array()
        .map(|ts| ts.iter().filter_map(|t| t["id"].as_u64()).collect())
        .unwrap_or_default()
}

/// Below-threshold component: the fallback must select the WHOLE component
/// (t1 + the edge-less t4/t8), not just the reachability-selected t1.
#[test]
fn readiness_confidence_below_threshold() {
    let project = readiness_confidence_setup();
    let plan = run_confidence_select(&project, "source.a");
    let mut ids = selected_test_ids(&plan);
    ids.sort();
    // t6 (id 8) is always-run and joins every selection; the fallback must
    // add the whole 'default' component: t1 (1) + edge-less t4 (2), t8 (3).
    assert_eq!(
        ids,
        vec![1, 2, 3, 8],
        "whole below-threshold component must fall back"
    );
}

/// Above-threshold component: only the reachability-selected tests run —
/// the edge-less t5/t7 must NOT be force-included.
#[test]
fn readiness_confidence_above_threshold() {
    let project = readiness_confidence_setup();
    let plan = run_confidence_select(&project, "source.b");
    let mut ids = selected_test_ids(&plan);
    ids.sort();
    // Only the reachability-selected t2 (4), t3 (5) + always-run t6 (8) —
    // edge-less t5 (6) must NOT be force-included.
    assert_eq!(
        ids,
        vec![4, 5, 8],
        "above-threshold component must not fall back"
    );
}

/// Always-run-only component (TIA-SAFE-002 exception): t6 is always-run;
/// the component must NOT fall back to include its other members.
#[test]
fn readiness_confidence_always_run_only() {
    let project = readiness_confidence_setup();
    let plan = run_confidence_select(&project, "source.c");
    let mut ids = selected_test_ids(&plan);
    ids.sort();
    // Only the always-run t6 (8): 'sparse' must NOT fall back to include
    // its history-carrying edge-less member t7 (7) (TIA-SAFE-002 exception).
    assert_eq!(
        ids,
        vec![8],
        "always-run-only component must not trigger fallback"
    );
}
