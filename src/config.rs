//! Configuration — parses `testaruda.toml` for adapter registrations and project settings.

use std::path::Path;

use serde::Deserialize;

/// Top-level project configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub adapters: AdapterConfig,
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
