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

/// Test that a genuine unknown subcommand still gets the genesis typo
/// suggestion (existing good behavior, kept intact by the ixp0 fix).
#[test]
fn unknown_subcommand_still_gets_suggestion() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .args(["slect"])
        .output()
        .expect("failed to run testaruda slect");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        stderr.contains("💡") || stderr.contains("Did you mean"),
        "unknown subcommand should trigger the suggestion engine\nstderr: {}",
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
