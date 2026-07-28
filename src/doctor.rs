//! Doctor command — suite-wide config lint checks via genesis::suite_linter.
//!
//! Each check implements `LintCheck`. The `LinterRegistry` runs them all,
//! genesis orchestrates the output.

use genesis::suite_linter::{LintCheck, LintResult, LinterRegistry, Severity};
use std::path::Path;

/// Build the linter registry with all testaruda-specific checks.
pub fn build_linter_registry() -> LinterRegistry {
    let mut reg = LinterRegistry::new();
    reg.register_all(vec![
        Box::new(ConfigExistsCheck),
        Box::new(AdapterConfigShapeCheck),
    ]);
    reg
}

use testaruda::config::normalize_adapters_config;
use testaruda::config::Config;
/// Re-export config types for use in this module.
use testaruda::config::{AdapterConfig, AdaptersConfigShape};

// ── Check: testaruda.toml exists ──────────────────────────────────────

struct ConfigExistsCheck;

impl LintCheck for ConfigExistsCheck {
    fn name(&self) -> &'static str {
        "testaruda.config-exists"
    }
    fn description(&self) -> &'static str {
        "Check that testaruda.toml exists in the project root"
    }
    fn run(&self, repo_root: &Path) -> Result<Vec<LintResult>, Box<dyn std::error::Error>> {
        let config_path = repo_root.join("testaruda.toml");
        if !config_path.exists() {
            return Ok(vec![LintResult::with_fix(
                format!("No testaruda.toml found at {}", config_path.display()),
                Severity::Error,
                "testaruda init",
            )]);
        }
        Ok(vec![])
    }
}

// ── Check: adapter config shape ───────────────────────────────────────

struct AdapterConfigShapeCheck;

impl LintCheck for AdapterConfigShapeCheck {
    fn name(&self) -> &'static str {
        "testaruda.adapter-config-shape"
    }
    fn description(&self) -> &'static str {
        "Validate that [adapters.extensions] uses the canonical format"
    }
    fn run(&self, repo_root: &Path) -> Result<Vec<LintResult>, Box<dyn std::error::Error>> {
        let config_path = repo_root.join("testaruda.toml");
        if !config_path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&config_path)?;
        match AdapterConfig::detect_shape(&content) {
            AdaptersConfigShape::Canonical => Ok(vec![]),
            AdaptersConfigShape::Flat => {
                let fix_msg = format!(
                    "Deprecated flat format detected in {}. Move extension keys under [adapters.extensions]",
                    config_path.display()
                );
                Ok(vec![LintResult::with_fix(
                    fix_msg,
                    Severity::Warning,
                    "Run `testaruda init` to regenerate, or manually edit the config",
                )])
            }
            AdaptersConfigShape::Missing => Ok(vec![LintResult::new(
                format!("No [adapters] section found in {}", config_path.display()),
                Severity::Warning,
            )]),
        }
    }
}

// ── Run doctor ────────────────────────────────────────────────────────

/// Run the doctor checks against the project root and print results.
/// Returns `true` if all checks passed (no errors), `false` otherwise.
pub fn run_doctor(repo_root: &Path, fix: bool) -> Result<bool, Box<dyn std::error::Error>> {
    let registry = build_linter_registry();
    let results = registry.run_all(repo_root);

    let mut has_errors = false;
    let mut has_warnings = false;

    for (check, check_results) in &results {
        for result in check_results {
            match result.severity {
                Severity::Error => {
                    has_errors = true;
                    eprintln!("❌ {}", result.format(check.name()));
                }
                Severity::Warning => {
                    has_warnings = true;
                    eprintln!("⚠️  {}", result.format(check.name()));
                }
                Severity::Advisory => {
                    eprintln!("💡 {}", result.format(check.name()));
                }
            }
        }
    }

    if !has_errors {
        eprintln!("✅ All checks passed");
    }

    // Apply --fix if requested
    if fix && (has_errors || has_warnings) {
        eprintln!();
        eprintln!("🔧 Running fixes...");

        // Fix: if testaruda.toml is missing, run init
        let config_path = repo_root.join("testaruda.toml");
        if !config_path.exists() {
            Config::write_default(repo_root)?;
            eprintln!("  ✅ Created {}", config_path.display());
        }

        // Fix: if flat format detected, normalize
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            if AdapterConfig::detect_shape(&content) == AdaptersConfigShape::Flat {
                let normalized = normalize_adapters_config(&content);
                if normalized != content {
                    std::fs::write(&config_path, &normalized)?;
                    eprintln!("  ✅ Normalized adapter config to canonical format");
                }
            }
        }
    }

    Ok(!has_errors)
}
