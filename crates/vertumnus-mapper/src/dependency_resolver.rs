//! Dependency-aware type resolution.
//!
//! Reads `Cargo.lock` to identify crate dependencies, then uses that
//! information to produce better type mapping fallbacks when fully-qualified
//! types from dependencies are encountered.
//!
//! # Example
//!
//! When the mapper encounters `url::Url` in a function signature, this module
//! identifies `url` as a known dependency and emits a warning like:
//!
//! > Type 'url::Url' is from dependency 'url v0.2.8' — add a mapping in
//! > .vertumnus/config.toml: \[type_mappings] "url::Url" = { python = "str",
//! > strategy = "native" }
//!
//! # Cargo.lock Format
//!
//! We parse only the subset of `Cargo.lock` (package name + version) needed
//! for dependency lookups. The full `Cargo.lock` spec includes dependency
//! edges, but for our purposes just knowing that a crate name is a direct
//! or transitive dependency is sufficient.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

/// Information about a single package in `Cargo.lock`.
#[derive(Debug, Clone, Deserialize)]
struct CargoLockPackage {
    /// Package name (e.g., "url")
    name: String,
    /// Package version (e.g., "2.3.4")
    version: String,
}

/// Parsed subset of a `Cargo.lock` file.
#[derive(Debug, Clone, Deserialize)]
struct CargoLockFile {
    /// List of all packages in the lock file.
    #[serde(default)]
    package: Vec<CargoLockPackage>,
}

/// Resolved dependency information for a crate.
#[derive(Debug, Clone)]
pub struct DependencyInfo {
    /// The crate name (e.g., "url")
    pub name: String,
    /// The version (e.g., "2.3.4")
    pub version: String,
}

/// Cached dependency information from `Cargo.lock`.
#[derive(Debug, Clone, Default)]
pub struct CargoLockInfo {
    /// Set of known dependency names (lowercase for case-insensitive lookup).
    known_deps: HashSet<String>,
    /// Full dependency details for the most useful warnings.
    deps: Vec<DependencyInfo>,
}

impl CargoLockInfo {
    /// Check if a crate name is a known dependency.
    ///
    /// Lookup is case-insensitive (lowercased).
    pub fn is_dependency(&self, name: &str) -> bool {
        self.known_deps.contains(&name.to_lowercase())
    }

    /// Get detailed info about a dependency by name.
    pub fn get_dep(&self, name: &str) -> Option<&DependencyInfo> {
        let lower = name.to_lowercase();
        self.deps.iter().find(|d| d.name.to_lowercase() == lower)
    }

    /// Returns true if there are any known dependencies.
    pub fn is_empty(&self) -> bool {
        self.deps.is_empty()
    }

    /// Returns the number of known dependencies.
    pub fn len(&self) -> usize {
        self.deps.len()
    }

    /// Add a dependency for testing purposes (only available in test builds).
    #[cfg(test)]
    pub fn add_dep(&mut self, name: &str, version: &str) {
        self.known_deps.insert(name.to_lowercase());
        self.deps.push(DependencyInfo {
            name: name.to_string(),
            version: version.to_string(),
        });
    }
}

/// Try to extract the crate name from a fully-qualified Rust type path.
///
/// # Examples
///
/// - `"url::Url"` -> `Some("url")`
/// - `"serde_json::Value"` -> `Some("serde_json")`
/// - `"std::collections::HashMap"` -> `Some("std")`
/// - `"i32"` -> `None`
/// - `"MyStruct"` -> `None`
pub fn extract_crate_name(type_str: &str) -> Option<&str> {
    // Look for the first `::` separator
    let sep = type_str.find("::")?;

    // The crate name is everything before the first `::`
    let candidate = &type_str[..sep];

    // Exclude known false positives that look like paths but aren't dependencies.
    // `std`, `core`, `alloc`, `proc_macro`, `test`, `cfg_eval` are part of the
    // Rust standard library / toolchain, not third-party deps.
    if matches!(candidate, "std" | "core" | "alloc" | "proc_macro" | "test") {
        return None;
    }

    // The crate name must be non-empty and should be a valid Rust identifier
    // (starts with a letter or underscore, contains alphanumeric + underscores).
    if candidate.is_empty() {
        return None;
    }

    // Check that it looks like a valid crate name (just alphanumeric + underscores + hyphens)
    if !candidate
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }

    Some(candidate)
}

/// Determine the suggested Python fallback type for an unknown dependency type.
///
/// Returns `("Bound'_, PyAny", "PyAny")` as the Python type and a human-readable
/// fallback name for warnings.
pub fn dependency_fallback_type() -> (&'static str, &'static str) {
    ("Bound<'_, PyAny>", "typing.Any")
}

/// Build a user-friendly warning message for a dependency type.
pub fn format_dependency_warning(
    type_str: &str,
    dep: Option<&DependencyInfo>,
    crate_root: Option<&Path>,
) -> String {
    let dep_info = match dep {
        Some(d) => format!("'{} v{}'", d.name, d.version),
        None => "a dependency".to_string(),
    };

    let config_hint = if let Some(root) = crate_root {
        format!(
            "\n  Add a mapping in {}/.vertumnus/config.toml:\n  [type_mappings]\n  \"{}\" = {{ python = \"<python_type>\", strategy = \"native\" }}",
            root.display(),
            type_str
        )
    } else {
        String::new()
    };

    format!(
        "Type '{}' is from dependency {} — not in the type registry. Fell back to `Bound<'_, PyAny>`.{}",
        type_str, dep_info, config_hint
    )
}

/// Load `Cargo.lock` from a crate directory.
///
/// Returns `None` if the file doesn't exist or can't be parsed (best-effort).
pub fn load_cargo_lock(crate_dir: &Path) -> Option<CargoLockInfo> {
    let lock_path = crate_dir.join("Cargo.lock");
    if !lock_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(lock_path).ok()?;
    let parsed: CargoLockFile = toml::from_str(&content).ok()?;

    let mut known_deps = HashSet::new();
    let mut deps = Vec::new();

    for pkg in &parsed.package {
        let lower = pkg.name.to_lowercase();
        known_deps.insert(lower);
        deps.push(DependencyInfo {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
        });
    }

    Some(CargoLockInfo { known_deps, deps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_lock(content: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("vertumnus_test_cargo_lock_{}", unique));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Cargo.lock");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "{}", content).unwrap();
        dir // return the directory, not the file path
    }

    #[test]
    fn test_extract_crate_name_simple() {
        assert_eq!(extract_crate_name("url::Url"), Some("url"));
        assert_eq!(extract_crate_name("serde_json::Value"), Some("serde_json"));
        assert_eq!(extract_crate_name("bytes::Bytes"), Some("bytes"));
    }

    #[test]
    fn test_extract_crate_name_std_skipped() {
        // Standard library types should NOT be treated as dependencies
        assert_eq!(extract_crate_name("std::collections::HashMap"), None);
        assert_eq!(extract_crate_name("core::option::Option"), None);
        assert_eq!(extract_crate_name("alloc::vec::Vec"), None);
    }

    #[test]
    fn test_extract_crate_name_no_path() {
        assert_eq!(extract_crate_name("i32"), None);
        assert_eq!(extract_crate_name("String"), None);
        assert_eq!(extract_crate_name("MyStruct"), None);
        assert_eq!(extract_crate_name(""), None);
    }

    #[test]
    fn test_extract_crate_name_deep_path() {
        assert_eq!(
            extract_crate_name("my_crate::some::module::Type"),
            Some("my_crate")
        );
    }

    #[test]
    fn test_load_cargo_lock_simple() {
        let content = r#"
[[package]]
name = "url"
version = "2.3.4"

[[package]]
name = "serde"
version = "1.0.188"
"#;
        let dir = write_temp_lock(content);
        let info = load_cargo_lock(&dir).unwrap();
        assert!(info.is_dependency("url"));
        assert!(info.is_dependency("serde"));
        assert!(!info.is_dependency("nonexistent"));

        let dep = info.get_dep("url").unwrap();
        assert_eq!(dep.name, "url");
        assert_eq!(dep.version, "2.3.4");
    }

    #[test]
    fn test_load_cargo_lock_case_insensitive() {
        let content = r#"
[[package]]
name = "Url"
version = "2.3.4"
"#;
        let dir = write_temp_lock(content);
        let info = load_cargo_lock(&dir).unwrap();
        assert!(info.is_dependency("url"));
        assert!(info.is_dependency("Url"));
        assert!(info.is_dependency("URL"));
    }

    #[test]
    fn test_load_cargo_lock_missing() {
        let dir = std::env::temp_dir().join("nonexistent_dir_for_test");
        let info = load_cargo_lock(&dir);
        assert!(info.is_none());
    }

    #[test]
    fn test_load_cargo_lock_empty() {
        // TOML requires at least some structure; use a file with just a comment
        let content = "# empty lock file\n";
        let dir = write_temp_lock(content);
        let info = load_cargo_lock(&dir).unwrap();
        assert!(info.is_empty());
    }

    #[test]
    fn test_format_dependency_warning() {
        let dep = DependencyInfo {
            name: "url".to_string(),
            version: "2.3.4".to_string(),
        };
        let warning = format_dependency_warning("url::Url", Some(&dep), None);
        assert!(warning.contains("url v2.3.4"));
        assert!(warning.contains("url::Url"));
        assert!(warning.contains("Bound<'_, PyAny>"));
    }

    #[test]
    fn test_format_dependency_warning_with_config_hint() {
        let dep = DependencyInfo {
            name: "serde".to_string(),
            version: "1.0.188".to_string(),
        };
        let root = Path::new("/my/project");
        let warning = format_dependency_warning("serde::Serialize", Some(&dep), Some(root));
        assert!(warning.contains(".vertumnus/config.toml"));
        assert!(warning.contains("serde::Serialize"));
    }

    #[test]
    fn test_dependency_fallback_type() {
        let (_, py) = dependency_fallback_type();
        assert_eq!(py, "typing.Any");
    }

    #[test]
    fn test_extract_crate_name_with_hyphen() {
        // Crate names can contain hyphens
        assert_eq!(extract_crate_name("my-crate::SomeType"), Some("my-crate"));
    }
}
