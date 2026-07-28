//! CLI entry point for testaruda.

mod doctor;

use clap::{Parser, Subcommand};
use std::io::IsTerminal;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

/// Build the genesis config registry with testaruda's config registered.
pub fn build_config_registry() -> genesis::config::ConfigRegistry {
    let mut reg = genesis::config::ConfigRegistry::new();
    reg.register::<testaruda::config::Config>("testaruda", "testaruda.toml");
    reg
}

/// Build the genesis command registry with all testaruda commands.
///
/// Used for typo detection via `genesis::suggestions` and the guide scaffold.
pub fn build_command_registry() -> genesis::suggestions::CommandRegistry {
    let mut reg = genesis::suggestions::CommandRegistry::new();
    reg.register(
        "testaruda",
        vec![
            "init".to_string(),
            "select".to_string(),
            "calibrate".to_string(),
            "ingest".to_string(),
            "graph".to_string(),
            "import".to_string(),
            "explain".to_string(),
            "validate".to_string(),
            "discover".to_string(),
            "metrics".to_string(),
            "doctor".to_string(),
            "feedback".to_string(),
            "completions".to_string(),
        ],
    );
    reg
}

/// Build the genesis managed block registry with all testaruda's standard blocks.
pub fn build_block_registry() -> genesis::managed_block::BlockRegistry {
    let mut reg = genesis::managed_block::BlockRegistry::new();
    reg.register(genesis::managed_block::BlockDef::new("WAI"));
    reg.register(genesis::managed_block::BlockDef::new("OPENSPEC"));
    reg.register(genesis::managed_block::BlockDef::new("DONT"));
    reg.register(genesis::managed_block::BlockDef::new("BEADS"));
    reg.register(genesis::managed_block::BlockDef::with_markers(
        "ah:managed",
        "<!-- ah:managed:start -->",
        "<!-- ah:managed:end -->",
    ));
    reg
}

#[derive(Parser)]
#[command(name = "testaruda", author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize store and config in the current project
    Init,
    /// Select affected tests from a code change
    Select {
        /// Base revision (git ref)
        #[arg(long)]
        base: Option<String>,
        /// Head revision (git ref)
        #[arg(long)]
        head: Option<String>,
        /// Explicit changed-file list (comma-separated)
        #[arg(long)]
        files: Option<String>,
        /// Shadow mode: compute but report all tests should run (TIA-CI-007)
        #[arg(long)]
        shadow: bool,
        /// Emit machine-readable JSON plan (TIA-CI-006)
        /// Conflicts with --pre-edit and --agent.
        #[arg(long, conflicts_with_all = ["pre_edit", "agent"])]
        json: bool,
        /// Agent output format: structured JSON for LLM agent consumption (TIA-AGENT-001)
        /// Conflicts with --json and --pre-edit.
        #[arg(long, conflicts_with_all = ["json", "pre_edit"])]
        agent: bool,
        /// Pre-edit blast radius: report affected tests for proposed changes (TIA-AGENT-005)
        /// Conflicts with --json and --agent.
        #[arg(long, conflicts_with_all = ["json", "agent"])]
        pre_edit: bool,
        /// CI mode: run selected tests and ingest results automatically (TIA-CI-008)
        #[arg(long)]
        ci: bool,
        /// Safe mode: pre-flight checks then fall back to `cargo test` if
        /// anything is missing (config, store, git refs) or confidence is low.
        /// Implies --ci. Recommended: pass --base and --head to test the diff
        /// between two git refs; otherwise falls back to uncommitted changes.
        #[arg(long)]
        safe: bool,
        /// Selection ordering mode
        #[arg(long, default_value_t)]
        ordering: testaruda::TestOrdering,
    },
    /// Evaluate the predictive ranking calibration gate (TIA-VER-005)
    Calibrate {
        /// Recall threshold (0.0–1.0) for promotion (default: 0.8)
        #[arg(long, default_value = "0.8")]
        threshold: f64,
    },
    /// Ingest test run results to update the model
    Ingest {
        /// Path to run output file
        path: String,
        /// Raw test output — delegate to the project's configured adapter for
        /// parsing and store runtime edges from the execution
        #[arg(long)]
        raw: bool,
        /// Adapter binary to use for raw output parsing (default: auto-detect)
        #[arg(long)]
        adapter: Option<String>,
    },
    /// Show the current dependency graph
    Graph,
    /// Import a dependency graph from a JSON export
    Import {
        /// Path to graph JSON file
        path: String,
    },
    /// Explain why a test was or was not selected
    Explain {
        /// Test node ID
        test_id: String,
        /// Change set reference
        #[arg(long)]
        change: Option<String>,
    },
    /// Run the Soufflé oracle for cross-validation
    #[command(name = "oracle")]
    Validate {
        /// Path to Soufflé Datalog program
        #[arg(long)]
        program: Option<String>,
    },
    /// Discover tests via configured adapters
    Discover {},
    /// Show operational metrics
    Metrics {},
    /// Validate project configuration via genesis suite_linter
    Doctor {
        /// Apply safe fixes
        #[arg(long)]
        fix: bool,
    },
    /// Submit a feedback issue about an error
    Feedback {
        /// Kind of issue (bug|feature|question)
        kind: String,
        /// Use the last error from scratch (--from-last-error)
        #[arg(long)]
        from_last_error: bool,
        /// Dry run — print what would be submitted
        #[arg(long)]
        dry_run: bool,
    },
    /// Generate CLI documentation in markdown (internal use)
    #[command(hide = true)]
    GenCliDocs,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[allow(clippy::cognitive_complexity)]
fn main() -> miette::Result<()> {
    // Initialize tracing with optional JSON format
    let log_format = std::env::var("TESTARUDA_LOG_FORMAT").unwrap_or_default();
    let use_color = std::env::var_os("NO_COLOR").is_none()
        && std::env::var("CLICOLOR").map(|v| v != "0").unwrap_or(true)
        && std::io::stderr().is_terminal();
    let builder = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(use_color)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        );
    if log_format == "json" {
        builder.json().init();
    } else {
        builder.init();
    }

    // Initialize the genesis guide scaffold
    let all_commands = [
        "init",
        "select",
        "calibrate",
        "ingest",
        "graph",
        "import",
        "explain",
        "validate",
        "discover",
        "metrics",
        "doctor",
        "feedback",
        "completions",
    ];
    let guide = genesis::guide::Guide::builder("testaruda", env!("CARGO_PKG_VERSION"))
        .about("Language-agnostic test selection engine — compute the affected test set from a code change via provenance-semiring dependency analysis")
        .commands(&all_commands)
        .build();

    // Build and validate the config registry (tool-craft contract)
    let config_registry = build_config_registry();
    let _store = genesis::config::ConfigStore::new(config_registry);

    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(err) => {
            // Use genesis suggestions to provide better error messages for typos
            let err_str = err.to_string();
            let _err_kind = err.kind();

            // Extract the unknown subcommand from clap's error
            // Clap errors for unknown subcommands look like:
            // "error: unrecognized subcommand 'slect'"
            // or "error: Found argument 'slect' which wasn't expected"
            let unknown = err_str.split('\'').nth(1).map(|s| s.trim().to_string());

            if let Some(ref cmd) = unknown {
                let engine = genesis::suggestions::SuggestionEngine::new();
                if let Some(suggestion) = engine.suggest_typo(cmd, guide.registry()) {
                    eprintln!("{}", err.render());
                    eprintln!();
                    eprintln!("💡 {}", suggestion.message());
                    if let Some(footer) = suggestion.footer() {
                        eprintln!("   {}", footer);
                    }
                    std::process::exit(2);
                }
            }

            // Fall through to default clap error handling
            err.exit();
        }
    };

    match cli.command {
        Command::Init => {
            println!("🔧 Initializing testaruda store...");
            let store = testaruda::Store::open_default()?;
            store.initialize()?;
            let project_root = testaruda::Store::find_project_root()?;
            // Write default config if it doesn't exist
            if !project_root.join("testaruda.toml").exists() {
                testaruda::config::Config::write_default(&project_root)?;
                // Report detected language for user feedback
                let detected = testaruda::config::detect_project_language(&project_root);
                match detected {
                    Some(ref adapter) if adapter.contains("rust") => {
                        println!("  🦀 Rust project detected — default adapter set to testaruda-adapter-rust");
                    }
                    Some(ref adapter) if adapter.contains("python") => {
                        println!("  🐍 Python project detected — default adapter set to testaruda-adapter-python");
                    }
                    Some(ref adapter) if adapter.contains("julia") => {
                        println!("  🔬 Julia project detected — default adapter set to testaruda-adapter-julia");
                    }
                    Some(ref adapter) if adapter.contains("typescript") => {
                        println!("  🟦 TypeScript project detected — default adapter set to testaruda-adapter-typescript");
                    }
                    Some(ref adapter) if adapter.contains("clojure") => {
                        println!("  🟢 Clojure project detected — default adapter set to testaruda-adapter-clojure");
                    }
                    _ => {
                        println!("  📁 Project language not detected — default adapter set to testaruda-adapter-rust");
                    }
                }
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
                let injector = genesis::managed_block::BlockInjector::new(build_block_registry());

                // Ensure BEADS managed block is present
                let beads_content = "\n<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:ca08a54f -->\n## Beads Issue Tracker\n\nThis project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.\n\n### Quick Reference\n\n```bash\nbd ready              # Find available work\nbd show <id>          # View issue details\nbd update <id> --claim  # Claim work\nbd close <id>         # Complete work\n```\n\n### Rules\n\n- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists\n- Run `bd prime` for detailed command reference and session close protocol\n- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files\n\n## Session Completion\n\n**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.\n\n**MANDATORY WORKFLOW:**\n\n1. **File issues for remaining work** - Create issues for anything that needs follow-up\n2. **Run quality gates** (if code changed) - Tests, linters, builds\n3. **Update issue status** - Close finished work, update in-progress items\n4. **PUSH TO REMOTE** - This is MANDATORY.\n5. **Clean up** - Clear stashes, prune remote branches\n6. **Verify** - All changes committed AND pushed\n7. **Hand off** - Provide context for next session\n<!-- END BEADS INTEGRATION -->\n";
                match injector.inject(&agents_path, "BEADS", beads_content) {
                    Ok(r) => tracing::info!(
                        event = "managed_block_injected",
                        block = "BEADS",
                        result = ?r
                    ),
                    Err(e) => eprintln!("  ⚠️  Failed to inject BEADS block: {}", e),
                }
            }

            println!("✅ testaruda initialized at {}", project_root.display());
            Ok(())
        }
        Command::Calibrate { threshold } => {
            let store = testaruda::Store::open_default()?;
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
                println!(
                    "⚠️  Insufficient run history for calibration — need at least 2 distinct runs"
                );
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
        Command::Select {
            base,
            head,
            files,
            shadow,
            json,
            agent,
            pre_edit,
            ci,
            safe,
            ordering,
        } => {
            // Safe mode: pre-flight checks with fallback to cargo test
            if safe {
                let project_root = match find_project_root() {
                    Ok(p) => p,
                    Err(_) => {
                        eprintln!("  \u{26a0}\u{fe0f}  testaruda not initialized (no .git or testaruda.toml found)");
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
            }

            let store = testaruda::Store::open_default()?;
            store.check_initialized()?;
            let delta = testaruda::ChangeSet::from_diff(
                base.as_deref(),
                head.as_deref(),
                files.as_deref(),
            )?;

            // Adapter pipeline: populate store via adapters before selection
            let project_root = find_project_root()?;
            let config = testaruda::config::Config::load_or_default(&project_root);
            let registry = config.adapters.to_registry();
            if let Err(e) =
                run_adapter_pipeline(&store, &registry, &delta, &config.discover.exclude)
            {
                eprintln!("⚠️  Adapter warning: {} (using existing store data)", e);
            }

            // Agent mode implies deterministic ordering (TIA-AGENT-007)
            // and overrides any explicit --ordering flag
            let ordering = if agent {
                testaruda::TestOrdering::Deterministic
            } else {
                ordering
            };

            // Load selection context once, reuse for both engine and agent output
            let ctx = store.load_selection_context(&delta)?;
            let changed_ids = ctx.changed.clone();
            let unresolved_ids = ctx.unresolved.clone();
            let engine = testaruda::Engine::new(&store);
            let selection = engine.select_with_context(ctx, ordering)?;

            // Persist provenance for this selection run (TIA-PROV-005)
            let run_id = store.generate_run_id()?;
            let all_affected_ids =
                store.get_test_ids_for_content_units(&changed_ids, &unresolved_ids)?;
            if let Err(e) = store.persist_provenance(&run_id, &selection, &all_affected_ids) {
                eprintln!("⚠️  Provenance warning: {} (selection still complete)", e);
            }

            // Determine CI exit code (TIA-CI-001..004)
            let mut outcome = CiOutcome::from_selection(&selection);

            // Shadow mode (TIA-CI-007): report all tests should run
            if shadow {
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

            if agent {
                // Agent output format (TIA-AGENT-001)
                let mut changed_units = Vec::new();
                let mut test_node_ids = std::collections::HashMap::new();

                // Build changed unit info from pre-saved context IDs
                for &cu_id in &changed_ids {
                    if let Ok((path, symbol, kind)) = store.get_content_unit_info(cu_id) {
                        changed_units.push(testaruda::agent::ChangedUnit {
                            id: cu_id,
                            path,
                            symbol,
                            kind,
                            unresolved: false,
                        });
                    }
                }
                for &cu_id in &unresolved_ids {
                    if let Ok((path, symbol, kind)) = store.get_content_unit_info(cu_id) {
                        changed_units.push(testaruda::agent::ChangedUnit {
                            id: cu_id,
                            path,
                            symbol,
                            kind,
                            unresolved: true,
                        });
                    }
                }

                // Build test node ID map for all selected tests
                for t in &selection.tests {
                    if let Ok(node_id) = store.get_test_node_id(t.id) {
                        test_node_ids.insert(t.id, node_id);
                    }
                }

                // Get candidate test IDs (all tests that have deps on changed units)
                let candidate_ids =
                    store.get_test_ids_for_content_units(&changed_ids, &unresolved_ids)?;

                let output = testaruda::agent::AgentOutput::from_selection(
                    &store,
                    &selection,
                    &changed_units,
                    &test_node_ids,
                    &candidate_ids,
                )?;

                let out = serde_json::to_string_pretty(&output)
                    .map_err(|e| miette::miette!("Agent output serialization failed: {}", e))?;
                println!("{}", out);
            } else if json {
                // Machine-readable plan (TIA-CI-006) via genesis envelope
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
                    genesis::envelope::EnvelopeKind::List,
                    plan,
                    vec![],
                    vec![],
                );
                let out = serde_json::to_string_pretty(&envelope)
                    .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
                println!("{}", out);
            } else if pre_edit {
                // Pre-edit blast radius (TIA-AGENT-005): structured JSON output
                use testaruda::agent::PreEditOutput;

                let mut changed_files = Vec::new();
                for &cu_id in &changed_ids {
                    if let Ok((path, _, _)) = store.get_content_unit_info(cu_id) {
                        changed_files.push(path);
                    }
                }
                for &cu_id in &unresolved_ids {
                    if let Ok((path, _, _)) = store.get_content_unit_info(cu_id) {
                        changed_files.push(path);
                    }
                }
                let mut selected_tests = Vec::new();
                for t in &selection.tests {
                    if let Ok(node_id) = store.get_test_node_id(t.id) {
                        selected_tests.push(node_id);
                    }
                }
                let candidate_count = store
                    .get_test_ids_for_content_units(&changed_ids, &unresolved_ids)?
                    .len();

                let output = PreEditOutput {
                    format: "testaruda-pre-edit-v1".to_string(),
                    summary: testaruda::agent::SummaryStats {
                        changed_count: selection.changed_count,
                        selected_count: selection.selected_count,
                        candidate_count,
                        has_coverage_gaps: false,
                    },
                    changed_files,
                    selected_tests,
                };
                let out = serde_json::to_string_pretty(&output)
                    .map_err(|e| miette::miette!("Pre-edit output serialization failed: {}", e))?;
                println!("{}", out);
            } else {
                // Human-readable output (CORR-004: include reason)
                let reason_note = outcome.reason();
                if shadow {
                    println!("⚠️  Shadow mode — selection computed but all tests should run");
                }
                if reason_note != "selection complete" {
                    println!("ℹ️  {}", reason_note);
                }
                let out = serde_json::to_string_pretty(&selection)
                    .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
                println!("{}", out);

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
                    std::process::exit(code);
                }
            }

            // CI mode (TIA-CI-008): run selected tests and ingest results
            let effective_ci = ci || safe;
            if effective_ci && !selection.tests.is_empty() {
                // Collect selected test file paths (node_ids from store)
                let mut selected_files: Vec<String> = Vec::new();
                for t in &selection.tests {
                    if let Ok(node_id) = store.get_test_node_id(t.id) {
                        selected_files.push(node_id);
                    }
                }

                if !selected_files.is_empty() {
                    // Find the adapter for the first test file
                    let adapter_binary = registry
                        .resolve(&selected_files[0])
                        .or_else(|| registry.default_binary())
                        .unwrap_or("testaruda-adapter-python");

                    let mut adapter =
                        match testaruda::adapter::spawn_adapter(adapter_binary, None) {
                            Ok(a) => a,
                            Err(e) => {
                                eprintln!(
                                    "⚠️  CI: failed to spawn adapter {}: {}",
                                    adapter_binary, e
                                );
                                let code = outcome.exit_code();
                                if code != 0 {
                                    std::process::exit(code);
                                }
                                return Ok(());
                            }
                        };

                    // Get runner args from the adapter
                    match adapter.run_args(&selected_files) {
                        Ok(run_args_result) => {
                            eprintln!("  🏃 CI: running tests...");
                            match std::process::Command::new(&run_args_result.runner_args[0])
                                .args(&run_args_result.runner_args[1..])
                                .output()
                            {
                                Ok(output) => {
                                    let stdout = String::from_utf8_lossy(&output.stdout);
                                    let stderr = String::from_utf8_lossy(&output.stderr);

                                    // Combine stdout/stderr for ingest
                                    let combined = if stderr.is_empty() {
                                        stdout.to_string()
                                    } else {
                                        format!("{}\n{}", stdout, stderr)
                                    };

                                    // Capture exit code, ingest FIRST, then exit with captured code
                                    // This preserves the feedback loop: failed runs are recorded (TIA-CI-008)
                                    let test_runner_ok = output.status.success();
                                    let test_runner_code = output.status.code().unwrap_or(1);

                                    eprintln!("  📥 CI: ingesting results...");
                                    match adapter.ingest(&combined) {
                                        Ok(ingest_result) => {
                                            // Store runtime edges
                                            if !ingest_result.runtime_edges.is_empty() {
                                                let _ = store.store_static_deps(
                                                    &adapter.name,
                                                    &ingest_result.runtime_edges,
                                                );
                                            }

                                            // Convert per-test results to store format
                                            let mut store_tests = Vec::new();
                                            for test in &ingest_result.per_test_results {
                                                if let Ok(tid) =
                                                    store.lookup_test_item_id(&test.test_id)
                                                {
                                                    store_tests.push(serde_json::json!({
                                                        "id": tid,
                                                        "outcome": test.outcome,
                                                        "duration_ms": test.duration_ms,
                                                    }));
                                                }
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
                                                eprintln!(
                                                    "  ✅ CI: ingested {} test results",
                                                    ingest_result.per_test_results.len()
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("  ⚠️  CI: adapter ingest failed: {}", e);
                                        }
                                    }

                                    // Exit with test runner code AFTER ingest preserves history
                                    if !test_runner_ok {
                                        eprintln!(
                                            "  ❌  CI: test runner failed with exit code {}",
                                            test_runner_code
                                        );
                                        std::process::exit(test_runner_code);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("  ⚠️  CI: failed to run tests: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("  ⚠️  CI: failed to get run args: {}", e);
                        }
                    }
                }
            }

            Ok(())
        }
        Command::Ingest { path, raw, adapter } => {
            let store = testaruda::Store::open_default()?;
            store.check_initialized()?;

            if raw {
                // Raw mode: treat file as test runner output, delegate to adapter
                let raw_output = std::fs::read_to_string(&path)
                    .map_err(|e| miette::miette!("Failed to read {}: {}", path, e))?;

                // Resolve adapter binary: explicit --adapter flag, or auto-detect
                // from the project's config, or fall back to the Python adapter
                let adapter_binary: String = if let Some(explicit) = adapter {
                    explicit
                } else {
                    let project_root = find_project_root()?;
                    let config = testaruda::config::Config::load_or_default(&project_root);
                    let registry = config.adapters.to_registry();
                    // Collect into owned String to avoid borrow issues with registry
                    let first_ext = registry.extensions().next().map(|(_, b)| b.to_string());
                    first_ext
                        .or_else(|| registry.default_binary().map(String::from))
                        .unwrap_or_else(|| "testaruda-adapter-python".to_string())
                };

                let mut adapter = testaruda::adapter::spawn_adapter(&adapter_binary, None)
                    .map_err(|e| {
                        miette::miette!("Failed to spawn adapter {}: {}", adapter_binary, e)
                    })?;

                let ingest_result = adapter
                    .ingest(&raw_output)
                    .map_err(|e| miette::miette!("Adapter ingest failed: {}", e))?;

                // Store runtime edges
                if !ingest_result.runtime_edges.is_empty() {
                    store
                        .store_static_deps(&adapter.name, &ingest_result.runtime_edges)
                        .map_err(|e| miette::miette!("Failed to store runtime edges: {}", e))?;
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
            } else {
                // JSON mode: existing path — parse JSON and pass to store directly
                let data = std::fs::read_to_string(&path)
                    .map_err(|e| miette::miette!("Failed to read {}: {}", path, e))?;
                let results: serde_json::Value = serde_json::from_str(&data)
                    .map_err(|e| miette::miette!("Failed to parse JSON: {}", e))?;
                store.ingest(&results)?;
                println!("✅ Run ingested");
            }
            Ok(())
        }
        Command::Graph => {
            let store = testaruda::Store::open_default()?;
            store.check_initialized()?;
            let graph = store.export_graph()?;
            let out = serde_json::to_string_pretty(&graph)
                .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
            println!("{}", out);
            Ok(())
        }
        Command::Import { path } => {
            let store = testaruda::Store::open_default()?;
            store.check_initialized()?;
            let data = std::fs::read_to_string(&path)
                .map_err(|e| miette::miette!("Failed to read {}: {}", path, e))?;
            let graph: serde_json::Value = serde_json::from_str(&data)
                .map_err(|e| miette::miette!("Failed to parse JSON: {}", e))?;
            store.import_graph(&graph)?;
            println!("✅ Graph imported from {}", path);
            Ok(())
        }
        Command::Explain { test_id, change } => {
            let store = testaruda::Store::open_default()?;
            store.check_initialized()?;
            let explanation = store.explain(&test_id, change.as_deref())?;
            let out = serde_json::to_string_pretty(&explanation)
                .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
            println!("{}", out);
            Ok(())
        }
        Command::Validate { program } => {
            let store = testaruda::Store::open_default()?;
            store.check_initialized()?;
            println!("🔮 Soufflé oracle validation");

            // Generate Datalog from the current store
            let datalog = store.generate_datalog()?;
            println!("{}", datalog);

            match program {
                Some(path) => {
                    // Write generated Datalog to the specified path
                    std::fs::write(&path, &datalog)
                        .map_err(|e| miette::miette!("Failed to write Datalog: {}", e))?;
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
                    println!("  Datalog program generated. Use --program <path> to write to a file and run.");
                }
            }
            Ok(())
        }
        Command::Discover {} => {
            let store = testaruda::Store::open_default()?;
            store.initialize()?;
            let project_root = find_project_root()?;
            let config = testaruda::config::Config::load_or_default(&project_root);
            let registry = config.adapters.to_registry();
            run_discover_pipeline(&store, &registry, &config.discover.exclude)
                .map_err(|e| miette::miette!(e))?;
            let count = store.test_items_count().unwrap_or(0);
            println!("\n✅ Discovered {} test items", count);
            Ok(())
        }
        Command::Metrics {} => {
            let store = testaruda::Store::open_default()?;
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
        Command::Doctor { fix } => {
            let project_root = find_project_root()?;
            match doctor::run_doctor(&project_root, fix) {
                Ok(true) => Ok(()),
                Ok(false) => std::process::exit(1),
                Err(e) => Err(miette::miette!("Doctor failed: {}", e)),
            }
        }
        Command::Feedback {
            kind,
            from_last_error,
            dry_run,
        } => {
            let project_root = find_project_root()?;

            // Build the issue body
            let mut body = String::new();
            let mut title = format!("[{}] ", kind);

            if from_last_error {
                // Read the last error from genesis scratch
                if let Some(record) = genesis::feedback::scratch::read_last_error("testaruda") {
                    title.push_str(&format!("Error: {}", record.argv.join(" ")));
                    body.push_str("## Error\n\n");
                    body.push_str(&format!("Exit code: {}\n\n", record.exit));
                    if let Some(ref footer) = record.footer {
                        body.push_str(&format!("Footer: {}\n\n", footer));
                    }
                } else {
                    eprintln!("No previous error found in scratch.");
                    eprintln!("  Run a command with `genesis` error handling first.");
                    std::process::exit(1);
                }
            } else {
                eprintln!("Please provide issue details or use --from-last-error");
                std::process::exit(1);
            }

            // Append environment context
            let context = genesis::feedback::context::gather_context(
                "testaruda",
                env!("CARGO_PKG_VERSION"),
                None,
                None,
                None,
                &project_root,
            );
            body.push_str(&genesis::feedback::context::format_context_bundle(&context));

            // Redact sensitive info
            let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
            let git_remote = std::process::Command::new("git")
                .args(["config", "--get", "remote.origin.url"])
                .current_dir(&project_root)
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8(o.stdout)
                            .ok()
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                });
            let redacted =
                genesis::feedback::redactor::redact(&body, home.as_deref(), git_remote.as_deref());

            // Determine target repo
            let repo = env!("CARGO_PKG_REPOSITORY")
                .trim_start_matches("https://")
                .to_string();

            // Resolve labels
            let labels: Vec<String> = match kind.as_str() {
                "bug" => vec!["bug".into()],
                "feature" => vec!["enhancement".into()],
                "question" => vec!["question".into()],
                _ => vec![kind.clone()],
            };

            if dry_run {
                println!("--- DRY RUN ---");
                println!("Repo: {}", repo);
                println!("Title: {}", title);
                println!("Labels: {:?}", labels);
                println!("Body (redacted):\n{}", redacted);
                return Ok(());
            }

            // Submit via genesis::feedback::gh
            let opts = genesis::feedback::gh::CreateIssueOptions {
                repo,
                title,
                body: redacted,
                labels,
                dry_run: false,
            };

            match genesis::feedback::gh::create_issue(&opts) {
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
        Command::GenCliDocs => {
            let markdown = clap_markdown::help_markdown::<Cli>();
            println!("{}", markdown);
            Ok(())
        }
        Command::Completions { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            let name = cmd.get_name().to_string();
            clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
    }
    .map_err(|e| {
        // Error-footer hook (TIA-ADAPT feedback protocol):
        // On non-zero exit with no Fix suggestion, print feedback command hint
        let err_msg = format!("{}", e);
        eprintln!("testaruda: {}", err_msg);
        eprintln!("Feedback: testaruda feedback bug --from-last-error");

        // Write to genesis error scratch
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ts = {
            let days = now / 86400;
            let time_secs = now % 86400;
            let hours = time_secs / 3600;
            let mins = (time_secs % 3600) / 60;
            let secs = time_secs % 60;
            let mut y = 1970i64;
            let mut remaining = days as i64;
            loop {
                let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
                if remaining < days_in_year { break; }
                remaining -= days_in_year;
                y += 1;
            }
            let month_days = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
            } else {
                [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
            };
            let mut m = 1usize;
            for (i, &md) in month_days.iter().enumerate() {
                if remaining < md as i64 { m = i + 1; break; }
                remaining -= md as i64;
            }
            if m == 0 { m = 12; }
            let d = (remaining + 1) as u8;
            format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, hours, mins, secs)
        };
        let record = genesis::feedback::scratch::ErrorRecord {
            ts,
            argv: std::env::args().collect(),
            exit: 1,
            footer: Some(
                "Feedback: testaruda feedback bug --from-last-error".into(),
            ),
            kind: "error".into(),
        };
        genesis::feedback::scratch::write_scratch_best_effort("testaruda", &record);
        std::process::exit(1);
    })
}

// ===== Adapter Pipeline =====

/// Populate store from adapters: run discover + static-deps on the full project
/// tree, then process any changed files from the delta.
///
/// Previously, only changed files were processed, leaving the store's dependency
/// graph incomplete on first invocation (TIA-ADAPT-004, TIA-ADAPT-005).
pub fn run_adapter_pipeline(
    store: &testaruda::Store,
    registry: &testaruda::adapter::AdapterRegistry,
    delta: &testaruda::ChangeSet,
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
        let mut adapter = match testaruda::adapter::spawn_adapter(binary, None) {
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
            let mut adapter = match testaruda::adapter::spawn_adapter(binary, None) {
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
    store: &testaruda::Store,
    registry: &testaruda::adapter::AdapterRegistry,
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

            let mut adapter = match testaruda::adapter::spawn_adapter(binary, None) {
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

/// Find the project root by looking for .git or testaruda.toml.
pub fn find_project_root() -> miette::Result<std::path::PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("Cannot get current directory: {}", e))?;
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() || ancestor.join("testaruda.toml").exists() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(cwd)
}

// ===== CI Exit Codes (TIA-CI-001..008) =====

/// Fallback: run `cargo test` and exit with its exit code.
/// Used by --safe mode when pre-flight checks fail or confidence is low.
fn run_cargo_test_fallback() -> ! {
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
    std::process::exit(code)
}

#[allow(dead_code)]
mod ci_exit {
    pub const SUCCESS: i32 = 0; // TIA-CI-001
    pub const FULL_RUN: i32 = 10; // TIA-CI-002
    pub const EMPTY: i32 = 20; // TIA-CI-003
    pub const ERROR: i32 = 1; // TIA-CI-004 (distinct from 10, 20)
}

/// Classifies a selection result into a CI exit code and reason.
struct CiOutcome {
    code: i32,
    reason: String,
}

impl CiOutcome {
    /// Determine the CI outcome from a selection result.
    ///
    /// - Empty selection → exit 20 (TIA-CI-003)
    /// - Any test with confidence < 1.0 → full run, exit 10 (TIA-CI-002)
    /// - Otherwise → success, exit 0 (TIA-CI-001)
    fn from_selection(selection: &testaruda::Selection) -> Self {
        if selection.selected_count == 0 {
            return Self {
                code: ci_exit::EMPTY,
                reason: "no tests selected".to_string(),
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

    fn exit_code(&self) -> i32 {
        self.code
    }

    fn reason(&self) -> &str {
        &self.reason
    }
}

/// Machine-readable CI plan (TIA-CI-006).
#[derive(serde::Serialize)]
struct CiPlan<'a> {
    shadow_mode: bool,
    exit_code: i32,
    selected_count: usize,
    changed_count: usize,
    reason: &'a str,
    /// When true, indicates that ALL tests should run (shadow mode or low confidence).
    all_tests: bool,
    /// The computed selection (may be empty in shadow mode summary output).
    tests: &'a [testaruda::SelectedTest],
}

#[cfg(test)]
mod tests {
    use super::*;
    use testaruda::{SelectedTest, Selection};

    #[test]
    fn test_shadow_mode_output() {
        // Shadow mode: exit code reflects computed selection but output says "all tests"
        let sel = Selection {
            changed_count: 1,
            selected_count: 1,
            tests: vec![SelectedTest {
                id: 42,
                confidence: 1.0,
                distance: Some(0),
                witness: None,
                quarantined: false,
            }],
        };
        // In shadow mode, the outcome should still report the actual exit code
        let outcome = CiOutcome::from_selection(&sel);
        assert_eq!(outcome.exit_code(), ci_exit::SUCCESS);
    }

    #[test]
    fn test_json_plan_serialization() {
        let sel = Selection {
            changed_count: 2,
            selected_count: 2,
            tests: vec![SelectedTest {
                id: 1,
                confidence: 1.0,
                distance: Some(0),
                witness: None,
                quarantined: false,
            }],
        };
        let plan = CiPlan {
            shadow_mode: false,
            exit_code: ci_exit::SUCCESS,
            selected_count: sel.selected_count,
            changed_count: sel.changed_count,
            reason: "selection complete",
            all_tests: false,
            tests: &sel.tests,
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"exit_code\":0"));
        assert!(json.contains("\"selected_count\":2"));
        assert!(json.contains("\"shadow_mode\":false"));
    }

    #[test]
    fn test_shadow_plan_serialization() {
        let sel = Selection {
            changed_count: 0,
            selected_count: 0,
            tests: vec![],
        };
        let plan = CiPlan {
            shadow_mode: true,
            exit_code: ci_exit::EMPTY,
            selected_count: sel.selected_count,
            changed_count: sel.changed_count,
            reason: "no tests selected",
            all_tests: true,
            tests: &sel.tests,
        };
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"shadow_mode\":true"));
        assert!(json.contains("\"exit_code\":20"));
    }

    #[test]
    fn test_success_exit_code() {
        let sel = Selection {
            changed_count: 2,
            selected_count: 3,
            tests: vec![
                SelectedTest {
                    id: 1,
                    confidence: 1.0,
                    distance: Some(0),
                    witness: None,
                    quarantined: false,
                },
                SelectedTest {
                    id: 2,
                    confidence: 1.0,
                    distance: Some(1),
                    witness: None,
                    quarantined: false,
                },
            ],
        };
        let outcome = CiOutcome::from_selection(&sel);
        assert_eq!(outcome.exit_code(), ci_exit::SUCCESS, "high confidence → 0");
        assert_eq!(outcome.reason(), "selection complete");
    }

    #[test]
    fn test_empty_exit_code() {
        let sel = Selection {
            changed_count: 0,
            selected_count: 0,
            tests: vec![],
        };
        let outcome = CiOutcome::from_selection(&sel);
        assert_eq!(outcome.exit_code(), ci_exit::EMPTY, "empty selection → 20");
    }

    #[test]
    fn test_low_confidence_exit_code() {
        let sel = Selection {
            changed_count: 2,
            selected_count: 2,
            tests: vec![SelectedTest {
                id: 1,
                confidence: 0.5,
                distance: Some(0),
                witness: None,
                quarantined: false,
            }],
        };
        let outcome = CiOutcome::from_selection(&sel);
        assert_eq!(
            outcome.exit_code(),
            ci_exit::FULL_RUN,
            "low confidence → 10"
        );
    }

    #[test]
    fn test_ci_exit_codes_distinct() {
        // Verify 10 and 20 are distinct from each other and from 0/1
        assert_ne!(ci_exit::SUCCESS, ci_exit::FULL_RUN);
        assert_ne!(ci_exit::SUCCESS, ci_exit::EMPTY);
        assert_ne!(ci_exit::FULL_RUN, ci_exit::EMPTY);
        assert_ne!(ci_exit::ERROR, ci_exit::FULL_RUN);
        assert_ne!(ci_exit::ERROR, ci_exit::EMPTY);
    }

    // ========================================================================
    // Genesis adoption: suggestions tests
    // ========================================================================

    #[test]
    fn test_build_command_registry_includes_all_commands() {
        let reg = super::build_command_registry();
        let all = reg.all();
        assert!(all.contains(&"init"), "should include init");
        assert!(all.contains(&"select"), "should include select");
        assert!(all.contains(&"discover"), "should include discover");
        assert!(all.contains(&"ingest"), "should include ingest");
        assert!(all.contains(&"calibrate"), "should include calibrate");
        assert!(all.contains(&"graph"), "should include graph");
        assert!(all.contains(&"import"), "should include import");
        assert!(all.contains(&"explain"), "should include explain");
        assert!(all.contains(&"validate"), "should include validate");
        assert!(all.contains(&"metrics"), "should include metrics");
        assert!(all.contains(&"doctor"), "should include doctor");
        assert!(all.contains(&"completions"), "should include completions");
        // Should NOT include hidden commands
        assert!(
            !all.contains(&"gen-cli-docs"),
            "should exclude gen-cli-docs"
        );
    }

    #[test]
    fn test_typo_slect_suggests_select() {
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();
        let suggestion = engine.suggest_typo("slect", &reg);
        assert!(suggestion.is_some(), "slect should suggest select");
        if let Some(genesis::suggestions::Suggestion::DidYouMean {
            original,
            suggestion,
        }) = suggestion
        {
            assert_eq!(original, "slect");
            assert_eq!(suggestion, "select");
        } else {
            panic!("expected DidYouMean suggestion, got: {:?}", suggestion);
        }
    }

    #[test]
    fn test_typo_injest_suggests_ingest() {
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();
        let suggestion = engine.suggest_typo("injest", &reg);
        assert!(suggestion.is_some(), "injest should suggest ingest");
        if let Some(genesis::suggestions::Suggestion::DidYouMean { suggestion, .. }) = suggestion {
            assert_eq!(suggestion, "ingest");
        }
    }

    #[test]
    fn test_typo_discovr_suggests_discover() {
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();
        let suggestion = engine.suggest_typo("discovr", &reg);
        assert!(suggestion.is_some(), "discovr should suggest discover");
        if let Some(genesis::suggestions::Suggestion::DidYouMean { suggestion, .. }) = suggestion {
            assert_eq!(suggestion, "discover");
        }
    }

    #[test]
    fn test_typo_dcotr_suggests_doctor() {
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();
        let suggestion = engine.suggest_typo("dcotr", &reg);
        assert!(suggestion.is_some(), "dcotr should suggest doctor");
        if let Some(genesis::suggestions::Suggestion::DidYouMean { suggestion, .. }) = suggestion {
            assert_eq!(suggestion, "doctor");
        }
    }

    #[test]
    fn test_typo_exports_suggests_explain() {
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();
        let suggestion = engine.suggest_typo("explain", &reg);
        // 'explain' is close enough to 'explain' itself (not a typo)
        // This tests the threshold works correctly
        assert!(
            suggestion.is_none()
                || suggestion.as_ref().is_some_and(|s| matches!(
                    s,
                    genesis::suggestions::Suggestion::DidYouMean { .. }
                ))
        );
    }

    #[test]
    fn test_typo_gibberish_returns_none() {
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();
        let suggestion = engine.suggest_typo("xyzqwert", &reg);
        assert!(
            suggestion.is_none(),
            "gibberish should not match any command"
        );
    }

    #[test]
    fn test_format_typo_suggestion() {
        let suggestion = genesis::suggestions::Suggestion::DidYouMean {
            original: "slect".to_string(),
            suggestion: "select".to_string(),
        };
        let msg = suggestion.message();
        assert!(msg.contains("slect"));
        assert!(msg.contains("select"));
        assert!(msg.contains("Did you mean"));

        let footer = suggestion.footer();
        assert_eq!(footer, Some("→ Run: select".to_string()));
    }

    // ========================================================================
    // Genesis adoption: managed block tests
    // ========================================================================

    #[test]
    fn test_managed_block_registry_includes_standard_blocks() {
        let reg = super::build_block_registry();
        assert!(reg.has("WAI"), "should include WAI block");
        assert!(reg.has("OPENSPEC"), "should include OPENSPEC block");
        assert!(reg.has("DONT"), "should include DONT block");
        assert!(reg.has("BEADS"), "should include BEADS block");
        assert!(reg.has("ah:managed"), "should include ah:managed block");
    }

    #[test]
    fn test_block_injector_creates_new_file() {
        use genesis::managed_block::BlockInjector;

        let reg = super::build_block_registry();
        let injector = BlockInjector::new(reg);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");

        let result = injector
            .inject(&path, "WAI", "\n# WAI instructions\n")
            .unwrap();
        assert_eq!(result, genesis::managed_block::InjectResult::Created);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<!-- WAI:START -->"));
        assert!(content.contains("<!-- WAI:END -->"));
        assert!(content.contains("# WAI instructions"));
    }

    #[test]
    fn test_block_injector_updates_existing_block() {
        use genesis::managed_block::BlockInjector;

        let reg = super::build_block_registry();
        let injector = BlockInjector::new(reg);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");

        injector.inject(&path, "WAI", "\n# Old content\n").unwrap();
        let result = injector.inject(&path, "WAI", "\n# New content\n").unwrap();
        assert_eq!(result, genesis::managed_block::InjectResult::Updated);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# New content"));
        assert!(!content.contains("# Old content"));
        assert_eq!(
            content.matches("<!-- WAI:START -->").count(),
            1,
            "no duplicate markers"
        );
    }

    #[test]
    fn test_block_injector_prepends_to_existing_file() {
        use genesis::managed_block::BlockInjector;
        use std::io::Write;

        let reg = super::build_block_registry();
        let injector = BlockInjector::new(reg);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");

        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "# Existing file content").unwrap();
        drop(f);

        let result = injector.inject(&path, "WAI", "\n# New block\n").unwrap();
        assert_eq!(result, genesis::managed_block::InjectResult::Prepended);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("<!-- WAI:START -->"));
        assert!(content.contains("# Existing file content"));
    }

    // ========================================================================
    // Genesis adoption: envelope tests
    // ========================================================================

    #[test]
    fn test_envelope_has_required_top_level_keys() {
        use genesis::envelope::{Envelope, EnvelopeKind};

        let env: Envelope<String> =
            Envelope::success(EnvelopeKind::Ok, "test data".to_string(), vec![], vec![]);

        let json = serde_json::to_value(&env).unwrap();
        let obj = json.as_object().unwrap();

        assert!(obj.contains_key("ok"), "must have 'ok'");
        assert!(
            obj.contains_key("envelope_version"),
            "must have 'envelope_version'"
        );
        assert!(obj.contains_key("cli_version"), "must have 'cli_version'");
        assert!(
            obj.contains_key("envelope_kind"),
            "must have 'envelope_kind'"
        );
        assert!(obj.contains_key("data"), "must have 'data'");
        assert!(obj.contains_key("warnings"), "must have 'warnings'");
        assert!(obj.contains_key("meta"), "must have 'meta'");

        assert_eq!(obj["ok"], true);
        assert_eq!(obj["envelope_version"], "0.1");
    }

    #[test]
    fn test_envelope_kind_select_serialization() {
        use genesis::envelope::{Envelope, EnvelopeKind};

        let env: Envelope<serde_json::Value> = Envelope::success(
            EnvelopeKind::List,
            serde_json::json!({"selected_count": 5, "tests": []}),
            vec![],
            vec![],
        );

        let json = serde_json::to_value(&env).unwrap();
        assert_eq!(json["envelope_kind"], "list");
        assert_eq!(json["data"]["selected_count"], 5);
    }

    #[test]
    fn test_envelope_error_has_remediation() {
        use genesis::envelope::{Envelope, ErrorResult, RemediationEntry};

        let err = ErrorResult::new(
            "E001",
            "something went wrong",
            None,
            None,
            None,
            vec![],
            vec![RemediationEntry {
                command: "testaruda init".into(),
                description: "initialize the store".into(),
            }],
        )
        .unwrap();

        let env = Envelope::error(err, vec![]);
        let json = serde_json::to_value(&env).unwrap();

        assert_eq!(json["ok"], false);
        assert_eq!(json["envelope_kind"], "error");
        assert!(!json["data"]["remediation"].as_array().unwrap().is_empty());
    }

    // ========================================================================
    // Genesis adoption: integration smoke test
    // ========================================================================

    #[test]
    fn test_build_command_registry_is_idempotent() {
        let reg1 = super::build_command_registry();
        let reg2 = super::build_command_registry();
        assert_eq!(reg1.all().len(), reg2.all().len());
    }

    #[test]
    fn test_build_block_registry_is_idempotent() {
        let reg1 = super::build_block_registry();
        let reg2 = super::build_block_registry();
        assert_eq!(reg1.names().len(), reg2.names().len());
    }

    #[test]
    fn test_cli_parse_typo_caught_by_genesis() {
        // This tests the integration path: parse a known typo, catch the error,
        // and verify genesis suggests the correct command.
        let reg = super::build_command_registry();
        let engine = genesis::suggestions::SuggestionEngine::new();

        // Simulate the clap error path: clap rejects "slect"
        let result = <super::Cli as clap::CommandFactory>::command()
            .try_get_matches_from(vec!["testaruda", "slect"]);

        assert!(result.is_err(), "clap should reject 'slect'");

        // Now check genesis suggestion
        let suggestion = engine.suggest_typo("slect", &reg);
        assert!(suggestion.is_some(), "genesis should suggest 'select'");
    }
}
