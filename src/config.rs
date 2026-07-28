//! Configuration — parses `testaruda.toml` for adapter registrations and project settings.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

/// The shape of the `[adapters]` section in testaruda.toml.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdaptersConfigShape {
    /// Canonical: extensions under `[adapters.extensions]` sub-table.
    Canonical,
    /// Deprecated: extension keys at the top-level `[adapters]` table.
    Flat,
    /// No adapter configuration found.
    Missing,
}

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
        // Normalize deprecated flat adapter format to canonical before parsing
        let normalized = normalize_adapters_config(&content);
        toml::from_str(&normalized)
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
# Default adapter when no extension matches
default = "{}"

[adapters.extensions]
# Map file extensions to adapter binaries.
# Adapters must be installed on PATH or specified as absolute paths.
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
".jl" = "testaruda-adapter-julia"
".ts" = "testaruda-adapter-typescript"
".tsx" = "testaruda-adapter-typescript"
".mts" = "testaruda-adapter-typescript"
".cts" = "testaruda-adapter-typescript"

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
///
/// Supports two TOML shapes (canonical and deprecated flat):
/// ```toml
/// # Canonical (preferred):
/// [adapters]
/// default = "testaruda-adapter-rust"
/// [adapters.extensions]
/// ".rs" = "testaruda-adapter-rust"
///
/// # Deprecated flat format:
/// [adapters]
/// ".rs" = "testaruda-adapter-rust"
/// default = "testaruda-adapter-rust"
/// ```
///
/// The deprecated flat format is normalized to canonical during `Config::load`.
#[derive(Debug, Clone, Deserialize)]
pub struct AdapterConfig {
    /// Extension-to-binary mappings.
    #[serde(default)]
    pub extensions: HashMap<String, String>,
    /// Default adapter binary.
    #[serde(default)]
    pub default: Option<String>,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        let mut extensions = std::collections::HashMap::new();
        extensions.insert(".rs".to_string(), "testaruda-adapter-rust".to_string());
        extensions.insert(".py".to_string(), "testaruda-adapter-python".to_string());
        extensions.insert(".jl".to_string(), "testaruda-adapter-julia".to_string());
        extensions.insert(
            ".ts".to_string(),
            "testaruda-adapter-typescript".to_string(),
        );
        extensions.insert(
            ".tsx".to_string(),
            "testaruda-adapter-typescript".to_string(),
        );
        extensions.insert(
            ".mts".to_string(),
            "testaruda-adapter-typescript".to_string(),
        );
        extensions.insert(
            ".cts".to_string(),
            "testaruda-adapter-typescript".to_string(),
        );
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
    // Check Julia (Project.toml marker)
    if project_root.join("Project.toml").exists() {
        return Some("testaruda-adapter-julia".to_string());
    }
    // Check TypeScript (vitest/jest config or package.json with vitest/jest)
    if project_root.join("vitest.config.ts").exists()
        || project_root.join("vitest.config.js").exists()
        || project_root.join("jest.config.ts").exists()
        || project_root.join("jest.config.js").exists()
    {
        return Some("testaruda-adapter-typescript".to_string());
    }
    if let Ok(content) = std::fs::read_to_string(project_root.join("package.json")) {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(deps) = parsed["devDependencies"].as_object() {
                if deps.contains_key("vitest") || deps.contains_key("jest") {
                    return Some("testaruda-adapter-typescript".to_string());
                }
            }
        }
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
    /// The `default` field is handled separately from extensions.
    pub fn to_registry(&self) -> crate::adapter::AdapterRegistry {
        let mut reg = crate::adapter::AdapterRegistry::new();
        for (ext, binary) in &self.extensions {
            reg.register(ext, binary);
        }
        if let Some(ref default) = self.default {
            reg.set_default(default);
        }
        reg
    }

    /// Detect the shape of the `[adapters]` section in raw TOML.
    /// Returns `AdaptersConfigShape::Flat` when extension-like keys (starting with `.`)
    /// are found directly under `[adapters]` instead of under `[adapters.extensions]`.
    pub fn detect_shape(toml_content: &str) -> AdaptersConfigShape {
        let value: Result<toml::Value, _> = toml_content.parse();
        let value = match value {
            Ok(v) => v,
            Err(_) => return AdaptersConfigShape::Missing,
        };

        let adapters = match value.get("adapters") {
            Some(v) => v,
            None => return AdaptersConfigShape::Missing,
        };

        let table = match adapters.as_table() {
            Some(t) => t,
            None => return AdaptersConfigShape::Missing,
        };

        // Canonical shape: extensions are under a sub-table `[adapters.extensions]`
        if table.contains_key("extensions") {
            return AdaptersConfigShape::Canonical;
        }

        // Flat shape: extension-like keys (starting with `.`) at the top level
        let has_extension_keys = table.keys().any(|k| k.starts_with('.'));
        if has_extension_keys {
            return AdaptersConfigShape::Flat;
        }

        // No extension keys found — could be missing or a config with only `default`
        AdaptersConfigShape::Canonical
    }
}

/// Normalize the deprecated flat `[adapters]` format to canonical `[adapters.extensions]`.
///
/// The flat format puts extension keys like `.rs` directly under `[adapters]`:
/// ```toml
/// [adapters]
/// ".rs" = "testaruda-adapter-rust"
/// default = "testaruda-adapter-rust"
/// ```
///
/// This is normalized to:
/// ```toml
/// [adapters]
/// default = "testaruda-adapter-rust"
/// [adapters.extensions]
/// ".rs" = "testaruda-adapter-rust"
/// ```
///
/// If the config is already canonical, or if parsing fails, returns the original content.
fn normalize_adapters_config(content: &str) -> String {
    // Strategy: parse the raw TOML, detect flat format, then string-manipulate
    // to move extension keys under [adapters.extensions].
    // We don't use toml::Value::to_string() because its Display output
    // uses inline tables which aren't parseable by toml::from_str.

    let value: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return content.to_string(),
    };

    let adapters = match value.get("adapters") {
        Some(v) => v,
        None => return content.to_string(),
    };

    let table = match adapters.as_table() {
        Some(t) => t,
        None => return content.to_string(),
    };

    // If it already has an extensions sub-table, it's canonical — no change needed
    if table.contains_key("extensions") {
        return content.to_string();
    }

    // Check for extension-like keys at the top level (flat format)
    let has_extension_keys: Vec<String> = table
        .keys()
        .filter(|k| k.starts_with('.'))
        .cloned()
        .collect();

    if has_extension_keys.is_empty() {
        return content.to_string();
    }

    // Build the normalized output via string manipulation of the original TOML.
    // We parse the adapters section, extract extension keys, and rewrite
    // the [adapters] section with extensions under [adapters.extensions].

    // Collect all non-extension keys from adapters (e.g., default)
    let non_extension_entries: Vec<(&String, &toml::Value)> =
        table.iter().filter(|(k, _)| !k.starts_with('.')).collect();

    // Serialize the canonical form using serde's toml serialization
    let mut canonical_adapters = toml::value::Table::new();
    for (key, val) in &non_extension_entries {
        canonical_adapters.insert((*key).clone(), (*val).clone());
    }

    let mut extensions = toml::value::Table::new();
    for key in &has_extension_keys {
        if let Some(val) = table.get(key) {
            extensions.insert(key.clone(), val.clone());
        }
    }
    canonical_adapters.insert("extensions".to_string(), toml::Value::Table(extensions));

    // Build the full output: for all top-level tables in the original TOML,
    // replace the adapters table with the canonical version.
    let mut out = toml::value::Table::new();
    if let Some(root_table) = value.as_table() {
        let canonical_adapters_value = toml::Value::Table(canonical_adapters);
        for (key, val) in root_table {
            if key == "adapters" {
                out.insert(key.clone(), canonical_adapters_value.clone());
            } else {
                out.insert(key.clone(), val.clone());
            }
        }
    }

    toml::to_string(&toml::Value::Table(out)).unwrap_or_else(|_| content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_config_round_trips_adapter_extensions() {
        let project = tempfile::tempdir().unwrap();

        Config::write_default(project.path()).unwrap();
        let config = Config::load(project.path()).unwrap();

        assert_eq!(
            config.adapters.extensions.get(".rs").map(String::as_str),
            Some("testaruda-adapter-rust")
        );
        assert_eq!(
            config.adapters.extensions.get(".py").map(String::as_str),
            Some("testaruda-adapter-python")
        );
        assert_eq!(
            config.adapters.extensions.get(".jl").map(String::as_str),
            Some("testaruda-adapter-julia")
        );
        assert_eq!(
            config.adapters.extensions.get(".ts").map(String::as_str),
            Some("testaruda-adapter-typescript")
        );
        assert_eq!(
            config.adapters.extensions.get(".tsx").map(String::as_str),
            Some("testaruda-adapter-typescript")
        );
        assert_eq!(
            config.adapters.extensions.get(".mts").map(String::as_str),
            Some("testaruda-adapter-typescript")
        );
        assert_eq!(
            config.adapters.extensions.get(".cts").map(String::as_str),
            Some("testaruda-adapter-typescript")
        );
        assert_eq!(
            config.adapters.default.as_deref(),
            Some("testaruda-adapter-rust")
        );
    }

    #[test]
    fn flat_format_parses_via_config_load() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("testaruda.toml");
        std::fs::write(
            &path,
            r#"
[adapters]
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
default = "testaruda-adapter-rust"
"#,
        )
        .unwrap();

        let config = Config::load(project.path()).unwrap();
        assert_eq!(
            config.adapters.extensions.get(".rs").map(String::as_str),
            Some("testaruda-adapter-rust")
        );
        assert_eq!(
            config.adapters.extensions.get(".py").map(String::as_str),
            Some("testaruda-adapter-python")
        );
        assert_eq!(
            config.adapters.default.as_deref(),
            Some("testaruda-adapter-rust")
        );
    }

    #[test]
    fn canonical_format_parses_correctly() {
        let toml_content = r#"
[adapters]
default = "testaruda-adapter-rust"

[adapters.extensions]
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
"#;
        let config: Config = toml::from_str(toml_content).unwrap();
        assert_eq!(
            config.adapters.extensions.get(".rs").map(String::as_str),
            Some("testaruda-adapter-rust")
        );
        assert_eq!(
            config.adapters.extensions.get(".py").map(String::as_str),
            Some("testaruda-adapter-python")
        );
        assert_eq!(
            config.adapters.default.as_deref(),
            Some("testaruda-adapter-rust")
        );
    }

    #[test]
    fn detect_shape_canonical() {
        let toml = r#"
[adapters]
default = "testaruda-adapter-rust"
[adapters.extensions]
".rs" = "testaruda-adapter-rust"
"#;
        assert_eq!(
            AdapterConfig::detect_shape(toml),
            AdaptersConfigShape::Canonical
        );
    }

    #[test]
    fn detect_shape_flat() {
        let toml = r#"
[adapters]
".rs" = "testaruda-adapter-rust"
default = "testaruda-adapter-rust"
"#;
        assert_eq!(AdapterConfig::detect_shape(toml), AdaptersConfigShape::Flat);
    }

    #[test]
    fn detect_shape_missing() {
        assert_eq!(
            AdapterConfig::detect_shape(""),
            AdaptersConfigShape::Missing
        );
    }

    #[test]
    fn both_formats_produce_equivalent_registry() {
        let canonical = r#"
[adapters]
default = "my-default"
[adapters.extensions]
".rs" = "my-rust"
".py" = "my-python"
"#;

        let c: Config = toml::from_str(canonical).unwrap();

        // Flat format needs to go through Config::load for normalization
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("testaruda.toml");
        std::fs::write(
            &path,
            r#"
[adapters]
".rs" = "my-rust"
".py" = "my-python"
default = "my-default"
"#,
        )
        .unwrap();
        let f = Config::load(project.path()).unwrap();

        let reg_c = c.adapters.to_registry();
        let reg_f = f.adapters.to_registry();

        // Both should resolve the same way
        assert_eq!(
            reg_c.resolve("foo.rs"),
            reg_f.resolve("foo.rs"),
            "canonical and flat should resolve .rs identically"
        );
        assert_eq!(
            reg_c.resolve("foo.py"),
            reg_f.resolve("foo.py"),
            "canonical and flat should resolve .py identically"
        );
        assert_eq!(
            reg_c.default_binary(),
            reg_f.default_binary(),
            "canonical and flat should have same default"
        );
    }

    #[test]
    fn normalize_adapters_config_converts_flat_to_canonical() {
        let flat = r#"[adapters]
".rs" = "testaruda-adapter-rust"
".py" = "testaruda-adapter-python"
default = "testaruda-adapter-rust"
"#;
        let normalized = super::normalize_adapters_config(flat);
        let value: toml::Value = normalized.parse().unwrap();
        let adapters = value.get("adapters").unwrap().as_table().unwrap();
        assert!(
            adapters.contains_key("extensions"),
            "normalized config should have [adapters.extensions]"
        );
        let extensions = adapters.get("extensions").unwrap().as_table().unwrap();
        assert_eq!(
            extensions.get(".rs").unwrap().as_str(),
            Some("testaruda-adapter-rust")
        );
        assert_eq!(
            extensions.get(".py").unwrap().as_str(),
            Some("testaruda-adapter-python")
        );
        assert_eq!(
            adapters.get("default").unwrap().as_str(),
            Some("testaruda-adapter-rust")
        );
    }

    #[test]
    fn normalize_leaves_canonical_unchanged() {
        let canonical = r#"[adapters]
default = "testaruda-adapter-rust"
[adapters.extensions]
".rs" = "testaruda-adapter-rust"
"#;
        let normalized = super::normalize_adapters_config(canonical);
        assert_eq!(normalized, canonical);
    }
}
