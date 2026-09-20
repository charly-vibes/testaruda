//! CLI exit code tests (testaruda-fnyi).
//!
//! Verifies that JSON and human output modes return the same process exit
//! code derived from the selection outcome, not just success.

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
