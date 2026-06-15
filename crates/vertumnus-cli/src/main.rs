//! # Vertumnus CLI
//!
//! Orchestrates all phases of the Vertumnus pipeline.
//!
//! ```bash
//! vertumnus wrap <path-to-crate>     # Full pipeline
//! vertumnus inspect <path-to-crate>  # Phase 1 only
//! vertumnus map <ir.json>            # Phase 2 only
//! vertumnus generate <annotated.json> # Phase 3 only
//! ```

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use vertumnus_mapper::config::VertumnusConfig;

mod cache;
mod registry;

/// Options for wrapping a single crate (used by both `Wrap` and `Batch::Wrap`).
#[derive(Debug, Clone)]
struct WrapOptions {
    path: PathBuf,
    config_path: Option<PathBuf>,
    out: Option<PathBuf>,
    package_name: Option<String>,
    dry_run: bool,
    no_build: bool,
    verbose: bool,
    overwrite: bool,
}

/// Result of wrapping a single crate in a batch operation.
#[derive(Debug, Default)]
struct BatchCrateResult {
    crate_name: String,
    status: BatchStatus,
    output_dir: Option<PathBuf>,
    error: Option<String>,
    warning_count: usize,
}

#[derive(Debug, Default)]
enum BatchStatus {
    Success,
    #[default]
    Skipped,
    Failed,
}

/// Transform any Rust crate into a Python package — with minimal manual binding work.
#[derive(Parser, Debug)]
#[command(name = "vertumnus", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Full pipeline: inspect, map, generate, and build a Python package
    Wrap {
        /// Path to the Rust crate to wrap
        path: PathBuf,

        /// Path to a .vertumnus/config.toml file for custom type mappings
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output directory for generated files (default: ../py-<crate_name>)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Python package name (default: py-<crate_name>)
        #[arg(long)]
        package_name: Option<String>,

        /// Inspect and map only; do not write files
        #[arg(long)]
        dry_run: bool,

        /// Generate files but do not invoke maturin
        #[arg(long)]
        no_build: bool,

        /// Print IR and mapping decisions to stdout
        #[arg(long, short)]
        verbose: bool,

        /// Overwrite existing output files
        #[arg(long)]
        overwrite: bool,
    },

    /// Phase 1: Inspect a crate and dump IR as JSON
    Inspect {
        /// Path to the Rust crate
        path: PathBuf,

        /// Output file for the IR JSON (default: stdout)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Verbose output
        #[arg(long, short)]
        verbose: bool,
    },

    /// Phase 2: Map an IR JSON file and dump annotated IR
    Map {
        /// Path to the IR JSON file
        path: PathBuf,

        /// Path to a .vertumnus/config.toml file for custom type mappings
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output file for the annotated IR JSON (default: stdout)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Verbose output
        #[arg(long, short)]
        verbose: bool,
    },

    /// Batch operations on multiple crates
    Batch {
        #[command(subcommand)]
        action: BatchAction,

        /// Print detailed information about batch operations
        #[arg(long, short, global = true)]
        verbose: bool,
    },

    /// Community type mapping registry — fetch, list, apply mappings
    Registry {
        #[command(subcommand)]
        action: RegistryAction,

        /// Print detailed information about registry operations
        #[arg(long, short, global = true)]
        verbose: bool,
    },

    /// Phase 3: Generate Rust bindings and .pyi stubs from annotated IR
    Generate {
        /// Path to the annotated IR JSON file
        path: PathBuf,

        /// Output directory for generated files (default: ../py-<crate_name> from IR path)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Python package name (default: py-<crate_name>)
        #[arg(long)]
        package_name: Option<String>,

        /// Print generation details to stdout
        #[arg(long, short)]
        verbose: bool,

        /// Overwrite existing output files
        #[arg(long)]
        overwrite: bool,
    },
}

/// Actions for the `vertumnus batch` subcommand.
#[derive(Subcommand, Debug)]
enum BatchAction {
    /// Wrap multiple Rust crates into Python packages
    Wrap {
        /// Paths to the Rust crate(s) to wrap. Can be directories or glob patterns.
        paths: Vec<PathBuf>,

        /// Path to a .vertumnus/config.toml file (shared across all crates)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Output directory root — each crate gets a subdirectory here (default: parent of each crate)
        #[arg(long)]
        out_dir: Option<PathBuf>,

        /// Generate files but do not invoke maturin
        #[arg(long)]
        no_build: bool,

        /// Overwrite existing output files
        #[arg(long)]
        overwrite: bool,

        /// Continue wrapping remaining crates even if one fails
        #[arg(long)]
        keep_going: bool,
    },
}

/// Actions for the `vertumnus registry` subcommand.
#[derive(Subcommand, Debug)]
enum RegistryAction {
    /// Fetch the latest community type mappings from the remote registry
    Fetch {
        /// URL of the community registry (default: official Vertumnus registry)
        #[arg(long)]
        registry_url: Option<String>,

        /// Save fetched mappings to a local file instead of the default cache
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// List all available type mappings in the community registry
    List {
        /// Optional search query to filter mappings
        query: Option<String>,

        /// Path to a cached registry file (default: auto-detect)
        #[arg(long)]
        registry_file: Option<PathBuf>,
    },

    /// Apply community registry mappings to the local .vertumnus/config.toml
    Apply {
        /// Path to a registry file (default: use cached community registry)
        #[arg(long)]
        registry_file: Option<PathBuf>,

        /// Path to the local config file (default: .vertumnus/config.toml in current dir)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Overwrite existing user-defined mappings (default: prefer user mappings)
        #[arg(long)]
        overwrite: bool,
    },

    /// Add a custom type mapping to the local config
    Add {
        /// Rust type to map (e.g. bytes::Bytes)
        rust_type: String,

        /// Python type to map to (e.g. bytes)
        python_type: String,

        /// PyO3 strategy to use (native, manual)
        #[arg(long, default_value = "native")]
        strategy: String,

        /// Path to the local config file (default: .vertumnus/config.toml in current dir)
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

/// Load a Vertumnus config from an optional `--config` path.
/// If `None`, tries to auto-detect `.vertumnus/config.toml` in the crate dir.
/// Returns `None` if neither exists (not an error).
fn load_config(
    config_path: Option<&Path>,
    crate_dir: &Path,
) -> anyhow::Result<Option<VertumnusConfig>> {
    match config_path {
        Some(path) => {
            let resolved = if path.is_relative() {
                crate_dir.join(path)
            } else {
                path.to_path_buf()
            };
            match VertumnusConfig::from_file(&resolved)? {
                Some(config) => {
                    eprintln!("📋 Loaded config: {}", resolved.display());
                    Ok(Some(config))
                }
                None => {
                    anyhow::bail!("Config file not found: {}", resolved.display());
                }
            }
        }
        None => {
            // Auto-detect .vertumnus/config.toml in crate directory
            match VertumnusConfig::auto_detect(crate_dir)? {
                Some(config) => {
                    eprintln!(
                        "📋 Auto-detected config: {}/.vertumnus/config.toml",
                        crate_dir.display()
                    );
                    Ok(Some(config))
                }
                None => Ok(None),
            }
        }
    }
}

fn write_generated_files(
    output_dir: &Path,
    package_name: &str,
    files: &vertumnus_generator::GeneratedFiles,
    verbose: bool,
    overwrite: bool,
) -> anyhow::Result<()> {
    // Create directory structure
    let src_dir = output_dir.join("src");
    let python_dir = output_dir.join("python").join(package_name);

    // Check for existing files if not overwrite
    if !overwrite {
        for path in &[
            src_dir.join("lib.rs"),
            output_dir.join(format!("{}.pyi", package_name)),
            python_dir.join("__init__.py"),
        ] {
            if path.exists() {
                anyhow::bail!(
                    "Output file '{}' exists. Use --overwrite to overwrite.",
                    path.display()
                );
            }
        }
    }

    std::fs::create_dir_all(&src_dir)?;
    std::fs::create_dir_all(&python_dir)?;

    // Write src/lib.rs
    let lib_rs_path = src_dir.join("lib.rs");
    std::fs::write(&lib_rs_path, &files.lib_rs)?;
    if verbose {
        eprintln!("📄 Generated: {}", lib_rs_path.display());
    }

    // Write .pyi stubs
    let pyi_path = output_dir.join(format!("{}.pyi", package_name));
    std::fs::write(&pyi_path, &files.pyi)?;
    if verbose {
        eprintln!("📄 Generated: {}", pyi_path.display());
    }

    // Write __init__.py
    let init_path = python_dir.join("__init__.py");
    std::fs::write(&init_path, &files.init_py)?;
    if verbose {
        eprintln!("📄 Generated: {}", init_path.display());
    }

    Ok(())
}

/// Run the full wrap pipeline for a single crate and return a result.
fn run_wrap(opts: &WrapOptions) -> Result<BatchCrateResult, anyhow::Error> {
    let mut result = BatchCrateResult::default();

    let WrapOptions {
        path,
        config_path,
        out,
        package_name,
        dry_run,
        no_build,
        verbose,
        overwrite,
    } = opts.clone();

    // Resolve canonical path for caching
    let canonical_path = path
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("Cannot resolve crate path '{}': {e}", path.display()))?;

    // Initialize cache (best-effort — don't fail if cache dir is unwritable)
    let cache = cache::Cache::new(&canonical_path).ok();

    // Phase 1: Inspect (or load from cache)
    let ir = if let Some(ref cache) = cache {
        if let Some(cached_ir) = cache.load_ir() {
            if verbose {
                eprintln!("🔍 Using cached IR (source unchanged)");
            }
            cached_ir
        } else {
            if verbose {
                eprintln!("🔍 Inspecting crate at: {}", path.display());
            }
            let ir = vertumnus_inspector::inspect_crate(&path)?;
            if let Err(e) = cache.save_ir(&ir) {
                if verbose {
                    eprintln!("  ℹ️  Cache write skipped: {e}");
                }
            }
            ir
        }
    } else {
        if verbose {
            eprintln!("🔍 Inspecting crate at: {}", path.display());
        }
        vertumnus_inspector::inspect_crate(&path)?
    };
    result.crate_name = ir.crate_name.clone();

    // Phase 2: Type mapping (or load from cache)
    let annotated = if let Some(ref cache) = cache {
        if let Some(cached_annotated) = cache.load_annotated_ir() {
            if verbose {
                eprintln!("🗺️  Using cached mapping (source unchanged)");
            }
            cached_annotated
        } else {
            if verbose {
                eprintln!("🗺️  Running type mapper on {} items...", ir.items.len());
            }
            let config = load_config(config_path.as_deref(), &path)?;
            let annotated = vertumnus_mapper::map_ir_with_full_context(
                &ir,
                config.as_ref(),
                Some(&canonical_path),
            )?;
            if let Err(e) = cache.save_annotated_ir(&annotated) {
                if verbose {
                    eprintln!("  ℹ️  Cache write skipped: {e}");
                }
            }
            annotated
        }
    } else {
        if verbose {
            eprintln!("🗺️  Running type mapper on {} items...", ir.items.len());
        }
        let config = load_config(config_path.as_deref(), &path)?;
        vertumnus_mapper::map_ir_with_full_context(&ir, config.as_ref(), Some(&canonical_path))?
    };

    if dry_run {
        // Dry-run: output annotated IR and exit
        println!("{}", annotated.to_json_pretty()?);
        result.status = BatchStatus::Success;
        return Ok(result);
    }

    if verbose {
        let total_warnings: usize = annotated
            .items
            .iter()
            .map(|i| i.mapping.warnings.len())
            .sum();
        eprintln!("✅ Type mapping complete. {} warnings.", total_warnings);
        for item in &annotated.items {
            for w in &item.mapping.warnings {
                eprintln!("  ⚠️  [{}] {}", w.location, w.message);
            }
        }
        result.warning_count = total_warnings;
    }

    // Check if any functions are async (needed for builder config)
    let has_async = annotated.items.iter().any(|item| match &item.original {
        vertumnus_inspector::ir::IrItem::Function(f) => f.is_async,
        vertumnus_inspector::ir::IrItem::Struct(s) => s.methods.iter().any(|m| m.is_async),
        vertumnus_inspector::ir::IrItem::Enum(e) => e.methods.iter().any(|m| m.is_async),
        _ => false,
    });

    // Phase 3: Generate bindings
    let package_name = package_name.unwrap_or_else(|| format!("py-{}", ir.crate_name));
    let package_name_safe = package_name.replace('-', "_");

    if verbose {
        eprintln!(
            "🔧 Generating Python bindings for '{}'...",
            package_name_safe
        );
    }

    let gen_config = vertumnus_generator::GeneratorConfig {
        package_name: package_name_safe.clone(),
        native_module_name: "_core".to_string(),
        derive_debug: false,
        derive_eq: false,
        overwrite,
    };
    let gen = vertumnus_generator::Generator::new(annotated, gen_config);
    let files = gen.generate()?;

    if verbose {
        eprintln!("✅ Bindings generated successfully.");
    }

    // Write output files
    let out_path = match out {
        Some(p) => {
            // If a specific output path is given, use it directly
            if p.exists() && !overwrite {
                anyhow::bail!(
                    "Output directory '{}' exists. Use --overwrite to overwrite.",
                    p.display()
                );
            }
            std::fs::create_dir_all(&p)?;
            p
        }
        None => {
            // Default: parent_dir/py-<crate_name> (outside the crate directory)
            let canonical_crate = path
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("Cannot resolve crate path: {e}"))?;
            let parent = canonical_crate.parent().unwrap_or_else(|| Path::new("."));
            let dir_name = format!("py-{}", ir.crate_name.replace('_', "-"));
            let default = parent.join(&dir_name);
            if default.exists() && !overwrite {
                anyhow::bail!(
                    "Output directory '{}' exists. Use --overwrite to overwrite.",
                    default.display()
                );
            }
            std::fs::create_dir_all(&default)?;
            default
        }
    };
    result.output_dir = Some(out_path.clone());

    write_generated_files(&out_path, &package_name_safe, &files, verbose, overwrite)?;

    if verbose {
        eprintln!("📄 Wrote bindings to: {}", out_path.display());
    }

    // Phase 4: Scaffold build configuration
    if verbose {
        eprintln!("🏗️  Scaffolding build configuration...");
    }

    // Read the actual crate name from Cargo.toml (preserves hyphens)
    let original_crate_name = vertumnus_builder::read_crate_name(&canonical_path)
        .unwrap_or_else(|_| ir.crate_name.clone());

    let builder_config = vertumnus_builder::BuilderConfig {
        output_dir: out_path.clone(),
        crate_path: canonical_path.clone(),
        package_name: package_name_safe.clone(),
        crate_name: original_crate_name,
        crate_version: ir.crate_version.clone(),
        needs_async: has_async,
    };

    // Always scaffold pyproject.toml and Cargo.toml
    let written = vertumnus_builder::scaffold_all(&builder_config)
        .map_err(|e| anyhow::anyhow!("Build scaffolding failed: {e}"))?;

    if verbose {
        for w in &written {
            eprintln!("   📄 Created: {}", w.display());
        }
    }

    // Optionally run maturin build
    if !no_build {
        if verbose {
            eprintln!("🔨 Running maturin build (release mode)...");
        }

        let wheel = vertumnus_builder::run_maturin_build(&builder_config, true)
            .map_err(|e| anyhow::anyhow!("maturin build failed: {e}"))?;

        match wheel {
            Some(wheel_path) => {
                eprintln!("✅ Built wheel: {}", wheel_path.display());
            }
            None => {
                eprintln!("✅ maturin build completed (wheel location unknown)");
            }
        }
    } else {
        if verbose {
            eprintln!("ℹ️  Skipping maturin build (--no-build).");
            eprintln!(
                "   Run `maturin build --release` in '{}' to build.",
                out_path.display()
            );
        }
    }

    result.status = BatchStatus::Success;
    Ok(result)
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Wrap {
            path,
            config,
            out,
            package_name,
            dry_run,
            no_build,
            verbose,
            overwrite,
        } => {
            let opts = WrapOptions {
                path,
                config_path: config,
                out,
                package_name,
                dry_run,
                no_build,
                verbose,
                overwrite,
            };
            let result = run_wrap(&opts)?;
            if matches!(result.status, BatchStatus::Failed) {
                if let Some(err) = &result.error {
                    eprintln!("❌ Wrap failed: {err}");
                }
                std::process::exit(1);
            }
        }

        Commands::Batch { action, verbose } => {
            match action {
                BatchAction::Wrap {
                    paths,
                    config,
                    out_dir,
                    no_build,
                    overwrite,
                    keep_going,
                } => {
                    if paths.is_empty() {
                        anyhow::bail!("No crate paths provided. Specify at least one crate path.");
                    }

                    // Expand any glob patterns in paths
                    let expanded_paths: Vec<PathBuf> = paths
                        .iter()
                        .flat_map(|p| {
                            let p_str = p.to_string_lossy();
                            if p_str.contains('*') || p_str.contains('?') {
                                match glob::glob(&p_str) {
                                    Ok(entries) => entries
                                        .filter_map(|e| e.ok())
                                        .filter(|e| e.join("Cargo.toml").exists())
                                        .collect(),
                                    Err(_) => vec![p.clone()], // fall back to literal
                                }
                            } else {
                                vec![p.clone()]
                            }
                        })
                        .collect();

                    if expanded_paths.is_empty() {
                        anyhow::bail!("No valid crate paths found after expanding patterns.");
                    }

                    let total = expanded_paths.len();
                    let mut results: Vec<BatchCrateResult> = Vec::with_capacity(total);

                    eprintln!("📦 Batch wrapping {} crate(s)...", total);

                    for (i, crate_path) in expanded_paths.iter().enumerate() {
                        eprintln!("\n[{}/{}] Wrapping: {}", i + 1, total, crate_path.display());

                        // For batch mode, determine output directory per crate
                        let crate_out = out_dir.as_ref().map(|root| {
                            // Use the crate's directory name as subfolder name
                            let dir_name = crate_path
                                .file_name()
                                .map(|n| format!("py-{}", n.to_string_lossy()))
                                .unwrap_or_else(|| "py-unknown".to_string());
                            root.join(&dir_name)
                        });

                        let batch_opts = WrapOptions {
                            path: crate_path.clone(),
                            config_path: config.clone(),
                            out: crate_out,
                            package_name: None,
                            dry_run: false,
                            no_build,
                            verbose,
                            overwrite,
                        };
                        match run_wrap(&batch_opts) {
                            Ok(result) => {
                                if matches!(result.status, BatchStatus::Success) {
                                    eprintln!(
                                        "✅ [{}/{}] {} — wrapped successfully",
                                        i + 1,
                                        total,
                                        result.crate_name
                                    );
                                }
                                results.push(result);
                            }
                            Err(e) => {
                                if keep_going {
                                    eprintln!(
                                        "❌ [{}/{}] {} — failed: {e}",
                                        i + 1,
                                        total,
                                        crate_path.display()
                                    );
                                    results.push(BatchCrateResult {
                                        crate_name: crate_path
                                            .file_name()
                                            .map(|s| s.to_string_lossy().to_string())
                                            .unwrap_or_default(),
                                        status: BatchStatus::Failed,
                                        error: Some(e.to_string()),
                                        ..Default::default()
                                    });
                                } else {
                                    eprintln!(
                                        "❌ [{}/{}] {} — failed: {e}",
                                        i + 1,
                                        total,
                                        crate_path.display()
                                    );
                                    eprintln!("💡 Use --keep-going to continue wrapping remaining crates.");
                                    return Err(e);
                                }
                            }
                        }
                    }

                    // Print summary
                    eprintln!("\n📊 Batch wrap summary:");
                    let successes = results
                        .iter()
                        .filter(|r| matches!(r.status, BatchStatus::Success))
                        .count();
                    let failures = results
                        .iter()
                        .filter(|r| matches!(r.status, BatchStatus::Failed))
                        .count();
                    let total_warnings: usize = results.iter().map(|r| r.warning_count).sum();

                    eprintln!("   Total: {total}, Success: {successes}, Failed: {failures}, Warnings: {total_warnings}");

                    for r in &results {
                        let icon = match r.status {
                            BatchStatus::Success => "✅",
                            BatchStatus::Skipped => "⏭️",
                            BatchStatus::Failed => "❌",
                        };
                        let out_str = r
                            .output_dir
                            .as_ref()
                            .map(|d| d.display().to_string())
                            .unwrap_or_default();
                        eprintln!(
                            "   {icon} {} — warnings: {} — output: {}",
                            r.crate_name, r.warning_count, out_str
                        );
                        if let Some(err) = &r.error {
                            eprintln!("       error: {err}");
                        }
                    }
                }
            }
        }

        Commands::Inspect {
            path,
            output,
            verbose,
        } => {
            if verbose {
                eprintln!("🔍 Inspecting crate at: {}", path.display());
            }

            let ir = vertumnus_inspector::inspect_crate(&path)?;

            let json = ir.to_json_pretty()?;

            match output {
                Some(out_path) => {
                    std::fs::write(&out_path, &json)?;
                    if verbose {
                        eprintln!("📄 IR written to: {}", out_path.display());
                    }
                }
                None => {
                    println!("{}", json);
                }
            }
        }

        Commands::Map {
            path,
            config,
            output,
            verbose,
        } => {
            let ir_content = std::fs::read_to_string(&path)?;
            let ir = vertumnus_inspector::IntermediateRepresentation::from_json(&ir_content)?;

            if verbose {
                eprintln!(
                    "🗺️  Mapping IR for crate '{}' v{} ({} items)...",
                    ir.crate_name,
                    ir.crate_version,
                    ir.items.len()
                );
            }

            // Load optional config for custom type mappings
            // For the Map command, use the parent dir of the IR file as the crate dir
            let crate_dir = path.parent().unwrap_or_else(|| Path::new("."));
            let config = load_config(config.as_deref(), crate_dir)?;

            let annotated = vertumnus_mapper::map_ir_with_config(&ir, config.as_ref())?;

            if verbose {
                let total_warnings: usize = annotated
                    .items
                    .iter()
                    .map(|i| i.mapping.warnings.len())
                    .sum();
                eprintln!(
                    "✅ Mapping complete. {} warnings generated.",
                    total_warnings
                );
                if total_warnings > 0 {
                    for item in &annotated.items {
                        for w in &item.mapping.warnings {
                            eprintln!("  ⚠️  [{}] {}", w.location, w.message);
                        }
                    }
                }
            }

            let json = annotated.to_json_pretty()?;

            match output {
                Some(out_path) => {
                    std::fs::write(&out_path, &json)?;
                    if verbose {
                        eprintln!("📄 Annotated IR written to: {}", out_path.display());
                    } else {
                        eprintln!("Annotated IR written to: {}", out_path.display());
                    }
                }
                None => {
                    println!("{}", json);
                }
            }
        }

        Commands::Registry { action, verbose } => {
            match action {
                RegistryAction::Fetch {
                    registry_url,
                    output,
                } => {
                    if verbose {
                        eprintln!("🌐 Fetching community registry...");
                    }
                    let result = registry::fetch_registry(registry_url.as_deref())?;

                    if verbose {
                        eprintln!(
                            "   Fetched {} type mappings (version: {})",
                            result.count,
                            result.version.as_deref().unwrap_or("unknown")
                        );
                    }

                    match output {
                        Some(out_path) => {
                            // Save to the specified file
                            let parent = out_path.parent().unwrap_or_else(|| Path::new("."));
                            registry::save_registry_cache(&result.mappings, parent)?;
                            eprintln!("✅ Registry saved to: {}", out_path.display());
                        }
                        None => {
                            // Save to default cache location
                            let cache_dir = registry::config_dir();
                            registry::save_registry_cache(&result.mappings, &cache_dir)?;
                            eprintln!(
                                "✅ Registry cached at: {}/community_registry.toml",
                                cache_dir.display()
                            );
                        }
                    }
                }
                RegistryAction::List {
                    query,
                    registry_file,
                } => {
                    if verbose {
                        eprintln!("📋 Loading community registry...");
                    }

                    let mappings = if let Some(file) = registry_file {
                        // Load from specified file
                        let content = std::fs::read_to_string(&file)?;
                        let reg: registry::RegistryFile = toml::from_str(&content)?;
                        reg.type_mappings
                    } else {
                        // Try loading from cache
                        let cache_dir = registry::config_dir();
                        match registry::load_registry_cache(&cache_dir)? {
                            Some(result) => result.mappings,
                            None => {
                                eprintln!("No cached registry found. Run `vertumnus registry fetch` first.");
                                return Ok(());
                            }
                        }
                    };

                    let mut keys: Vec<&String> = mappings.keys().collect();
                    keys.sort();

                    if let Some(query_str) = &query {
                        let query_lower = query_str.to_lowercase();
                        keys.retain(|k| k.to_lowercase().contains(&query_lower));
                    }

                    if keys.is_empty() {
                        eprintln!("No matching type mappings found.");
                        return Ok(());
                    }

                    eprintln!("Found {} type mappings:", keys.len());
                    for key in keys {
                        let entry = &mappings[key];
                        eprintln!("   {} → {} (via {})", key, entry.python, entry.strategy);
                    }
                }
                RegistryAction::Apply {
                    registry_file,
                    config,
                    overwrite,
                } => {
                    if verbose {
                        eprintln!("🔄 Applying registry mappings to local config...");
                    }

                    // Determine registry mappings source
                    let mappings = if let Some(file) = registry_file {
                        let content = std::fs::read_to_string(&file)?;
                        let reg: registry::RegistryFile = toml::from_str(&content)?;
                        reg.type_mappings
                    } else {
                        let cache_dir = registry::config_dir();
                        match registry::load_registry_cache(&cache_dir)? {
                            Some(result) => result.mappings,
                            None => {
                                eprintln!("No cached registry found. Run `vertumnus registry fetch` first.");
                                return Ok(());
                            }
                        }
                    };

                    // Determine config path
                    let config_path = match config {
                        Some(p) => p,
                        None => PathBuf::from(".vertumnus/config.toml"),
                    };

                    if !overwrite {
                        // Normal mode: merge (user mappings take priority)
                        registry::apply_registry_to_config(&mappings, &config_path)?;
                    } else {
                        // Overwrite mode: replace user config entirely
                        if let Some(parent) = config_path.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        let mut content = String::from(
                            "# Vertumnus configuration\n# Generated from community registry\n\n[type_mappings]\n",
                        );
                        let mut keys: Vec<&String> = mappings.keys().collect();
                        keys.sort();
                        for key in keys {
                            let entry = &mappings[key];
                            content.push_str(&format!(
                                "\"{}\" = {{ python = \"{}\", strategy = \"{}\" }}\n",
                                key, entry.python, entry.strategy
                            ));
                        }
                        std::fs::write(&config_path, &content)?;
                    }

                    eprintln!(
                        "✅ Applied {} mappings to {}",
                        mappings.len(),
                        config_path.display()
                    );
                }
                RegistryAction::Add {
                    rust_type,
                    python_type,
                    strategy,
                    config,
                } => {
                    if verbose {
                        eprintln!(
                            "➕ Adding mapping: {} → {} (strategy: {})",
                            rust_type, python_type, strategy
                        );
                    }

                    let config_path = match config {
                        Some(p) => p,
                        None => PathBuf::from(".vertumnus/config.toml"),
                    };

                    registry::add_mapping_to_config(
                        &rust_type,
                        &python_type,
                        &strategy,
                        &config_path,
                    )?;
                    eprintln!("✅ Mapping added to: {}", config_path.display());
                }
            }
        }

        Commands::Generate {
            path,
            output,
            package_name,
            verbose,
            overwrite,
        } => {
            if verbose {
                eprintln!(
                    "🔧 Generating bindings from annotated IR: {}",
                    path.display()
                );
            }

            let ir_content = std::fs::read_to_string(&path)?;
            let annotated = vertumnus_mapper::annotated_ir::AnnotatedIr::from_json(&ir_content)?;

            let package_name =
                package_name.unwrap_or_else(|| format!("py-{}", annotated.crate_name));
            let package_name_safe = package_name.replace('-', "_");

            if verbose {
                eprintln!("   Package name: {}", package_name_safe);
                eprintln!("   Items to generate: {}", annotated.items.len());
            }

            let config = vertumnus_generator::GeneratorConfig {
                package_name: package_name_safe.clone(),
                native_module_name: "_core".to_string(),
                derive_debug: false,
                derive_eq: false,
                overwrite,
            };
            let gen = vertumnus_generator::Generator::new(annotated, config);

            // Determine output directory
            let output_dir = match output {
                Some(p) => p,
                None => {
                    // Default: use parent of the IR file, named py-<crate_name>
                    let parent = path.parent().unwrap_or_else(|| Path::new("."));
                    let dir_name = format!("py-{}", package_name_safe.replace('_', "-"));
                    parent.join(&dir_name)
                }
            };

            // Create output directory
            std::fs::create_dir_all(&output_dir)?;

            let files = gen.generate()?;
            write_generated_files(&output_dir, &package_name_safe, &files, verbose, overwrite)?;

            if verbose {
                eprintln!("✅ Bindings written to: {}", output_dir.display());
            }
        }
    }

    Ok(())
}
