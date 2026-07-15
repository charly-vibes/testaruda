//! Configuration — parses `testaruda.toml` for adapter registrations and project settings.

use std::path::Path;

use serde::Deserialize;

/// Top-level project configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub adapters: AdapterConfig,
    /// Confidence threshold in [0.0, 1.0] (TIA-CONF-002, TIA-SAFE-002).
    /// If the minimum Viterbi path confidence across reachability-selected
    /// tests in a component falls below this threshold, all tests in that
    /// component are selected (component-scoped fallback).
    /// Default: 0.5
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Must-run rules (TIA-SAFE-009): path glob patterns mapped to test
    /// node IDs. When a file matching the pattern changes, the mapped tests
    /// are force-selected.
    #[serde(default)]
    pub must_run: MustRunConfig,
    /// Periodic full-run configuration (TIA-SAFE-006).
    #[serde(default)]
    pub periodic_full_run: PeriodicFullRunConfig,
    /// Environment configuration (TIA-CORE-008, TIA-RUN-006).
    #[serde(default)]
    pub environment: EnvironmentConfig,
    /// Discover configuration (TIA-ADAPT-004).
    #[serde(default)]
    pub discover: DiscoverConfig,
}

/// Environment configuration (TIA-CORE-008).
#[derive(Debug, Clone, Deserialize)]
pub struct EnvironmentConfig {
    /// Name/fingerprint of the current environment. Defaults to "default".
    #[serde(default = "default_environment_name")]
    pub name: String,
}

fn default_environment_name() -> String {
    "default".to_string()
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            name: default_environment_name(),
        }
    }
}

/// Discover configuration: directory exclusion patterns (TIA-ADAPT-004).
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoverConfig {
    /// Directory/file names to exclude from discover walks.
    /// Defaults to common patterns (target, .git, node_modules, .venv, etc.).
    #[serde(default = "default_exclude_list")]
    pub exclude: Vec<String>,
}

fn default_exclude_list() -> Vec<String> {
    vec![
        "target".to_string(),
        ".git".to_string(),
        "node_modules".to_string(),
        ".venv".to_string(),
        "venv".to_string(),
        "__pycache__".to_string(),
        ".mypy_cache".to_string(),
        ".pytest_cache".to_string(),
        "build".to_string(),
        "dist".to_string(),
        ".tox".to_string(),
    ]
}

impl Default for DiscoverConfig {
    fn default() -> Self {
        Self {
            exclude: default_exclude_list(),
        }
    }
}

/// Must-run rules configuration (TIA-SAFE-009).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MustRunConfig {
    /// Glob patterns mapped to lists of test node IDs.
    /// Flattened so TOML looks like:
    /// ```toml
    /// [must_run]
    /// "*.config" = ["config-test"]
    /// ```
    #[serde(flatten)]
    pub rules: std::collections::HashMap<String, Vec<String>>,
}

/// Periodic full-run configuration (TIA-SAFE-006).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PeriodicFullRunConfig {
    /// How often (in hours) to run a full test suite, regardless of
    /// change-based selection. 0 or missing means disabled.
    #[serde(default)]
    pub interval_hours: u64,
}

fn default_confidence_threshold() -> f64 {
    0.5
}

impl Default for Config {
    fn default() -> Self {
        Self {
            adapters: AdapterConfig::default(),
            confidence_threshold: default_confidence_threshold(),
            must_run: MustRunConfig::default(),
            periodic_full_run: PeriodicFullRunConfig::default(),
            environment: EnvironmentConfig::default(),
            discover: DiscoverConfig::default(),
        }
    }
}

impl Config {
    /// Load config from the project root.
    pub fn load(project_root: &Path) -> miette::Result<Self> {
        let path = project_root.join("testaruda.toml");
        let content = std::fs::read_to_string(&path)
            .map_err(|e| miette::miette!("Failed to read {}: {}", path.display(), e))?;
        toml::from_str(&content)
            .map_err(|e| miette::miette!("Failed to parse {}: {}", path.display(), e))
    }

    /// Load config or return defaults if file doesn't exist.
    pub fn load_or_default(project_root: &Path) -> Self {
        Self::load(project_root).unwrap_or_default()
    }

    /// Write a default config file, auto-detecting the project language.
    pub fn write_default(project_root: &Path) -> miette::Result<()> {
        let default_adapter = detect_project_language(project_root)
            .unwrap_or_else(|| "testaruda-adapter-rust".to_string());

        let path = project_root.join("testaruda.toml");
        let content = format!(
            r#"# testaruda configuration

[adapters]
# Map file extensions to adapter binaries.
# Adapters must be installed on PATH or specified as absolute paths.
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"

# Default adapter when no extension matches
default = "{}"

[discover]
# Directory/file names to exclude from discover walks.
# Matches the end of the directory name (e.g., ".venv" matches any path
# ending in ".venv"). The default list covers common tool/build artifacts.
exclude = ["target", ".git", "node_modules", ".venv", "venv",
          "__pycache__", ".mypy_cache", ".pytest_cache",
          "build", "dist", ".tox"]
"#,
            default_adapter
        );
        std::fs::write(&path, content)
            .map_err(|e| miette::miette!("Failed to write {}: {}", path.display(), e))?;
        println!("✅ Created testaruda.toml");
        Ok(())
    }
}

/// Adapter registry configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    /// Extension-to-binary mappings.
    #[serde(default)]
    pub extensions: std::collections::HashMap<String, String>,
    /// Default adapter binary.
    #[serde(default)]
    pub default: Option<String>,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        let mut extensions = std::collections::HashMap::new();
        extensions.insert(".rs".to_string(), "testaruda-adapter-rust".to_string());
        extensions.insert(".py".to_string(), "testaruda-adapter-python".to_string());
        Self {
            extensions,
            default: Some("testaruda-adapter-rust".to_string()),
        }
    }
}

/// Detect the primary project language by probing for common marker files.
/// Returns the adapter name for the detected language, or None if unknown.
pub fn detect_project_language(project_root: &Path) -> Option<String> {
    // Check Rust first (most specific marker)
    if project_root.join("Cargo.toml").exists() {
        return Some("testaruda-adapter-rust".to_string());
    }
    // Check Python (multiple possible markers)
    if project_root.join("pyproject.toml").exists()
        || project_root.join("setup.py").exists()
        || project_root.join("setup.cfg").exists()
        || project_root.join("requirements.txt").exists()
        || project_root.join("Pipfile").exists()
    {
        return Some("testaruda-adapter-python".to_string());
    }
    None
}

/// Build a filter_entry closure from a list of directory names to exclude.
/// Returns a function that returns `true` if the entry should be included,
/// `false` if it should be skipped (matched by name).
pub fn make_exclude_filter(exclude: &[String]) -> impl Fn(&walkdir::DirEntry) -> bool + '_ {
    move |entry: &walkdir::DirEntry| {
        let name = entry.file_name().to_string_lossy();
        !exclude.iter().any(|pat| name == pat.as_str())
    }
}

impl AdapterConfig {
    /// Build an `AdapterRegistry` from this config.
    /// Excludes the "default" key from extensions (handled separately).
    pub fn to_registry(&self) -> crate::adapter::AdapterRegistry {
        let mut reg = crate::adapter::AdapterRegistry::new();
        for (ext, binary) in &self.extensions {
            if ext != "default" {
                reg.register(ext, binary);
            }
        }
        if let Some(ref default) = self.default {
            reg.set_default(default);
        }
        reg
    }
}
