//! # Vertumnus Type Mapper
//!
//! Phase 2 of the Vertumnus pipeline: maps Rust types in the IR to Python
//! types and PyO3 strategies, producing an annotated IR.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vertumnus_mapper::map_ir;
//! use vertumnus_inspector::IntermediateRepresentation;
//!
//! let ir = IntermediateRepresentation::new("my_crate".into(), "1.0.0".into());
//! let annotated = map_ir(&ir).unwrap();
//! println!("{}", annotated.to_json_pretty().unwrap());
//! ```

pub mod annotated_ir;
pub mod config;
pub mod dependency_resolver;
pub mod mapper;
pub mod monomorphization;
pub mod type_parser;

pub use annotated_ir::{AnnotatedIr, AnnotatedItem, MappingWarning, PyO3Strategy, TypeMapping};
pub use config::{VertumnusConfig, TypeMappingEntry};
pub use dependency_resolver::{load_cargo_lock, CargoLockInfo};
pub use mapper::{map_ir, map_ir_with_config, map_ir_with_full_context, MapError};
pub use type_parser::{map_named_type, map_type, map_type_with_config, MappedType};
