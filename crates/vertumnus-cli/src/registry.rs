//! Community type mapping registry.
//!
//! Vertumnus maintains a community-contributed registry of type mappings for
//! popular Rust crates. This module provides subcommands to fetch, list, and
//! apply mappings from the registry.
//!
//! # Registry Format
//!
//! The registry is a TOML file with the same format as `.vertumnus/config.toml`,
//! optionally including a `[registry]` metadata section. It is hosted at a
//! configurable URL (default: a Vertumnus-maintained GitHub repo).
//!
//! ```toml
//! [registry]
//! version = "1"
//! description = "Vertumnus community type mappings"
//!
//! [type_mappings]
//! "bytes::Bytes" = { python = "bytes", strategy = "native" }
//! "url::Url" = { python = "str", strategy = "native" }
//! "std::time::Duration" = { python = "float", strategy = "native" }
//! ```
//!
//! # Subcommands
//!
//! - `vertumnus registry fetch` — Fetch the latest community registry
//! - `vertumnus registry list` — List all available mappings (filter by query)
//! - `vertumnus registry apply` — Apply community mappings to local config
//! - `vertumnus registry add <rust_type>=<python_type> --strategy <strategy>` — Add a custom mapping to local config

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use vertumnus_mapper::config::{TypeMappingEntry, VertumnusConfig};

/// Default URL for the community registry.
///
/// This points to the official Vertumnus community mappings repository.
/// Users can override with the `--registry-url` flag.
const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/vertumnus/registry/main/registry.toml";

/// Metadata about the registry file itself.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct RegistryMetadata {
    version: Option<String>,
    description: Option<String>,
}

/// The full registry file (metadata + type mappings).
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryFile {
    registry: Option<RegistryMetadata>,
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMappingEntry>,
}

/// Result of fetching the registry.
#[derive(Debug)]
pub struct RegistryResult {
    /// The fetched type mappings
    pub mappings: HashMap<String, TypeMappingEntry>,
    /// Number of mappings fetched
    pub count: usize,
    /// Registry version (if available)
    pub version: Option<String>,
}

/// Fetch the community registry from the default URL.
///
/// Returns the parsed mappings and metadata.
pub fn fetch_registry(url: Option<&str>) -> Result<RegistryResult, RegistryError> {
    let url = url.unwrap_or(DEFAULT_REGISTRY_URL);

    // Fetch the registry file
    let response = ureq::get(url)
        .call()
        .map_err(|e| RegistryError::Fetch(format!("Failed to fetch registry: {e}")))?;

    let body = response
        .into_string()
        .map_err(|e| RegistryError::Fetch(format!("Failed to read response: {e}")))?;

    let registry: RegistryFile = toml::from_str(&body)
        .map_err(|e| RegistryError::Parse(format!("Failed to parse registry: {e}")))?;

    let count = registry.type_mappings.len();
    let version = registry.registry.and_then(|m| m.version);

    Ok(RegistryResult {
        mappings: registry.type_mappings,
        count,
        version,
    })
}

/// Save the fetched registry mappings to a local cache file.
pub fn save_registry_cache(
    mappings: &HashMap<String, TypeMappingEntry>,
    cache_dir: &Path,
) -> Result<(), RegistryError> {
    let cache_path = cache_dir.join("community_registry.toml");
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| RegistryError::Io(format!("Failed to create cache dir: {e}")))?;
    }

    let mut content =
        String::from("# Vertumnus community type mappings\n# Fetched from remote registry\n\n");
    content.push_str("[type_mappings]\n");

    // Sort keys for deterministic output
    let mut keys: Vec<&String> = mappings.keys().collect();
    keys.sort();
    for key in keys {
        let entry = &mappings[key];
        content.push_str(&format!(
            "\"{}\" = {{ python = \"{}\", strategy = \"{}\" }}\n",
            key, entry.python, entry.strategy
        ));
    }

    std::fs::write(&cache_path, &content)
        .map_err(|e| RegistryError::Io(format!("Failed to write registry cache: {e}")))?;

    Ok(())
}

/// Load the cached community registry from disk.
pub fn load_registry_cache(cache_dir: &Path) -> Result<Option<RegistryResult>, RegistryError> {
    let cache_path = cache_dir.join("community_registry.toml");
    if !cache_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&cache_path)
        .map_err(|e| RegistryError::Io(format!("Failed to read registry cache: {e}")))?;

    let registry: RegistryFile = toml::from_str(&content)
        .map_err(|e| RegistryError::Parse(format!("Failed to parse cached registry: {e}")))?;

    let count = registry.type_mappings.len();
    let version = registry.registry.and_then(|m| m.version);

    Ok(Some(RegistryResult {
        mappings: registry.type_mappings,
        count,
        version,
    }))
}

/// Apply registry mappings to a local config file.
///
/// Merges the registry mappings into the user's `.vertumnus/config.toml`,
/// preferring user-defined mappings when there's a conflict.
pub fn apply_registry_to_config(
    registry: &HashMap<String, TypeMappingEntry>,
    config_path: &Path,
) -> Result<(), RegistryError> {
    // Load existing config (if any)
    let existing_config = VertumnusConfig::from_file(config_path)
        .map_err(|e| RegistryError::Io(format!("Failed to read existing config: {e}")))?;

    let mut merged = match existing_config {
        Some(config) => config.type_mappings,
        None => HashMap::new(),
    };

    // Add registry mappings that don't already exist (user mappings take priority)
    for (key, entry) in registry {
        merged.entry(key.clone()).or_insert_with(|| entry.clone());
    }

    // Write back the merged config
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| RegistryError::Io(format!("Failed to create config dir: {e}")))?;
    }

    let mut content = String::from(
        "# Vertumnus configuration\n# Auto-generated with community registry mappings\n\n",
    );
    content.push_str("[type_mappings]\n");

    let mut keys: Vec<&String> = merged.keys().collect();
    keys.sort();
    for key in keys {
        let entry = &merged[key];
        content.push_str(&format!(
            "\"{}\" = {{ python = \"{}\", strategy = \"{}\" }}\n",
            key, entry.python, entry.strategy
        ));
    }

    std::fs::write(config_path, &content)
        .map_err(|e| RegistryError::Io(format!("Failed to write config: {e}")))?;

    Ok(())
}

/// Add a single mapping to the local config file.
pub fn add_mapping_to_config(
    rust_type: &str,
    python_type: &str,
    strategy: &str,
    config_path: &Path,
) -> Result<(), RegistryError> {
    // Load existing config
    let existing_config = VertumnusConfig::from_file(config_path)
        .map_err(|e| RegistryError::Io(format!("Failed to read existing config: {e}")))?;

    let mut mappings = match existing_config {
        Some(config) => config.type_mappings,
        None => HashMap::new(),
    };

    // Add or update the mapping
    mappings.insert(
        rust_type.to_string(),
        TypeMappingEntry {
            python: python_type.to_string(),
            strategy: strategy.to_string(),
        },
    );

    // Write back
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| RegistryError::Io(format!("Failed to create config dir: {e}")))?;
    }

    let mut content = String::from("# Vertumnus configuration\n\n[type_mappings]\n");
    let mut keys: Vec<&String> = mappings.keys().collect();
    keys.sort();
    for key in keys {
        let entry = &mappings[key];
        content.push_str(&format!(
            "\"{}\" = {{ python = \"{}\", strategy = \"{}\" }}\n",
            key, entry.python, entry.strategy
        ));
    }

    std::fs::write(config_path, &content)
        .map_err(|e| RegistryError::Io(format!("Failed to write config: {e}")))?;

    Ok(())
}

/// Get the user's config directory for Vertumnus.
pub fn config_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("vertumnus")
}

/// Errors related to the registry.
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Failed to fetch registry: {0}")]
    Fetch(String),
    #[error("Failed to parse registry: {0}")]
    Parse(String),
    #[error("I/O error: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Write a temporary config file for testing.
    fn write_temp_config(dir: &Path, content: &str) -> PathBuf {
        let path = dir.join("config.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "{}", content).unwrap();
        path
    }

    #[test]
    fn test_apply_registry_to_config_new() {
        let dir = std::env::temp_dir().join("vertumnus_test_registry_apply_new");
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.toml");

        let mut registry = HashMap::new();
        registry.insert(
            "bytes::Bytes".to_string(),
            TypeMappingEntry {
                python: "bytes".to_string(),
                strategy: "native".to_string(),
            },
        );
        registry.insert(
            "url::Url".to_string(),
            TypeMappingEntry {
                python: "str".to_string(),
                strategy: "native".to_string(),
            },
        );

        apply_registry_to_config(&registry, &config_path).unwrap();

        // Verify the file was created
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("bytes::Bytes"));
        assert!(content.contains("url::Url"));
    }

    #[test]
    fn test_apply_registry_to_config_merge() {
        let dir = std::env::temp_dir().join("vertumnus_test_registry_merge");
        std::fs::create_dir_all(&dir).unwrap();

        // Write existing config with a user mapping
        let existing = r#"
[type_mappings]
"user::Type" = { python = "int", strategy = "native" }
"#;
        let config_path = write_temp_config(&dir, existing);

        let mut registry = HashMap::new();
        registry.insert(
            "url::Url".to_string(),
            TypeMappingEntry {
                python: "str".to_string(),
                strategy: "native".to_string(),
            },
        );
        // This should NOT override the user mapping
        registry.insert(
            "user::Type".to_string(),
            TypeMappingEntry {
                python: "float".to_string(),
                strategy: "native".to_string(),
            },
        );

        apply_registry_to_config(&registry, &config_path).unwrap();

        // Verify user mapping preserved (registry didn't override)
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("url::Url"));
        assert!(content.contains("user::Type"));
        // User mapping should still have "int", not "float"
        assert!(content.contains("\"int\""));
    }

    #[test]
    fn test_add_mapping_to_config() {
        let dir = std::env::temp_dir().join("vertumnus_test_registry_add");
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = dir.join("config.toml");

        add_mapping_to_config("bytes::Bytes", "bytes", "native", &config_path).unwrap();

        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("bytes::Bytes"));
        assert!(content.contains("bytes"));
    }

    #[test]
    fn test_save_and_load_registry_cache() {
        let dir = std::env::temp_dir().join("vertumnus_test_registry_cache");
        std::fs::create_dir_all(&dir).unwrap();

        let mut mappings = HashMap::new();
        mappings.insert(
            "url::Url".to_string(),
            TypeMappingEntry {
                python: "str".to_string(),
                strategy: "native".to_string(),
            },
        );

        save_registry_cache(&mappings, &dir).unwrap();

        let loaded = load_registry_cache(&dir).unwrap();
        assert!(loaded.is_some());
        let result = loaded.unwrap();
        assert_eq!(result.count, 1);
        assert!(result.mappings.contains_key("url::Url"));
    }

    #[test]
    fn test_load_registry_cache_missing() {
        let dir = std::env::temp_dir().join("vertumnus_test_registry_cache_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let result = load_registry_cache(&dir).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_config_dir() {
        let dir = config_dir();
        assert!(dir.ends_with("vertumnus"));
    }
}
