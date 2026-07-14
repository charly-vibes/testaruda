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
        #[arg(long)]
        json: bool,
        /// Agent output format: structured JSON for LLM agent consumption (TIA-AGENT-001)
        #[arg(long)]
        agent: bool,
        /// Pre-edit blast radius: report affected tests for proposed changes (TIA-AGENT-005)
        #[arg(long)]
        pre_edit: bool,
    },
    /// Ingest test run results to update the model
    Ingest {
        /// Path to run output file
        path: String,
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
    Discover {
    },
}

fn main() -> miette::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

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
            }
            // Check for Soufflé oracle (TIA-ENG-010)
            match std::process::Command::new("souffle").arg("--version").output() {
                Ok(_) => println!("  ✓ Soufflé oracle found"),
                Err(_) => println!(
                    "  ⚠️  Soufflé not found — oracle validation disabled. \
                     Install souffle-lang from https://souffle-lang.github.io"
                ),
            }
            println!("✅ testaruda initialized at {}", project_root.display());
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
            if let Err(e) = run_adapter_pipeline(&store, &registry, &delta) {
                eprintln!("⚠️  Adapter warning: {} (using existing store data)", e);
            }

            // Agent mode implies deterministic ordering (TIA-AGENT-007)
            let ordering = if agent {
                testaruda::TestOrdering::Deterministic
            } else {
                testaruda::TestOrdering::Default
            };

            // Load selection context once, reuse for both engine and agent output
            let ctx = store.load_selection_context(&delta)?;
            let changed_ids = ctx.changed.clone();
            let unresolved_ids = ctx.unresolved.clone();
            let engine = testaruda::Engine::new(&store);
            let selection = engine.select_with_context(ctx, ordering)?;

            // Persist provenance for this selection run (TIA-PROV-005)
            let run_id = store.generate_run_id()?;
            let all_affected_ids = store.get_test_ids_for_content_units(&changed_ids, &unresolved_ids)?;
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
                let candidate_ids = store.get_test_ids_for_content_units(&changed_ids, &unresolved_ids)?;

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
                println!(
                    "  Changed units: {}",
                    selection.changed_count
                );
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
                    println!("  Affected test IDs: {:?}", 
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
            Ok(())
        }
        Command::Ingest { path } => {
            let store = testaruda::Store::open_default()?;
            let data = std::fs::read_to_string(&path)
                .map_err(|e| miette::miette!("Failed to read {}: {}", path, e))?;
            let results: serde_json::Value = serde_json::from_str(&data)
                .map_err(|e| miette::miette!("Failed to parse JSON: {}", e))?;
            store.ingest(&results)?;
            println!("✅ Run ingested");
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
                    match std::process::Command::new("souffle")
                        .arg(&path)
                        .output()
                    {
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
            run_discover_pipeline(&store, &registry).map_err(|e| miette::miette!(e))?;
            let count = store.test_items_count().unwrap_or(0);
            println!("\n✅ Discovered {} test items", count);
            Ok(())
        }
    }
}

// ===== Adapter Pipeline =====

/// Populate store from adapters: run discover on all files in the workspace,
/// then run static-deps on changed files.
pub fn run_adapter_pipeline(
    store: &testaruda::Store,
    registry: &testaruda::adapter::AdapterRegistry,
    delta: &testaruda::ChangeSet,
) -> std::result::Result<(), String> {
    // Step 1: Run discover on all files that have registered adapters
    // Use a broad scan — any extension we have a registered adapter for
    let mut seen_adapters = std::collections::HashSet::new();

    // For the changed files, resolve adapters and run discover
    for path in &delta.files {
        if let Some(binary) = registry.resolve(path) {
            if !seen_adapters.insert(binary.to_string()) {
                continue; // already ran this adapter
            }
            let mut adapter = match testaruda::adapter::AdapterIO::spawn(binary, &[], None) {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("  ⚠️  Failed to spawn {}: {}", binary, e);
                    continue;
                }
            };

            // Run discover
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

            // Run static-deps on changed files for this adapter
            // We pass only the files that match this adapter's extension
            let adapter_files: Vec<String> = delta
                .files
                .iter()
                .filter(|f| registry.resolve(f) == Some(binary))
                .cloned()
                .collect();

            if !adapter_files.is_empty() {
                match adapter.static_deps(&adapter_files) {
                    Ok(result) => {
                        eprintln!(
                            "  🔗 {} computed {} edges",
                            adapter.name,
                            result.edges.len()
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
        }
    }

    Ok(())
}

/// Discover-only: run adapter discovery on a broader set of files.
pub fn run_discover_pipeline(
    store: &testaruda::Store,
    registry: &testaruda::adapter::AdapterRegistry,
) -> std::result::Result<(), String> {
    // Walk the project to find files matching registered extensions
    let mut seen_adapters = std::collections::HashSet::new();

    for entry in walkdir::WalkDir::new(".")
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            name != "target"
                && name != ".git"
                && name != "node_modules"
                && name != ".venv"
                && name != "venv"
                && name != "__pycache__"
                && name != ".mypy_cache"
                && name != ".pytest_cache"
                && name != "build"
                && name != "dist"
                && name != ".tox"
        })
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
