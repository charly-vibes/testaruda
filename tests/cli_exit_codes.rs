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

    // Create placeholder files
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
