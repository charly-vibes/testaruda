//! testaruda-adapter-typescript — Reference adapter for TypeScript projects.
//!
//! Reads JSON commands from stdin, responds on stdout.
//! Protocol: single JSON line → single JSON line response.
//!
//! Uses tree-sitter-typescript for parsing `.ts`/`.tsx`/`.mts`/`.cts` files,
//! extracting imports, test declarations, and exports via Scheme queries.

use std::io::{BufRead, BufReader, Write};

fn main() {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        let response = handle_command(&trimmed);
        let out = serde_json::to_string(&response)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization failed"}"#.to_string());
        println!("{}", out);
        std::io::stdout().flush().ok();
    }
}

fn handle_command(input: &str) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return json_err(&format!("invalid JSON: {}", e)),
    };

    let command = cmd["command"].as_str().unwrap_or("");
    match command {
        "handshake" => cmd_handshake(),
        "discover" => cmd_discover(&cmd),
        "static-deps" => cmd_static_deps(&cmd),
        "fingerprint" => cmd_fingerprint(&cmd),
        "run-args" => cmd_run_args(&cmd),
        "ingest" => cmd_ingest(&cmd),
        _ => json_err(&format!("unknown command: {}", command)),
    }
}

fn json_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "result": result})
}

fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": msg})
}

/// Handshake: declare capabilities (TIA-ADAPT-020).
fn cmd_handshake() -> serde_json::Value {
    json_ok(serde_json::json!({
        "name": "typescript-adapter",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["typescript"],
        "granularity": "file",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}

/// Discover: find test files by convention and parse test declarations.
/// (Stub for scaffolding — will be implemented with tree-sitter queries.)
fn cmd_discover(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "tests": [],
        "warnings": ["discover not yet implemented"]
    }))
}

/// Static deps: extract import/require expressions from changed files.
/// (Stub for scaffolding — will be implemented with tree-sitter queries.)
fn cmd_static_deps(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "edges": [],
        "warnings": ["static-deps not yet implemented"]
    }))
}

/// Fingerprint: compute blake3 hash of file contents.
fn cmd_fingerprint(cmd: &serde_json::Value) -> serde_json::Value {
    let path = match cmd["path"].as_str() {
        Some(p) => p,
        None => return json_err("missing 'path' field"),
    };

    let contents = match std::fs::read(path) {
        Ok(c) => c,
        Err(e) => return json_err(&format!("failed to read file: {}", e)),
    };

    let hash = blake3::hash(&contents);
    json_ok(serde_json::json!({
        "path": path,
        "fingerprint": hash.to_hex().to_string()
    }))
}

/// Run args: detect test runner and build command-line arguments.
/// (Stub for scaffolding — will be implemented with runner detection.)
fn cmd_run_args(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "runner": "vitest",
        "args": ["npx", "vitest", "run", "--reporter=junit"],
        "warnings": ["run-args not yet fully implemented"]
    }))
}

/// Ingest: parse test runner output (JUnit XML) into runtime edges.
/// (Stub for scaffolding — will be implemented with JUnit parsing.)
fn cmd_ingest(_cmd: &serde_json::Value) -> serde_json::Value {
    json_ok(serde_json::json!({
        "results": [],
        "edges": [],
        "warnings": ["ingest not yet implemented"]
    }))
}
