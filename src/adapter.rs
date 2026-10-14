//! Adapter protocol — JSON request/response over stdin/stdout.
//!
//! See TIA-ADAPT-001 through TIA-ADAPT-013 for the full specification.
//!
//! # Protocol format
//!
//! The core sends a JSON command on one line to the adapter's stdin:
//! ```json
//! {"command":"handshake"}
//! ```
//!
//! The adapter responds with a single JSON line on stdout:
//! ```json
//! {"ok":true,"result":{...}}
//! ```
//!
//! Diagnostics go to stderr. Exit code indicates process status.

use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The current protocol version supported by the core.
pub const PROTOCOL_VERSION: u32 = 1;

/// Default timeout for adapter responses (milliseconds).
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

// ===== Capabilities =====

/// Capability flags an adapter may declare in its handshake (TIA-ADAPT-002).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterCapabilities {
    /// Whether the adapter models symbol-level dependencies (TIA-CHG-004).
    #[serde(default)]
    pub symbol_model_complete: bool,
    /// Whether the adapter supports content fingerprinting.
    #[serde(default = "return_true")]
    pub fingerprinting: bool,
    /// Whether the adapter collects runtime edges.
    #[serde(default)]
    pub runtime_edges: bool,
}

impl Default for AdapterCapabilities {
    fn default() -> Self {
        Self {
            symbol_model_complete: false,
            fingerprinting: true,
            runtime_edges: false,
        }
    }
}

fn return_true() -> bool {
    true
}

// ===== Handshake =====

/// Adapter handshake response (TIA-ADAPT-002).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub name: String,
    pub version: String,
    pub protocol: u32,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default = "default_granularity")]
    pub granularity: String,
    #[serde(default)]
    pub capabilities: AdapterCapabilities,
}

fn default_granularity() -> String {
    "file".to_string()
}

/// Envelope for handshake responses (TIA-ADAPT-002).
/// Matches the `{"ok": true, "result": {...}}` wire format used by all adapter responses.
#[derive(Debug, Deserialize)]
struct HandshakeResponse {
    result: Option<Handshake>,
    #[allow(dead_code)]
    error: Option<String>,
}

// ===== Discover =====

/// A single test item (TIA-ADAPT-004).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestItem {
    pub node_id: String,
    pub suite_kind: String,
    pub file: String,
}

// ===== Protocol IO =====

/// Sends a command and receives a typed response over a piped subprocess.
///
/// IO is bounded (testaruda-v9ey): a background thread turns the child's
/// stdout into complete lines on a channel so `read_response` can enforce the
/// deadline with `recv_timeout` — a silent or partial-line adapter can no
/// longer block forever on a live pipe. A second thread continuously drains
/// the child's stderr into a capped buffer so a chatty adapter cannot fill
/// the pipe and wedge itself.
pub struct AdapterIO {
    child: Child,
    stdin: Box<dyn Write + Send>,
    lines: std::sync::mpsc::Receiver<std::io::Result<String>>,
    stderr_buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    timeout: Duration,
    /// Adapter name from handshake.
    pub name: String,
    /// Adapter capabilities from handshake.
    pub capabilities: AdapterCapabilities,
}

/// Cap on the drained stderr snapshot kept for failure diagnostics — the
/// tail is retained, earlier bytes are dropped (testaruda-v9ey).
const STDERR_DRAIN_CAP: usize = 256 * 1024;

impl AdapterIO {
    /// Spawn an adapter binary, perform the handshake, and return a connected IO handle.
    ///
    /// Validates protocol version (TIA-ADAPT-011) and rejects incompatible adapters.
    pub fn spawn(
        binary: &str,
        args: &[&str],
        timeout_ms: Option<u64>,
    ) -> Result<Self, AdapterError> {
        Self::spawn_in(binary, args, timeout_ms, None)
    }

    /// Spawn an adapter child with an optional explicit working directory.
    pub fn spawn_in(
        binary: &str,
        args: &[&str],
        timeout_ms: Option<u64>,
        cwd: Option<&std::path::Path>,
    ) -> Result<Self, AdapterError> {
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS));

        let mut cmd = Command::new(binary);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Own process group (testaruda-cd4u): shim chains (sh -> adapter) run
        // as one group, so deadline kills can take the whole tree instead of
        // orphaning grandchildren.
        #[cfg(unix)]
        cmd.process_group(0);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AdapterError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("adapter binary not found: '{}'", binary),
                ))
            } else {
                AdapterError::Io(e)
            }
        })?;

        let stdin: Box<dyn Write + Send> = Box::new(child.stdin.take().ok_or_else(|| {
            AdapterError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stdin not available",
            ))
        })?);

        let stdout: Box<dyn std::io::Read + Send> =
            Box::new(child.stdout.take().ok_or_else(|| {
                AdapterError::Io(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "stdout not available",
                ))
            })?);

        // stderr drain thread (testaruda-v9ey): continuously reads the child's
        // stderr into a capped shared buffer. Without this, a chatty adapter
        // can fill the 64KB pipe buffer and wedge itself mid-response.
        let mut stderr_pipe = child.stderr.take().ok_or_else(|| {
            AdapterError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stderr not available",
            ))
        })?;
        let stderr_buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        {
            let buf = std::sync::Arc::clone(&stderr_buf);
            std::thread::spawn(move || {
                let mut chunk = [0u8; 8192];
                loop {
                    match std::io::Read::read(&mut stderr_pipe, &mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Ok(mut b) = buf.lock() {
                                b.extend_from_slice(&chunk[..n]);
                                let len = b.len();
                                if len > STDERR_DRAIN_CAP {
                                    b.drain(..len - STDERR_DRAIN_CAP);
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // stdout reader thread (testaruda-v9ey): turns the child's stdout
        // into complete lines on a channel. `read_response` then enforces the
        // deadline with `recv_timeout` instead of blocking inside `read_line`
        // on a live pipe.
        let (line_tx, line_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if line_tx.send(Ok(line.clone())).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = line_tx.send(Err(e));
                        break;
                    }
                }
            }
        });

        let mut io = Self {
            child,
            stdin,
            lines: line_rx,
            stderr_buf,
            timeout,
            name: String::new(),
            capabilities: AdapterCapabilities::default(),
        };

        // Handshake (TIA-ADAPT-002)
        // Deserialize through the envelope (TIA-ADAPT-001 — every response is wrapped)
        let hs_resp: HandshakeResponse = io.send(&AdapterCommand::handshake())?;
        let hs = hs_resp.result.ok_or_else(|| {
            AdapterError::MalformedResponse("missing result in handshake response".to_string())
        })?;
        io.name = hs.name;
        io.capabilities = hs.capabilities;

        // Protocol version check (TIA-ADAPT-011)
        if hs.protocol != PROTOCOL_VERSION {
            io.kill_tree();
            return Err(AdapterError::VersionMismatch {
                core: PROTOCOL_VERSION,
                adapter: hs.protocol,
            });
        }

        Ok(io)
    }

    /// Send a command and read a single JSON line response.
    pub fn send<T: serde::de::DeserializeOwned>(
        &mut self,
        cmd: &AdapterCommand,
    ) -> Result<T, AdapterError> {
        let req = serde_json::to_string(cmd)?;
        writeln!(self.stdin, "{}", req)?;
        self.stdin.flush()?;
        self.read_response()
    }

    /// Send a command with params and read a single JSON line response.
    pub fn send_with_params<T: serde::de::DeserializeOwned>(
        &mut self,
        cmd: &CommandWithParams,
    ) -> Result<T, AdapterError> {
        let req = serde_json::to_string(cmd)?;
        writeln!(self.stdin, "{}", req)?;
        self.stdin.flush()?;
        self.read_response()
    }

    /// Send the discover command (TIA-ADAPT-004).
    pub fn discover(&mut self) -> Result<Vec<TestItem>, AdapterError> {
        let resp: DiscoverResponse = self.send(&AdapterCommand::discover())?;
        Ok(resp.result.unwrap_or_default())
    }

    /// Send the static-deps command (TIA-ADAPT-005).
    ///
    /// Returns candidate tests, K-valued edges, and unresolved files.
    /// When `symbol_model_complete` is true, includes per-symbol edges.
    pub fn static_deps(
        &mut self,
        changed_files: &[String],
    ) -> Result<StaticDepsResult, AdapterError> {
        let cmd = CommandWithParams::new(
            AdapterCommand::STATIC_DEPS,
            serde_json::json!({"changed_files": changed_files}),
        );
        let resp: StaticDepsResponse = self.send_with_params(&cmd)?;
        Ok(StaticDepsResult {
            candidates: resp.candidates.unwrap_or_default(),
            edges: resp.edges.unwrap_or_default(),
            unresolved: resp.unresolved.unwrap_or_default(),
            symbol_edges: resp.symbol_edges.unwrap_or_default(),
        })
    }

    /// Send the fingerprint command (TIA-ADAPT-006).
    pub fn fingerprint(
        &mut self,
        files: &[String],
    ) -> Result<Vec<ContentFingerprint>, AdapterError> {
        let cmd = CommandWithParams::new(
            AdapterCommand::FINGERPRINT,
            serde_json::json!({"files": files}),
        );
        let resp: FingerprintResponse = self.send_with_params(&cmd)?;
        Ok(resp.fingerprints.unwrap_or_default())
    }

    /// Send the run-args command (TIA-ADAPT-007).
    ///
    /// Returns native runner arguments and a collection path for the selected test set.
    /// Does NOT execute the tests.
    pub fn run_args(&mut self, selected: &[String]) -> Result<RunArgsResult, AdapterError> {
        let cmd = CommandWithParams::new(
            AdapterCommand::RUN_ARGS,
            serde_json::json!({"selected": selected}),
        );
        let resp: RunArgsResponse = self.send_with_params(&cmd)?;
        resp.result.ok_or_else(|| {
            AdapterError::MalformedResponse("missing result in run-args response".to_string())
        })
    }

    /// Send the ingest command (TIA-ADAPT-008).
    ///
    /// Parses test runner output and returns runtime edges, per-test results,
    /// and observed external inputs.
    pub fn ingest(&mut self, run_output: &str) -> Result<IngestResult, AdapterError> {
        let cmd = CommandWithParams::new(
            AdapterCommand::INGEST,
            serde_json::json!({"run_output": run_output}),
        );
        let resp: IngestResponse = self.send_with_params(&cmd)?;
        resp.result.ok_or_else(|| {
            AdapterError::MalformedResponse("missing result in ingest response".to_string())
        })
    }

    /// Snapshot of the drained stderr tail, for failure diagnostics.
    fn stderr_snapshot(&self) -> String {
        self.stderr_buf
            .lock()
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default()
    }

    /// Read a single JSON response line from the adapter, bounded by the
    /// deadline (testaruda-v9ey). Silence, a never-terminated line, or a wedged
    /// stderr all resolve to a killed child and `AdapterError::Timeout`.
    fn read_response<T: serde::de::DeserializeOwned>(&mut self) -> Result<T, AdapterError> {
        let start = Instant::now();

        loop {
            // Check timeout
            let elapsed = start.elapsed();
            if elapsed > self.timeout {
                self.kill_tree();
                return Err(AdapterError::Timeout(self.timeout.as_millis() as u64));
            }

            // Check if adapter exited
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    let stderr = self.stderr_snapshot();
                    return Err(AdapterError::ProcessExit(
                        status.code().unwrap_or(-1),
                        stderr,
                    ));
                }
                Ok(None) => {}
                Err(e) => return Err(AdapterError::Io(e)),
            }

            // Bounded wait on the stdout line channel: at most 100ms per
            // recv, so the deadline and child-exit checks above stay live.
            let remaining = self.timeout.saturating_sub(elapsed);
            let poll = std::cmp::min(remaining, Duration::from_millis(100));
            match self.lines.recv_timeout(poll) {
                Ok(Ok(line)) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue; // blank line — keep waiting
                    }
                    // Parse response envelope to check ok/error
                    let value: serde_json::Value = serde_json::from_str(trimmed)?;
                    if let Some(false) = value.get("ok").and_then(|v| v.as_bool()) {
                        let err = value
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("adapter error");
                        return Err(AdapterError::MalformedResponse(err.to_string()));
                    }
                    // Parse the full response as T using the same Value
                    return serde_json::from_value(value).map_err(AdapterError::Json);
                }
                Ok(Err(e)) => return Err(AdapterError::Io(e)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // EOF — adapter closed stdout; it may still be exiting.
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
        }
    }

    /// Check if the adapter has a given capability (TIA-ADAPT-010).
    pub fn has_capability(&self, cap: &str) -> bool {
        match cap {
            "symbol_model_complete" => self.capabilities.symbol_model_complete,
            "fingerprinting" => self.capabilities.fingerprinting,
            "runtime_edges" => self.capabilities.runtime_edges,
            _ => false,
        }
    }

    /// Kill the adapter process tree.
    ///
    /// Kills the child's process group when possible (testaruda-cd4u) so
    /// shim chains (`/bin/sh shim` -> real adapter) do not orphan their
    /// grandchildren, then reaps the direct child with a bounded wait —
    /// the unbounded `wait()` here was the do_wait wedge surface.
    fn kill_tree(&mut self) {
        // Already exited and reaped (e.g. ProcessExit in read_response)? Skip
        // the kill entirely: the pid may have been recycled, and signaling a
        // recycled group could hit an unrelated process (testaruda-cd4u).
        if let Ok(Some(_)) = self.child.try_wait() {
            return;
        }
        #[cfg(unix)]
        {
            // Safe: signal-only FFI on a pid we own. The child was spawned
            // with `process_group(0)`, so its pid IS the group id.
            let pgid = self.child.id() as i32;
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        }
        #[cfg(not(unix))]
        let _ = self.child.kill();
        // Bounded reap: SIGKILL cannot be blocked, so this returns quickly;
        // the bound keeps a pathological uninterruptible child from wedging us.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }

    /// Kill the adapter process.
    pub fn kill(mut self) -> Result<(), AdapterError> {
        self.kill_tree();
        Ok(())
    }
}

impl Drop for AdapterIO {
    fn drop(&mut self) {
        self.kill_tree();
    }
}

// ===== Wire formats =====

/// Command sent to the adapter.
#[derive(Debug, Serialize)]
pub struct AdapterCommand {
    pub command: String,
}

impl AdapterCommand {
    pub const HANDSHAKE: &'static str = "handshake";
    pub const DISCOVER: &'static str = "discover";
    pub const STATIC_DEPS: &'static str = "static-deps";
    pub const FINGERPRINT: &'static str = "fingerprint";
    pub const RUN_ARGS: &'static str = "run-args";
    pub const INGEST: &'static str = "ingest";

    pub fn handshake() -> Self {
        Self {
            command: Self::HANDSHAKE.to_string(),
        }
    }
    pub fn discover() -> Self {
        Self {
            command: Self::DISCOVER.to_string(),
        }
    }
    pub fn static_deps() -> Self {
        Self {
            command: Self::STATIC_DEPS.to_string(),
        }
    }
    pub fn fingerprint() -> Self {
        Self {
            command: Self::FINGERPRINT.to_string(),
        }
    }
    pub fn run_args() -> Self {
        Self {
            command: Self::RUN_ARGS.to_string(),
        }
    }
    pub fn ingest() -> Self {
        Self {
            command: Self::INGEST.to_string(),
        }
    }
}

/// Command with params sent to the adapter.
#[derive(Debug, Serialize)]
pub struct CommandWithParams {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl CommandWithParams {
    pub fn new(command: &str, params: serde_json::Value) -> Self {
        Self {
            command: command.to_string(),
            params: Some(params),
        }
    }
}

/// Typed discover response.
#[derive(Debug, Deserialize)]
struct DiscoverResponse {
    result: Option<Vec<TestItem>>,
    #[allow(dead_code)]
    error: Option<String>,
}

/// A dependency edge with semiring weight (TIA-ADAPT-009).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepEdge {
    pub from: String,
    pub to: String,
    /// Semiring weight (ppm), defaults to multiplicative identity (1_000_000).
    #[serde(default = "default_weight")]
    pub weight: u32,
    /// Origin of the edge (static, runtime, manual).
    #[serde(default = "default_origin")]
    pub origin: String,
}

fn default_weight() -> u32 {
    crate::engine::ONE
}
fn default_origin() -> String {
    "static".to_string()
}

/// Result of a static-deps command (TIA-ADAPT-005).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticDepsResult {
    pub candidates: Vec<String>,
    pub edges: Vec<DepEdge>,
    pub unresolved: Vec<String>,
    #[serde(default)]
    pub symbol_edges: Vec<DepEdge>,
}

#[derive(Debug, Deserialize)]
struct StaticDepsResponse {
    candidates: Option<Vec<String>>,
    edges: Option<Vec<DepEdge>>,
    unresolved: Option<Vec<String>>,
    #[serde(default)]
    symbol_edges: Option<Vec<DepEdge>>,
    #[allow(dead_code)]
    error: Option<String>,
}

/// Run arguments result (TIA-ADAPT-007) — native runner argv, no test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunArgsResult {
    /// Native runner arguments (e.g. `["cargo", "test", "--test", "foo"]`)
    pub runner_args: Vec<String>,
    /// Path to the test results file/collection (e.g. JUnit XML path)
    pub collection_path: String,
}

/// A single test run result (TIA-ADAPT-008).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestRunResult {
    pub test_id: String,
    pub outcome: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error_text: Option<String>,
}

/// Ingest result (TIA-ADAPT-008) — runtime edges, per-test results, external inputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    #[serde(default)]
    pub runtime_edges: Vec<DepEdge>,
    #[serde(default)]
    pub per_test_results: Vec<TestRunResult>,
    #[serde(default)]
    pub external_inputs: Vec<String>,
}

/// A content fingerprint at file or symbol granularity (TIA-ADAPT-006).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentFingerprint {
    pub file: String,
    pub fingerprint: String,
    #[serde(default)]
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FingerprintResponse {
    fingerprints: Option<Vec<ContentFingerprint>>,
    #[allow(dead_code)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RunArgsResponse {
    result: Option<RunArgsResult>,
    #[allow(dead_code)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IngestResponse {
    result: Option<IngestResult>,
    #[allow(dead_code)]
    error: Option<String>,
}

// ===== Registry =====

/// Resolves file extensions to adapter binaries.
#[derive(Debug, Clone)]
pub struct AdapterRegistry {
    extensions: Vec<(String, String)>,
    default: Option<String>,
    /// Per-response deadline for spawned adapters (testaruda-cd4u).
    /// `None` falls back to `DEFAULT_TIMEOUT_MS` in `AdapterIO::spawn_in`.
    timeout_ms: Option<u64>,
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            extensions: Vec::new(),
            default: None,
            timeout_ms: None,
        }
    }

    pub fn register(&mut self, extension: &str, binary: &str) {
        self.extensions
            .push((extension.to_string(), binary.to_string()));
    }

    pub fn set_default(&mut self, binary: &str) {
        self.default = Some(binary.to_string());
    }

    /// Set the per-response adapter deadline (testaruda-cd4u).
    pub fn set_timeout_ms(&mut self, timeout_ms: Option<u64>) {
        self.timeout_ms = timeout_ms;
    }

    /// The configured per-response adapter deadline, if any.
    pub fn timeout_ms(&self) -> Option<u64> {
        self.timeout_ms
    }

    /// Find the adapter binary for a file path by extension.
    pub fn resolve(&self, path: &str) -> Option<&str> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e));
        if let Some(ref ext_str) = ext {
            for (registered, binary) in &self.extensions {
                if registered == ext_str {
                    return Some(binary);
                }
            }
        }
        self.default.as_deref()
    }

    /// Returns an iterator over registered extension-to-binary mappings.
    pub fn extensions(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.extensions
            .iter()
            .map(|(e, b)| (e.as_str(), b.as_str()))
    }

    /// Returns the default adapter binary name, if set.
    pub fn default_binary(&self) -> Option<&str> {
        self.default.as_deref()
    }
}

// ===== Errors =====

#[derive(Error, Debug)]
pub enum AdapterError {
    #[error("Adapter exited with code {0}: {1}")]
    ProcessExit(i32, String),
    #[error("Adapter timeout after {0}ms")]
    Timeout(u64),
    #[error("Protocol version mismatch: core={core}, adapter={adapter}")]
    VersionMismatch { core: u32, adapter: u32 },
    #[error("Capability not available: {0}")]
    MissingCapability(String),
    #[error("Malformed response: {0}")]
    MalformedResponse(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ===== Command-string adapter invocation =====

/// Shell-split a command string into a binary path and argument list.
///
/// Returns the binary as an owned String and args as owned Vec<String> so the
/// result can be inspected in tests without lifetime issues.
///
/// # Errors
///
/// Returns `AdapterError::Io` with `InvalidInput` kind for:
/// - Empty command strings
/// - Malformed quoting (via `shell_words::split`)
pub fn parse_command_string(command_string: &str) -> Result<(String, Vec<String>), AdapterError> {
    if command_string.is_empty() {
        return Err(AdapterError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "empty command string in adapter extension mapping — configure a valid adapter binary"
                .to_string(),
        )));
    }

    let words = shell_words::split(command_string).map_err(|e| {
        AdapterError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "failed to shell-split adapter command string '{}': {}",
                command_string, e
            ),
        ))
    })?;

    let mut words = words;
    if words.is_empty() {
        return Err(AdapterError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "empty result after shell-splitting adapter command string '{}'",
                command_string
            ),
        )));
    }

    let binary = words.remove(0);
    Ok((binary, words))
}

/// Spawn an adapter from a command string, shell-splitting into `(binary, args)`.
///
/// This is the single entry point for all adapter spawns. It replaces direct calls
/// to `AdapterIO::spawn(binary, &[], None)` which only work for bare-binary adapters.
///
/// The shell-split (TIA-ADAPT-024) supports:
/// - Bare binaries: `"testaruda-adapter-rust"` → `("testaruda-adapter-rust", [])`
/// - Subcommand adapters: `"titi testaruda-adapter"` → `("titi", ["testaruda-adapter"])`
/// - Quoted-arg adapters: `"julia --project=. -e '...'"` → quoted body preserved as one arg
/// - Windows paths: `"\"C:\\Program Files\\titi\\titi.exe\" testaruda-adapter"` → path with spaces
///
/// Empty command strings produce a config-error diagnostic and return `AdapterError::Io`
/// so the caller falls back per TIA-ADAPT-012.
pub fn spawn_adapter(
    command_string: &str,
    timeout_ms: Option<u64>,
) -> Result<AdapterIO, AdapterError> {
    spawn_adapter_in(command_string, timeout_ms, None)
}

/// Spawn an adapter command string, optionally rooted at `cwd`.
///
/// The child gets an explicit working directory when provided — callers must
/// not rely on (or mutate) the harness process cwd (testaruda-pzh6).
pub fn spawn_adapter_in(
    command_string: &str,
    timeout_ms: Option<u64>,
    cwd: Option<&std::path::Path>,
) -> Result<AdapterIO, AdapterError> {
    let (binary, args) = parse_command_string(command_string)?;
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    AdapterIO::spawn_in(&binary, &args_refs, timeout_ms, cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // ===== Bounded adapter IO (testaruda-v9ey) =====
    // A silent or partial-line adapter must hit the deadline instead of
    // blocking forever on a live pipe, and a stderr-flooding adapter must
    // not wedge on an undrained pipe.

    /// Write a fake adapter script and return its spawn command string.
    fn io_script(dir: &Path, body: &str) -> String {
        let path = dir.join("io_adapter.py");
        std::fs::write(&path, format!("import json, sys, time\n{body}\n")).unwrap();
        format!("python3 {}", path.display())
    }

    const HANDSHAKE_BODY: &str = r#"HS = json.dumps(dict(ok=True, result=dict(name='io', version='1', protocol=1, languages=['fake'], granularity='file', capabilities={})))
for line in sys.stdin:
    cmd = json.loads(line)['command']
    if cmd == 'handshake':
        print(HS, flush=True)
    elif cmd == 'static-deps':
        print(json.dumps(dict(ok=True, result=dict(candidates=[], edges=[], unresolved=[]))), flush=True)
    else:
        print(json.dumps(dict(ok=True, result={})), flush=True)
"#;

    #[test]
    fn readiness_io_success() {
        let dir = tempfile::tempdir().unwrap();
        let mut io = spawn_adapter(&io_script(dir.path(), HANDSHAKE_BODY), Some(5_000)).unwrap();
        let result = io.static_deps(&["src/lib.rs".to_string()]);
        assert!(result.is_ok(), "healthy adapter must respond");
    }

    #[test]
    fn readiness_io_silence_times_out() {
        let dir = tempfile::tempdir().unwrap();
        // Never reads stdin, never writes: the deadline must fire.
        let script = io_script(dir.path(), "time.sleep(30)");
        let result = spawn_adapter(&script, Some(300));
        match result {
            Err(AdapterError::Timeout(ms)) => assert_eq!(ms, 300),
            Err(other) => panic!("expected Timeout, got: {other}"),
            Ok(_) => panic!("expected Timeout, but adapter responded"),
        }
    }

    /// Scan /proc for a live process whose cmdline contains `marker`.
    #[cfg(unix)]
    fn process_with_cmdline(marker: &str) -> bool {
        let proc_dir = std::path::Path::new("/proc");
        let Ok(entries) = std::fs::read_dir(proc_dir) else {
            return false; // no /proc — cannot verify, assume absent
        };
        for entry in entries.flatten() {
            let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let cmd = String::from_utf8_lossy(&cmdline).replace('\0', " ");
            if cmd.contains(marker) {
                return true;
            }
        }
        false
    }

    #[test]
    #[cfg(unix)]
    fn readiness_io_timeout_kills_process_group() {
        // testaruda-cd4u: the deadline kill must take the whole adapter
        // process group, not just the direct child. The observed incident was
        // exec -> /bin/sh shim -> adapter, where killing the direct child
        // orphaned the real adapter process for days. Here the adapter (a)
        // never reads stdin (deadline fires during handshake) and (b) spawned
        // a long-lived grandchild with a unique marker; after the timeout the
        // grandchild must be gone.
        let dir = tempfile::tempdir().unwrap();
        let script = io_script(
            dir.path(),
            "import subprocess\nsubprocess.Popen(['sleep', '424242'])\ntime.sleep(30)",
        );
        let result = spawn_adapter(&script, Some(300));
        match result {
            Err(AdapterError::Timeout(_)) => {}
            Err(other) => panic!("expected Timeout, got: {other}"),
            Ok(_) => panic!("expected Timeout, but adapter responded"),
        }
        // Give the kernel a moment to reap the killed group.
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            !process_with_cmdline("sleep 424242"),
            "orphaned grandchild survived the adapter timeout (testaruda-cd4u)"
        );
    }

    #[test]
    fn readiness_io_partial_line_times_out() {
        let dir = tempfile::tempdir().unwrap();
        // Handshake fine, then writes a partial JSON line with no newline and
        // stalls: the deadline must fire instead of blocking on the pipe.
        let script = io_script(
            dir.path(),
            r#"HS = json.dumps(dict(ok=True, result=dict(name='io', version='1', protocol=1, languages=['fake'], granularity='file', capabilities={})))
for line in sys.stdin:
    cmd = json.loads(line)['command']
    if cmd == 'handshake':
        print(HS, flush=True)
    elif cmd == 'static-deps':
        sys.stdout.write('{"ok":true,"resu')
        sys.stdout.flush()
        time.sleep(30)
"#,
        );
        let mut io = spawn_adapter(&script, Some(5_000)).unwrap();
        let result = io.static_deps(&["src/lib.rs".to_string()]);
        match result {
            Err(AdapterError::Timeout(_)) => {}
            Err(other) => panic!("expected Timeout, got: {other}"),
            Ok(_) => panic!("expected Timeout, but adapter responded"),
        }
    }

    #[test]
    fn readiness_io_stderr_flood_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        // Floods stderr (far beyond the 64KB pipe buffer) BEFORE answering:
        // the core must keep draining stderr so the adapter can proceed.
        let script = io_script(
            dir.path(),
            r#"HS = json.dumps(dict(ok=True, result=dict(name='io', version='1', protocol=1, languages=['fake'], granularity='file', capabilities={})))
for line in sys.stdin:
    cmd = json.loads(line)['command']
    if cmd == 'handshake':
        for _ in range(20):
            sys.stderr.write('x' * (1024 * 1024))
        sys.stderr.flush()
        print(HS, flush=True)
    elif cmd == 'static-deps':
        print(json.dumps(dict(ok=True, result=dict(candidates=[], edges=[], unresolved=[]))), flush=True)
"#,
        );
        let mut io = spawn_adapter(&script, Some(10_000)).unwrap();
        let result = io.static_deps(&["src/lib.rs".to_string()]);
        assert!(result.is_ok(), "stderr flood must not wedge the adapter");
    }

    #[test]
    fn test_parse_command_string_bare_binary() {
        let (binary, args) = parse_command_string("testaruda-adapter-rust").unwrap();
        assert_eq!(binary, "testaruda-adapter-rust");
        assert!(args.is_empty());
    }

    #[test]
    fn test_parse_command_string_subcommand() {
        let (binary, args) = parse_command_string("titi testaruda-adapter").unwrap();
        assert_eq!(binary, "titi");
        assert_eq!(args, vec!["testaruda-adapter"]);
    }

    #[test]
    fn test_parse_command_string_julia_quoted() {
        let cmd =
            "julia --project=. -e 'using Testimonial; Testimonial.Protocol.run_adapter_protocol()'";
        let (binary, args) = parse_command_string(cmd).unwrap();
        assert_eq!(binary, "julia");
        assert_eq!(
            args,
            vec![
                "--project=.",
                "-e",
                "using Testimonial; Testimonial.Protocol.run_adapter_protocol()"
            ]
        );
    }

    #[test]
    fn test_parse_command_string_empty() {
        let err = parse_command_string("").unwrap_err();
        match &err {
            AdapterError::Io(e) => {
                assert_eq!(e.kind(), std::io::ErrorKind::InvalidInput);
            }
            _ => panic!("expected Io error"),
        }
    }

    #[test]
    fn test_parse_command_string_windows_path() {
        let cmd = "\"C:\\Program Files\\titi\\titi.exe\" testaruda-adapter";
        let (binary, args) = parse_command_string(cmd).unwrap();
        assert_eq!(binary, "C:\\Program Files\\titi\\titi.exe");
        assert_eq!(args, vec!["testaruda-adapter"]);
    }

    #[test]
    fn test_adapter_registry_resolve() {
        let mut reg = AdapterRegistry::new();
        reg.register(".rs", "rust-adapter");
        reg.register(".py", "python-adapter");
        assert_eq!(reg.resolve("src/main.rs"), Some("rust-adapter"));
        assert_eq!(reg.resolve("tests/test.py"), Some("python-adapter"));
        assert_eq!(reg.resolve("config.toml"), None);
    }

    #[test]
    fn test_adapter_registry_default() {
        let mut reg = AdapterRegistry::new();
        reg.set_default("generic");
        reg.register(".rs", "rust-adapter");
        assert_eq!(reg.resolve("src/main.rs"), Some("rust-adapter"));
        assert_eq!(reg.resolve("config.toml"), Some("generic"));
    }

    #[test]
    fn test_adapter_command_serialization() {
        let cmd = serde_json::to_string(&AdapterCommand::handshake()).unwrap();
        assert_eq!(cmd, r#"{"command":"handshake"}"#);
    }

    #[test]
    fn test_handshake_deserialization() {
        // Real wire format: enveloped like every other adapter response (TIA-ADAPT-001)
        let json = r#"{
            "ok": true,
            "result": {
                "name": "rust-adapter",
                "version": "1.0.0",
                "protocol": 1,
                "languages": ["rust"],
                "granularity": "symbol",
                "capabilities": {
                    "symbol_model_complete": true,
                    "fingerprinting": true,
                    "runtime_edges": false
                }
            }
        }"#;
        let resp: HandshakeResponse = serde_json::from_str(json).unwrap();
        let hs = resp.result.unwrap();
        assert_eq!(hs.name, "rust-adapter");
        assert_eq!(hs.protocol, 1);
        assert!(hs.capabilities.symbol_model_complete);
        assert!(!hs.capabilities.runtime_edges);
    }

    #[test]
    fn test_handshake_defaults() {
        // Real wire format: enveloped with minimal handshake data
        let json = r#"{
            "ok": true,
            "result": {
                "name": "min",
                "version": "1.0",
                "protocol": 1
            }
        }"#;
        let resp: HandshakeResponse = serde_json::from_str(json).unwrap();
        let hs = resp.result.unwrap();
        assert_eq!(hs.granularity, "file");
        assert!(!hs.capabilities.symbol_model_complete);
        assert!(hs.capabilities.fingerprinting);
    }

    #[test]
    fn test_discover_response() {
        let json = r#"{
            "ok": true,
            "result": [
                {"node_id":"t1","suite_kind":"unit","file":"src/main.rs"}
            ]
        }"#;
        let resp: DiscoverResponse = serde_json::from_str(json).unwrap();
        let items = resp.result.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].node_id, "t1");
    }

    #[test]
    fn test_discover_response_error() {
        let json = r#"{"ok":false,"error":"no files found"}"#;
        let resp: DiscoverResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.error, Some("no files found".to_string()));
    }

    #[test]
    fn test_version_mismatch_detection() {
        let json = r#"{
            "ok": true,
            "result": {
                "name": "old",
                "version": "0.5",
                "protocol": 0
            }
        }"#;
        let resp: HandshakeResponse = serde_json::from_str(json).unwrap();
        let hs = resp.result.unwrap();
        assert_ne!(hs.protocol, PROTOCOL_VERSION);
    }

    #[test]
    fn test_static_deps_response() {
        let json = r#"{
            "candidates": ["test_foo", "test_bar"],
            "edges": [
                {"from": "test_foo", "to": "src/lib.rs", "weight": 1000000, "origin": "static"}
            ],
            "unresolved": ["config.yaml"]
        }"#;
        let resp: StaticDepsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.candidates.unwrap().len(), 2);
        assert_eq!(resp.edges.unwrap().len(), 1);
        assert_eq!(resp.unresolved.unwrap().len(), 1);
    }

    #[test]
    fn test_static_deps_with_symbol_edges() {
        let json = r#"{
            "candidates": ["test_mod"],
            "edges": [
                {"from": "test_mod", "to": "src/lib.rs", "weight": 1000000, "origin": "static"}
            ],
            "unresolved": [],
            "symbol_edges": [
                {"from": "test_mod", "to": "src/lib.rs::foo", "weight": 1000000, "origin": "static"}
            ]
        }"#;
        let resp: StaticDepsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.symbol_edges.unwrap().len(), 1);
    }

    #[test]
    fn test_fingerprint_response() {
        let json = r#"{"fingerprints": [
            {"file": "src/main.rs", "fingerprint": "abc123", "symbol": null}
        ]}"#;
        let resp: FingerprintResponse = serde_json::from_str(json).unwrap();
        let fps = resp.fingerprints.unwrap();
        assert_eq!(fps.len(), 1);
        assert_eq!(fps[0].file, "src/main.rs");
        assert_eq!(fps[0].fingerprint, "abc123");
        assert!(fps[0].symbol.is_none());
    }

    #[test]
    fn test_fingerprint_with_symbol() {
        let json = r#"{"fingerprints": [
            {"file": "src/lib.rs", "fingerprint": "def456", "symbol": "foo"}
        ]}"#;
        let resp: FingerprintResponse = serde_json::from_str(json).unwrap();
        let fp = &resp.fingerprints.unwrap()[0];
        assert_eq!(fp.symbol.as_deref(), Some("foo"));
    }

    #[test]
    fn test_dep_edge_defaults() {
        let json = r#"{"from": "t", "to": "f"}"#;
        let edge: DepEdge = serde_json::from_str(json).unwrap();
        assert_eq!(edge.weight, crate::engine::ONE);
        assert_eq!(edge.origin, "static");
    }

    #[test]
    fn test_static_deps_result_defaults() {
        let json = r#"{"candidates": [], "edges": [], "unresolved": []}"#;
        let _result: StaticDepsResponse = serde_json::from_str(json).unwrap();
        // Should deserialize with empty defaults
    }

    #[test]
    fn test_run_args_result_deserialization() {
        let json = r#"{
            "ok": true,
            "result": {
                "runner_args": ["cargo", "test", "--test", "foo"],
                "collection_path": "target/test-results.xml"
            }
        }"#;
        let resp: RunArgsResponse = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result.runner_args, vec!["cargo", "test", "--test", "foo"]);
        assert_eq!(result.collection_path, "target/test-results.xml");
    }

    #[test]
    fn test_run_args_response_error() {
        let json = r#"{"ok": false, "error": "no tests selected"}"#;
        let resp: RunArgsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_none());
        assert_eq!(resp.error, Some("no tests selected".to_string()));
    }

    #[test]
    fn test_ingest_result_deserialization() {
        let json = r#"{
            "ok": true,
            "result": {
                "runtime_edges": [
                    {"from": "test_foo", "to": "src/lib.rs", "weight": 1000000, "origin": "runtime"}
                ],
                "per_test_results": [
                    {
                        "test_id": "test_foo",
                        "outcome": "passed",
                        "duration_ms": 150,
                        "error_text": null
                    }
                ],
                "external_inputs": ["config.json"]
            }
        }"#;
        let resp: IngestResponse = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result.runtime_edges.len(), 1);
        assert_eq!(result.runtime_edges[0].from, "test_foo");
        assert_eq!(result.runtime_edges[0].origin, "runtime");
        assert_eq!(result.per_test_results.len(), 1);
        assert_eq!(result.per_test_results[0].test_id, "test_foo");
        assert_eq!(result.per_test_results[0].outcome, "passed");
        assert_eq!(result.per_test_results[0].duration_ms, Some(150));
        assert!(result.per_test_results[0].error_text.is_none());
        assert_eq!(result.external_inputs, vec!["config.json"]);
    }

    #[test]
    fn test_ingest_result_empty_defaults() {
        let json = r#"{
            "ok": true,
            "result": {}
        }"#;
        let resp: IngestResponse = serde_json::from_str(json).unwrap();
        let result = resp.result.unwrap();
        assert!(result.runtime_edges.is_empty());
        assert!(result.per_test_results.is_empty());
        assert!(result.external_inputs.is_empty());
    }

    #[test]
    fn test_run_args_command_serialization() {
        let cmd = serde_json::to_string(&AdapterCommand::run_args()).unwrap();
        assert_eq!(cmd, r#"{"command":"run-args"}"#);
    }

    #[test]
    fn test_ingest_command_serialization() {
        let cmd = serde_json::to_string(&AdapterCommand::ingest()).unwrap();
        assert_eq!(cmd, r#"{"command":"ingest"}"#);
    }

    #[test]
    fn test_run_args_with_params_serialization() {
        let cmd = CommandWithParams::new(
            AdapterCommand::RUN_ARGS,
            serde_json::json!({"selected": ["test_a"]}),
        );
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"command\":\"run-args\""));
        assert!(json.contains("\"selected\":[\"test_a\"]"));
    }

    #[test]
    fn test_test_run_result_defaults() {
        let json = r#"{"test_id":"t1","outcome":"failed"}"#;
        let tr: TestRunResult = serde_json::from_str(json).unwrap();
        assert_eq!(tr.test_id, "t1");
        assert_eq!(tr.outcome, "failed");
        assert!(tr.duration_ms.is_none());
        assert!(tr.error_text.is_none());
    }

    #[test]
    fn test_run_args_result_empty_selected() {
        // Empty selected set should still produce valid struct
        let result = RunArgsResult {
            runner_args: Vec::new(),
            collection_path: "-".to_string(),
        };
        assert!(result.runner_args.is_empty());
        assert_eq!(result.collection_path, "-");
    }

    #[test]
    fn test_ingest_result_full_data() {
        let result = IngestResult {
            runtime_edges: vec![DepEdge {
                from: "t1".to_string(),
                to: "mod.rs".to_string(),
                weight: 500_000,
                origin: "runtime".to_string(),
            }],
            per_test_results: vec![TestRunResult {
                test_id: "t1".to_string(),
                outcome: "flaky".to_string(),
                duration_ms: Some(200),
                error_text: Some("intermittent failure".to_string()),
            }],
            external_inputs: vec!["config.yaml".to_string(), "env.txt".to_string()],
        };
        assert_eq!(result.runtime_edges.len(), 1);
        assert_eq!(result.external_inputs.len(), 2);
        assert_eq!(result.per_test_results[0].outcome, "flaky");
    }

    #[test]
    fn test_run_args_result_serialization_roundtrip() {
        let orig = RunArgsResult {
            runner_args: vec!["pytest".to_string(), "-x".to_string()],
            collection_path: "results.xml".to_string(),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let restored: RunArgsResult = serde_json::from_str(&json).unwrap();
        assert_eq!(orig.runner_args, restored.runner_args);
        assert_eq!(orig.collection_path, restored.collection_path);
    }

    #[test]
    fn test_ingest_result_serialization_roundtrip() {
        let orig = IngestResult {
            runtime_edges: vec![DepEdge {
                from: "ta".to_string(),
                to: "tb".to_string(),
                weight: 750_000,
                origin: "runtime".to_string(),
            }],
            per_test_results: vec![TestRunResult {
                test_id: "ta".to_string(),
                outcome: "passed".to_string(),
                duration_ms: None,
                error_text: None,
            }],
            external_inputs: Vec::new(),
        };
        let json = serde_json::to_string(&orig).unwrap();
        let restored: IngestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(orig.runtime_edges[0].from, restored.runtime_edges[0].from);
        assert_eq!(
            orig.per_test_results[0].outcome,
            restored.per_test_results[0].outcome
        );
        assert!(restored.external_inputs.is_empty());
    }
}
