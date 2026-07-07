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
    },
    /// Ingest test run results to update the model
    Ingest {
        /// Path to run output file
        path: String,
    },
    /// Show the current dependency graph
    Graph,
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
}

fn main() -> miette::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Init => {
            println!("🔧 Initializing testaruda store...");
            let store = testaruda::Store::open_default()?;
            store.initialize()?;
            println!("✅ testaruda initialized");
            Ok(())
        }
        Command::Select { base, head, files } => {
            let store = testaruda::Store::open_default()?;
            let delta = testaruda::ChangeSet::from_diff(base.as_deref(), head.as_deref(), files.as_deref())?;
            let selection = testaruda::Selector::select(&store, &delta)?;
            let out = serde_json::to_string_pretty(&selection)
                .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
            println!("{}", out);
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
        Command::Explain { test_id, change } => {
            let store = testaruda::Store::open_default()?;
            let explanation = store.explain(&test_id, change.as_deref())?;
            let out = serde_json::to_string_pretty(&explanation)
                .map_err(|e| miette::miette!("JSON serialization failed: {}", e))?;
            println!("{}", out);
            Ok(())
        }
        Command::Validate { program } => {
            println!("🔮 Soufflé oracle validation");
            if let Some(path) = program {
                let output = std::process::Command::new("souffle")
                    .arg(&path)
                    .output()
                    .map_err(|e| miette::miette!("Soufflé not found: {}", e))?;
                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
            Ok(())
        }
    }
}