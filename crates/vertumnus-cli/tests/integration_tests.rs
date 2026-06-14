//! Integration tests for the Vertumnus CLI.
//!
//! These tests run the `vertumnus` binary on the test fixture crates
//! and verify that each phase produces correct output.
//!
//! Run with: `cargo test -p vertumnus-cli --test integration_tests`

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

/// Path to the workspace root, resolved from the test file location.
fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // vertumnus-cli is at crates/vertumnus-cli, workspace root is two levels up
    manifest_dir
        .parent()
        .expect("crates dir")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// Path to the built vertumnus binary.
fn vertumnus_binary() -> PathBuf {
    // In tests, we should use `cargo run` or the built binary.
    // The binary is at target/debug/vertumnus (or target/release).
    let workspace = workspace_root();
    let target_dir = workspace.join("target").join("debug");
    let binary = target_dir.join("vertumnus");
    if binary.exists() {
        return binary;
    }
    // Fall back to cargo run
    workspace.join("target").join("debug").join("vertumnus")
}

/// Create a unique temporary directory for test output.
fn temp_out_dir(name: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "vertumnus-int-test-{}-{}-{}",
        name,
        std::process::id(),
        count
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Run `vertumnus <args>` and return (stdout, stderr, status).
fn run_vertumnus(args: &[&str]) -> (String, String, std::process::ExitStatus) {
    let binary = vertumnus_binary();
    let output = Command::new(&binary)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("Failed to run vertumnus binary at {:?}: {}", binary, e));
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status)
}

/// Check if maturin is available for build tests.
fn maturin_available() -> bool {
    Command::new("maturin")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Inspect tests
// ---------------------------------------------------------------------------

#[test]
fn test_inspect_simple_math() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");

    let (stdout, stderr, status) = run_vertumnus(&["inspect", fixture.to_str().unwrap()]);

    assert!(status.success(), "inspect failed: {}", stderr);
    assert!(!stdout.is_empty(), "stdout should contain IR JSON");

    // Parse as JSON and check structure
    let ir: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(ir["vertumnus_ir_version"], "0.1");
    assert_eq!(ir["crate_name"], "simple_math");
    assert_eq!(ir["crate_version"], "0.1.0");

    let items = ir["items"].as_array().expect("items should be an array");
    assert!(!items.is_empty(), "should have items");

    // Check for specific items
    let names: Vec<&str> = items
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(names.contains(&"add"), "should contain 'add' function");
    assert!(names.contains(&"div"), "should contain 'div' function");
    assert!(names.contains(&"Point"), "should contain 'Point' struct");
    assert!(
        names.contains(&"Direction"),
        "should contain 'Direction' enum"
    );
}

#[test]
fn test_inspect_string_utils() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("string-utils");

    let (stdout, stderr, status) = run_vertumnus(&["inspect", fixture.to_str().unwrap()]);

    assert!(status.success(), "inspect failed: {}", stderr);
    let ir: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(ir["crate_name"], "string_utils");
    let items = ir["items"].as_array().expect("items should be an array");
    let names: Vec<&str> = items
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(names.contains(&"reverse"));
    assert!(names.contains(&"TextProcessor"));
    assert!(names.contains(&"ProcessStatus"));
}

#[test]
fn test_inspect_data_structures() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("data-structures");

    let (stdout, stderr, status) = run_vertumnus(&["inspect", fixture.to_str().unwrap()]);

    assert!(status.success(), "inspect failed: {}", stderr);
    let ir: serde_json::Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    assert_eq!(ir["crate_name"], "data_structures");
    let items = ir["items"].as_array().expect("items should be an array");

    let names: Vec<&str> = items
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(names.contains(&"sum_list"), "should contain sum_list");
    assert!(
        names.contains(&"word_frequencies"),
        "should contain word_frequencies"
    );
    assert!(names.contains(&"merge_maps"), "should contain merge_maps");
    assert!(
        names.contains(&"unique_words"),
        "should contain unique_words"
    );
    assert!(names.contains(&"DataStore"), "should contain DataStore");
    assert!(names.contains(&"Counter"), "should contain Counter");
    assert!(names.contains(&"Color"), "should contain Color");
    assert!(names.contains(&"OpStatus"), "should contain OpStatus");
}

// ---------------------------------------------------------------------------
// Inspect to file tests
// ---------------------------------------------------------------------------

#[test]
fn test_inspect_to_file() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("inspect-to-file");
    let out_file = out_dir.join("ir.json");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "inspect",
        fixture.to_str().unwrap(),
        "--output",
        out_file.to_str().unwrap(),
    ]);

    assert!(status.success(), "inspect --output failed: {}", stderr);
    assert!(
        _stdout.is_empty() || _stdout.trim().is_empty(),
        "stdout should be empty when --output is used, got: {}",
        _stdout
    );

    assert!(out_file.exists(), "output file should exist");
    let content = std::fs::read_to_string(&out_file).unwrap();
    let ir: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");
    assert_eq!(ir["crate_name"], "simple_math");

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Map tests
// ---------------------------------------------------------------------------

#[test]
fn test_map_simple_math() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("map-simple-math");
    let ir_file = out_dir.join("ir.json");

    // First inspect to file
    let (_, _, status) = run_vertumnus(&[
        "inspect",
        fixture.to_str().unwrap(),
        "--output",
        ir_file.to_str().unwrap(),
    ]);
    assert!(status.success());

    // Then map from file
    let (stdout, stderr, status) = run_vertumnus(&["map", ir_file.to_str().unwrap()]);
    assert!(status.success(), "map failed: {}", stderr);

    let annotated: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid annotated IR JSON");
    assert_eq!(annotated["crate_name"], "simple_math");

    // Check that items have mapping info
    let items = annotated["items"].as_array().unwrap();
    for item in items {
        let mapping = &item["mapping"];
        assert!(
            mapping.get("python_type").is_some(),
            "item '{}' should have python_type in mapping",
            item["name"]
        );
        assert!(
            mapping.get("pyo3_strategy").is_some(),
            "item '{}' should have pyo3_strategy in mapping",
            item["name"]
        );
    }

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Generate tests
// ---------------------------------------------------------------------------

#[test]
fn test_generate_simple_math() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("generate-simple-math");
    let ir_file = out_dir.join("ir.json");
    let annotated_file = out_dir.join("annotated.json");
    let gen_dir = out_dir.join("generated");

    // Inspect → Map to files
    run_vertumnus(&[
        "inspect",
        fixture.to_str().unwrap(),
        "--output",
        ir_file.to_str().unwrap(),
    ]);
    run_vertumnus(&[
        "map",
        ir_file.to_str().unwrap(),
        "--output",
        annotated_file.to_str().unwrap(),
    ]);

    // Generate with --overwrite
    let (_stdout, stderr, status) = run_vertumnus(&[
        "generate",
        annotated_file.to_str().unwrap(),
        "--output",
        gen_dir.to_str().unwrap(),
        "--package-name",
        "simple_math",
        "--overwrite",
    ]);
    assert!(status.success(), "generate failed: {}", stderr);

    // Check generated files exist
    assert!(
        gen_dir.join("src").join("lib.rs").exists(),
        "lib.rs should exist"
    );
    assert!(
        gen_dir.join("simple_math.pyi").exists(),
        ".pyi should exist"
    );
    assert!(
        gen_dir
            .join("python")
            .join("simple_math")
            .join("__init__.py")
            .exists(),
        "__init__.py should exist"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Wrap (dry-run) tests
// ---------------------------------------------------------------------------

#[test]
fn test_wrap_dry_run_simple_math() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("wrap-dry-run");

    let (stdout, stderr, status) = run_vertumnus(&[
        "wrap",
        fixture.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--package-name",
        "simple_math",
        "--dry-run",
    ]);
    assert!(status.success(), "wrap --dry-run failed: {}", stderr);

    // Dry-run should print annotated IR to stdout
    let annotated: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid annotated IR JSON");
    assert_eq!(annotated["crate_name"], "simple_math");

    // Should NOT have created any files
    assert!(
        !out_dir.join("src").exists(),
        "Should not create src/ in dry-run"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_wrap_dry_run_data_structures() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("data-structures");

    let (stdout, stderr, status) = run_vertumnus(&["wrap", fixture.to_str().unwrap(), "--dry-run"]);
    assert!(
        status.success(),
        "wrap --dry-run data-structures failed: {}",
        stderr
    );

    let annotated: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid annotated IR JSON");
    assert_eq!(annotated["crate_name"], "data_structures");
}

// ---------------------------------------------------------------------------
// Wrap (no-build) — full pipeline without maturin
// ---------------------------------------------------------------------------

#[test]
fn test_wrap_no_build_simple_math() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("wrap-nobuild");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "wrap",
        fixture.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--package-name",
        "simple_math",
        "--no-build",
        "--overwrite",
    ]);
    assert!(status.success(), "wrap --no-build failed: {}", stderr);

    // Check generated files
    assert!(
        out_dir.join("src").join("lib.rs").exists(),
        "lib.rs should exist"
    );
    assert!(
        out_dir.join("simple_math.pyi").exists(),
        ".pyi should exist"
    );
    assert!(
        out_dir
            .join("python")
            .join("simple_math")
            .join("__init__.py")
            .exists(),
        "__init__.py should exist"
    );

    // Check scaffolded config files
    assert!(
        out_dir.join("pyproject.toml").exists(),
        "pyproject.toml should exist"
    );
    assert!(
        out_dir.join("Cargo.toml").exists(),
        "Cargo.toml should exist"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_wrap_no_build_string_utils() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("string-utils");
    let out_dir = temp_out_dir("wrap-nobuild-str");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "wrap",
        fixture.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--package-name",
        "string_utils",
        "--no-build",
        "--overwrite",
    ]);
    assert!(
        status.success(),
        "wrap --no-build string-utils failed: {}",
        stderr
    );

    assert!(
        out_dir.join("src").join("lib.rs").exists(),
        "lib.rs should exist"
    );
    assert!(
        out_dir.join("string_utils.pyi").exists(),
        ".pyi should exist"
    );
    assert!(
        out_dir.join("pyproject.toml").exists(),
        "pyproject.toml should exist"
    );
    assert!(
        out_dir.join("Cargo.toml").exists(),
        "Cargo.toml should exist"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_wrap_no_build_data_structures() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("data-structures");
    let out_dir = temp_out_dir("wrap-nobuild-ds");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "wrap",
        fixture.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--package-name",
        "data_structures",
        "--no-build",
        "--overwrite",
    ]);
    assert!(
        status.success(),
        "wrap --no-build data-structures failed: {}",
        stderr
    );

    assert!(
        out_dir.join("src").join("lib.rs").exists(),
        "lib.rs should exist"
    );
    assert!(
        out_dir.join("data_structures.pyi").exists(),
        ".pyi should exist"
    );
    assert!(
        out_dir.join("pyproject.toml").exists(),
        "pyproject.toml should exist"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Wrap with verbose flag
// ---------------------------------------------------------------------------

#[test]
fn test_wrap_verbose_simple_math() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("wrap-verbose");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "wrap",
        fixture.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--no-build",
        "--overwrite",
        "--verbose",
    ]);
    assert!(status.success(), "wrap --verbose failed: {}", stderr);
    // Verbose output goes to stderr
    assert!(
        stderr.contains("Inspecting"),
        "verbose should mention inspecting"
    );
    assert!(
        stderr.contains("type mapper"),
        "verbose should mention type mapper"
    );
    assert!(
        stderr.contains("bindings"),
        "verbose should mention bindings"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Full wrap with maturin (requires maturin installed)
// ---------------------------------------------------------------------------

#[test]
fn test_full_wrap_simple_math() {
    if !maturin_available() {
        eprintln!("Skipping full wrap test: maturin not found");
        return;
    }

    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("simple-math");
    let out_dir = temp_out_dir("full-wrap");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "wrap",
        fixture.to_str().unwrap(),
        "--out",
        out_dir.to_str().unwrap(),
        "--package-name",
        "simple_math",
        "--overwrite",
    ]);
    assert!(status.success(), "full wrap failed: {}", stderr);

    // Check wheel was built
    let wheels_dir = out_dir.join("target").join("wheels");
    assert!(wheels_dir.exists(), "wheels directory should exist");
    let has_wheel = std::fs::read_dir(&wheels_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .any(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "whl")
                .unwrap_or(false)
        });
    assert!(has_wheel, "should have built a .whl file");

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Edge case tests
// ---------------------------------------------------------------------------

#[test]
fn test_inspect_nonexistent_crate() {
    let (_stdout, stderr, status) =
        run_vertumnus(&["inspect", "/tmp/nonexistent-crate-path-vertumnus-test"]);
    assert!(!status.success(), "should fail for nonexistent path");
    assert!(!stderr.is_empty(), "should have error message");
}

#[test]
fn test_map_invalid_json() {
    let out_dir = temp_out_dir("map-invalid");
    let bad_file = out_dir.join("bad.json");
    std::fs::write(&bad_file, "not valid json").unwrap();

    let (_stdout, stderr, status) = run_vertumnus(&["map", bad_file.to_str().unwrap()]);
    assert!(!status.success(), "should fail for invalid JSON");
    assert!(!stderr.is_empty(), "should have error message");

    let _ = std::fs::remove_dir_all(&out_dir);
}
