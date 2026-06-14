//! # Vertumnus Builder
//!
//! Phase 4 of the Vertumnus pipeline: scaffolds build configuration
//! (pyproject.toml, Cargo.toml) and invokes maturin to produce
//! a distributable Python wheel.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vertumnus_builder::{BuilderConfig, scaffold_all, run_maturin_build};
//! use std::path::PathBuf;
//!
//! let config = BuilderConfig {
//!     output_dir: PathBuf::from("../py-original-crate"),
//!     crate_path: PathBuf::from("/path/to/original-crate"),
//!     package_name: "my_package".to_string(),
//!     crate_name: "original_crate".to_string(),
//!     crate_version: "0.1.0".to_string(),
//! };
//! scaffold_all(&config).unwrap();
//! run_maturin_build(&config, true).unwrap();
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

/// Read the actual crate name from a Rust crate's Cargo.toml.
///
/// Parses `[package] name = "..."` from the file. This is more reliable
/// than using the normalized name from the IR, because Cargo allows hyphens
/// in package names which are normalized to underscores in some contexts.
pub fn read_crate_name(crate_path: &Path) -> Result<String, BuildError> {
    let cargo_toml_path = crate_path.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml_path)
        .map_err(|_| BuildError::CratePathNotFound(crate_path.to_path_buf()))?;

    // Simple TOML parser for [package] name = "..."
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[package]") {
            in_package = true;
            continue;
        }
        if in_package {
            if trimmed.starts_with('[') {
                // Reached next section without finding name
                break;
            }
            if let Some(name_val) = trimmed.strip_prefix("name = ") {
                // Handle both "..." and '...'
                let name = name_val
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                if !name.is_empty() {
                    return Ok(name);
                }
            }
        }
    }

    Err(BuildError::CratePathNotFound(crate_path.to_path_buf()))
}

/// Errors that can occur during the build phase.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("Failed to write pyproject.toml: {0}")]
    WritePyprojectToml(std::io::Error),

    #[error("Failed to write Cargo.toml: {0}")]
    WriteCargoToml(std::io::Error),

    #[error("Output directory does not exist: {0}")]
    OutputDirNotFound(PathBuf),

    #[error("Maturin not found. Is maturin installed? Run: pip install maturin")]
    MaturinNotFound,

    #[error("Maturin build failed: {0}")]
    MaturinBuildFailed(String),

    #[error("Maturin develop failed: {0}")]
    MaturinDevelopFailed(String),

    #[error("Failed to create directory {path}: {err}")]
    CreateDirFailed { path: PathBuf, err: std::io::Error },

    #[error("Original crate path does not exist: {0}")]
    CratePathNotFound(PathBuf),

    #[error("Failed to write CI workflow: {0}")]
    WriteCiWorkflow(std::io::Error),

    #[error("Maturin version check failed: {0}")]
    MaturinVersionCheck(String),
}

/// Configuration for the builder phase.
#[derive(Debug, Clone)]
pub struct BuilderConfig {
    /// Path to the output directory where generated files live
    pub output_dir: PathBuf,
    /// Path to the original Rust crate (for Cargo.toml path dependency)
    pub crate_path: PathBuf,
    /// Python package name
    pub package_name: String,
    /// Original Rust crate name
    pub crate_name: String,
    /// Original Rust crate version
    pub crate_version: String,
}

/// Scaffold all build configuration files in the output directory.
///
/// Writes:
/// - `pyproject.toml` — Maturin build configuration
/// - `Cargo.toml` — Rust crate configuration with pyo3 dependency
///   pointing back to the original crate via path dependency.
///
/// # Arguments
/// * `config` - Builder configuration
///
/// # Returns
/// A list of files that were written (relative to output_dir).
pub fn scaffold_all(config: &BuilderConfig) -> Result<Vec<PathBuf>, BuildError> {
    let mut written = Vec::new();

    // Ensure output directory exists
    if !config.output_dir.exists() {
        return Err(BuildError::OutputDirNotFound(config.output_dir.clone()));
    }

    // Ensure crate path exists
    if !config.crate_path.exists() {
        return Err(BuildError::CratePathNotFound(config.crate_path.clone()));
    }

    // Resolve the original crate path to an absolute path for the Cargo.toml dependency
    let abs_crate_path = config
        .crate_path
        .canonicalize()
        .unwrap_or_else(|_| config.crate_path.clone());

    // Write pyproject.toml
    let pyproject = generate_pyproject_toml(config);
    let pyproject_path = config.output_dir.join("pyproject.toml");
    std::fs::write(&pyproject_path, &pyproject).map_err(BuildError::WritePyprojectToml)?;
    written.push(pyproject_path);

    // Write Cargo.toml
    let cargo_toml = generate_cargo_toml(config, &abs_crate_path);
    let cargo_path = config.output_dir.join("Cargo.toml");
    std::fs::write(&cargo_path, &cargo_toml).map_err(BuildError::WriteCargoToml)?;
    written.push(cargo_path);

    Ok(written)
}

/// Generate the contents of `pyproject.toml` for a maturin-based project.
fn generate_pyproject_toml(config: &BuilderConfig) -> String {
    let pkg_name = &config.package_name;
    let version = &config.crate_version;

    let native_mod_name = format!("{}._core", pkg_name);
    format!(
        r#"[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "{pkg_name}"
version = "{version}"
description = "Python bindings for `{crate_name}` v{crate_version}"
requires-python = ">=3.8"

[project.urls]
Source = "https://github.com/Zikithezikit/Vertumnus"

[tool.maturin]
python-source = "python"
module-name = "{native_mod_name}"
"#,
        pkg_name = pkg_name,
        version = version,
        crate_name = config.crate_name,
        crate_version = config.crate_version,
        native_mod_name = native_mod_name,
    )
}

/// Generate the contents of `Cargo.toml` for the binding crate.
///
/// This creates a new Rust crate that:
/// - Has a unique name (the package name)
/// - Depends on `pyo3` with the `extension-module` feature
/// - Depends on the original crate via a path dependency
/// - Points `src/lib.rs` to the generated bindings
fn generate_cargo_toml(config: &BuilderConfig, abs_crate_path: &Path) -> String {
    let _lib_name = config.package_name.replace('-', "_");
    let original_crate_path = abs_crate_path.to_string_lossy();
    let original_crate_name = &config.crate_name;

    format!(
        r#"[package]
name = "{package_name}"
version = "{crate_version}"
edition = "2021"

# Explicitly declare this is not part of any parent workspace
[workspace]

[lib]
name = "_core"
crate-type = ["cdylib"]

[dependencies]
pyo3 = {{ version = "0.22", features = ["extension-module"] }}
{original_crate_name} = {{ path = "{original_crate_path}" }}
"#,
        package_name = config.package_name,
        crate_version = config.crate_version,
        original_crate_name = original_crate_name,
        original_crate_path = original_crate_path,
    )
}

/// Generate the contents of `.github/workflows/build.yml` for a maturin-based project.
///
/// This workflow builds the Python package on Linux, macOS, and Windows
/// using the `maturin-action` GitHub Action.
pub fn generate_ci_workflow(config: &BuilderConfig) -> String {
    let pkg_name = &config.package_name;
    let crate_name = &config.crate_name;

    let _ = crate_name; // used in the template comment below

    format!(
        r#"# CI workflow for {pkg_name} — auto-generated by Vertumnus
#
# Builds the Python wheel on Linux, macOS, and Windows,
# then publishes to PyPI on tagged releases.

name: Build & Publish {pkg_name}

on:
  push:
    branches: [main]
    tags: ["v*.*.*"]
  pull_request:
    branches: [main]

jobs:
  wheel:
    name: Build wheel (${{{{ matrix.os }}}})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        python-version: ["3.8", "3.9", "3.10", "3.11", "3.12"]

    runs-on: ${{{{ matrix.os }}}}

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: ${{{{ matrix.python-version }}}}

      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          args: --release --out dist
          working-directory: .

      - name: Upload wheels
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{{{ matrix.os }}}}-${{{{ matrix.python-version }}}}
          path: dist/

  publish:
    name: Publish to PyPI
    if: startsWith(github.ref, 'refs/tags/v')
    needs: [wheel]
    runs-on: ubuntu-latest

    steps:
      - uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist/

      - name: Publish to PyPI
        uses: PyO3/maturin-action@v1
        env:
          MATURIN_PYPI_TOKEN: ${{{{ secrets.PYPI_API_TOKEN }}}}
        with:
          command: upload
          args: --skip-existing dist/*
"#,
        pkg_name = pkg_name,
    )
}

/// Scaffold a GitHub Actions CI workflow in the output directory.
///
/// Writes `.github/workflows/build.yml` with a matrix build for
/// Linux, macOS, and Windows across Python 3.8–3.12.
///
/// # Arguments
/// * `config` - Builder configuration
///
/// # Returns
/// Path to the written workflow file.
pub fn scaffold_ci(config: &BuilderConfig) -> Result<PathBuf, BuildError> {
    let workflow = generate_ci_workflow(config);

    let workflows_dir = config.output_dir.join(".github").join("workflows");
    std::fs::create_dir_all(&workflows_dir).map_err(|e| BuildError::CreateDirFailed {
        path: workflows_dir.clone(),
        err: e,
    })?;

    let workflow_path = workflows_dir.join("build.yml");
    std::fs::write(&workflow_path, &workflow).map_err(BuildError::WriteCiWorkflow)?;

    Ok(workflow_path)
}

/// Run `maturin build --release` in the output directory to produce a wheel.
///
/// # Arguments
/// * `config` - Builder configuration
/// * `release` - Whether to build in release mode (default: true)
///
/// # Returns
/// Path to the built wheel file, if it can be determined.
pub fn run_maturin_build(
    config: &BuilderConfig,
    release: bool,
) -> Result<Option<PathBuf>, BuildError> {
    // Check if maturin is available
    let maturin_check = Command::new("maturin")
        .arg("--version")
        .output()
        .map_err(|_| BuildError::MaturinNotFound)?;

    if !maturin_check.status.success() {
        return Err(BuildError::MaturinNotFound);
    }

    // Build the wheel
    let mut cmd = Command::new("maturin");
    cmd.arg("build");
    cmd.current_dir(&config.output_dir);

    if release {
        cmd.arg("--release");
    }

    // Capture output
    let output = cmd
        .output()
        .map_err(|e| BuildError::MaturinBuildFailed(format!("Failed to execute maturin: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(BuildError::MaturinBuildFailed(format!(
            "maturin build failed.\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )));
    }

    // Try to find the built wheel in target/wheels/
    let wheels_dir = config.output_dir.join("target").join("wheels");
    let wheel_path = if wheels_dir.exists() {
        std::fs::read_dir(&wheels_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.extension().map(|e| e == "whl").unwrap_or(false))
    } else {
        None
    };

    Ok(wheel_path)
}

/// Run `maturin develop` in the output directory to install the package
/// in the current Python environment (for local development).
///
/// # Arguments
/// * `config` - Builder configuration
///
/// # Returns
/// Whether the command succeeded.
pub fn run_maturin_develop(config: &BuilderConfig) -> Result<(), BuildError> {
    // Check if maturin is available
    let maturin_check = Command::new("maturin")
        .arg("--version")
        .output()
        .map_err(|_| BuildError::MaturinNotFound)?;

    if !maturin_check.status.success() {
        return Err(BuildError::MaturinNotFound);
    }

    let output = Command::new("maturin")
        .arg("develop")
        .current_dir(&config.output_dir)
        .output()
        .map_err(|e| {
            BuildError::MaturinDevelopFailed(format!("Failed to execute maturin develop: {e}"))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(BuildError::MaturinDevelopFailed(format!(
            "maturin develop failed.\nstderr:\n{stderr}"
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a unique temporary directory for testing.
    /// Uses a global counter to avoid collisions between parallel tests.
    fn create_temp_dir() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

        let count = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "vertumnus-builder-test-{}-{}",
            std::process::id(),
            count
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_generate_pyproject_toml() {
        let config = BuilderConfig {
            output_dir: PathBuf::from("/tmp/out"),
            crate_path: PathBuf::from("/tmp/crate"),
            package_name: "my_package".to_string(),
            crate_name: "my_crate".to_string(),
            crate_version: "0.1.0".to_string(),
        };

        let content = generate_pyproject_toml(&config);
        assert!(content.contains("my_package"));
        assert!(content.contains("0.1.0"));
        assert!(content.contains("maturin"));
        assert!(content.contains("[build-system]"));
        assert!(content.contains("[project]"));
        assert!(content.contains("[tool.maturin]"));
    }

    #[test]
    fn test_generate_cargo_toml() {
        let config = BuilderConfig {
            output_dir: PathBuf::from("/tmp/out"),
            crate_path: PathBuf::from("/tmp/crate"),
            package_name: "my_package".to_string(),
            crate_name: "my_crate".to_string(),
            crate_version: "0.1.0".to_string(),
        };

        let content = generate_cargo_toml(&config, Path::new("/tmp/crate"));
        assert!(content.contains(r#"name = "my_package""#));
        assert!(content.contains(r#"name = "_core""#));
        assert!(content.contains("0.1.0"));
        assert!(content.contains("pyo3"));
        assert!(content.contains("extension-module"));
        assert!(content.contains(r#"my_crate = { path = "/tmp/crate" }"#));
        assert!(content.contains("[lib]"));
        assert!(content.contains("crate-type = [\"cdylib\"]"));
    }

    #[test]
    fn test_scaffold_all_creates_files() {
        let out_dir = create_temp_dir();
        let crate_dir = create_temp_dir();

        // Create a minimal Cargo.toml in the faux crate dir
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"test-crate\"\n",
        )
        .unwrap();

        let config = BuilderConfig {
            output_dir: out_dir.clone(),
            crate_path: crate_dir,
            package_name: "test_pkg".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.2.3".to_string(),
        };

        let written = scaffold_all(&config).unwrap();
        assert_eq!(written.len(), 2);

        let pyproject_path = out_dir.join("pyproject.toml");
        let cargo_path = out_dir.join("Cargo.toml");

        assert!(pyproject_path.exists(), "pyproject.toml should exist");
        assert!(cargo_path.exists(), "Cargo.toml should exist");

        let pyproject_content = fs::read_to_string(pyproject_path).unwrap();
        assert!(pyproject_content.contains("test_pkg"));

        let cargo_content = fs::read_to_string(cargo_path).unwrap();
        assert!(cargo_content.contains("test_pkg"));
        assert!(cargo_content.contains("pyo3"));

        // Cleanup
        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn test_scaffold_missing_output_dir() {
        let config = BuilderConfig {
            output_dir: PathBuf::from("/tmp/nonexistent-dir-12345"),
            crate_path: PathBuf::from("/tmp"),
            package_name: "pkg".to_string(),
            crate_name: "crate".to_string(),
            crate_version: "0.1.0".to_string(),
        };

        let result = scaffold_all(&config);
        assert!(result.is_err());
        match result {
            Err(BuildError::OutputDirNotFound(_)) => {} // expected
            _ => panic!("Expected OutputDirNotFound error"),
        }
    }

    #[test]
    fn test_scaffold_missing_crate_path() {
        let out_dir = create_temp_dir();
        let config = BuilderConfig {
            output_dir: out_dir.clone(),
            crate_path: PathBuf::from("/tmp/nonexistent-crate-path-67890"),
            package_name: "pkg".to_string(),
            crate_name: "crate".to_string(),
            crate_version: "0.1.0".to_string(),
        };

        let result = scaffold_all(&config);
        assert!(result.is_err());
        match result {
            Err(BuildError::CratePathNotFound(_)) => {} // expected
            _ => panic!("Expected CratePathNotFound error"),
        }

        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn test_read_crate_name_from_toml() {
        let dir = create_temp_dir();

        // Write a Cargo.toml with a hyphenated name
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let name = read_crate_name(&dir).unwrap();
        assert_eq!(name, "my-crate");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_read_crate_name_with_underscores() {
        let dir = create_temp_dir();

        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my_crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
        )
        .unwrap();

        let name = read_crate_name(&dir).unwrap();
        assert_eq!(name, "my_crate");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_generate_ci_workflow() {
        let config = BuilderConfig {
            output_dir: PathBuf::from("/tmp/out"),
            crate_path: PathBuf::from("/tmp/crate"),
            package_name: "my_package".to_string(),
            crate_name: "my_crate".to_string(),
            crate_version: "0.1.0".to_string(),
        };

        let content = generate_ci_workflow(&config);
        assert!(content.contains("my_package"));
        assert!(content.contains("maturin-action"));
        assert!(content.contains("ubuntu-latest"));
        assert!(content.contains("macos-latest"));
        assert!(content.contains("windows-latest"));
        assert!(
            content.contains("PYPI_API_TOKEN"),
            "Expected PYPI_API_TOKEN (all-caps GitHub secret) in content"
        );
        assert!(content.contains("python-version"));
        assert!(content.contains("3.8"));
        assert!(content.contains("3.12"));
    }

    #[test]
    fn test_scaffold_ci_creates_workflow() {
        let out_dir = create_temp_dir();

        let config = BuilderConfig {
            output_dir: out_dir.clone(),
            crate_path: PathBuf::from("/tmp/crate"),
            package_name: "test_pkg".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "0.1.0".to_string(),
        };

        let path = scaffold_ci(&config).unwrap();
        assert!(path.exists(), "Workflow file should exist");

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("test_pkg"));
        assert!(content.contains("maturin-action"));
        assert!(content.contains("PYPI_API_TOKEN"));

        let _ = fs::remove_dir_all(&out_dir);
    }

    #[test]
    fn test_scaffold_uses_actual_crate_name() {
        let out_dir = create_temp_dir();
        let crate_dir = create_temp_dir();

        // Create a Cargo.toml with hyphenated name
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"simple-math\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let config = BuilderConfig {
            output_dir: out_dir.clone(),
            crate_path: crate_dir.clone(),
            package_name: "simple_math".to_string(),
            crate_name: read_crate_name(&crate_dir).unwrap(),
            crate_version: "0.1.0".to_string(),
        };

        let _ = scaffold_all(&config).unwrap();

        let cargo_content = fs::read_to_string(out_dir.join("Cargo.toml")).unwrap();
        // The Cargo.toml should reference "simple-math" (with hyphen) as path dep
        assert!(
            cargo_content.contains(r#"simple-math = { path"#),
            "Should use original crate name with hyphens, got: {}",
            cargo_content
        );

        let _ = fs::remove_dir_all(&out_dir);
        let _ = fs::remove_dir_all(&crate_dir);
    }
}
