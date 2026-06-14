//! Annotated Intermediate Representation types.
//!
//! The output of the Type Mapper (Phase 2). Each symbol in the original IR
//! is decorated with its Python type mapping, PyO3 strategy, and any
//! warnings for unsupported types.

use serde::{Deserialize, Serialize};

use vertumnus_inspector::ir::IrItem;

/// Current version of the annotated IR schema.
pub const ANNOTATED_IR_VERSION: &str = "0.1";

/// Top-level annotated IR — wraps the original IR with per-item mappings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AnnotatedIr {
    /// Schema version, e.g. "0.1"
    pub vertumnus_annotated_ir_version: String,
    /// Name of the Rust crate
    pub crate_name: String,
    /// Version of the Rust crate
    pub crate_version: String,
    /// Annotated public API items
    pub items: Vec<AnnotatedItem>,
}

/// An annotated item: the original IR item plus its type mapping.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnnotatedItem {
    /// The original IR item
    pub original: IrItem,
    /// Type mapping and warnings for this item
    pub mapping: TypeMapping,
}

/// Type mapping for a single symbol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TypeMapping {
    /// The Python type string (e.g., "int", "str", "list[int]", "Point")
    pub python_type: String,
    /// The PyO3 strategy to use for binding generation
    pub pyo3_strategy: PyO3Strategy,
    /// Any warnings about this type mapping
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<MappingWarning>,
}

/// PyO3 binding strategy for a type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PyO3Strategy {
    /// Direct native conversion (int, float, bool, str, etc.)
    Native,
    /// Wrapped as a `#[pyclass]`
    PyClass,
    /// Wrapped as a `#[pyclass]` enum (or IntEnum for C-like)
    PyEnum,
    /// Wrapped as a `#[pyfunction]`
    PyFunction,
    /// Result<T, E> — map Err to Python exception
    MapErr,
    /// Manual stub required — `// VERTUMNUS: manual binding required`
    ManualStub,
}

/// A warning about a type mapping decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingWarning {
    /// The warning message
    pub message: String,
    /// Where the warning applies (e.g., "safe_div.return_type", "Point.x")
    pub location: String,
}

impl AnnotatedIr {
    /// Create a new annotated IR.
    pub fn new(crate_name: String, crate_version: String) -> Self {
        Self {
            vertumnus_annotated_ir_version: ANNOTATED_IR_VERSION.to_string(),
            crate_name,
            crate_version,
            items: Vec::new(),
        }
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON string.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
