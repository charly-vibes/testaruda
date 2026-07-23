//! Julia adapter integration tests.
//!
//! Verifies the Julia adapter protocol via subprocess spawning.
//! Tests are conditionally skipped when Julia or the adapter binary
//! is not available on PATH.
//!
//! The adapter lives in Testimonial.jl as a separate package — these
//! tests verify the wire protocol from testaruda's side, not the
//! adapter's internal logic (which is tested in Testimonial.jl's own
//! test suite).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Check if the Julia adapter binary is available on PATH.
fn adapter_available() -> bool {
    Command::new("which")
        .arg("testaruda-adapter-julia")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if Julia itself is available.
fn julia_available() -> bool {
    Command::new("julia")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Send a JSON command to an adapter subprocess and return the response line.
fn send_command(child: &mut std::process::Child, cmd: &str) -> String {
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{}", cmd).unwrap();
    stdin.flush().unwrap();

    let stdout = child.stdout.as_mut().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    line.trim().to_string()
}

#[test]
fn julia_adapter_handshake() {
    if !julia_available() {
        eprintln!("Julia not available — skipping Julia adapter test");
        return;
    }
    if !adapter_available() {
        eprintln!("testaruda-adapter-julia not on PATH — skipping Julia adapter test");
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "handshake should succeed"
    );
    let result = &parsed["result"];
    assert_eq!(result["name"], "testimonial-adapter");
    assert_eq!(result["protocol"], 1);
    assert_eq!(result["languages"], serde_json::json!(["julia"]));
    assert_eq!(result["granularity"], "file");
    assert!(result["capabilities"]["fingerprinting"]
        .as_bool()
        .unwrap_or(false));
    assert!(result["capabilities"]["runtime_edges"]
        .as_bool()
        .unwrap_or(false));
    assert!(!result["capabilities"]["symbol_model_complete"]
        .as_bool()
        .unwrap_or(true));

    // Clean shutdown
    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_discover() {
    if !julia_available() || !adapter_available() {
        return;
    }

    // Create a temp dir with a test file
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let test_file = dir.path().join("test_foo.jl");
    std::fs::write(&test_file, r#"@testitem "my_test" begin @test 1==1 end"#)
        .expect("failed to write test file");

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        dir.path().display()
    );
    let resp = send_command(&mut child, &cmd);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "discover should succeed"
    );
    let nodes = parsed["result"]
        .as_array()
        .expect("result should be an array");
    assert_eq!(nodes.len(), 1, "should discover one @testitem");

    let node = &nodes[0];
    assert_eq!(node["suite_kind"], "ReTestItems.jl");
    let abs_file = std::fs::canonicalize(&test_file).unwrap();
    assert_eq!(node["file"], abs_file.to_string_lossy().as_ref());
    assert!(
        node["node_id"]
            .as_str()
            .unwrap_or("")
            .contains("test_foo.jl:"),
        "node_id should contain file:line format"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_fingerprint() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let test_file = dir.path().join("test.jl");
    std::fs::write(&test_file, "hello world").expect("failed to write test file");

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let cmd = format!(
        r#"{{"command":"fingerprint","params":{{"files":["{}"]}}}}"#,
        test_file.display()
    );
    let resp = send_command(&mut child, &cmd);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("fingerprint response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "fingerprint should succeed"
    );
    let fps = parsed["result"]["fingerprints"].as_array().unwrap();
    assert_eq!(fps.len(), 1);
    assert_eq!(fps[0]["file"], test_file.to_string_lossy().as_ref());
    // SHA-256 produces 64 hex chars
    let fp = fps[0]["fingerprint"].as_str().unwrap();
    assert_eq!(fp.len(), 64, "SHA-256 fingerprint should be 64 hex chars");
    assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_run_args() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(
        &mut child,
        r#"{"command":"run-args","params":{"selected":["test_a.jl:item_1"]}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("run-args response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "run-args should succeed"
    );
    let result = &parsed["result"];
    assert!(
        result["runner_args"].is_array(),
        "runner_args should be an array"
    );
    assert!(
        result["collection_path"].is_string(),
        "collection_path should be a string"
    );

    // First arg should be "julia"
    let args = result["runner_args"].as_array().unwrap();
    assert_eq!(args[0], "julia", "runner should be julia");
    // Should contain ReTestItems reference
    let expr = args[3].as_str().unwrap_or("");
    assert!(
        expr.contains("ReTestItems"),
        "should call ReTestItems.runtests"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_static_deps_unresolved() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(
        &mut child,
        r#"{"command":"static-deps","params":{"changed_files":["src/foo.jl"]}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "static-deps should succeed"
    );
    let edges = &parsed["result"]["edges"];
    assert_eq!(
        edges["src/foo.jl"], "unresolved",
        "without prior ingest, changed files should be unresolved"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_unknown_command() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter");

    let resp = send_command(&mut child, r#"{"command":"bogus"}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("response should be valid JSON");

    assert!(
        !parsed["ok"].as_bool().unwrap_or(true),
        "unknown command should fail"
    );
    assert!(parsed["error"]["message"]
        .as_str()
        .unwrap_or("")
        .contains("unknown command"));

    child.kill().ok();
    child.wait().ok();
}

// ============================================================================
// Multi-package monorepo tests
// ============================================================================

/// Shared helper: discover a single package's test directory.
fn assert_monorepo_discover(package_subdir: &str, expected_count: usize) {
    let fix = monorepo_fixture_path();
    let test_dir = fix.join("packages").join(package_subdir).join("test");

    // If the package has no test directory, verify the path doesn't exist and return 0
    if expected_count == 0 && !test_dir.exists() {
        return;
    }

    let mut child = spawn_adapter();

    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let hs: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));

    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        test_dir.display()
    );
    let resp = send_command(&mut child, &cmd);
    let disc: serde_json::Value =
        serde_json::from_str(&resp).expect("discover should be valid JSON");
    assert!(disc["ok"].as_bool().unwrap_or(false));
    let nodes = disc["result"].as_array().unwrap();
    assert_eq!(
        nodes.len(),
        expected_count,
        "package {} should have {} @testitems",
        package_subdir,
        expected_count
    );

    for node in nodes {
        let file = node["file"].as_str().unwrap();
        assert!(
            file.contains(package_subdir),
            "discovered file should be in {}: {}",
            package_subdir,
            file
        );
        assert_eq!(node["suite_kind"], "ReTestItems.jl");
    }

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_monorepo_discover_per_package_a() {
    if !julia_available() || !adapter_available() {
        return;
    }
    assert_monorepo_discover("pkg_a", 3);
}

#[test]
fn julia_adapter_monorepo_discover_per_package_b() {
    if !julia_available() || !adapter_available() {
        return;
    }
    assert_monorepo_discover("pkg_b", 2);
}

#[test]
fn julia_adapter_monorepo_discover_per_package_c_no_tests() {
    if !julia_available() || !adapter_available() {
        return;
    }
    // pkg_c has a Project.toml but no test/ directory — verifies empty-package handling
    assert_monorepo_discover("pkg_c", 0);
}

#[test]
/// Per-package invocation model: discover each package's test directory in
/// separate adapter processes, then verify the combined total. This follows
/// Decision 5 — per-package invocation (Option 2), not a single multi-package scan.
fn julia_adapter_monorepo_separate_discover_for_each_package() {
    if !julia_available() || !adapter_available() {
        return;
    }

    // Discover pkg_a — has 3 @testitems (2 in test/runtests.jl, 1 in test/sub/deep_test.jl)
    let fix = monorepo_fixture_path();
    let pkg_a_test = fix.join("packages").join("pkg_a").join("test");

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let hs: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));

    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        pkg_a_test.display()
    );
    let resp = send_command(&mut child, &cmd);
    let disc: serde_json::Value =
        serde_json::from_str(&resp).expect("discover should be valid JSON");
    assert!(disc["ok"].as_bool().unwrap_or(false));
    let pkg_a_nodes = disc["result"].as_array().unwrap();
    assert_eq!(
        pkg_a_nodes.len(),
        3,
        "pkg_a should have 3 @testitems (2 root + 1 nested)"
    );
    child.kill().ok();
    child.wait().ok();

    // Discover pkg_b — has 2 @testitems
    let pkg_b_test = fix.join("packages").join("pkg_b").join("test");

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let hs: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));

    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        pkg_b_test.display()
    );
    let resp = send_command(&mut child, &cmd);
    let disc: serde_json::Value =
        serde_json::from_str(&resp).expect("discover should be valid JSON");
    assert!(disc["ok"].as_bool().unwrap_or(false));
    let pkg_b_nodes = disc["result"].as_array().unwrap();
    assert_eq!(pkg_b_nodes.len(), 2, "pkg_b should have 2 @testitems");
    child.kill().ok();
    child.wait().ok();

    // Verify disjoint node_ids across packages
    let pkg_a_ids: Vec<&str> = pkg_a_nodes
        .iter()
        .map(|n| n["node_id"].as_str().unwrap())
        .collect();
    let pkg_b_ids: Vec<&str> = pkg_b_nodes
        .iter()
        .map(|n| n["node_id"].as_str().unwrap())
        .collect();
    for a_id in &pkg_a_ids {
        assert!(
            !pkg_b_ids.contains(a_id),
            "node_id {} should not appear in both packages",
            a_id
        );
    }
}

#[test]
fn julia_adapter_monorepo_fingerprint() {
    // Per-package: fingerprint src files from each package.
    if !julia_available() || !adapter_available() {
        return;
    }

    let fix = monorepo_fixture_path();
    let src_a = fix
        .join("packages")
        .join("pkg_a")
        .join("src")
        .join("PkgA.jl");
    let src_b = fix
        .join("packages")
        .join("pkg_b")
        .join("src")
        .join("PkgB.jl");

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let hs: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));

    // Fingerprint both files
    let cmd = format!(
        r#"{{"command":"fingerprint","params":{{"files":["{}","{}"]}}}}"#,
        src_a.display(),
        src_b.display()
    );
    let resp = send_command(&mut child, &cmd);
    let fp: serde_json::Value =
        serde_json::from_str(&resp).expect("fingerprint should be valid JSON");
    assert!(fp["ok"].as_bool().unwrap_or(false));
    let fingerprints = fp["result"]["fingerprints"].as_array().unwrap();
    assert_eq!(fingerprints.len(), 2, "should fingerprint 2 files");

    for f in fingerprints {
        let fp_str = f["fingerprint"].as_str().unwrap();
        assert_eq!(
            fp_str.len(),
            64,
            "SHA-256 fingerprint should be 64 hex chars"
        );
        assert!(fp_str.chars().all(|c| c.is_ascii_hexdigit()));
    }

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_monorepo_static_deps_cross_package() {
    // Per-package static-deps: verify cross-package source files return
    // unresolved when there is no prior ingest. Also verify that sending
    // both files in a single call produces expected keys for each.
    if !julia_available() || !adapter_available() {
        return;
    }

    let fix = monorepo_fixture_path();
    let src_a = fix
        .join("packages")
        .join("pkg_a")
        .join("src")
        .join("PkgA.jl");
    let src_b = fix
        .join("packages")
        .join("pkg_b")
        .join("src")
        .join("PkgB.jl");

    let mut child = spawn_adapter();

    // Handshake
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let hs: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));

    // Static-deps on pkg_a's source (single file)
    let cmd = format!(
        r#"{{"command":"static-deps","params":{{"changed_files":["{}"]}}}}"#,
        src_a.display()
    );
    let resp = send_command(&mut child, &cmd);
    let sd_a: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps should be valid JSON");
    assert!(sd_a["ok"].as_bool().unwrap_or(false));
    let edges_a = &sd_a["result"]["edges"];
    // Compare by filename suffix to avoid canonicalize fragility (EDGE-004)
    let src_a_str = src_a.to_string_lossy();
    assert!(
        edges_a
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.ends_with("PkgA.jl")),
        "static-deps result should contain pkg_a's src file: {}",
        src_a_str
    );
    // The value for src_a should be "unresolved"
    for (key, val) in edges_a.as_object().unwrap() {
        if key.ends_with("PkgA.jl") {
            assert_eq!(
                val, "unresolved",
                "without prior ingest, changed files should be unresolved"
            );
        }
    }

    // Static-deps on pkg_b's source (single file)
    let cmd = format!(
        r#"{{"command":"static-deps","params":{{"changed_files":["{}"]}}}}"#,
        src_b.display()
    );
    let resp = send_command(&mut child, &cmd);
    let sd_b: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps should be valid JSON");
    assert!(sd_b["ok"].as_bool().unwrap_or(false));
    let edges_b = &sd_b["result"]["edges"];
    let src_b_str = src_b.to_string_lossy();
    assert!(
        edges_b
            .as_object()
            .unwrap()
            .keys()
            .any(|k| k.ends_with("PkgB.jl")),
        "static-deps result should contain pkg_b's src file: {}",
        src_b_str
    );
    for (key, val) in edges_b.as_object().unwrap() {
        if key.ends_with("PkgB.jl") {
            assert_eq!(
                val, "unresolved",
                "without prior ingest, changed files should be unresolved"
            );
        }
    }

    // Combined static-deps: both files in a single call (EDGE-003)
    let cmd = format!(
        r#"{{"command":"static-deps","params":{{"changed_files":["{}","{}"]}}}}"#,
        src_a.display(),
        src_b.display()
    );
    let resp = send_command(&mut child, &cmd);
    let sd_both: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps should be valid JSON");
    assert!(sd_both["ok"].as_bool().unwrap_or(false));
    let edges_both = sd_both["result"]["edges"].as_object().unwrap();
    let has_pkg_a = edges_both.keys().any(|k| k.ends_with("PkgA.jl"));
    let has_pkg_b = edges_both.keys().any(|k| k.ends_with("PkgB.jl"));
    assert!(
        has_pkg_a && has_pkg_b,
        "combined static-deps should contain both src files"
    );

    child.kill().ok();
    child.wait().ok();
}

// ============================================================================

/// Path to the Julia fixture project (relative to crate root).
fn fixture_path() -> std::path::PathBuf {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_root.join("tests").join("fixtures").join("julia")
}

/// Path to the Julia monorepo fixture (relative to crate root).
fn monorepo_fixture_path() -> std::path::PathBuf {
    let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_root
        .join("tests")
        .join("fixtures")
        .join("julia-monorepo")
}

/// Spawn the Julia adapter subprocess.
fn spawn_adapter() -> std::process::Child {
    Command::new("testaruda-adapter-julia")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn Julia adapter")
}

#[test]
fn julia_adapter_full_pipeline() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let fix = fixture_path();
    let test_dir = fix.join("test");
    let src_file = fix.join("src").join("MyFixture.jl");

    let mut child = spawn_adapter();

    // 1. Handshake
    let resp = send_command(&mut child, r#"{"command":"handshake"}"#);
    let hs: serde_json::Value =
        serde_json::from_str(&resp).expect("handshake should be valid JSON");
    assert!(hs["ok"].as_bool().unwrap_or(false));
    assert_eq!(hs["result"]["name"], "testimonial-adapter");
    assert_eq!(hs["result"]["languages"], serde_json::json!(["julia"]));

    // 2. Discover
    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        test_dir.display()
    );
    let resp = send_command(&mut child, &cmd);
    let disc: serde_json::Value =
        serde_json::from_str(&resp).expect("discover should be valid JSON");
    assert!(disc["ok"].as_bool().unwrap_or(false));
    let nodes = disc["result"].as_array().unwrap();
    assert_eq!(nodes.len(), 3, "should discover 3 @testitems");

    // Verify node IDs are file:line format
    for node in nodes {
        assert_eq!(node["suite_kind"], "ReTestItems.jl");
        let node_id = node["node_id"].as_str().unwrap();
        assert!(node_id.contains(":"), "node_id should contain :");
    }

    // 3. Fingerprint
    let cmd = format!(
        r#"{{"command":"fingerprint","params":{{"files":["{}"]}}}}"#,
        src_file.display()
    );
    let resp = send_command(&mut child, &cmd);
    let fp: serde_json::Value =
        serde_json::from_str(&resp).expect("fingerprint should be valid JSON");
    assert!(fp["ok"].as_bool().unwrap_or(false));
    let fingerprints = fp["result"]["fingerprints"].as_array().unwrap();
    assert_eq!(fingerprints.len(), 1);
    assert_eq!(fingerprints[0]["fingerprint"].as_str().unwrap().len(), 64);

    // 4. Static-deps (no prior coverage) — all unresolved
    let cmd = format!(
        r#"{{"command":"static-deps","params":{{"changed_files":["{}"]}}}}"#,
        src_file.display()
    );
    let resp = send_command(&mut child, &cmd);
    let sd: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps should be valid JSON");
    assert!(sd["ok"].as_bool().unwrap_or(false));
    let sd_edges = &sd["result"]["edges"];
    // The key is the absolute path, and it should be "unresolved"
    let abs_src = std::fs::canonicalize(&src_file).unwrap();
    let abs_src_str = abs_src.to_string_lossy();
    assert_eq!(
        sd_edges[abs_src_str.as_ref()],
        "unresolved",
        "without prior ingest, changed files should be unresolved"
    );

    // 5. Ingest via run_output (TIA-ADAPT-008)
    // The adapter parses run_output JSON lines and attempts to record
    // real coverage for each item. If coverage recording fails (e.g.
    // no Julia subprocess runner available in this context), per-test
    // results are still returned but runtime_edges may be empty.
    let discovered_ids: Vec<String> = nodes
        .iter()
        .map(|n| n["node_id"].as_str().unwrap().to_string())
        .collect();
    let run_output_lines: Vec<String> = discovered_ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"test_id":"{}","outcome":"passed","duration_ms":10}}"#,
                id
            )
        })
        .collect();
    let run_output = run_output_lines.join("\n");

    let ingest_cmd = serde_json::json!({
        "command": "ingest",
        "params": {
            "run_output": run_output
        }
    });
    let resp = send_command(&mut child, &ingest_cmd.to_string());
    let ingest: serde_json::Value =
        serde_json::from_str(&resp).expect("ingest should be valid JSON");
    assert!(ingest["ok"].as_bool().unwrap_or(false));
    assert!(
        ingest["result"]["runtime_edges"].is_array(),
        "runtime_edges should be an array"
    );
    assert!(
        ingest["result"]["per_test_results"].is_array(),
        "per_test_results should be an array"
    );
    assert_eq!(
        ingest["result"]["per_test_results"]
            .as_array()
            .unwrap()
            .len(),
        3,
        "should have 3 per-test results"
    );
    // Note: runtime_edges may be empty or partial here because real
    // coverage recording requires the TestimonialRunner subprocess.
    // Full coverage integration is tested in Testimonial.jl's own suite.

    // 6. Run-args — should produce a valid Julia invocation
    let selected = vec![discovered_ids[0].clone()];
    let run_args_cmd = serde_json::json!({
        "command": "run-args",
        "params": {
            "selected": selected
        }
    });
    let resp = send_command(&mut child, &run_args_cmd.to_string());
    let ra: serde_json::Value = serde_json::from_str(&resp).expect("run-args should be valid JSON");
    assert!(ra["ok"].as_bool().unwrap_or(false));
    let runner_args = ra["result"]["runner_args"].as_array().unwrap();
    assert_eq!(runner_args[0], "julia");
    let expr = runner_args[3].as_str().unwrap_or("");
    assert!(
        expr.contains("ReTestItems"),
        "run-args should emit ReTestItems.runtests invocation"
    );

    child.kill().ok();
    child.wait().ok();
}

// ============================================================================
// Error cases
// ============================================================================

#[test]
fn julia_adapter_malformed_json() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, "not valid json at all");
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("response should be valid JSON");

    assert!(
        !parsed["ok"].as_bool().unwrap_or(true),
        "malformed JSON should fail"
    );
    let err = parsed["error"].to_string();
    assert!(
        err.to_lowercase().contains("malformed json")
            || err.to_lowercase().contains("json parsing error"),
        "error should mention JSON parsing: {}",
        err
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_discover_empty_dir() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let dir = tempfile::tempdir().expect("failed to create temp dir");

    let mut child = spawn_adapter();
    let cmd = format!(
        r#"{{"command":"discover","params":{{"test_directories":["{}"]}}}}"#,
        dir.path().display()
    );
    let resp = send_command(&mut child, &cmd);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("discover response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "discover should succeed on empty dir"
    );
    let nodes = parsed["result"]
        .as_array()
        .expect("result should be an array");
    assert!(nodes.is_empty(), "empty dir should have no tests");

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_static_deps_no_changed_files() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"static-deps","params":{}}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

    assert!(
        !parsed["ok"].as_bool().unwrap_or(true),
        "missing changed_files should fail"
    );
    let err = parsed["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        err.contains("changed_files") || err.contains("missing"),
        "error should mention missing changed_files: {}",
        err
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_static_deps_multiple_files() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(
        &mut child,
        r#"{"command":"static-deps","params":{"changed_files":["src/a.jl","src/b.jl"]}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("static-deps response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "static-deps with multiple files should succeed"
    );
    let edges = &parsed["result"]["edges"];
    assert!(
        edges["src/a.jl"] == "unresolved" || edges["src/a.jl"] == serde_json::json!([]),
        "multiple changed files should each be handled: {:?}",
        edges
    );
    assert!(
        edges["src/b.jl"] == "unresolved" || edges["src/b.jl"] == serde_json::json!([]),
        "multiple changed files should each be handled: {:?}",
        edges
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_ingest_empty_output() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(
        &mut child,
        r#"{"command":"ingest","params":{"run_output":""}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("ingest response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "ingest with empty output should succeed with empty results"
    );
    assert!(
        parsed["result"]["per_test_results"]
            .as_array()
            .unwrap()
            .is_empty(),
        "empty run_output should produce no results"
    );
    assert!(
        parsed["result"]["runtime_edges"]
            .as_array()
            .unwrap()
            .is_empty(),
        "empty run_output should produce no runtime edges"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_ingest_mixed_outcomes() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let run_output = r#"{"test_id":"test_a.jl:1","outcome":"passed","duration_ms":5}
{"test_id":"test_b.jl:10","outcome":"failed","duration_ms":20}
{"test_id":"test_c.jl:20","outcome":"passed","duration_ms":3}"#;
    let cmd = serde_json::json!({
        "command": "ingest",
        "params": {
            "run_output": run_output
        }
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("ingest response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "ingest with valid output should succeed"
    );

    let results = parsed["result"]["per_test_results"].as_array().unwrap();
    assert_eq!(results.len(), 3, "should have 3 per-test results");

    // Find each outcome
    let passed: Vec<&str> = results
        .iter()
        .filter(|r| r["outcome"].as_str() == Some("passed"))
        .filter_map(|r| r["test_id"].as_str())
        .collect();
    let failed: Vec<&str> = results
        .iter()
        .filter(|r| r["outcome"].as_str() == Some("failed"))
        .filter_map(|r| r["test_id"].as_str())
        .collect();

    assert_eq!(passed.len(), 2, "should have 2 passed tests");
    assert_eq!(failed.len(), 1, "should have 1 failed test");
    assert!(
        passed.iter().any(|id| id.ends_with("test_a.jl:1")),
        "test_a should be in passed"
    );
    assert!(
        failed.iter().any(|id| id.ends_with("test_b.jl:10")),
        "test_b should be in failed"
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_ingest_without_duration() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    // Test result without duration_ms field
    let run_output = r#"{"test_id":"test_x.jl:5","outcome":"passed"}"#;
    let cmd = serde_json::json!({
        "command": "ingest",
        "params": {
            "run_output": run_output
        }
    });
    let resp = send_command(&mut child, &cmd.to_string());
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("ingest response should be valid JSON");

    assert!(
        parsed["ok"].as_bool().unwrap_or(false),
        "ingest without duration should succeed"
    );
    let results = parsed["result"]["per_test_results"].as_array().unwrap();
    assert_eq!(results.len(), 1, "should have 1 result");
    assert_eq!(results[0]["outcome"].as_str(), Some("passed"));
    // The adapter prepends the CWD to relative paths
    let test_id = results[0]["test_id"].as_str().unwrap();
    assert!(
        test_id.ends_with("test_x.jl:5"),
        "test_id should end with test_x.jl:5, got: {}",
        test_id
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_run_args_empty_selected() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(
        &mut child,
        r#"{"command":"run-args","params":{"selected":[]}}"#,
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("run-args response should be valid JSON");

    assert!(
        !parsed["ok"].as_bool().unwrap_or(true),
        "empty selected should fail"
    );
    let err = parsed["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        err.contains("empty") || err.contains("missing"),
        "error should mention empty/missing selected: {}",
        err
    );

    child.kill().ok();
    child.wait().ok();
}

#[test]
fn julia_adapter_missing_params_structure() {
    if !julia_available() || !adapter_available() {
        return;
    }

    let mut child = spawn_adapter();
    let resp = send_command(&mut child, r#"{"command":"fingerprint","params":{}}"#);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp).expect("response should be valid JSON");

    assert!(
        !parsed["ok"].as_bool().unwrap_or(true),
        "fingerprint without files should fail"
    );
    let err = parsed["error"]["message"]
        .as_str()
        .unwrap_or("")
        .to_lowercase();
    assert!(
        err.contains("files") || err.contains("missing"),
        "error should mention missing files: {}",
        err
    );

    child.kill().ok();
    child.wait().ok();
}
