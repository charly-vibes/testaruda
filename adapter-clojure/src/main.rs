//! testaruda-adapter-clojure — JSON-over-stdin/stdout adapter for Clojure.
//!
//! Reads JSON commands from stdin, responds on stdout.
//! Protocol: single JSON line → single JSON line response.
//!
//! Supports the 6 adapter protocol commands (TIA-ADAPT-001):
//! - handshake: declare capabilities
//! - discover: enumerate tests via tree-sitter queries
//! - static-deps: extract :require/:use/:import dependencies
//! - fingerprint: blake3 hash of file contents
//! - run-args: build runner CLI args (deps.edn vs project.clj)
//! - ingest: parse JUnit XML or stdout for results
//!
//! **Currently implemented:** handshake only (testaruda-iq6).
//! All other commands return "not implemented" errors — they land in
//! follow-up tickets (testaruda-fjj, testaruda-cch, etc.).

mod query;

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
        println!("{out}");
        std::io::stdout().flush().ok();
    }
}

fn handle_command(input: &str) -> serde_json::Value {
    let cmd: serde_json::Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(e) => return json_err(&format!("invalid JSON: {e}")),
    };

    let command = cmd["command"].as_str().unwrap_or("");
    match command {
        "handshake" => cmd_handshake(),
        "discover" => json_err("not implemented: discover (see testaruda-cch)"),
        "static-deps" => json_err("not implemented: static-deps (see testaruda-cch)"),
        "fingerprint" => json_err("not implemented: fingerprint (see testaruda-cch)"),
        "run-args" => json_err("not implemented: run-args (see testaruda-fjj)"),
        "ingest" => json_err("not implemented: ingest (see testaruda-fjj)"),
        _ => json_err(&format!("unknown command: {command}")),
    }
}

fn json_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({"ok": true, "result": result})
}

fn json_err(msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": msg})
}

/// Handshake: declare capabilities (TIA-ADAPT-017).
fn cmd_handshake() -> serde_json::Value {
    json_ok(serde_json::json!({
        "name": "testaruda-adapter-clojure",
        "version": "0.1.0",
        "protocol": 1,
        "languages": ["clojure"],
        "granularity": "file",
        "capabilities": {
            "symbol_model_complete": false,
            "fingerprinting": true,
            "runtime_edges": false
        }
    }))
}
