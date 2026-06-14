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

pub mod inspector;
pub mod ir;

pub use inspector::inspect_crate;
pub use ir::*;
