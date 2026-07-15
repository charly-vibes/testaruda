//! CLI entry point for testaruda.

use clap::{Parser, Subcommand};

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
        /// Selection ordering mode (default|deterministic|duration|predictive)
        #[arg(long, default_value = "default")]
        ordering: String,
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
}

fn main() -> miette::Result<()> {
    // Initialize tracing with optional JSON format
    let log_format = std::env::var("TESTARUDA_LOG_FORMAT").unwrap_or_default();
    let builder = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        );
    if log_format == "json" {
        builder.json().init();
    } else {
        builder.init();
    }

    let cli = Cli::parse();

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
                    Some(ref adapter) if adapter.contains("python") => {
                        println!("  🐍 Python project detected — default adapter set to testaruda-adapter-python");
                    }
                    Some(ref adapter) if adapter.contains("rust") => {
                        println!("  🦀 Rust project detected — default adapter set to testaruda-adapter-rust");
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
            println!("✅ testaruda initialized at {}", project_root.display());
            Ok(())
        }
        Command::Calibrate { threshold } => {
            let store = testaruda::Store::open_default()?;
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
            ordering,
        } => {
            let store = testaruda::Store::open_default()?;
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
                match ordering.as_str() {
                    "deterministic" => testaruda::TestOrdering::Deterministic,
                    "duration" => testaruda::TestOrdering::ByDuration,
                    "predictive" => testaruda::TestOrdering::Predictive,
                    _ => testaruda::TestOrdering::Default,
                }
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
                // Machine-readable plan (TIA-CI-006)
                let plan = CiPlan {
                    shadow_mode: shadow,
                    exit_code: outcome.exit_code(),
                    selected_count: selection.selected_count,
                    changed_count: selection.changed_count,
                    reason: outcome.reason(),
                    all_tests: shadow,
                    tests: &selection.tests,
                };
                let out = serde_json::to_string_pretty(&plan)
                    .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
                println!("{}", out);
            } else if pre_edit {
                // Pre-edit blast radius (TIA-AGENT-005): report affected tests
                println!("📡 Blast radius — pre-edit analysis");
                println!("  Changed units: {}", selection.changed_count);
                println!(
                    "  Affected tests: {} ({})",
                    selection.selected_count,
                    if selection.selected_count == 1 {
                        "1 test".to_string()
                    } else {
                        format!("{} tests", selection.selected_count)
                    }
                );
                if !selection.tests.is_empty() {
                    println!(
                        "  Affected test IDs: {:?}",
                        selection.tests.iter().map(|t| t.id).collect::<Vec<_>>()
                    );
                }
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
                    std::process::exit(code);
                }
            }

            // CI mode (TIA-CI-008): run selected tests and ingest results
            if ci && !selection.tests.is_empty() {
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
                        match testaruda::adapter::AdapterIO::spawn(adapter_binary, &[], None) {
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

                let mut adapter = testaruda::adapter::AdapterIO::spawn(&adapter_binary, &[], None)
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
            let explanation = store.explain(&test_id, change.as_deref())?;
            let out = serde_json::to_string_pretty(&explanation)
                .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
            println!("{}", out);
            Ok(())
        }
        Command::Validate { program } => {
            let store = testaruda::Store::open_default()?;
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
    }
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
        let mut adapter = match testaruda::adapter::AdapterIO::spawn(binary, &[], None) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("  ⚠️  Failed to spawn {}: {}", binary, e);
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
            let mut adapter = match testaruda::adapter::AdapterIO::spawn(binary, &[], None) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("  ⚠️  Failed to spawn {}: {}", binary, e);
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

            let mut adapter = match testaruda::adapter::AdapterIO::spawn(binary, &[], None) {
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

/// Exit codes for CI pipeline integration.
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
}
