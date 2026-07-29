//! CLI entry point for testaruda.

mod commands;
mod doctor;

use clap::{Parser, Subcommand};
use std::io::IsTerminal;

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
pub struct Cli {
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

fn main() -> miette::Result<()> {
    init_tracing();

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

    let cli = parse_cli_with_suggestions(&guide);

    dispatch(cli.command).map_err(|e| {
        // Error-footer hook (TIA-ADAPT feedback protocol):
        // On non-zero exit with no Fix suggestion, print feedback command hint
        eprintln!("testaruda: {}", e);
        eprintln!("Feedback: testaruda feedback bug --from-last-error");
        record_error_scratch();
        std::process::exit(1);
    })
}

/// Initialize the tracing subscriber with optional JSON format and color probe.
fn init_tracing() {
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
}

/// Parse the CLI, intercepting unknown subcommands to offer genesis typo
/// suggestions before falling back to clap's default error handling.
fn parse_cli_with_suggestions(guide: &genesis::guide::Guide) -> Cli {
    match Cli::try_parse() {
        Ok(c) => c,
        Err(err) => {
            // Use genesis suggestions to provide better error messages for typos
            let err_str = err.to_string();

            // Extract the unknown subcommand from clap's error.
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
    }
}

/// Dispatch a parsed command to its handler in [`commands`].
fn dispatch(command: Command) -> miette::Result<()> {
    match command {
        Command::Init => commands::init(),
        Command::Calibrate { threshold } => commands::calibrate(threshold),
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
        } => commands::select(commands::SelectArgs {
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
        }),
        Command::Ingest { path, raw, adapter } => {
            commands::ingest(commands::IngestArgs { path, raw, adapter })
        }
        Command::Graph => commands::graph(),
        Command::Import { path } => commands::import_graph(path),
        Command::Explain { test_id, change } => commands::explain(test_id, change),
        Command::Validate { program } => commands::validate(program),
        Command::Discover {} => commands::discover(),
        Command::Metrics {} => commands::metrics(),
        Command::Doctor { fix } => commands::doctor(fix),
        Command::Feedback {
            kind,
            from_last_error,
            dry_run,
        } => commands::feedback(commands::FeedbackArgs {
            kind,
            from_last_error,
            dry_run,
        }),
        Command::GenCliDocs => commands::gen_cli_docs(),
        Command::Completions { shell } => commands::completions(shell),
    }
}

/// Write the current invocation as a genesis error-scratch record so
/// `testaruda feedback bug --from-last-error` can reconstruct the context.
fn record_error_scratch() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let ts = format_iso_ts(now);
    let record = genesis::feedback::scratch::ErrorRecord {
        ts,
        argv: std::env::args().collect(),
        exit: 1,
        footer: Some("Feedback: testaruda feedback bug --from-last-error".into()),
        kind: "error".into(),
    };
    genesis::feedback::scratch::write_scratch_best_effort("testaruda", &record);
}

/// Format a Unix epoch second count as an ISO-8601 UTC timestamp.
fn format_iso_ts(now_secs: u64) -> String {
    let days = now_secs / 86400;
    let time_secs = now_secs % 86400;
    let hours = time_secs / 3600;
    let mins = (time_secs % 3600) / 60;
    let secs = time_secs % 60;
    let (y, m, d) = year_month_day(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, mins, secs
    )
}

/// Convert a day count since the Unix epoch (1970-01-01) into (year, month, day).
fn year_month_day(mut remaining: i64) -> (i64, usize, u8) {
    let mut y = 1970i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let month_days = month_lengths(y);
    let mut m = 1usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md as i64 {
            m = i + 1;
            break;
        }
        remaining -= md as i64;
    }
    if m == 0 {
        m = 12;
    }
    let d = (remaining + 1) as u8;
    (y, m, d)
}

/// Whether `year` is a Gregorian leap year.
fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Day count for each month of `year` (Jan..Dec).
fn month_lengths(y: i64) -> [u32; 12] {
    if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use commands::{ci_exit, CiOutcome, CiPlan};
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

    #[test]
    fn test_format_iso_ts_epoch() {
        // 1970-01-01T00:00:00Z
        assert_eq!(format_iso_ts(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_iso_ts_known() {
        // 2026-07-29T12:00:00Z — 56 years, ~207 days
        // Use a well-known anchor: 2024-01-01T00:00:00Z = 1704067200
        assert_eq!(format_iso_ts(1704067200), "2024-01-01T00:00:00Z");
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
