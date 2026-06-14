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

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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

        /// Output directory for generated files
        #[arg(long, default_value = "./vertumnus-out")]
        out: PathBuf,

        /// Python package name (default: crate name)
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

        /// Output file for the annotated IR JSON (default: stdout)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Phase 3: Generate Rust bindings and .pyi stubs from annotated IR
    Generate {
        /// Path to the annotated IR JSON file
        path: PathBuf,

        /// Output directory for generated files
        #[arg(long, default_value = "./vertumnus-out")]
        output: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Wrap {
            path,
            out,
            package_name,
            dry_run,
            no_build,
            verbose,
            overwrite,
        } => {
            // M1: Implement full wrap pipeline
            if verbose {
                eprintln!("🔍 Inspecting crate at: {}", path.display());
            }

            let ir = vertumnus_inspector::inspect_crate(&path)?;

            if dry_run {
                println!("{}", ir.to_json_pretty()?);
                return Ok(());
            }

            // TODO M4: mapper, generator, builder phases
            if verbose {
                eprintln!("✅ Inspection complete. {} items found.", ir.items.len());
                eprintln!("ℹ️  Remaining phases (mapper, generator, builder) not yet implemented.");
            }

            if !no_build {
                eprintln!("Warning: --no-build not specified but build not yet supported.");
            }

            // Write IR to output file
            let out_path = if out.exists() && !overwrite {
                anyhow::bail!(
                    "Output directory '{}' exists. Use --overwrite to overwrite.",
                    out.display()
                );
            } else {
                std::fs::create_dir_all(&out)?;
                out
            };

            let ir_path = out_path.join("ir.json");
            std::fs::write(&ir_path, ir.to_json_pretty()?)?;
            if verbose {
                eprintln!("📄 IR written to: {}", ir_path.display());
            }

            let package_name = package_name.unwrap_or_else(|| ir.crate_name.clone());
            if verbose {
                eprintln!("📦 Package name: {}", package_name);
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

        Commands::Map { path, output } => {
            let ir_content = std::fs::read_to_string(&path)?;
            let _ir = vertumnus_inspector::IntermediateRepresentation::from_json(&ir_content)?;

            // TODO M2: Call mapper phase
            eprintln!("⚠️  Type mapper not yet implemented (M2).");

            if let Some(out_path) = output {
                // Just copy the IR for now as placeholder
                std::fs::write(&out_path, &ir_content)?;
            }
        }

        Commands::Generate { path, output } => {
            let _ir_content = std::fs::read_to_string(&path)?;

            // TODO M3: Call generator phase
            eprintln!("⚠️  Binding generator not yet implemented (M3).");

            std::fs::create_dir_all(&output)?;
        }
    }

    Ok(())
}
