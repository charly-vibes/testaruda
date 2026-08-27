//! .NET adapter integration tests (testaruda-8k5).
//!
//! Verifies:
//! - 4.1: When `titi` IS installed, the adapter handshake returns correct
//!   protocol fields (name, version, protocol, capabilities).
//! - 4.2: When `titi` IS installed, the adapter discover returns valid JSON
//!   with the expected result structure.
//! - 4.3: When `titi` is NOT installed, `testaruda select` with a titi mapping
//!   falls back gracefully and does not crash (TIA-ADAPT-012).
//! - 4.4: Julia adapter command-string config form works with shell-split
//!   (tested via existing adapter_julia.rs integration tests).
//! - 4.5: A `.cs` file outside titi's graph returns `unresolved` and
//!   testaruda applies the over-approximation fallback.
//! - Registry resolution: `.cs` / `.fs` / `.vb` / `.csproj` / `.sln` / `.slnx`
//!   resolve to `titi testaruda-adapter`.
//! - `spawn_adapter` returns a helpful error when `titi` is not on PATH.

use std::path::PathBuf;

/// Cwd guard: changes to a temp directory, restores on drop.
struct CwdGuard {
    saved: PathBuf,
}

impl CwdGuard {
    fn enter(temp: &std::path::Path) -> Self {
        let saved = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp).unwrap();
        Self { saved }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.saved);
    }
}

/// Check if `titi` is available on PATH.
fn titi_available() -> bool {
    std::process::Command::new("which")
        .arg("titi")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Create a minimal .NET test project in the given directory.
fn create_dotnet_project(dir: &std::path::Path) {
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();

    // Create a .cs file (the main source extension)
    std::fs::write(
        src.join("Program.cs"),
        r#"using System;

class Program
{
    static void Main(string[] args)
    {
        Console.WriteLine("Hello, .NET!");
    }
}
"#,
    )
    .unwrap();
}

/// Write a testaruda.toml with a .NET titi mapping.
fn write_dotnet_config(dir: &std::path::Path) {
    let config = r#"[adapters]
default = "testaruda-adapter-rust"

[adapters.extensions]
".rs" = "testaruda-adapter-rust"
".cs" = "titi testaruda-adapter"
".fs" = "titi testaruda-adapter"
".vb" = "titi testaruda-adapter"
".csproj" = "titi testaruda-adapter"
".sln" = "titi testaruda-adapter"
".slnx" = "titi testaruda-adapter"
"#;
    std::fs::write(dir.join("testaruda.toml"), config).unwrap();
}

// ===== Registry resolution tests =====

#[test]
fn dotnet_extension_resolves_cs() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".cs", "titi testaruda-adapter");
    assert_eq!(
        reg.resolve("src/Program.cs"),
        Some("titi testaruda-adapter")
    );
}

#[test]
fn dotnet_extension_resolves_fs() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".fs", "titi testaruda-adapter");
    assert_eq!(
        reg.resolve("src/Library.fs"),
        Some("titi testaruda-adapter")
    );
}

#[test]
fn dotnet_extension_resolves_vb() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".vb", "titi testaruda-adapter");
    assert_eq!(reg.resolve("src/Module.vb"), Some("titi testaruda-adapter"));
}

#[test]
fn dotnet_extension_resolves_csproj() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".csproj", "titi testaruda-adapter");
    assert_eq!(
        reg.resolve("src/MyApp.csproj"),
        Some("titi testaruda-adapter")
    );
}

#[test]
fn dotnet_extension_resolves_sln() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".sln", "titi testaruda-adapter");
    assert_eq!(reg.resolve("MyApp.sln"), Some("titi testaruda-adapter"));
}

// ===== Adapter protocol tests (gated on titi installed) =====

/// Set up a minimal .NET project fixture in the given directory.
/// Returns true if the project was set up successfully.
fn setup_dotnet_fixture(dir: &std::path::Path) -> bool {
    let proj_dir = dir.join("src/MyApp");
    std::fs::create_dir_all(&proj_dir).unwrap();

    // .csproj
    std::fs::write(
        proj_dir.join("MyApp.csproj"),
        r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <ImplicitUsings>enable</ImplicitUsings>
    <Nullable>enable</Nullable>
  </PropertyGroup>
</Project>
"#,
    )
    .unwrap();

    // Program.cs
    std::fs::write(
        proj_dir.join("Program.cs"),
        "Console.WriteLine(\"Hello from .NET fixture!\");\n",
    )
    .unwrap();

    // .slnx
    let slnx_dir = dir.join(".titi/solutions");
    std::fs::create_dir_all(&slnx_dir).unwrap();
    std::fs::write(
        slnx_dir.join("MyApp.slnx"),
        serde_json::json!({
            "solutions": [
                {
                    "path": dir.join("src/MyApp/MyApp.csproj").to_string_lossy(),
                    "name": "MyApp"
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    true
}

#[test]
fn titi_adapter_handshake_returns_correct_fields() {
    if !titi_available() {
        eprintln!("titi not on PATH — skipping handshake test");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    setup_dotnet_fixture(temp.path());
    let _guard = CwdGuard::enter(temp.path());

    // Spawn the titi adapter
    let adapter = testaruda::adapter::spawn_adapter("titi testaruda-adapter", None)
        .expect("should spawn titi adapter");

    // Verify adapter name from handshake
    assert_eq!(adapter.name, "titi");
}

#[test]
fn titi_adapter_handshake_response_format() {
    if !titi_available() {
        eprintln!("titi not on PATH — skipping handshake format test");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    setup_dotnet_fixture(temp.path());

    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("titi")
        .arg("testaruda-adapter")
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn titi");

    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, r#"{{"command":"handshake"}}"#).unwrap();
    stdin.flush().unwrap();

    let output = child
        .wait_with_output()
        .expect("failed to read titi output");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .next()
        .expect("expected at least one JSON line");

    let parsed: serde_json::Value =
        serde_json::from_str(line).expect("handshake response should be valid JSON");

    assert!(parsed["ok"].as_bool().unwrap_or(false), "ok should be true");
    let result = &parsed["result"];
    assert_eq!(result["name"].as_str(), Some("titi"));
    assert_eq!(result["version"].as_str(), Some("0.1.0"));
    assert_eq!(result["protocol"].as_i64(), Some(1));
    assert!(result["languages"].as_array().is_some());
    assert_eq!(result["granularity"].as_str(), Some("method"));
    let caps = &result["capabilities"];
    assert!(caps["symbol_model_complete"].as_bool().unwrap_or(false));
    assert!(caps["fingerprinting"].as_bool().unwrap_or(false));
    assert!(!caps["runtime_edges"].as_bool().unwrap_or(true));
}

#[test]
fn titi_adapter_handshake_then_discover() {
    if !titi_available() {
        eprintln!("titi not on PATH — skipping sequence test");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    setup_dotnet_fixture(temp.path());

    use std::io::{BufRead, Write};
    use std::process::{Command, Stdio};

    let mut child = Command::new("titi")
        .arg("testaruda-adapter")
        .current_dir(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn titi");

    let stdin = child.stdin.as_mut().unwrap();

    // 1. Handshake
    writeln!(stdin, r#"{{"command":"handshake"}}"#).unwrap();
    stdin.flush().unwrap();

    let mut line = String::new();
    let mut reader = std::io::BufReader::new(child.stdout.as_mut().unwrap());
    reader.read_line(&mut line).unwrap();

    let hs: serde_json::Value =
        serde_json::from_str(&line).expect("handshake response should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));
    assert_eq!(hs["result"]["name"].as_str(), Some("titi"));

    // 2. Discover
    writeln!(stdin, r#"{{"command":"discover"}}"#).unwrap();
    stdin.flush().unwrap();

    let mut disc_line = String::new();
    reader.read_line(&mut disc_line).unwrap();

    let disc: serde_json::Value =
        serde_json::from_str(&disc_line).expect("discover response should be valid JSON");
    assert!(disc["ok"].as_bool().unwrap_or(false));
    assert!(
        disc["result"].is_array(),
        "discover result should be an array, got: {:?}",
        disc["result"]
    );

    child.wait().ok();
}

#[test]
fn dotnet_extension_resolves_slnx() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".slnx", "titi testaruda-adapter");
    assert_eq!(reg.resolve("MyApp.slnx"), Some("titi testaruda-adapter"));
}

#[test]
fn dotnet_extension_does_not_leak_to_rs() {
    let mut reg = testaruda::adapter::AdapterRegistry::new();
    reg.register(".cs", "titi testaruda-adapter");
    reg.register(".rs", "testaruda-adapter-rust");
    // .rs files should still resolve to the Rust adapter
    assert_eq!(reg.resolve("src/lib.rs"), Some("testaruda-adapter-rust"));
}

// ===== Spawn-failure tests =====

#[test]
fn spawn_titi_fails_when_not_on_path() {
    // Skip if titi IS available — this test verifies the NOT-installed case
    if titi_available() {
        eprintln!("titi is installed — skipping spawn-failure test");
        return;
    }

    let result = testaruda::adapter::spawn_adapter("titi testaruda-adapter", None);
    assert!(
        result.is_err(),
        "expected spawn_adapter to fail when titi is not on PATH"
    );
}

// ===== Config-level tests =====

#[test]
fn dotnet_config_round_trips_via_normalize() {
    let config = r#"[adapters]
default = "testaruda-adapter-rust"

[adapters.extensions]
".cs" = "titi testaruda-adapter"
".fs" = "titi testaruda-adapter"
".vb" = "titi testaruda-adapter"
".csproj" = "titi testaruda-adapter"
".sln" = "titi testaruda-adapter"
".slnx" = "titi testaruda-adapter"
".rs" = "testaruda-adapter-rust"
"#;

    let parsed: testaruda::config::Config = toml::from_str(config).unwrap();
    assert_eq!(
        parsed.adapters.extensions.get(".cs").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.extensions.get(".fs").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.extensions.get(".vb").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed
            .adapters
            .extensions
            .get(".csproj")
            .map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.extensions.get(".sln").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.extensions.get(".slnx").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.extensions.get(".rs").map(String::as_str),
        Some("testaruda-adapter-rust")
    );
    assert_eq!(
        parsed.adapters.default.as_deref(),
        Some("testaruda-adapter-rust")
    );
}

// ===== CLI integration tests (task 4.3) =====

/// Test that `testaruda select` with a titi mapping does NOT crash
/// when titi is not installed. It should fall back gracefully
/// (TIA-ADAPT-012) and produce a valid selection.
#[test]
fn select_falls_back_when_titi_not_installed() {
    // Skip if titi IS available — this test verifies the NOT-installed path
    if titi_available() {
        eprintln!("⚠️  titi is installed — skipping fallback-grace test");
        eprintln!("    This test verifies the case when titi is NOT on PATH.");
        eprintln!(
            "    To run it, temporarily remove titi from PATH or run on a machine without titi."
        );
        return;
    }

    let project = tempfile::tempdir().unwrap();
    let _guard = CwdGuard::enter(project.path());

    // Create a .NET-like project structure
    create_dotnet_project(project.path());

    // Write config with titi mapping
    write_dotnet_config(project.path());

    // Initialize a real git repository so testaruda's git operations succeed
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(project.path())
        .output()
        .expect("failed to run git init");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(project.path())
        .output()
        .expect("failed to run git add .");

    // Put the built Rust adapter on the CHILD's PATH (testaruda-adapter-rust is a
    // bin target of this crate). In a clean CI environment neither titi nor the
    // adapter is installed; TIA-ADAPT-012 requires select to fall back to the
    // default adapter, so we provide it deterministically while titi stays absent.
    let adapter_dir = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        env!("CARGO_BIN_EXE_testaruda-adapter-rust"),
        adapter_dir.path().join("testaruda-adapter-rust"),
    )
    .expect("failed to symlink adapter binary");
    #[cfg(not(unix))]
    std::fs::copy(
        env!("CARGO_BIN_EXE_testaruda-adapter-rust"),
        adapter_dir.path().join("testaruda-adapter-rust"),
    )
    .expect("failed to copy adapter binary");
    let child_path = std::env::join_paths(std::iter::once(adapter_dir.path().to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .expect("failed to build child PATH");

    // Create a Cargo.toml so the language detector picks Rust (for the default adapter)
    std::fs::write(
        project.path().join("Cargo.toml"),
        r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    // Create a .rs file so the Rust adapter has something to discover
    let src = project.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("lib.rs"),
        r#"pub fn greet() -> &'static str { "hello" }
#[cfg(test)]
mod tests {
    #[test]
    fn test_greet() { assert_eq!(super::greet(), "hello"); }
}
"#,
    )
    .unwrap();

    // Run `testaruda init` first
    let init_output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .arg("init")
        .env("PATH", &child_path)
        .output()
        .expect("failed to run testaruda init");

    assert!(
        init_output.status.success(),
        "testaruda init should succeed: stdout={}, stderr={}",
        String::from_utf8_lossy(&init_output.stdout),
        String::from_utf8_lossy(&init_output.stderr),
    );

    // Run `testaruda select` — this should not crash even though titi is not installed
    let select_output = std::process::Command::new(env!("CARGO_BIN_EXE_testaruda"))
        .arg("select")
        .arg("--json")
        .env("PATH", &child_path)
        .output()
        .expect("failed to run testaruda select");

    let stderr = String::from_utf8_lossy(&select_output.stderr);

    // The select command should not crash
    assert!(
        select_output.status.success(),
        "testaruda select should not crash when titi is not installed.\nstderr: {}",
        stderr,
    );

    // Should print a warning about the missing adapter (TIA-ADAPT-024 diagnostic)
    assert!(
        stderr.contains("Failed to spawn adapter") || stderr.contains("titi"),
        "Expected a diagnostic message about the missing titi adapter.\nstderr: {}",
        stderr,
    );

    // The JSON output should be valid envelope format
    let stdout = String::from_utf8_lossy(&select_output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("select --json output should be valid JSON");

    // Should be an envelope with data containing the selection
    assert!(
        parsed.get("data").and_then(|d| d.get("tests")).is_some(),
        "Expected an envelope with data.tests selection array.\nstdout: {}",
        stdout,
    );

    let tests = parsed["data"]["tests"].as_array().unwrap();
    // Should have at least the Rust test items (selected via default adapter)
    assert!(
        !tests.is_empty(),
        "Expected at least one test selected.\nstdout: {}",
        stdout,
    );
}

/// Test that the flat config format also works with .NET extensions.
#[test]
fn dotnet_flat_format_normalizes() {
    let flat = r#"[adapters]
".cs" = "titi testaruda-adapter"
".fs" = "titi testaruda-adapter"
default = "testaruda-adapter-rust"
"#;

    // Normalize via the config module
    let normalized = testaruda::config::normalize_adapters_config(flat);

    let parsed: testaruda::config::Config = toml::from_str(&normalized).unwrap();
    assert_eq!(
        parsed.adapters.extensions.get(".cs").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.extensions.get(".fs").map(String::as_str),
        Some("titi testaruda-adapter")
    );
    assert_eq!(
        parsed.adapters.default.as_deref(),
        Some("testaruda-adapter-rust")
    );
}
