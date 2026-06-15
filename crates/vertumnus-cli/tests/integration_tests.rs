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
        stderr.contains("Inspecting") || stderr.contains("cached IR"),
        "verbose should mention inspecting or cached IR"
    );
    assert!(
        stderr.contains("type mapper") || stderr.contains("cached mapping"),
        "verbose should mention type mapper or cached mapping"
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
// Data-carrying enum tests
// ---------------------------------------------------------------------------

#[test]
fn test_inspect_data_enum_has_fields() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("data-structures");

    let (stdout, stderr, status) = run_vertumnus(&[
        "inspect",
        fixture.to_str().unwrap(),
    ]);
    assert!(status.success(), "inspect failed: {}", stderr);

    let ir: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Find the ValidationError enum
    let items = ir["items"].as_array().expect("items should be an array");
    let validation_enum = items.iter().find(|item| {
        item["kind"] == "enum" && item["name"] == "ValidationError"
    });
    assert!(validation_enum.is_some(), "ValidationError enum should be in IR");

    let variants = validation_enum.unwrap()["variants"].as_array()
        .expect("variants should be an array");
    
    // Check EmptyInput has no fields
    let empty = variants.iter().find(|v| v["name"] == "EmptyInput").unwrap();
    assert_eq!(empty["fields"].as_array().unwrap().len(), 0, "EmptyInput should have no fields");

    // Check TooLong has fields
    let too_long = variants.iter().find(|v| v["name"] == "TooLong").unwrap();
    assert!(!too_long["fields"].as_array().unwrap().is_empty(), "TooLong should have fields");

    // Check InvalidCharacter has a field
    let invalid = variants.iter().find(|v| v["name"] == "InvalidCharacter").unwrap();
    assert!(!invalid["fields"].as_array().unwrap().is_empty(), "InvalidCharacter should have fields");
}

#[test]
fn test_map_data_enum_uses_data_enum_strategy() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("data-structures");

    // Inspect to get IR
    let (stdout, _stderr, status) = run_vertumnus(&[
        "inspect",
        fixture.to_str().unwrap(),
    ]);
    assert!(status.success(), "inspect failed");

    // Pipe inspect -> map via a temp file
    let ir_file = temp_out_dir("data-enum-ir").join("ir.json");
    std::fs::write(&ir_file, &stdout).unwrap();

    let (map_stdout, map_stderr, map_status) = run_vertumnus(&[
        "map",
        ir_file.to_str().unwrap(),
    ]);
    assert!(map_status.success(), "map failed: {}", map_stderr);

    let annotated: serde_json::Value =
        serde_json::from_str(&map_stdout).expect("stdout should be valid JSON");

    // Find ValidationError in annotated IR
    let items = annotated["items"].as_array().expect("items should be an array");
    let validation_item = items.iter().find(|item| {
        item["original"]["kind"] == "enum" && item["original"]["name"] == "ValidationError"
    });
    assert!(validation_item.is_some(), "ValidationError should be in annotated IR");
    assert_eq!(
        validation_item.unwrap()["mapping"]["pyo3_strategy"],
        "data_enum",
        "ValidationError should use DataEnum strategy"
    );
}

#[test]
fn test_wrap_no_build_data_enum_generates_constructors() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("data-structures");

    // Remove cache to ensure fresh inspection
    let cache_dir = fixture.join(".cache");
    let _ = std::fs::remove_dir_all(&cache_dir);

    let out_dir = temp_out_dir("wrap-data-enum");

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

    // Check that generated lib.rs has DataEnum pattern
    let lib_rs = std::fs::read_to_string(out_dir.join("src").join("lib.rs"))
        .expect("lib.rs should exist");
    assert!(
        lib_rs.contains("ValidationError"),
        "lib.rs should contain ValidationError"
    );
    assert!(
        lib_rs.contains("empty_input"),
        "lib.rs should contain empty_input constructor"
    );
    assert!(
        lib_rs.contains("invalid_character"),
        "lib.rs should contain invalid_character constructor"
    );
    assert!(
        lib_rs.contains("is_empty_input"),
        "lib.rs should contain is_empty_input check"
    );
    assert!(
        lib_rs.contains("is_too_long"),
        "lib.rs should contain is_too_long check"
    );

    // Check that .pyi stub has async def
    let pyi = std::fs::read_to_string(out_dir.join("data_structures.pyi"))
        .expect("pyi should exist");
    assert!(
        pyi.contains("class ValidationError:"),
        "pyi should have ValidationError class"
    );
    assert!(
        pyi.contains("def empty_input"),
        "pyi should have empty_input method"
    );
    assert!(
        pyi.contains("def invalid_character"),
        "pyi should have invalid_character method"
    );
    assert!(
        pyi.contains("def is_too_long"),
        "pyi should have is_too_long property"
    );

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

// ---------------------------------------------------------------------------
// Async function support tests
// ---------------------------------------------------------------------------

#[test]
fn test_inspect_async_function_is_detected() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("string-utils");

    let (stdout, stderr, status) = run_vertumnus(&[
        "inspect",
        "--verbose",
        fixture.to_str().unwrap(),
    ]);
    assert!(status.success(), "inspect failed: {}", stderr);

    let ir: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");

    // Find the async_greeting function
    let items = ir["items"].as_array().expect("items should be an array");
    let async_fn = items.iter().find(|item| {
        item["kind"] == "function" && item["name"] == "async_greeting"
    });
    assert!(
        async_fn.is_some(),
        "async_greeting function should be in IR"
    );
    assert_eq!(
        async_fn.unwrap()["is_async"],
        true,
        "async_greeting should be marked as async"
    );
}

#[test]
fn test_map_async_function_uses_async_wrapper_strategy() {
    let workspace = workspace_root();
    let fixture = workspace.join("tests").join("fixtures").join("string-utils");

    // First inspect to get IR
    let (stdout, _stderr, status) = run_vertumnus(&[
        "inspect",
        fixture.to_str().unwrap(),
    ]);
    assert!(status.success(), "inspect failed");

    // Pipe inspect -> map via a temp file
    let ir_file = temp_out_dir("async-ir").join("ir.json");
    std::fs::write(&ir_file, &stdout).unwrap();

    let (map_stdout, map_stderr, map_status) = run_vertumnus(&[
        "map",
        ir_file.to_str().unwrap(),
    ]);
    assert!(map_status.success(), "map failed: {}", map_stderr);

    let annotated: serde_json::Value =
        serde_json::from_str(&map_stdout).expect("stdout should be valid JSON");

    let items = annotated["items"].as_array().expect("items should be an array");
    let async_item = items.iter().find(|item| {
        item["original"]["kind"] == "function" && item["original"]["name"] == "async_greeting"
    });
    assert!(
        async_item.is_some(),
        "async_greeting should be in annotated IR"
    );
    assert_eq!(
        async_item.unwrap()["mapping"]["pyo3_strategy"],
        "async_wrapper",
        "async_greeting should use AsyncWrapper strategy"
    );
}

#[test]
fn test_wrap_no_build_async_generates_pyo3_asyncio_dep() {
    let workspace = workspace_root();
    let fixture = workspace
        .join("tests")
        .join("fixtures")
        .join("string-utils");

    // Remove cache to ensure fresh inspection with the async function
    let cache_dir = fixture.join(".cache");
    let _ = std::fs::remove_dir_all(&cache_dir);

    let out_dir = temp_out_dir("wrap-async");

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

    // Check that Cargo.toml includes pyo3-asyncio
    let cargo_toml = std::fs::read_to_string(out_dir.join("Cargo.toml"))
        .expect("Cargo.toml should exist");
    assert!(
        cargo_toml.contains("pyo3-asyncio"),
        "Cargo.toml should contain pyo3-asyncio dependency: {}",
        cargo_toml
    );

    // Check that generated lib.rs has future_into_py import
    let lib_rs = std::fs::read_to_string(out_dir.join("src").join("lib.rs"))
        .expect("lib.rs should exist");
    assert!(
        lib_rs.contains("future_into_py"),
        "lib.rs should import future_into_py"
    );
    assert!(
        lib_rs.contains("async_greeting"),
        "lib.rs should contain async_greeting wrapper"
    );

    // Check that .pyi stub has async def
    let pyi = std::fs::read_to_string(out_dir.join("string_utils.pyi"))
        .expect("pyi should exist");
    assert!(
        pyi.contains("async def async_greeting"),
        "pyi should have async def for async_greeting"
    );

    let _ = std::fs::remove_dir_all(&out_dir);
}

// ---------------------------------------------------------------------------
// Batch wrap tests
// ---------------------------------------------------------------------------

#[test]
fn test_batch_wrap_multiple_crates() {
    let workspace = workspace_root();
    let simple_math = workspace
        .join("tests")
        .join("fixtures")
        .join("simple-math");
    let string_utils = workspace
        .join("tests")
        .join("fixtures")
        .join("string-utils");

    // Remove caches to ensure fresh inspection
    let _ = std::fs::remove_dir_all(simple_math.join(".cache"));
    let _ = std::fs::remove_dir_all(string_utils.join(".cache"));

    let out_dir = temp_out_dir("batch-multi");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "batch",
        "wrap",
        simple_math.to_str().unwrap(),
        string_utils.to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
        "--no-build",
        "--overwrite",
    ]);
    assert!(
        status.success(),
        "batch wrap failed: {}",
        stderr
    );

    // Check that both crate outputs exist (named py-<directory_name>)
    let simple_math_out = out_dir.join("py-simple-math");
    let string_utils_out = out_dir.join("py-string-utils");

    assert!(
        simple_math_out.join("src").join("lib.rs").exists(),
        "simple-math output should exist at {:?}",
        simple_math_out
    );
    assert!(
        string_utils_out.join("src").join("lib.rs").exists(),
        "string-utils output should exist at {:?}",
        string_utils_out
    );

    // Check for summary in stderr
    assert!(stderr.contains("Batch wrap summary"), "stderr should contain summary");
    assert!(stderr.contains("Success: 2"), "should report 2 successes");

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_batch_wrap_empty_paths_error() {
    let (_stdout, stderr, status) = run_vertumnus(&["batch", "wrap"]);
    assert!(
        !status.success(),
        "batch wrap with no paths should fail"
    );
    assert!(
        stderr.contains("No crate paths provided"),
        "should give helpful error message"
    );
}

#[test]
fn test_batch_wrap_keep_going() {
    let workspace = workspace_root();
    let simple_math = workspace
        .join("tests")
        .join("fixtures")
        .join("simple-math");
    let nonexistent = workspace.join("tests").join("fixtures").join("nonexistent-crate");

    let out_dir = temp_out_dir("batch-keep-going");

    let (_stdout, stderr, status) = run_vertumnus(&[
        "batch",
        "wrap",
        simple_math.to_str().unwrap(),
        nonexistent.to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
        "--no-build",
        "--overwrite",
        "--keep-going",
    ]);
    // Should succeed overall because --keep-going allows partial failures
    assert!(
        status.success(),
        "batch wrap with --keep-going should succeed: {}",
        stderr
    );

    // Check that simple-math succeeded
    let simple_math_out = out_dir.join("py-simple-math");
    assert!(
        simple_math_out.join("src").join("lib.rs").exists(),
        "simple-math output should exist"
    );

    // Summary should show 1 success, 1 failure
    assert!(stderr.contains("Success: 1"), "should report 1 success");
    assert!(stderr.contains("Failed: 1"), "should report 1 failure");

    let _ = std::fs::remove_dir_all(&out_dir);
}

#[test]
fn test_batch_wrap_rejects_duplicate_output() {
    let workspace = workspace_root();
    let simple_math = workspace
        .join("tests")
        .join("fixtures")
        .join("simple-math");

    let out_dir = temp_out_dir("batch-duplicate");

    // First batch wrap should succeed
    let (_stdout, stderr, status) = run_vertumnus(&[
        "batch",
        "wrap",
        simple_math.to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
        "--no-build",
        "--overwrite",
    ]);
    assert!(status.success(), "first batch wrap failed: {}", stderr);

    // Second batch wrap without --overwrite should fail on the existing output
    let (_stdout, stderr, status) = run_vertumnus(&[
        "batch",
        "wrap",
        simple_math.to_str().unwrap(),
        "--out-dir",
        out_dir.to_str().unwrap(),
        "--no-build",
    ]);
    assert!(
        !status.success(),
        "second batch wrap without --overwrite should fail"
    );
    assert!(stderr.contains("exists") || stderr.contains("exist"), "should mention existing output");

    let _ = std::fs::remove_dir_all(&out_dir);
}
