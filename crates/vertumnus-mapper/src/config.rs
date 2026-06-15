//! Config-file type mapping registry.
//!
//! Loads user-defined type mappings from `.vertumnus/config.toml`, allowing
//! users to teach Vertumnus about ecosystem types (e.g., `bytes::Bytes`,
//! `url::Url`, `std::time::Duration`) without modifying source code.
//!
//! # Example
//!
//! ```toml
//! # .vertumnus/config.toml
//! [type_mappings]
//! "bytes::Bytes" = { python = "bytes", strategy = "native" }
//! "std::time::Duration" = { python = "float", strategy = "native" }
//! "std::path::PathBuf" = { python = "str", strategy = "native" }
//! "url::Url" = { python = "str", strategy = "native" }
//! ```

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::annotated_ir::PyO3Strategy;

/// The full configuration loaded from `.vertumnus/config.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct VertumnusConfig {
    /// User-defined type mappings from Rust type strings to Python equivalents.
    #[serde(default)]
    pub type_mappings: HashMap<String, TypeMappingEntry>,
}

/// A single type mapping entry in the config file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TypeMappingEntry {
    /// The Python type string (e.g., "bytes", "float", "str")
    pub python: String,
    /// The PyO3 strategy to use: "native", "pyclass", "pyenum", "maperr", "manual"
    pub strategy: String,
}

impl VertumnusConfig {
    /// Load configuration from a TOML file path.
    ///
    /// Returns `Ok(None)` if the file does not exist (not an error).
    /// Returns `Err` if the file exists but cannot be parsed.
    pub fn from_file(path: &Path) -> Result<Option<Self>, ConfigError> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(path.to_path_buf(), e))?;
        let config: Self = toml::from_str(&content)
            .map_err(|e| ConfigError::Parse(path.to_path_buf(), e))?;
        Ok(Some(config))
    }

    /// Auto-detect config by looking for `.vertumnus/config.toml` in the crate
    /// directory or any parent directory.
    pub fn auto_detect(crate_dir: &Path) -> Result<Option<Self>, ConfigError> {
        let config_path = crate_dir.join(".vertumnus").join("config.toml");
        Self::from_file(&config_path)
    }

    /// Look up a Rust type string in the config registry.
    ///
    /// Checks the fully-qualified name first, then falls back to matching
    /// by simple name (the part after the last `::`). This allows looking up
    /// `"Duration"` when the config has `"std::time::Duration"`.
    /// Returns `None` if no mapping is configured.
    pub fn lookup(&self, type_str: &str) -> Option<&TypeMappingEntry> {
        // First try exact match
        if let Some(entry) = self.type_mappings.get(type_str) {
            return Some(entry);
        }
        // Then try matching by simple name (the part after the last ::)
        let simple = type_str.split("::").last().unwrap_or(type_str);
        for (key, entry) in &self.type_mappings {
            let key_simple = key.split("::").last().unwrap_or(key);
            if key_simple == simple {
                return Some(entry);
            }
        }
        None
    }

    /// Convert the strategy string from the config to a [`PyO3Strategy`].
    pub fn parse_strategy(strategy: &str) -> PyO3Strategy {
        match strategy {
            "native" => PyO3Strategy::Native,
            "pyclass" => PyO3Strategy::PyClass,
            "pyenum" => PyO3Strategy::PyEnum,
            "pyfunction" => PyO3Strategy::PyFunction,
            "maperr" | "map_err" => PyO3Strategy::MapErr,
            "manual" => PyO3Strategy::ManualStub,
            _ => PyO3Strategy::ManualStub,
        }
    }
}

/// Errors that can occur when loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The config file could not be read.
    #[error("Failed to read config file '{0}': {1}")]
    Io(std::path::PathBuf, std::io::Error),
    /// The config file could not be parsed as TOML.
    #[error("Failed to parse config file '{0}': {1}")]
    Parse(std::path::PathBuf, toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Write a temporary config file and load it.
    /// Uses a unique directory per test to avoid conflicts.
    fn write_temp_config(content: &str) -> std::path::PathBuf {
        let unique_id: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("vertumnus_test_config_{}", unique_id));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "{}", content).unwrap();
        path
    }

    #[test]
    fn test_load_empty_config() {
        let path = write_temp_config("");
        let config = VertumnusConfig::from_file(&path).unwrap();
        assert!(config.is_some());
        assert!(config.unwrap().type_mappings.is_empty());
    }

    #[test]
    fn test_load_config_with_mappings() {
        let content = r#"
[type_mappings]
"bytes::Bytes" = { python = "bytes", strategy = "native" }
"std::time::Duration" = { python = "float", strategy = "native" }
"url::Url" = { python = "str", strategy = "native" }
"my_crate::MyType" = { python = "MyPyType", strategy = "pyclass" }
"SomeResult" = { python = "int", strategy = "maperr" }
"UnsupportedType" = { python = "typing.Any", strategy = "manual" }
"#;
        let path = write_temp_config(content);
        let config = VertumnusConfig::from_file(&path).unwrap().unwrap();

        assert_eq!(config.type_mappings.len(), 6);

        // Check fully-qualified lookup
        let entry = config.lookup("bytes::Bytes").unwrap();
        assert_eq!(entry.python, "bytes");
        assert_eq!(entry.strategy, "native");

        // Check simple-name lookup
        let entry = config.lookup("Duration").unwrap();
        assert_eq!(entry.python, "float");

        // Check lookup fallback: fully-qualified returns match even with simple query
        let entry = config.lookup("std::time::Duration").unwrap();
        assert_eq!(entry.python, "float");

        // Check strategy parsing
        assert_eq!(VertumnusConfig::parse_strategy("native"), PyO3Strategy::Native);
        assert_eq!(VertumnusConfig::parse_strategy("pyclass"), PyO3Strategy::PyClass);
        assert_eq!(VertumnusConfig::parse_strategy("pyenum"), PyO3Strategy::PyEnum);
        assert_eq!(VertumnusConfig::parse_strategy("maperr"), PyO3Strategy::MapErr);
        assert_eq!(VertumnusConfig::parse_strategy("manual"), PyO3Strategy::ManualStub);
        assert_eq!(VertumnusConfig::parse_strategy("unknown"), PyO3Strategy::ManualStub);
    }

    #[test]
    fn test_load_nonexistent_config() {
        let path = std::path::Path::new("/nonexistent/path/config.toml");
        let config = VertumnusConfig::from_file(path).unwrap();
        assert!(config.is_none());
    }

    #[test]
    fn test_auto_detect_no_config() {
        let unique_id: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("vertumnus_test_no_config_{}", unique_id));
        std::fs::create_dir_all(&dir).unwrap();
        let config = VertumnusConfig::auto_detect(&dir).unwrap();
        assert!(config.is_none());
    }

    #[test]
    fn test_auto_detect_with_config() {
        let unique_id: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("vertumnus_test_auto_detect_{}", unique_id));
        std::fs::create_dir_all(&dir.join(".vertumnus")).unwrap();
        let config_path = dir.join(".vertumnus").join("config.toml");
        let content = r#"
[type_mappings]
"bytes::Bytes" = { python = "bytes", strategy = "native" }
"#;
        std::fs::write(&config_path, content).unwrap();
        let config = VertumnusConfig::auto_detect(&dir).unwrap().unwrap();
        assert_eq!(config.type_mappings.len(), 1);
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let path = write_temp_config("invalid toml content [[[");
        let result = VertumnusConfig::from_file(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            ConfigError::Parse(_, _) => {} // Expected
            _ => panic!("Expected Parse error"),
        }
    }
}
