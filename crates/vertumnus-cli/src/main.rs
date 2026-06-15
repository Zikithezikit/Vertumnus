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

/// Load a Vertumnus config from an optional `--config` path.
/// If `None`, tries to auto-detect `.vertumnus/config.toml` in the crate dir.
/// Returns `None` if neither exists (not an error).
fn load_config(config_path: Option<&Path>, crate_dir: &Path) -> anyhow::Result<Option<VertumnusConfig>> {
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
                    eprintln!("📋 Auto-detected config: {}/.vertumnus/config.toml", crate_dir.display());
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
            // Phase 1: Inspect
            if verbose {
                eprintln!("🔍 Inspecting crate at: {}", path.display());
            }

            let ir = vertumnus_inspector::inspect_crate(&path)?;

            // Phase 2: Type mapping
            if verbose {
                eprintln!("🗺️  Running type mapper on {} items...", ir.items.len());
            }

            // Load optional config for custom type mappings
            let config = load_config(config.as_deref(), &path)?;

            let annotated = vertumnus_mapper::map_ir_with_config(&ir, config.as_ref())?;

            if dry_run {
                // Dry-run: output annotated IR and exit
                println!("{}", annotated.to_json_pretty()?);
                return Ok(());
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
            }

            // Phase 3: Generate bindings
            let package_name = package_name.unwrap_or_else(|| format!("py-{}", ir.crate_name));
            let package_name_safe = package_name.replace('-', "_");

            if verbose {
                eprintln!(
                    "🔧 Generating Python bindings for '{}'...",
                    package_name_safe
                );
            }

            let config = vertumnus_generator::GeneratorConfig {
                package_name: package_name_safe.clone(),
                native_module_name: "_core".to_string(),
                derive_debug: false,
                derive_eq: false,
                overwrite,
            };
            let gen = vertumnus_generator::Generator::new(annotated, config);
            let files = gen.generate()?;

            if verbose {
                eprintln!("✅ Bindings generated successfully.");
            }

            // Write output files
            let out_path = match out {
                Some(p) => {
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

            write_generated_files(&out_path, &package_name_safe, &files, verbose, overwrite)?;

            if verbose {
                eprintln!("📄 Wrote bindings to: {}", out_path.display());
            }

            // Phase 4: Scaffold build configuration
            if verbose {
                eprintln!("🏗️  Scaffolding build configuration...");
            }

            let canonical_path = path
                .canonicalize()
                .map_err(|e| anyhow::anyhow!("Cannot resolve crate path: {e}"))?;

            // Read the actual crate name from Cargo.toml (preserves hyphens)
            let original_crate_name = vertumnus_builder::read_crate_name(&canonical_path)
                .unwrap_or_else(|_| ir.crate_name.clone());

            let builder_config = vertumnus_builder::BuilderConfig {
                output_dir: out_path.clone(),
                crate_path: canonical_path,
                package_name: package_name_safe.clone(),
                crate_name: original_crate_name,
                crate_version: ir.crate_version.clone(),
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
                    Some(path) => {
                        eprintln!("✅ Built wheel: {}", path.display());
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
