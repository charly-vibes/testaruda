//! Purpose: Implement testaruda CLI command workflows.
//! Responsibilities:
//! - Coordinate selection, adapter evidence and store updates.
//! - Execute and ingest selected groups and report actionable failures.
//!
//! Rationale: Distinguish completed execution from unavailable groups so CI
//! cannot accept silently skipped tests (testaruda-oeft).

use genesis::guide::{CliFormat, OutputFormat};
use miette::miette;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use testaruda::adapter::{spawn_adapter, AdapterRegistry};
use testaruda::agent::{ChangedUnit, PreEditOutput, SummaryStats};
use testaruda::{ChangeSet, Engine, SelectedTest, Selection, Store, TestOrdering};

use crate::build_block_registry;

// ===== CI Exit Codes (TIA-CI-001..008) =====

/// CI exit-code constants. Distinct values let a CI runner discriminate between
/// "selection complete" (0), "run everything" (10), "nothing to run" (20), and
/// hard errors (1).
#[allow(dead_code)]
pub mod ci_exit {
    pub const SUCCESS: i32 = 0; // TIA-CI-001
    pub const FULL_RUN: i32 = 10; // TIA-CI-002
    pub const EMPTY: i32 = 20; // TIA-CI-003
    pub const ERROR: i32 = 1; // TIA-CI-004 (distinct from 10, 20)
}

/// Classifies a selection result into a CI exit code and reason.
pub struct CiOutcome {
    code: i32,
    reason: String,
}

impl CiOutcome {
    /// Determine the CI outcome from a selection result.
    ///
    /// - Empty selection → exit 20 (TIA-CI-003)
    /// - Full-suite selection on a real change → exit 10 (TIA-CI-002), with a
    ///   reason exposing *why* the selection degraded (testaruda-ls4t):
    ///   no dependency edges resolved, or the coverage/confidence floor.
    /// - Any test with confidence < 1.0 → full run, exit 10 (TIA-CI-002)
    /// - Otherwise → success, exit 0 (TIA-CI-001)
    ///
    /// `total_tests` is the number of tests known to the store;
    /// `changed_units` is the count of changed + unresolved content units
    /// (unresolved units over-approximate per SAFE-004);
    /// `edges_resolved` reports whether any of them linked to at least one
    /// test via dependency edges.
    pub fn from_selection(
        selection: &Selection,
        total_tests: usize,
        changed_units: usize,
        edges_resolved: bool,
    ) -> Self {
        if selection.selected_count == 0 {
            return Self {
                code: ci_exit::EMPTY,
                reason: "no tests selected".to_string(),
            };
        }

        // Degenerate full-suite selection on a real change (testaruda-ls4t):
        // recall-safe but the minimal-selection promise silently failed.
        // Classify as FULL_RUN and surface the cause instead of "success".
        if total_tests > 0 && selection.selected_count == total_tests && changed_units > 0 {
            let reason = if edges_resolved {
                "run everything: coverage/confidence floor".to_string()
            } else {
                "over-selected: no dependency edges resolved — all tests selected for recall safety"
                    .to_string()
            };
            return Self {
                code: ci_exit::FULL_RUN,
                reason,
            };
        }

        // Check if any selected test has low confidence (TIA-CI-002)
        let has_low_confidence = selection.tests.iter().any(|t| t.confidence < 1.0);
        if has_low_confidence {
            return Self {
                code: ci_exit::FULL_RUN,
                reason: "confidence below threshold".to_string(),
            };
        }

        Self {
            code: ci_exit::SUCCESS,
            reason: "selection complete".to_string(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        self.code
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

/// Machine-readable CI plan (TIA-CI-006).
#[derive(serde::Serialize)]
pub struct CiPlan<'a> {
    pub shadow_mode: bool,
    pub exit_code: i32,
    pub selected_count: usize,
    pub changed_count: usize,
    pub reason: &'a str,
    /// When true, indicates that ALL tests should run (shadow mode or low confidence).
    pub all_tests: bool,
    /// The computed selection (may be empty in shadow mode summary output).
    pub tests: &'a [SelectedTest],
}

// ===== Shared helpers =====

/// Find the project root by looking for .git or testaruda.toml.
pub fn find_project_root() -> miette::Result<std::path::PathBuf> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Cannot get current directory: {}", e))?;
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() || ancestor.join("testaruda.toml").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(cwd)
}

/// Fallback: run `cargo test` and exit with its exit code.
/// Used by --safe mode when pre-flight checks fail or confidence is low.
pub fn run_cargo_test_fallback() -> ! {
    eprintln!("  \u{25b6}\u{fe0f}  cargo test (fallback)");
    let status = std::process::Command::new("cargo")
        .args(["test"])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("  \u{274c}  Failed to run cargo test: {}", e);
            std::process::exit(1)
        });
    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        eprintln!(
            "  \u{26a0}\u{fe0f}  cargo test was interrupted (signal {})",
            signal
        );
        std::process::exit(128 + signal);
    }
    let code = status.code().unwrap_or(1);
    std::process::exit(code);
}

/// Populate store from adapters: run discover + static-deps on the full project
/// tree, then process any changed files from the delta.
///
/// Previously, only changed files were processed, leaving the store's dependency
/// graph incomplete on first invocation (TIA-ADAPT-004, TIA-ADAPT-005).
pub fn run_adapter_pipeline(
    store: &Store,
    registry: &AdapterRegistry,
    delta: &ChangeSet,
    exclude: &[String],
) -> std::result::Result<(), String> {
    // Step 1: Walk the project tree to find all files matching registered adapters
    let mut adapter_files: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(testaruda::config::make_exclude_filter(exclude))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        let clean_path = path.strip_prefix("./").unwrap_or(&path);
        if let Some(binary) = registry.resolve(clean_path) {
            adapter_files
                .entry(binary.to_string())
                .or_default()
                .push(clean_path.to_string());
        }
    }

    // Step 2: For each adapter, run discover + static-deps on all files
    for (binary, files) in &adapter_files {
        let mut adapter = match spawn_adapter(binary, None) {
            Ok(a) => a,
            Err(e) => {
                // Extract just the resolved binary name for the diagnostic (TIA-ADAPT-024)
                let binary_name = testaruda::adapter::parse_command_string(binary)
                    .map(|(b, _)| b)
                    .unwrap_or_else(|_| binary.to_string());
                eprintln!("  ⚠️  Failed to spawn adapter {}: {}", binary_name, e);
                continue;
            }
        };

        // Run discover (TIA-ADAPT-004)
        match adapter.discover() {
            Ok(items) => {
                eprintln!(
                    "  📋 {} discovered {} test items",
                    adapter.name,
                    items.len()
                );
                store
                    .store_test_items(&adapter.name, &items)
                    .map_err(|e| format!("store error: {}", e))?;
            }
            Err(e) => {
                eprintln!("  ⚠️  Discover failed for {}: {}", binary, e);
            }
        }

        // Run static-deps on ALL files for this adapter (not just changed files)
        // This builds the full import graph so the store has complete dependency
        // data for any file that might be changed in future selections.
        match adapter.static_deps(files) {
            Ok(result) => {
                eprintln!(
                    "  🔗 {} computed {} edges from {} files",
                    adapter.name,
                    result.edges.len(),
                    files.len()
                );
                store
                    .store_static_deps(&adapter.name, &result.edges)
                    .map_err(|e| format!("store error: {}", e))?;
            }
            Err(e) => {
                eprintln!("  ⚠️  Static-deps failed for {}: {}", binary, e);
            }
        }
    }

    // Step 3: Also process any changed files that aren't already covered
    // (e.g., files with no registered adapter, or files in directories
    // that were excluded from the full walk)
    for path in &delta.files {
        if let Some(binary) = registry.resolve(path) {
            if adapter_files.contains_key(binary) {
                continue; // already processed in the full walk
            }
            // Edge case: a changed file for an adapter not seen in the full walk
            let mut adapter = match spawn_adapter(binary, None) {
                Ok(a) => a,
                Err(e) => {
                    // Extract just the resolved binary name for the diagnostic (TIA-ADAPT-024)
                    let binary_name = testaruda::adapter::parse_command_string(binary)
                        .map(|(b, _)| b)
                        .unwrap_or_else(|_| binary.to_string());
                    eprintln!("  ⚠️  Failed to spawn adapter {}: {}", binary_name, e);
                    continue;
                }
            };
            if let Ok(items) = adapter.discover() {
                store
                    .store_test_items(&adapter.name, &items)
                    .map_err(|e| format!("store error: {}", e))?;
            }
            if let Ok(result) = adapter.static_deps(std::slice::from_ref(path)) {
                store
                    .store_static_deps(&adapter.name, &result.edges)
                    .map_err(|e| format!("store error: {}", e))?;
            }
        }
    }

    Ok(())
}

/// Discover-only: run adapter discovery on a broader set of files.
pub fn run_discover_pipeline(
    store: &Store,
    registry: &AdapterRegistry,
    exclude: &[String],
) -> std::result::Result<(), String> {
    // Walk the project to find files matching registered extensions
    let mut seen_adapters = std::collections::HashSet::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(testaruda::config::make_exclude_filter(exclude))
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path().to_string_lossy().to_string();
        if let Some(binary) = registry.resolve(&path) {
            if !seen_adapters.insert(binary.to_string()) {
                continue;
            }

            let mut adapter = match spawn_adapter(binary, None) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("  ⚠️  Failed to spawn {}: {}", binary, e);
                    continue;
                }
            };

            match adapter.discover() {
                Ok(items) => {
                    eprintln!("  📋 {} found {} tests", adapter.name, items.len());
                    store
                        .store_test_items(&adapter.name, &items)
                        .map_err(|e| format!("store error: {}", e))?;
                }
                Err(e) => {
                    eprintln!("  ⚠️  Discover failed for {}: {}", binary, e);
                }
            }
        }
    }

    Ok(())
}

// ===== Command handlers =====

/// `testaruda init` — initialize store and config in the current project.
pub fn init() -> miette::Result<()> {
    println!("🔧 Initializing testaruda store...");
    let store = Store::open_default()?;
    store.initialize()?;
    let project_root = Store::find_project_root()?;

    // Use Scaffold builder for directory creation
    use genesis::scaffold::Scaffold;
    let scaffold = Scaffold::new(&project_root).dir(".testaruda");
    let _result = scaffold
        .build()
        .map_err(|e| miette::miette!("Failed to scaffold project: {}", e))?;

    // Write default config if it doesn't exist (dynamic content based on language detection)
    if !project_root.join("testaruda.toml").exists() {
        testaruda::config::Config::write_default(&project_root)?;
        let detected = testaruda::config::detect_project_language(&project_root);
        report_detected_language(&detected);
    }

    // Register in genesis tool manifest (v0.4.0 discovery module)
    match genesis::discovery::register(
        &project_root,
        "testaruda",
        "Language-agnostic test selection engine",
        "file",
        "testaruda.toml",
    ) {
        Ok(_) => println!("  ✓ Registered in .genesis/tools.toml"),
        Err(e) => eprintln!("  ⚠️  Failed to register: {}", e),
    }

    // Check for Soufflé oracle (TIA-ENG-010)
    match std::process::Command::new("souffle")
        .arg("--version")
        .output()
    {
        Ok(_) => println!("  ✓ Soufflé oracle found"),
        Err(_) => println!(
            "  ⚠️  Soufflé not found — oracle validation disabled. \
             Install souffle-lang from https://souffle-lang.github.io"
        ),
    }
    // Inject managed blocks into AGENTS.md if it exists
    let agents_path = project_root.join("AGENTS.md");
    if agents_path.exists() {
        inject_beads_block(&agents_path);
    }

    println!("✅ testaruda initialized at {}", project_root.display());
    Ok(())
}

/// Print which language adapter was auto-selected during `init`.
fn report_detected_language(detected: &Option<String>) {
    match detected {
        Some(ref adapter) if adapter.contains("rust") => {
            println!("  🦀 Rust project detected — default adapter set to testaruda-adapter-rust");
        }
        Some(ref adapter) if adapter.contains("python") => {
            println!(
                "  🐍 Python project detected — default adapter set to testaruda-adapter-python"
            );
        }
        Some(ref adapter) if adapter.contains("julia") => {
            println!(
                "  🔬 Julia project detected — default adapter set to testaruda-adapter-julia"
            );
        }
        Some(ref adapter) if adapter.contains("typescript") => {
            println!("  🟦 TypeScript project detected — default adapter set to testaruda-adapter-typescript");
        }
        Some(ref adapter) if adapter.contains("clojure") => {
            println!(
                "  🟢 Clojure project detected — default adapter set to testaruda-adapter-clojure"
            );
        }
        _ => {
            println!("  📁 Project language not detected — default adapter set to testaruda-adapter-rust");
        }
    }
}

/// Inject the BEADS managed block into an existing AGENTS.md.
fn inject_beads_block(agents_path: &std::path::Path) {
    let injector = genesis::managed_block::BlockInjector::new(build_block_registry());

    // Ensure BEADS managed block is present
    let beads_content = "\n<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->\n## Beads Issue Tracker\n\nThis project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.\n\n### Quick Reference\n\n```bash\nbd ready              # Find available work\nbd show <id>          # View issue details\nbd update <id> --claim  # Claim work\nbd close <id>         # Complete work\n```\n\n### Rules\n\n- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists\n- Run `bd prime` for detailed command reference and session close protocol\n- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files\n\n## Session Completion\n\n**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.\n\n**MANDATORY WORKFLOW:**\n\n1. **File issues for remaining work** - Create issues for anything that needs follow-up\n2. **Run quality gates** (if code changed) - Tests, linters, builds\n3. **Update issue status** - Close finished work, update in-progress items\n4. **PUSH TO REMOTE** - This is MANDATORY.\n5. **Clean up** - Clear stashes, prune remote branches\n6. **Verify** - All changes committed AND pushed\n7. **Hand off** - Provide context for next session\n<!-- END BEADS INTEGRATION -->\n";
    match injector.inject(agents_path, "BEADS", beads_content) {
        Ok(r) => tracing::info!(event = "managed_block_injected", block = "BEADS", result = ?r),
        Err(e) => eprintln!("  ⚠️  Failed to inject BEADS block: {}", e),
    }
}

/// Arguments for the `exec` command (gh-26 / testaruda-n5b4).
pub struct ExecArgs {
    pub base: Option<String>,
    pub head: Option<String>,
    pub files: Option<String>,
    pub ordering: TestOrdering,
    pub threshold: f64,
}

/// `testaruda exec` — the full selection loop: select → run → ingest →
/// calibrate.
///
/// Selection-only consumers (e.g. `select --json` piped nowhere) pay selection
/// cost without ever feeding results back. `exec` completes the loop: runs the
/// selected tests via the adapter (TIA-CI-008 path), ingests results into run
/// history, then evaluates the calibration gate. Warns when the store has no
/// run history — selection is then purely static-edge analysis and can't
/// improve from observed behavior.
pub fn exec(args: ExecArgs) -> miette::Result<()> {
    // Uncalibrated advisory, checked BEFORE selection: on non-zero selection
    // outcomes (10/20) select exits the process with the CI code, so a
    // post-run check would never fire on those paths.
    {
        let store = Store::open_default()?;
        if store.check_initialized().is_ok() && store.run_history_is_empty().unwrap_or(false) {
            eprintln!(
                "⚠️  Store is uncalibrated: no ingested run history. Selection is purely static-edge analysis — ingest real runs (exec does it automatically) so selection can improve."
            );
        }
    }

    select(SelectArgs {
        base: args.base,
        head: args.head,
        files: args.files,
        shadow: false,
        format: CliFormat {
            json: false,
            human: false,
        },
        agent: false,
        pre_edit: false,
        ci: true,
        safe: false,
        ordering: args.ordering,
    })?;

    calibrate(args.threshold)
}

/// `testaruda calibrate` — evaluate the predictive ranking calibration gate (TIA-VER-005).
pub fn calibrate(threshold: f64) -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    let metrics = store.evaluate_ranking_calibration()?;
    println!("=== Predictive Ranking Calibration Gate ===");
    println!("Hold-out tests: {}", metrics.total_test_items);
    println!("Actual failures: {}", metrics.total_failures);
    println!(
        "Captured failures (recall@k): {} (k={})",
        metrics.captured_failures, metrics.k
    );
    println!("Recall@k: {:.3}", metrics.recall_at_k);
    println!("Threshold: {:.3}", threshold);
    if metrics.total_test_items == 0 {
        println!("⚠️  Insufficient run history for calibration — need at least 2 distinct runs");
    } else if metrics.recall_at_k >= threshold {
        println!("✓ CALIBRATED — ranking model meets recall threshold");
    } else {
        println!(
            "✗ NOT CALIBRATED — recall {:.3} below threshold {:.3}",
            metrics.recall_at_k, threshold
        );
    }
    Ok(())
}

/// Arguments for the `select` command.
pub struct SelectArgs {
    pub base: Option<String>,
    pub head: Option<String>,
    pub files: Option<String>,
    pub shadow: bool,
    pub format: CliFormat,
    pub agent: bool,
    pub pre_edit: bool,
    pub ci: bool,
    pub safe: bool,
    pub ordering: TestOrdering,
}

/// `testaruda select` — select affected tests from a code change.
pub fn select(args: SelectArgs) -> miette::Result<()> {
    // Safe mode: pre-flight checks with fallback to cargo test
    if args.safe {
        preflight_safe()?;
    }

    let store = Store::open_default()?;
    store.check_initialized()?;
    let delta = ChangeSet::from_diff(
        args.base.as_deref(),
        args.head.as_deref(),
        args.files.as_deref(),
    )?;

    // Adapter pipeline: populate store via adapters before selection
    let project_root = find_project_root()?;
    let config = testaruda::config::Config::load_or_default(&project_root);
    let registry = config.adapters.to_registry();
    if let Err(e) = run_adapter_pipeline(&store, &registry, &delta, &config.discover.exclude) {
        eprintln!("⚠️  Adapter warning: {} (using existing store data)", e);
    }

    // Agent mode implies deterministic ordering (TIA-AGENT-007)
    // and overrides any explicit --ordering flag
    let ordering = if args.agent {
        TestOrdering::Deterministic
    } else {
        args.ordering
    };

    // Load selection context once, reuse for both engine and agent output
    let ctx = store.load_selection_context(&delta)?;
    let changed_ids = ctx.changed.clone();
    let unresolved_ids = ctx.unresolved.clone();
    let engine = Engine::new(&store);
    let selection = engine.select_with_context(ctx, ordering)?;

    // Persist provenance for this selection run (TIA-PROV-005)
    let run_id = store.generate_run_id()?;
    let all_affected_ids = store.get_test_ids_for_content_units(&changed_ids, &unresolved_ids)?;
    if let Err(e) = store.persist_provenance(&run_id, &selection, &all_affected_ids) {
        eprintln!("⚠️  Provenance warning: {} (selection still complete)", e);
    }

    // Determine CI exit code (TIA-CI-001..004)
    // edges_resolved: did any changed/unresolved unit link to tests? If the
    // engine over-selected everything with zero linking edges, the outcome
    // must say so instead of reporting success (testaruda-ls4t).
    let total_tests = store.test_items_count().unwrap_or(0);
    let changed_units = selection.changed_count + unresolved_ids.len();
    let edges_resolved = !all_affected_ids.is_empty();
    let mut outcome =
        CiOutcome::from_selection(&selection, total_tests, changed_units, edges_resolved);

    // Shadow mode (TIA-CI-007): report all tests should run
    if args.shadow {
        let all_tests_count = store.test_items_count().unwrap_or(0);
        outcome = CiOutcome {
            code: ci_exit::FULL_RUN,
            reason: if all_tests_count > 0 {
                format!(
                    "shadow mode — {} known tests should all run",
                    all_tests_count
                )
            } else {
                "shadow mode — all tests should run".to_string()
            },
        };
    }

    // Emit metrics (TIA-OBS-003)
    tracing::info!(
        event = "selection",
        run_id = %run_id,
        changed_count = selection.changed_count,
        selected_count = selection.selected_count,
        total_tests = store.test_items_count().unwrap_or(0),
        exit_code = outcome.exit_code(),
        reason = %outcome.reason(),
    );

    // CI mode (TIA-CI-008): run selected tests and ingest results. On runner
    // failure run_ci_tests exits with the runner's code after ingesting; on
    // success we fall through and preserve the selection outcome code (10/20)
    // for the CI contract (testaruda-ls4t) — the run has already happened.
    let effective_ci = args.ci || args.safe;
    let state = SelectState {
        store: &store,
        selection: &selection,
        outcome: &outcome,
        changed_ids: &changed_ids,
        unresolved_ids: &unresolved_ids,
        shadow: args.shadow,
        format: args.format,
        agent: args.agent,
        pre_edit: args.pre_edit,
        safe: args.safe,
        defer_exit: effective_ci && !selection.tests.is_empty(),
    };
    emit_select_output(&state)?;

    if effective_ci && !selection.tests.is_empty() {
        run_ci_tests(&CiRunCtx {
            store: &store,
            registry: &registry,
            selection: &selection,
        })?;
        if !args.safe {
            let code = outcome.exit_code();
            if code != 0 {
                std::process::exit(code);
            }
        }
    }

    Ok(())
}

/// Borrowed view of a `select` run, shared with the output emitter.
struct SelectState<'a> {
    store: &'a Store,
    selection: &'a Selection,
    outcome: &'a CiOutcome,
    changed_ids: &'a [u32],
    unresolved_ids: &'a [u32],
    shadow: bool,
    format: CliFormat,
    agent: bool,
    pre_edit: bool,
    safe: bool,
    /// CI mode defers the emitter's exit-on-outcome so the selected tests can
    /// run and ingest first (testaruda-1m3i): a degenerate selection (10/20)
    /// on a cold store must not abort the select→run→ingest→calibrate loop.
    defer_exit: bool,
}

/// `--safe` pre-flight: verify the project is initialized; otherwise fall back
/// to `cargo test`. Diverges on fallback.
fn preflight_safe() -> miette::Result<()> {
    let project_root = match find_project_root() {
        Ok(p) => p,
        Err(_) => {
            eprintln!(
                "  \u{26a0}\u{fe0f}  testaruda not initialized (no .git or testaruda.toml found)"
            );
            run_cargo_test_fallback();
        }
    };
    if !project_root.join("testaruda.toml").exists() {
        eprintln!("  \u{26a0}\u{fe0f}  testaruda not configured (no testaruda.toml)");
        run_cargo_test_fallback();
    }
    if !project_root.join(".testaruda").exists() {
        eprintln!("  \u{26a0}\u{fe0f}  testaruda store not initialized");
        run_cargo_test_fallback();
    }
    Ok(())
}

/// Emit selection output in the requested format (agent / json / pre-edit / human).
fn emit_select_output(state: &SelectState) -> miette::Result<()> {
    let SelectState {
        store,
        selection,
        outcome,
        changed_ids,
        unresolved_ids,
        shadow,
        format,
        agent,
        pre_edit,
        safe,
        defer_exit,
    } = state;

    if *agent {
        emit_agent_output(
            store,
            selection,
            outcome,
            changed_ids,
            unresolved_ids,
            *defer_exit,
        )?;
    } else if *pre_edit {
        emit_pre_edit(
            store,
            selection,
            outcome,
            changed_ids,
            unresolved_ids,
            *defer_exit,
        )?;
    } else {
        // Use global CliFormat for Human vs Json dispatch (genesis v0.4.0)
        match format.format() {
            OutputFormat::Json => emit_json_plan(selection, outcome, *shadow, *defer_exit)?,
            OutputFormat::Human => {
                emit_human_output(selection, outcome, *shadow, *safe, *defer_exit)?
            }
        }
    }
    Ok(())
}

/// Agent output format (TIA-AGENT-001).
#[allow(clippy::too_many_arguments)]
fn emit_agent_output(
    store: &Store,
    selection: &Selection,
    outcome: &CiOutcome,
    changed_ids: &[u32],
    unresolved_ids: &[u32],
    defer_exit: bool,
) -> miette::Result<()> {
    let changed_units = build_changed_units(store, changed_ids, unresolved_ids);

    // Build test node ID map for all selected tests
    let mut test_node_ids = std::collections::HashMap::new();
    for t in &selection.tests {
        if let Ok(node_id) = store.get_test_node_id(t.id) {
            test_node_ids.insert(t.id, node_id);
        }
    }

    // Get candidate test IDs (all tests that have deps on changed units)
    let candidate_ids = store.get_test_ids_for_content_units(changed_ids, unresolved_ids)?;

    let output = testaruda::agent::AgentOutput::from_selection(
        store,
        selection,
        &changed_units,
        &test_node_ids,
        &candidate_ids,
    )?;

    let out = serde_json::to_string_pretty(&output)
        .map_err(|e| miette!("Agent output serialization failed: {}", e))?;
    println!("{}", out);

    // Agent output is a machine-readable CI decision contract (testaruda-9lbm):
    // the process status must carry the outcome code exactly like JSON plan
    // mode (testaruda-fnyi) and human mode. A "tested nothing" agent run
    // must not report green. Flush before exiting — piped stdout is
    // block-buffered and std::process::exit must not lose the payload
    // (testaruda-b7ks).
    use std::io::Write;
    std::io::stdout().flush().ok();
    let code = outcome.exit_code();
    if code != 0 && !defer_exit {
        std::process::exit(code);
    }
    Ok(())
}

/// Machine-readable JSON plan (TIA-CI-006) via genesis envelope.
fn emit_json_plan(
    selection: &Selection,
    outcome: &CiOutcome,
    shadow: bool,
    defer_exit: bool,
) -> miette::Result<()> {
    let plan = CiPlan {
        shadow_mode: shadow,
        exit_code: outcome.exit_code(),
        selected_count: selection.selected_count,
        changed_count: selection.changed_count,
        reason: outcome.reason(),
        all_tests: shadow,
        tests: &selection.tests,
    };
    let envelope = genesis::envelope::Envelope::success(
        env!("CARGO_PKG_VERSION"),
        genesis::envelope::EnvelopeKind::List,
        plan,
        vec![],
        vec![],
    );
    let out = serde_json::to_string_pretty(&envelope)
        .map_err(|e| miette!("JSON serialization failed: {}", e))?;
    println!("{}", out);

    // Preserve CI exit status in JSON mode (testaruda-fnyi): match human mode.
    // Flush explicitly before exiting — piped stdout is block-buffered and
    // std::process::exit must not lose the CI decision payload (testaruda-b7ks).
    use std::io::Write;
    std::io::stdout().flush().ok();
    let code = outcome.exit_code();
    if code != 0 && !defer_exit {
        std::process::exit(code);
    }
    Ok(())
}

/// Pre-edit blast radius (TIA-AGENT-005): structured JSON output.
#[allow(clippy::too_many_arguments)]
fn emit_pre_edit(
    store: &Store,
    selection: &Selection,
    outcome: &CiOutcome,
    changed_ids: &[u32],
    unresolved_ids: &[u32],
    defer_exit: bool,
) -> miette::Result<()> {
    let mut changed_files = Vec::new();
    for &cu_id in changed_ids {
        if let Ok((path, _, _)) = store.get_content_unit_info(cu_id) {
            changed_files.push(path);
        }
    }
    for &cu_id in unresolved_ids {
        if let Ok((path, _, _)) = store.get_content_unit_info(cu_id) {
            changed_files.push(path);
        }
    }
    let selected_tests = collect_test_node_ids(store, &selection.tests);
    let candidate_count = store
        .get_test_ids_for_content_units(changed_ids, unresolved_ids)?
        .len();

    let output = PreEditOutput {
        format: "testaruda-pre-edit-v1".to_string(),
        summary: SummaryStats {
            changed_count: selection.changed_count,
            selected_count: selection.selected_count,
            candidate_count,
            has_coverage_gaps: false,
        },
        changed_files,
        selected_tests,
    };
    let out = serde_json::to_string_pretty(&output)
        .map_err(|e| miette!("Pre-edit output serialization failed: {}", e))?;
    println!("{}", out);

    // Preserve CI exit status in pre-edit mode (testaruda-ljeg): mirror
    // agent mode (testaruda-9lbm) and JSON plan mode (testaruda-fnyi).
    // Flush explicitly before exiting — piped stdout is block-buffered and
    // std::process::exit must not lose the CI decision payload (testaruda-b7ks).
    use std::io::Write;
    std::io::stdout().flush().ok();
    let code = outcome.exit_code();
    if code != 0 && !defer_exit {
        std::process::exit(code);
    }
    Ok(())
}

/// Human-readable output (CORR-004: include reason).
fn emit_human_output(
    selection: &Selection,
    outcome: &CiOutcome,
    shadow: bool,
    safe: bool,
    defer_exit: bool,
) -> miette::Result<()> {
    let reason_note = outcome.reason();
    if shadow {
        println!("⚠️  Shadow mode — selection computed but all tests should run");
    }
    if reason_note != "selection complete" {
        println!("ℹ️  {}", reason_note);
    }
    let out = serde_json::to_string_pretty(&selection)
        .map_err(|e| miette!("JSON serialization failed: {}", e))?;
    println!("{}", out);

    // Flush before exit so the CI decision payload is never lost
    // (piped stdout is block-buffered; testaruda-b7ks).
    use std::io::Write;
    std::io::stdout().flush().ok();

    let code = outcome.exit_code();
    if code != 0 {
        if safe && code == 10 {
            eprintln!("  \u{26a0}\u{fe0f}  Low confidence — running full test suite");
            run_cargo_test_fallback();
        }
        if safe && code == 20 {
            eprintln!("  \u{2705}  No tests affected by this change — skipping");
            std::process::exit(0);
        }
        if !defer_exit {
            std::process::exit(code);
        }
    }
    Ok(())
}

/// Build the changed-unit list for agent output, marking unresolved units.
fn build_changed_units(
    store: &Store,
    changed_ids: &[u32],
    unresolved_ids: &[u32],
) -> Vec<ChangedUnit> {
    let mut changed_units = Vec::new();
    for &cu_id in changed_ids {
        if let Ok((path, symbol, kind)) = store.get_content_unit_info(cu_id) {
            changed_units.push(ChangedUnit {
                id: cu_id,
                path,
                symbol,
                kind,
                unresolved: false,
            });
        }
    }
    for &cu_id in unresolved_ids {
        if let Ok((path, symbol, kind)) = store.get_content_unit_info(cu_id) {
            changed_units.push(ChangedUnit {
                id: cu_id,
                path,
                symbol,
                kind,
                unresolved: true,
            });
        }
    }
    changed_units
}

/// Collect node IDs for the given tests (skips any that fail lookup).
fn collect_test_node_ids(store: &Store, tests: &[SelectedTest]) -> Vec<String> {
    collect_selected_by_adapter(store, tests)
        .into_iter()
        .flat_map(|(_, ids)| ids)
        .collect()
}

/// Group selected test node_ids by the adapter that discovered them.
///
/// Keys are handshake names as stored in test_items.adapter (e.g.
/// "python-adapter"); groups preserve first-seen order. Stale ids (item no
/// longer in the store) are skipped, matching collect_test_node_ids.
fn collect_selected_by_adapter(
    store: &Store,
    tests: &[SelectedTest],
) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for t in tests {
        let Ok((node_id, adapter)) = store.get_test_identity(t.id) else {
            continue;
        };
        if let Some(group) = groups.iter_mut().find(|(a, _)| *a == adapter) {
            group.1.push(node_id);
        } else {
            groups.push((adapter, vec![node_id]));
        }
    }
    groups
}

/// How many stderr tail lines to print when a runner fails (testaruda-ylyl).
const RUNNER_STDERR_TAIL_LINES: usize = 5;

/// Borrowed view for the CI test-running phase of `select`.
struct CiRunCtx<'a> {
    store: &'a Store,
    registry: &'a AdapterRegistry,
    selection: &'a Selection,
}

/// CI mode (TIA-CI-008): run selected tests via the adapters and ingest results.
///
/// The selection is grouped by discovering adapter (testaruda-kkno): a
/// polyglot repo runs one runner per adapter group. Resolving a single
/// adapter from selected[0] sent every group's ids to it — pytest received
/// .ts paths, exited 4, ingested nothing, and the store stayed cold forever.
fn run_ci_tests(ctx: &CiRunCtx) -> miette::Result<()> {
    let groups = collect_selected_by_adapter(ctx.store, &ctx.selection.tests);
    if groups.is_empty() {
        return Ok(());
    }

    // Map handshake names (test_items.adapter) to binaries by spawning each
    // distinct registry binary once and reading its handshake name. There is
    // no static name→binary mapping: the handshake is the source of truth.
    let mut binaries: Vec<&str> = ctx.registry.extensions().map(|(_, b)| b).collect();
    if let Some(default) = ctx.registry.default_binary() {
        if !binaries.contains(&default) {
            binaries.push(default);
        }
    }
    let mut known_adapters: Vec<(String, &str)> = Vec::new();
    for binary in &binaries {
        match spawn_adapter(binary, None) {
            Ok(adapter) => {
                known_adapters.push((adapter.name.clone(), binary));
            }
            Err(e) => {
                eprintln!("⚠️  CI: failed to spawn adapter {}: {}", binary, e);
            }
        }
    }

    // Run each adapter group's runner and ingest its results. Every group is
    // processed before exiting so a failing runner doesn't starve the other
    // groups' run history (the ingest-before-exit contract, TIA-CI-008).
    let mut first_failure: Option<i32> = None;
    let mut spawned_all = true;
    for (adapter_name, node_ids) in &groups {
        let Some(binary) = known_adapters
            .iter()
            .find(|(name, _)| name == adapter_name)
            .map(|(_, b)| *b)
        else {
            eprintln!(
                "⚠️  CI: no adapter binary answers to handshake name '{}' — skipping {} test(s)",
                adapter_name,
                node_ids.len()
            );
            spawned_all = false;
            continue;
        };
        match run_adapter_group(ctx, binary, node_ids) {
            Ok(Some(code)) => {
                if first_failure.is_none() {
                    first_failure = Some(code);
                }
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("⚠️  CI: {}: {}", adapter_name, e);
                spawned_all = false;
            }
        }
    }

    // All runnable groups have executed and ingested before reporting errors.
    if !spawned_all {
        return Err(miette!("CI: incomplete execution of selected test groups"));
    }
    if let Some(code) = first_failure {
        eprintln!("  ❌  CI: test runner failed with exit code {}", code);
        std::process::exit(code);
    }

    Ok(())
}

/// Run one adapter group's runner and ingest its results.
///
/// Returns Some(exit_code) when the runner failed, None on successful execution,
/// and an error when execution or ingestion could not complete.
fn run_adapter_group(
    ctx: &CiRunCtx,
    adapter_binary: &str,
    node_ids: &[String],
) -> miette::Result<Option<i32>> {
    let mut adapter = match spawn_adapter(adapter_binary, None) {
        Ok(a) => a,
        Err(e) => {
            return Err(miette!("failed to spawn adapter {}: {}", adapter_binary, e));
        }
    };

    // Get runner args from the adapter
    let run_args_result = match adapter.run_args(node_ids) {
        Ok(r) => r,
        Err(e) => {
            return Err(miette!("failed to get run args: {}", e));
        }
    };

    let Some((runner, args)) = run_args_result.runner_args.split_first() else {
        return Err(miette!(
            "adapter {} returned empty runner arguments",
            adapter_binary
        ));
    };
    // Snapshot the collection file's mtime so a stale result file from a
    // previous run is never mistaken for this run's output.
    let collection_mtime = std::fs::metadata(&run_args_result.collection_path)
        .ok()
        .and_then(|m| m.modified().ok());
    eprintln!("  🏃 CI: running tests via {}...", adapter_binary);
    let output = match std::process::Command::new(runner).args(args).output() {
        Ok(o) => o,
        Err(e) => {
            return Err(miette!("failed to run tests via {}: {}", runner, e));
        }
    };

    // Ingest FIRST, then report the exit code. This preserves the feedback
    // loop: failed runs are recorded (TIA-CI-008).
    let combined = combine_output(&output.stdout, &output.stderr);
    let run_output = run_output_for_ingest(
        &run_args_result.collection_path,
        collection_mtime,
        &combined,
    );
    eprintln!("  📥 CI: ingesting results...");
    ingest_ci_results(&mut adapter, ctx.store, &run_output)?;

    if output.status.success() {
        return Ok(None);
    }
    // Surface the tail of the runner's stderr so the failure is actionable
    // (testaruda-ylyl): "exit code 101" alone hid julia's
    // "Package Aqua not found in current path" remediation line.
    let tail = stderr_tail(&output.stderr, RUNNER_STDERR_TAIL_LINES);
    if !tail.is_empty() {
        eprintln!("  ❌  CI: runner stderr (last {RUNNER_STDERR_TAIL_LINES} lines):\n{tail}");
    }
    Ok(Some(output.status.code().unwrap_or(1)))
}

/// Last lines of a runner's stderr, for failure diagnosis (testaruda-ylyl).
///
/// The runner's output is already forwarded to the adapter for ingest, but a
/// failed run previously printed only "test runner failed with exit code N" —
/// the operator had no way to see WHY (e.g. julia's "Package Aqua not found
/// in current path" from un-instantiated `[extras]` test deps). Returns the
/// final `max_lines` non-empty stderr lines, verbatim, as a single string
/// (empty when the runner wrote nothing to stderr).
fn stderr_tail(stderr: &[u8], max_lines: usize) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

/// Combine a test runner's stdout/stderr into a single string for adapter ingest.
fn combine_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout_s = String::from_utf8_lossy(stdout);
    let stderr_s = String::from_utf8_lossy(stderr);
    if stderr_s.is_empty() {
        stdout_s.into_owned()
    } else {
        format!("{}\n{}", stdout_s, stderr_s)
    }
}

/// Decide what to feed the adapter's ingest after a runner run.
///
/// Adapters whose runners write structured results to a file (vitest
/// `--outputFile`, pytest `--junitxml`) declare that path as
/// `collection_path` in their run-args response — the XML never reaches
/// stdout, so feeding the runner's console output to ingest parses 0
/// testcases and the store stays cold (tambor-06p). Prefer the collection
/// file's contents when the runner (re)wrote it during this run; a stale
/// file from a previous run must not be ingested, and adapters with no
/// collection_path (mocha, ava) fall back to the combined stdout/stderr.
fn run_output_for_ingest(
    collection_path: &str,
    mtime_before: Option<std::time::SystemTime>,
    combined: &str,
) -> String {
    if collection_path.is_empty() {
        return combined.to_string();
    }
    let Ok(meta) = std::fs::metadata(collection_path) else {
        return combined.to_string();
    };
    let rewritten = match (meta.modified().ok(), mtime_before) {
        (Some(after), Some(before)) => after > before,
        (Some(_), None) => true, // file appeared during the run
        (None, _) => false,
    };
    if !rewritten {
        return combined.to_string();
    }
    match std::fs::read_to_string(collection_path) {
        Ok(content) if !content.trim().is_empty() => content,
        _ => combined.to_string(),
    }
}

/// Parse combined runner output via the adapter and persist results to the store.
fn ingest_ci_results(
    adapter: &mut testaruda::adapter::AdapterIO,
    store: &Store,
    combined: &str,
) -> miette::Result<()> {
    match adapter.ingest(combined) {
        Ok(ingest_result) => {
            // Store runtime edges
            if !ingest_result.runtime_edges.is_empty() {
                let _ = store.store_static_deps(&adapter.name, &ingest_result.runtime_edges);
            }

            // Convert per-test results to store format. Runner ids (cargo's
            // `tests::foo`) never equal file-derived node_ids, so resolve
            // tolerantly; report unmatched ids instead of silently dropping
            // them (testaruda-1m3i).
            let mut store_tests = Vec::new();
            let mut unmatched: Vec<&str> = Vec::new();
            for test in &ingest_result.per_test_results {
                if let Some(tid) = store.resolve_test_item_id(&test.test_id) {
                    store_tests.push(serde_json::json!({
                        "id": tid,
                        "outcome": test.outcome,
                        "duration_ms": test.duration_ms,
                    }));
                } else {
                    unmatched.push(&test.test_id);
                }
            }
            if !unmatched.is_empty() {
                eprintln!(
                    "  ⚠️  CI: {} run result(s) matched no known test item: {}",
                    unmatched.len(),
                    unmatched
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            let ci_run_id = store.generate_run_id()?;
            let payload = serde_json::json!({
                "run_id": ci_run_id,
                "tests": store_tests,
                "full_run": true,
            });
            if let Err(e) = store.ingest(&payload) {
                eprintln!("  ⚠️  CI: ingest failed: {}", e);
            } else {
                eprintln!("  ✅ CI: ingested {} test results", store_tests.len());
            }
        }
        Err(e) => {
            eprintln!("  ⚠️  CI: adapter ingest failed: {}", e);
        }
    }
    Ok(())
}

/// Arguments for the `ingest` command.
pub struct IngestArgs {
    pub path: String,
    pub raw: bool,
    pub adapter: Option<String>,
}

/// `testaruda ingest` — ingest test run results to update the model.
pub fn ingest(args: IngestArgs) -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;

    if args.raw {
        ingest_raw(&store, &args.path, args.adapter)?;
    } else {
        ingest_json(&store, &args.path)?;
    }
    Ok(())
}

/// Resolve which adapter binary should parse raw test output.
///
/// Priority: project default adapter, then the first registered extension
/// mapping, then the Python adapter. The default must win over extension
/// iteration order — the first registered extension may map to an adapter
/// for a language the raw output isn't from (testaruda-fgoc).
pub fn resolve_ingest_adapter(registry: &testaruda::adapter::AdapterRegistry) -> String {
    registry
        .default_binary()
        .or_else(|| registry.extensions().next().map(|(_, b)| b))
        .unwrap_or("testaruda-adapter-python")
        .to_string()
}

/// Raw mode: treat file as test runner output, delegate to adapter.
fn ingest_raw(store: &Store, path: &str, adapter: Option<String>) -> miette::Result<()> {
    let raw_output =
        std::fs::read_to_string(path).map_err(|e| miette!("Failed to read {}: {}", path, e))?;

    // Resolve adapter binary: explicit --adapter flag, or auto-detect
    // from the project's config, or fall back to the Python adapter
    let adapter_binary: String = if let Some(explicit) = adapter {
        explicit
    } else {
        let project_root = find_project_root()?;
        let config = testaruda::config::Config::load_or_default(&project_root);
        let registry = config.adapters.to_registry();
        resolve_ingest_adapter(&registry)
    };

    let mut adapter = spawn_adapter(&adapter_binary, None)
        .map_err(|e| miette!("Failed to spawn adapter {}: {}", adapter_binary, e))?;

    let ingest_result = adapter
        .ingest(&raw_output)
        .map_err(|e| miette!("Adapter ingest failed: {}", e))?;

    // Store runtime edges
    if !ingest_result.runtime_edges.is_empty() {
        store
            .store_static_deps(&adapter.name, &ingest_result.runtime_edges)
            .map_err(|e| miette!("Failed to store runtime edges: {}", e))?;
        eprintln!(
            "  🔗 Stored {} runtime edges",
            ingest_result.runtime_edges.len()
        );
    }

    // Convert per-test results to store ingest format
    // Adapter returns test_id strings (e.g., "tests/test_model.py::test_something")
    // Store expects integer test_item_ids — look up from node_id
    let mut store_tests = Vec::new();
    for test in &ingest_result.per_test_results {
        if let Ok(test_item_id) = store.lookup_test_item_id(&test.test_id) {
            store_tests.push(serde_json::json!({
                "id": test_item_id,
                "outcome": test.outcome,
                "duration_ms": test.duration_ms,
            }));
        } else {
            eprintln!("  ⚠️  Unknown test node_id: {} (skipped)", test.test_id);
        }
    }

    let run_id = store.generate_run_id()?;
    let ingest_payload = serde_json::json!({
        "run_id": run_id,
        "tests": store_tests,
        "full_run": true,
    });

    store.ingest(&ingest_payload)?;
    tracing::info!(
        event = "ingest",
        run_id = %run_id,
        test_count = ingest_result.per_test_results.len(),
        runtime_edges = ingest_result.runtime_edges.len(),
        mode = "raw",
    );
    println!(
        "✅ Run ingested ({} tests, {} runtime edges)",
        ingest_result.per_test_results.len(),
        ingest_result.runtime_edges.len()
    );
    Ok(())
}

/// JSON mode: parse JSON and pass to store directly.
fn ingest_json(store: &Store, path: &str) -> miette::Result<()> {
    let data =
        std::fs::read_to_string(path).map_err(|e| miette!("Failed to read {}: {}", path, e))?;
    let results: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| miette!("Failed to parse JSON: {}", e))?;
    store.ingest(&results)?;
    println!("✅ Run ingested");
    Ok(())
}

/// `testaruda graph` — show the current dependency graph.
pub fn graph() -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    let graph = store.export_graph()?;
    let out = serde_json::to_string_pretty(&graph)
        .map_err(|e| miette!("JSON serialization failed: {}", e))?;
    println!("{}", out);
    Ok(())
}

/// `testaruda import` — import a dependency graph from a JSON export.
pub fn import_graph(path: String) -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    let data =
        std::fs::read_to_string(&path).map_err(|e| miette!("Failed to read {}: {}", path, e))?;
    let graph: serde_json::Value =
        serde_json::from_str(&data).map_err(|e| miette!("Failed to parse JSON: {}", e))?;
    store.import_graph(&graph)?;
    println!("✅ Graph imported from {}", path);
    Ok(())
}

/// `testaruda explain` — explain why a test was or was not selected.
pub fn explain(test_id: String, change: Option<String>) -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    let explanation = store.explain(&test_id, change.as_deref())?;
    let out = serde_json::to_string_pretty(&explanation)
        .map_err(|e| miette!("JSON serialization failed: {}", e))?;
    println!("{}", out);
    Ok(())
}

/// `testaruda oracle` — run the Soufflé oracle for cross-validation.
pub fn validate(program: Option<String>) -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    println!("🔮 Soufflé oracle validation");

    // Generate Datalog from the current store
    let datalog = store.generate_datalog()?;
    println!("{}", datalog);

    match program {
        Some(path) => {
            // Write generated Datalog to the specified path
            std::fs::write(&path, &datalog)
                .map_err(|e| miette!("Failed to write Datalog: {}", e))?;
            println!("  Datalog program written to {}", path);
            // Try to run through Soufflé
            match std::process::Command::new("souffle").arg(&path).output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stdout.is_empty() {
                        println!("  Soufflé output:\n{}", stdout);
                    }
                    if !stderr.is_empty() {
                        eprintln!("  Soufflé stderr:\n{}", stderr);
                    }
                }
                Err(e) => {
                    eprintln!(
                        "  ⚠️  Soufflé not found ({}). Install souffle-lang to run the oracle.",
                        e
                    );
                }
            }
        }
        None => {
            println!(
                "  Datalog program generated. Use --program <path> to write to a file and run."
            );
        }
    }
    Ok(())
}

/// `testaruda discover` — discover tests via configured adapters.
pub fn discover() -> miette::Result<()> {
    let store = Store::open_default()?;
    store.initialize()?;
    let project_root = find_project_root()?;
    let config = testaruda::config::Config::load_or_default(&project_root);
    let registry = config.adapters.to_registry();
    run_discover_pipeline(&store, &registry, &config.discover.exclude)
        .map_err(|e| miette!("{}", e))?;
    let count = store.test_items_count().unwrap_or(0);
    println!("\n✅ Discovered {} test items", count);
    Ok(())
}

/// `testaruda metrics` — show operational metrics.
pub fn metrics() -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    let test_count = store.test_items_count().unwrap_or(0);
    let run_count = store.run_count().unwrap_or(0);
    let quarantined_count = store.quarantined_count().unwrap_or(0);
    let schema_version = store.schema_version().unwrap_or(0);

    println!("📊 testaruda metrics");
    println!("  Tests tracked:      {}", test_count);
    println!("  Runs ingested:      {}", run_count);
    println!("  Quarantined (flaky): {}", quarantined_count);
    println!("  Schema version:     {}", schema_version);

    tracing::info!(
        event = "metrics",
        test_count = test_count,
        run_count = run_count,
        quarantined_count = quarantined_count,
        schema_version = schema_version,
    );

    Ok(())
}

/// `testaruda fingerprint` — refresh all content unit fingerprints from disk.
pub fn fingerprint() -> miette::Result<()> {
    let store = Store::open_default()?;
    store.check_initialized()?;
    let updated = store.refresh_fingerprints()?;
    println!("🔑 Refreshed fingerprints for {} content units", updated);
    tracing::info!(event = "fingerprint", updated = updated,);
    Ok(())
}

/// `testaruda doctor` — validate project configuration via genesis suite_linter.
pub fn doctor(fix: bool) -> miette::Result<()> {
    let project_root = find_project_root()?;
    match crate::doctor::run_doctor(&project_root, fix) {
        Ok(report) => {
            // Print human-readable results
            let mut has_errors = false;

            for check in &report.checks {
                match check.status {
                    genesis::doctor::CheckStatus::Pass => {}
                    genesis::doctor::CheckStatus::Warn => {
                        eprintln!("⚠️  {}: {}", check.name, check.message);
                    }
                    genesis::doctor::CheckStatus::Fail => {
                        has_errors = true;
                        eprintln!("❌ {}: {}", check.name, check.message);
                    }
                }
                if let Some(ref fix_cmd) = check.fix {
                    eprintln!("   🔧 Fix: {}", fix_cmd);
                }
            }

            if !has_errors {
                eprintln!("✅ All checks passed");
            }

            let exit_code = report.exit_code();
            if exit_code != 0 {
                std::process::exit(exit_code);
            }
            Ok(())
        }
        Err(e) => Err(miette!("Doctor failed: {}", e)),
    }
}

/// `testaruda feedback` — submit a feedback issue about an error.
///
/// Delegates to [`genesis::feedback::handle_feedback`] for body construction,
/// redaction, and issue filing.
pub fn feedback(args: genesis::feedback::FeedbackArgs) -> miette::Result<()> {
    let project_root = find_project_root()?;

    let repo = env!("CARGO_PKG_REPOSITORY")
        .trim_start_matches("https://")
        .to_string();

    match genesis::feedback::handle_feedback(
        &args,
        "testaruda",
        env!("CARGO_PKG_VERSION"),
        &repo,
        &project_root,
    ) {
        Ok(genesis::feedback::gh::GhResult::Created { url, number }) => {
            println!("✅ Created issue #{}: {}", number, url);
            Ok(())
        }
        Ok(genesis::feedback::gh::GhResult::FallbackUrl(url)) => {
            eprintln!("⚠️  Could not create issue directly.");
            eprintln!("   Open this URL to create the issue manually:");
            eprintln!("   {}", url);
            Ok(())
        }
        Ok(genesis::feedback::gh::GhResult::LocalFile(path)) => {
            eprintln!("⚠️  Network unavailable. Issue saved to:");
            eprintln!("   {}", path.display());
            Ok(())
        }
        Err(msg) => {
            eprintln!("❌ Failed to create issue: {}", msg);
            std::process::exit(1);
        }
    }
}

/// `testaruda status` — display cross-tool health summary.
pub fn status() -> miette::Result<()> {
    let project_root = find_project_root()?;

    use genesis::status::StatusBuilder;
    let runner = crate::doctor::build_doctor_runner();
    let mut builder = StatusBuilder::new();
    builder.register_doctor("testaruda", runner);

    let report = builder
        .build(&project_root)
        .map_err(|e| miette::miette!("Status check failed: {}", e))?;

    let envelope = report.to_envelope(env!("CARGO_PKG_VERSION"));
    let json = serde_json::to_string_pretty(&envelope)
        .map_err(|e| miette::miette!("Serialization failed: {}", e))?;

    println!("{}", json);
    Ok(())
}

/// `testaruda gen-cli-docs` (hidden) — generate CLI documentation in markdown.
pub fn gen_cli_docs() -> miette::Result<()> {
    let markdown = clap_markdown::help_markdown::<crate::Cli>();
    println!("{}", markdown);
    Ok(())
}

/// `testaruda completions` — generate shell completions.
pub fn completions(shell: clap_complete::Shell) -> miette::Result<()> {
    let mut cmd = <crate::Cli as clap::CommandFactory>::command();
    genesis::cli::generate_completions(&mut cmd, shell)
        .map_err(|e| miette::miette!("Failed to generate completions: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use testaruda::adapter::TestItem;

    /// A failed runner's stderr tail must surface its remediation lines
    /// (testaruda-ylyl): julia's
    ///
    ///   "Package Aqua not found in current path.
    ///   - Run `import Pkg; Pkg.add("Aqua")` to install the Aqua package."
    ///
    /// was previously invisible behind "test runner failed with exit code 1".
    #[test]
    fn stderr_tail_returns_last_nonempty_lines() {
        let stderr = b"WARNING: dep
Package Aqua not found in current path.
- Run `import Pkg; Pkg.add(\"Aqua\")` to install the Aqua package.
Stacktrace:
 [1] require
";
        let tail = stderr_tail(stderr, 5);
        assert!(tail.contains("Package Aqua not found"), "{tail}");
        assert!(tail.contains("Pkg.add"), "{tail}");
        assert!(tail.ends_with(" [1] require"), "{tail}");
        assert_eq!(tail.lines().count(), 5, "capped at max_lines: {tail}");
    }

    #[test]
    fn stderr_tail_handles_empty_and_whitespace_only_stderr() {
        assert_eq!(stderr_tail(b"", 5), "");
        assert_eq!(stderr_tail(b"\n\n  \n", 5), "");
    }

    #[test]
    fn stderr_tail_returns_all_lines_when_under_cap() {
        let tail = stderr_tail(
            b"line one
line two",
            5,
        );
        assert_eq!(tail, "line one\nline two");
    }

    /// A runner whose results land in a file (vitest --outputFile, pytest
    /// --junitxml) leaves its JUnit XML out of stdout entirely, so feeding
    /// the console output to ingest parses 0 testcases (tambor-06p). The
    /// adapter-declared collection_path must win when the runner (re)wrote
    /// it during this run; a stale file from a previous run must NOT be
    /// ingested, and adapters with no collection_path fall back to stdout.
    #[test]
    fn run_output_prefers_freshly_written_collection_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-results.xml");
        std::fs::write(&path, "<?xml version=\"1.0\"?><testsuites tests=\"1\"/>").unwrap();

        // mtime_before = None models a file that appeared during the run.
        let out = run_output_for_ingest(path.to_str().unwrap(), None, "console output");
        assert_eq!(out, "<?xml version=\"1.0\"?><testsuites tests=\"1\"/>");
    }

    #[test]
    fn run_output_ignores_stale_collection_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-results.xml");
        std::fs::write(&path, "<?xml version=\"1.0\"?><stale/>").unwrap();

        // mtime_before after the file's mtime models a run that did NOT
        // rewrite the file.
        let stale_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
        let later = stale_mtime + std::time::Duration::from_secs(1);
        let out = run_output_for_ingest(path.to_str().unwrap(), Some(later), "console output");
        assert_eq!(out, "console output");
    }

    #[test]
    fn run_output_falls_back_without_collection_path() {
        assert_eq!(
            run_output_for_ingest("", None, "console output"),
            "console output"
        );
        assert_eq!(
            run_output_for_ingest("/nonexistent/test-results.xml", None, "console output"),
            "console output"
        );
    }

    #[test]
    fn run_output_falls_back_on_empty_collection_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-results.xml");
        std::fs::write(&path, "   \n").unwrap();
        let out = run_output_for_ingest(path.to_str().unwrap(), None, "console output");
        assert_eq!(out, "console output");
    }

    /// Polyglot selections must group by the adapter that discovered each
    /// test item (testaruda-kkno): run_ci_tests used to resolve ONE adapter
    /// from selected[0] and send every group's ids to it — pytest received
    /// .ts paths, exited 4, ingested 0 results, and the store stayed cold.
    #[test]
    fn group_selected_by_adapter_separates_adapters() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let py_items: Vec<TestItem> = (0..3)
            .map(|i| TestItem {
                node_id: format!("tests/test_py_{i}.py::test_{i}"),
                suite_kind: "unit".into(),
                file: format!("tests/test_py_{i}.py"),
            })
            .collect();
        let ts_items: Vec<TestItem> = (0..2)
            .map(|i| TestItem {
                node_id: format!("src/t{i}.test.ts::t{i} test"),
                suite_kind: "unit".into(),
                file: format!("src/t{i}.test.ts"),
            })
            .collect();
        store.store_test_items("python-adapter", &py_items).unwrap();
        store
            .store_test_items("typescript-adapter", &ts_items)
            .unwrap();

        // SelectedTest ids: look up inserted ids in insertion order.
        let ids_for = |adapter: &str, n: usize| -> Vec<SelectedTest> {
            let mut stmt = store
                .conn()
                .prepare("SELECT id FROM test_items WHERE adapter = ?1 ORDER BY id")
                .unwrap();
            let ids: Vec<u32> = stmt
                .query_map([adapter], |r| r.get(0))
                .unwrap()
                .map_while(Result::ok)
                .take(n)
                .collect();
            ids.into_iter()
                .map(|id| SelectedTest {
                    id,
                    confidence: 1.0,
                    distance: None,
                    witness: None,
                    quarantined: false,
                })
                .collect()
        };
        let mut tests = ids_for("typescript-adapter", 2);
        tests.extend(ids_for("python-adapter", 3));

        let groups = collect_selected_by_adapter(&store, &tests);

        assert_eq!(groups.len(), 2, "two adapters → two groups: {groups:?}");
        let ts = groups
            .iter()
            .find(|(a, _)| a == "typescript-adapter")
            .expect("typescript group");
        let py = groups
            .iter()
            .find(|(a, _)| a == "python-adapter")
            .expect("python group");
        assert_eq!(ts.1.len(), 2, "all 163-style ts ids in one group");
        assert_eq!(py.1.len(), 3, "all python ids in one group");
        assert!(
            ts.1.iter().all(|id| id.contains(".ts")),
            "ts group only has ts ids: {:?}",
            ts.1
        );
        assert!(
            py.1.iter().all(|id| id.contains(".py")),
            "py group only has py ids: {:?}",
            py.1
        );
    }

    /// A selected id that no longer resolves (deleted item) must not break
    /// grouping — it's skipped, matching collect_test_node_ids behavior.
    #[test]
    fn group_selected_by_adapter_skips_unresolved_ids() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join(".testaruda")).unwrap();
        store.initialize().unwrap();

        let items = vec![TestItem {
            node_id: "tests/test_a.py::test_a".into(),
            suite_kind: "unit".into(),
            file: "tests/test_a.py".into(),
        }];
        store.store_test_items("python-adapter", &items).unwrap();
        let id: u32 = store
            .conn()
            .query_row("SELECT id FROM test_items", [], |r| r.get(0))
            .unwrap();

        let tests = vec![
            SelectedTest {
                id,
                confidence: 1.0,
                distance: None,
                witness: None,
                quarantined: false,
            },
            SelectedTest {
                id: 999_999,
                confidence: 1.0,
                distance: None,
                witness: None,
                quarantined: false,
            },
        ];

        let groups = collect_selected_by_adapter(&store, &tests);
        assert_eq!(groups.len(), 1, "stale id skipped: {groups:?}");
        assert_eq!(groups[0].0, "python-adapter");
        assert_eq!(groups[0].1, vec!["tests/test_a.py::test_a".to_string()]);
    }
}
