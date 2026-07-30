//! Doctor command — suite-wide config lint checks via genesis doctor framework.
//!
//! Each check implements [`DoctorCheck`]. The [`DoctorRunner`] orchestrates
//! execution and produces a structured [`DoctorReport`] compatible with the
//! genesis envelope protocol.

use genesis::doctor::{DoctorCheck, DoctorReport, DoctorRunner};
use genesis::suite_linter::{LintResult, Severity};
use std::path::Path;

use testaruda::config::normalize_adapters_config;
use testaruda::config::Config;
use testaruda::config::{AdapterConfig, AdaptersConfigShape};

// ── Check: testaruda.toml exists ──────────────────────────────────────

struct ConfigExistsCheck;

impl DoctorCheck for ConfigExistsCheck {
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
    fn auto_fixable(&self) -> bool {
        true
    }
    fn fix(&self, repo_root: &Path) -> Result<Vec<LintResult>, Box<dyn std::error::Error>> {
        let config_path = repo_root.join("testaruda.toml");
        if !config_path.exists() {
            Config::write_default(repo_root)?;
        }
        Ok(vec![])
    }
}

// ── Check: adapter config shape ───────────────────────────────────────

struct AdapterConfigShapeCheck;

impl DoctorCheck for AdapterConfigShapeCheck {
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
    fn auto_fixable(&self) -> bool {
        true
    }
    fn fix(&self, repo_root: &Path) -> Result<Vec<LintResult>, Box<dyn std::error::Error>> {
        let config_path = repo_root.join("testaruda.toml");
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            if AdapterConfig::detect_shape(&content) == AdaptersConfigShape::Flat {
                let normalized = normalize_adapters_config(&content);
                if normalized != content {
                    std::fs::write(&config_path, &normalized)?;
                }
            }
        }
        Ok(vec![])
    }
}

// ── Build runner ──────────────────────────────────────────────────────

/// Build the doctor runner with all testaruda-specific checks.
pub fn build_doctor_runner() -> DoctorRunner {
    DoctorRunner::new(vec![
        Box::new(ConfigExistsCheck),
        Box::new(AdapterConfigShapeCheck),
    ])
    .with_tool_name("testaruda")
}

// ── Run doctor ────────────────────────────────────────────────────────

/// Run the doctor checks against the project root.
///
/// Returns a [`DoctorReport`] with structured results including summary counts,
/// individual check entries, and optional fix commands.
///
/// If `fix` is `true`, auto-fixable checks will be repaired automatically.
pub fn run_doctor(repo_root: &Path, fix: bool) -> Result<DoctorReport, Box<dyn std::error::Error>> {
    let runner = build_doctor_runner();
    runner.run(repo_root, fix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_config(dir: &Path, content: &str) {
        std::fs::write(dir.join("testaruda.toml"), content).unwrap();
    }

    // ── ConfigExistsCheck ─────────────────────────────────────────────

    #[test]
    fn test_config_exists_pass_when_file_present() {
        let dir = tmp();
        write_config(dir.path(), "[adapters]\nextensions = {}\n");
        let check = ConfigExistsCheck;
        let results = check.run(dir.path()).unwrap();
        assert!(results.is_empty(), "expected pass, got issues");
    }

    #[test]
    fn test_config_exists_fails_when_missing() {
        let dir = tmp();
        let check = ConfigExistsCheck;
        let results = check.run(dir.path()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].severity, Severity::Error);
        assert_eq!(results[0].fix.as_deref(), Some("testaruda init"));
    }

    #[test]
    fn test_config_exists_auto_fixable() {
        let check = ConfigExistsCheck;
        assert!(check.auto_fixable());
    }

    #[test]
    fn test_config_exists_fix_creates_file() {
        let dir = tmp();
        let check = ConfigExistsCheck;
        check.fix(dir.path()).unwrap();
        assert!(dir.path().join("testaruda.toml").exists());
    }

    // ── AdapterConfigShapeCheck ───────────────────────────────────────

    #[test]
    fn test_adapter_shape_pass_when_canonical() {
        let dir = tmp();
        write_config(dir.path(), "[adapters]\nextensions = { rust = [] }\n");
        let check = AdapterConfigShapeCheck;
        let results = check.run(dir.path()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_adapter_shape_warns_on_flat() {
        let dir = tmp();
        write_config(
            dir.path(),
            "[adapters]\n\".rs\" = \"testaruda-adapter-rust\"\ndefault = \"testaruda-adapter-rust\"\n",
        );
        let check = AdapterConfigShapeCheck;
        let results = check.run(dir.path()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].severity, Severity::Warning);
    }

    #[test]
    fn test_adapter_shape_warns_on_missing() {
        let dir = tmp();
        write_config(dir.path(), "[other]\nkey = \"val\"\n");
        let check = AdapterConfigShapeCheck;
        let results = check.run(dir.path()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].severity, Severity::Warning);
        assert!(results[0].message.contains("[adapters]"));
    }

    #[test]
    fn test_adapter_shape_skips_when_no_config() {
        let dir = tmp();
        let check = AdapterConfigShapeCheck;
        let results = check.run(dir.path()).unwrap();
        assert!(results.is_empty());
    }

    // ── DoctorRunner integration ──────────────────────────────────────

    #[test]
    fn test_runner_all_pass() {
        let dir = tmp();
        write_config(dir.path(), "[adapters]\nextensions = { rust = [] }\n");
        let report = run_doctor(dir.path(), false).unwrap();
        assert!(
            report.is_healthy(),
            "expected healthy, got {:?}",
            report.summary
        );
    }

    #[test]
    fn test_runner_reports_missing_config() {
        let dir = tmp();
        let report = run_doctor(dir.path(), false).unwrap();
        assert!(!report.is_healthy());
        assert_eq!(report.summary.fail, 1);
    }

    #[test]
    fn test_runner_fix_creates_config() {
        let dir = tmp();
        // Run with fix=true, config should be created
        let report = run_doctor(dir.path(), true).unwrap();
        assert!(
            report.is_healthy(),
            "post-fix should be healthy: {:?}",
            report.summary
        );
        assert!(dir.path().join("testaruda.toml").exists());
    }

    // ── DoctorReport envelope ─────────────────────────────────────────

    #[test]
    fn test_report_to_envelope() {
        let dir = tmp();
        let report = run_doctor(dir.path(), false).unwrap();
        let envelope = report.to_envelope();
        assert_eq!(envelope.data.tool, "testaruda");
        // Envelope::success always sets ok=true; health is in data.summary
        assert!(envelope.ok);
        assert_eq!(envelope.data.summary.fail, report.summary.fail);
    }

    #[test]
    fn test_report_exit_code() {
        let dir = tmp();
        write_config(dir.path(), "[adapters]\nextensions = { rust = [] }\n");
        let report = run_doctor(dir.path(), false).unwrap();
        assert_eq!(report.exit_code(), 0);

        let dir2 = tmp();
        let report2 = run_doctor(dir2.path(), false).unwrap();
        assert_eq!(report2.exit_code(), 1);
    }
}
