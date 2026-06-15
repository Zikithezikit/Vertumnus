//! # Vertumnus Inspector
//!
//! Phase 1 of the Vertumnus pipeline: inspect a Rust crate's public API
//! and produce an Intermediate Representation (IR) that downstream phases
//! consume.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vertumnus_inspector::inspect_crate;
//! use std::path::Path;
//!
//! let ir = inspect_crate(Path::new("/path/to/crate")).unwrap();
//! println!("{}", ir.to_json_pretty().unwrap());
//! ```
//!
//! ## Fallback Parsing
//!
//! The primary inspection method uses `cargo +nightly rustdoc` with JSON
//! output. If nightly is not available, the inspector falls back to
//! `syn`-based source code parsing automatically.
//!
//! You can also force `syn` parsing directly:
//!
//! ```rust,no_run
//! use vertumnus_inspector::inspect_crate_with_syn;
//! use std::path::Path;
//!
//! let ir = inspect_crate_with_syn(Path::new("/path/to/crate")).unwrap();
//! ```

pub mod inspector;
pub mod ir;
pub mod syn_parser;

pub use inspector::{inspect_crate, inspect_crate_rustdoc};
pub use syn_parser::inspect_crate_with_syn;
pub use ir::*;
