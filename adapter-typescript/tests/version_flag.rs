//! `--version` regression (testaruda-7r9l).
//!
//! The adapter used to ignore argv entirely: `--version` exited 0 printing
//! nothing, so a consumer's install guard could only test binary presence,
//! never the version. Tambor CI run 38632309310 then silently ran a stale
//! cached v0.5.1-era adapter against a v0.5.5 pin — its discovery chained
//! sequential top-level tests combinatorially, the store filled with
//! polluted `file::t1::t2::t3` ids, and junit ingest resolved only 60/333.
//! The guard needs a real version line to detect that drift.
use std::process::Command;

#[test]
fn version_flag_prints_adapter_name_and_crate_version() {
    let exe = env!("CARGO_BIN_EXE_testaruda-adapter-typescript");
    let output = match Command::new(exe).arg("--version").output() {
        Ok(o) => o,
        Err(e) => panic!("failed to spawn {exe}: {e}"),
    };
    assert!(output.status.success(), "--version must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.contains("testaruda-adapter-typescript"),
        "--version output must name the adapter (consumer install guards match on it): {stdout:?}"
    );
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "--version output must carry the crate version so a stale cached binary is detectable: {stdout:?}"
    );
}

#[test]
fn version_flag_short_alias_prints_the_same_line() {
    let exe = env!("CARGO_BIN_EXE_testaruda-adapter-typescript");
    let output = match Command::new(exe).arg("-V").output() {
        Ok(o) => o,
        Err(e) => panic!("failed to spawn {exe}: {e}"),
    };
    assert!(output.status.success(), "-V must exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        stdout.contains("testaruda-adapter-typescript")
            && stdout.contains(env!("CARGO_PKG_VERSION")),
        "-V output must equal the --version line: {stdout:?}"
    );
}

#[test]
fn no_version_flag_starts_the_stdio_loop_silently() {
    // Without the flag the adapter must stay in stdio command mode: an empty
    // stdin closes the loop and the process exits 0 without printing a
    // version line (the pre-7r9l behavior must not become the default).
    let exe = env!("CARGO_BIN_EXE_testaruda-adapter-typescript");
    let output = match Command::new(exe)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(o) => o,
        Err(e) => panic!("failed to spawn {exe}: {e}"),
    };
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "bare invocation must not print a version line"
    );
}
