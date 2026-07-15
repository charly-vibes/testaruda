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

    /// Write a default config file.
    pub fn write_default(project_root: &Path) -> miette::Result<()> {
        let path = project_root.join("testaruda.toml");
        let content = r#"# testaruda configuration

[adapters]
# Map file extensions to adapter binaries.
# Adapters must be installed on PATH or specified as absolute paths.
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"

# Default adapter when no extension matches
default = "testaruda-adapter-rust"
"#;
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
