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
