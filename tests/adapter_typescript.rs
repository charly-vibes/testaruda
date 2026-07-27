//! TypeScript adapter scaffolding tests (testaruda-xq1).
//!
//! Verifies the adapter-typescript crate compiles and the binary is on PATH.

use std::io::{BufRead, Write};
use std::process::Command;

/// Check if the TypeScript adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-typescript")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The adapter-typescript crate directory must exist with expected structure.
#[test]
fn adapter_typescript_crate_exists() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = manifest_dir.join("adapter-typescript");

    assert!(
        crate_dir.exists(),
        "adapter-typescript/ directory not found"
    );

    let cargo_toml = crate_dir.join("Cargo.toml");
    assert!(
        cargo_toml.exists(),
        "adapter-typescript/Cargo.toml not found"
    );

    let main_rs = crate_dir.join("src/main.rs");
    assert!(main_rs.exists(), "adapter-typescript/src/main.rs not found");

    let queries_dir = crate_dir.join("queries");
    assert!(
        queries_dir.exists(),
        "adapter-typescript/queries/ not found"
    );

    // Verify workspace membership
    let root_toml =
        std::fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("root Cargo.toml readable");
    assert!(
        root_toml.contains("adapter-typescript"),
        "adapter-typescript not in workspace members"
    );
}

/// The adapter must respond to a handshake with valid JSON.
#[test]
fn adapter_typescript_handshake() {
    if !adapter_available() {
        eprintln!("testaruda-adapter-typescript not on PATH — skipping");
        return;
    }

    let mut child = Command::new("testaruda-adapter-typescript")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to spawn TypeScript adapter");

    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, r#"{{"command":"handshake"}}"#).unwrap();
    stdin.flush().unwrap();

    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = std::io::BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let resp = line.trim().to_string();

    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "handshake should succeed: {}",
        resp
    );

    let result = &parsed["result"];
    assert_eq!(
        result["name"], "typescript-adapter",
        "adapter name mismatch"
    );
    assert_eq!(result["protocol"], 1, "protocol version mismatch");
    assert!(
        result["languages"]
            .as_array()
            .map(|a| a.contains(&serde_json::json!("typescript")))
            .unwrap_or(false),
        "should declare typescript language: {}",
        resp
    );
    assert_eq!(result["granularity"], "file", "should be file-level");

    // Clean up: wait for child to avoid zombie process
    let _ = child.wait();
}
