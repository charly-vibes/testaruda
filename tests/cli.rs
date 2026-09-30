//! CLI integration tests.
//!
//! Verifies:
//! - `--help` on subcommands shows clean output, not garbled genesis suggestions

/// Test that clap errors carrying a quoted VALUE (invalid value for an
/// option) never trigger the genesis subcommand-suggestion path — the
/// extracted "unknown" is an option value, not a subcommand
/// (testaruda-ixp0).
#[test]
fn invalid_option_value_gets_no_subcommand_suggestion() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--ordering", "sttus"])
        .output()
        .expect("failed to run testaruda select --ordering sttus");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // clap's own invalid-value error must be present
    assert!(
        stderr.contains("invalid value 'sttus'"),
        "clap error should be shown verbatim\nstderr: {}",
        stderr
    );

    // and no garbled genesis suggestion on top
    assert!(
        !stderr.contains("Unknown command"),
        "option value must not be misread as an unknown subcommand\nstderr: {}",
        stderr
    );
    assert!(
        !stderr.contains('💡'),
        "no suggestion emoji for option-value errors\nstderr: {}",
        stderr
    );
}

/// Test that a typo'd subcommand doesn't print BOTH clap's "tip: a similar
/// subcommand exists" and genesis's "💡 Unknown command ..." — de-duplicated
/// suggestion output (testaruda-p1uv).
#[test]
fn no_duplicate_typo_suggestion() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["slect"])
        .output()
        .expect("failed to run testaruda slect");

    let stderr = String::from_utf8_lossy(&output.stderr);

    let has_clap_tip = stderr.contains("tip: a similar subcommand");
    let has_genesis_tip = stderr.contains("💡");

    assert!(
        !(has_clap_tip && has_genesis_tip),
        "clap tip and genesis suggestion must not both print\nstderr: {}",
        stderr
    );

    // Exactly one suggestion source must still fire
    assert!(
        has_clap_tip || has_genesis_tip,
        "a suggestion should still be present\nstderr: {}",
        stderr
    );
}

/// Test that a genuine unknown subcommand still gets a typo suggestion —
/// from clap's tip or genesis's block, whichever wins the de-duplication
/// (testaruda-ixp0, adjusted for testaruda-p1uv).
#[test]
fn unknown_subcommand_still_gets_suggestion() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["slect"])
        .output()
        .expect("failed to run testaruda slect");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("💡") || stderr.contains("tip: a similar subcommand"),
        "unknown subcommand should trigger a suggestion\nstderr: {}",
        stderr
    );
}

/// Test that `testaruda select --help` shows clean help text without garbled
/// genesis suggestion output (testaruda-ll8).
#[test]
fn subcommand_help_is_clean() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["select", "--help"])
        .output()
        .expect("failed to run testaruda select --help");

    assert!(
        output.status.success(),
        "testaruda select --help should exit 0, got: {}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should not contain garbled suggestion text from the typo engine
    assert!(
        !stdout.contains('💡'),
        "Help output should not contain suggestion emoji\nstdout: {}",
        stdout
    );
    assert!(
        !stderr.contains('💡'),
        "Help stderr should not contain suggestion emoji\nstderr: {}",
        stderr
    );
    assert!(
        !stdout.contains("Unknown command"),
        "Help output should not contain 'Unknown command'\nstdout: {}",
        stdout
    );

    // Should contain the expected help header
    assert!(
        stdout.contains("Select affected tests from a code change"),
        "Help output should start with the select command description\nstdout: {}",
        stdout
    );
}

/// Regression test for gh-27 / testaruda-p5zl: multi-line piped stdin must
/// NOT be truncated to the first line. genesis 0.7 used `read_line` (first
/// line only, silent data loss); genesis 0.8 reads the full input and
/// partitions it — first line becomes the title, the rest lands in the
/// Description. This test pins that behavior at the CLI boundary.
#[test]
fn feedback_multiline_stdin_is_not_truncated() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["feedback", "bug", "--dry-run"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn testaruda feedback --dry-run");

    child
        .stdin
        .take()
        .expect("stdin piped")
        .write_all(b"First line is the title\nSecond line with detail\nThird line with more\n")
        .expect("write multi-line stdin");

    let output = child.wait_with_output().expect("wait for feedback");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "dry-run should exit 0\nstderr: {}",
        stderr
    );

    // First line was promoted to the title (not lost, not left in the body)
    assert!(
        stderr.contains(r#"--title "[bug] First line is the title""#),
        "first stdin line must become the issue title\nstderr: {}",
        stderr
    );

    // Remaining lines must survive into the Description body
    assert!(
        stderr.contains("Second line with detail") && stderr.contains("Third line with more"),
        "multi-line description must be preserved\nstderr: {}\nstdout: {}",
        stderr,
        stdout
    );
}

/// `testaruda exec` on a repo with no initialized store must fail cleanly
/// with the standard init diagnostic (same contract as select).
#[test]
fn exec_without_initialized_store_fails_cleanly() {
    let project = tempfile::tempdir().expect("tempdir");
    std::fs::write(project.path().join("README.md"), "x").unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(project.path())
        .output()
        .expect("git init");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["exec"])
        .current_dir(project.path())
        .output()
        .expect("run testaruda exec");

    assert!(
        !output.status.success(),
        "exec without store must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("has not been initialized"),
        "expected init diagnostic\nstderr: {}",
        stderr
    );
}

/// `testaruda exec` on an initialized-but-uncalibrated store (no run history)
/// must print the uncalibrated advisory — even on non-zero selection outcomes
/// like exit 20 (nothing selected), since select process-exits before any
/// post-run check could fire (gh-26 / testaruda-n5b4).
#[test]
fn exec_warns_when_store_is_uncalibrated() {
    let project = tempfile::tempdir().expect("tempdir");
    std::fs::write(project.path().join("README.md"), "x").unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(project.path())
        .output()
        .expect("git init");
    std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["init"])
        .current_dir(project.path())
        .output()
        .expect("testaruda init");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["exec"])
        .current_dir(project.path())
        .output()
        .expect("run testaruda exec");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("uncalibrated"),
        "expected uncalibrated advisory\nstderr: {}",
        stderr
    );
}

/// `testaruda exec` on a cold (uncalibrated) store with a degenerate selection
/// (outcome 10, full-run) must STILL run the selected tests and ingest the
/// results — before the fix, the emitter's exit-on-outcome fired before the
/// CI run, so run_history stayed empty forever and calibration was
/// unreachable (testaruda-1m3i, found in the dulce-de-leche value-prop round).
#[test]
fn exec_on_cold_store_runs_tests_and_ingests() {
    let project = tempfile::tempdir().expect("tempdir");
    let dir = project.path();

    // Minimal Rust crate with one test, committed to git.
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir)
        .output()
        .expect("git init");
    std::fs::write(
        dir.join("Cargo.toml"),
        r#"[package]
name = "exec-fixture"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        "#[cfg(test)]\nmod tests {\n    #[test]\n    fn always_passes() {\n        assert!(true);\n    }\n}\n",
    )
    .unwrap();
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    let git_steps: &[&[&str]] = &[
        &["add", "-A"],
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "init",
        ],
    ];
    for args in git_steps {
        std::process::Command::new("git")
            .args(*args)
            .current_dir(dir)
            .output()
            .expect("git");
    }

    // Touch a non-source file so the changed set resolves no edges →
    // degenerate full-run outcome (exit 10) on the cold store.
    std::fs::write(dir.join("README.md"), "changed\n").unwrap();
    std::process::Command::new("git")
        .args([
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-aqm",
            "docs",
        ])
        .current_dir(dir)
        .output()
        .expect("git commit");

    std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("testaruda init");

    // Prepend the freshly-built binary dir so the engine's adapter spawn
    // (testaruda-adapter-rust) resolves to current code, not a stale installed
    // binary (testaruda-wpil gotcha).
    let target_debug = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug");
    let path_var = std::env::var("PATH").unwrap_or_default();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["exec", "--base", "HEAD~1", "--head", "HEAD"])
        .current_dir(dir)
        .env("PATH", format!("{}:{}", target_debug.display(), path_var))
        .output()
        .expect("run testaruda exec");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("running tests") && stderr.contains("ingesting"),
        "exec must run and ingest even on a degenerate selection\nstderr: {}",
        stderr
    );

    // The feedback loop must actually close: run history non-empty.
    let store = testaruda::Store::open(dir.join(".testaruda")).expect("open store");
    assert!(
        !store.run_history_is_empty().expect("run history check"),
        "exec must ingest run results — run_history is empty"
    );
}
