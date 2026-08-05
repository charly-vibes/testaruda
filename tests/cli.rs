//! CLI integration tests.
//!
//! Verifies:
//! - `--help` on subcommands shows clean output, not garbled genesis suggestions

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
