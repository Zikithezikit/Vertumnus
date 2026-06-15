//! Main generator — orchestrates the generation of PyO3 bindings and Python stubs.
//!
//! This is the primary entry point for Phase 3 of the Vertumnus pipeline.
//! It takes an [`AnnotatedIr`] and produces:
//!
//! - Rust glue code (`src/lib.rs`) — PyO3-annotated wrapper code
//! - Python stubs (`<package_name>.pyi`) — type stubs for type checkers
//! - Python shim (`python/<package_name>/__init__.py`) — thin re-export module

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use vertumnus_inspector::ir::{FunctionItem, IrItem};
use vertumnus_mapper::annotated_ir::{AnnotatedIr, PyO3Strategy};

use crate::codegen;
use crate::stubs;

pub use crate::codegen::GeneratedRust;
pub use crate::stubs::GeneratedStubs;

/// Configuration for the binding generator.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// The Python package name (defaults to crate name)
    pub package_name: String,
    /// The native extension module name (defaults to "_core").
    /// This is the last component of `module-name` in pyproject.toml's `[tool.maturin]`,
    /// and must match the `#[pymodule]` function name in generated Rust code.
    /// Example: if the PyO3 module is imported as `my_pkg._core`, this should be `"_core"`.
    pub native_module_name: String,
    /// Whether to derive Debug for pyclasses (generates `__repr__`)
    pub derive_debug: bool,
    /// Whether to derive PartialEq for pyclasses (generates `__eq__`)
    pub derive_eq: bool,
    /// Whether to overwrite existing output files
    pub overwrite: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            package_name: String::new(),
            native_module_name: "_core".to_string(),
            derive_debug: false,
            derive_eq: true,
            overwrite: false,
        }
    }
}

/// All generated output files from Phase 3.
#[derive(Debug, Clone)]
pub struct GeneratedFiles {
    /// `src/lib.rs` — PyO3-annotated Rust glue code
    pub lib_rs: String,
    /// `<package_name>.pyi` — Python type stub file
    pub pyi: String,
    /// `python/<package_name>/__init__.py` — thin Python shim
    pub init_py: String,
}

/// Errors that can occur during binding generation.
#[derive(Debug, thiserror::Error)]
pub enum GenError {
    #[error("No items to generate for crate '{0}'")]
    EmptyCrate(String),
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

/// Result of processing a single item for Rust code generation (used in parallel).
struct ItemCodegenResult {
    code: String,
    fn_registration: Option<String>,
    class_registration: Option<String>,
    enum_registration: Option<String>,
}

/// The main generator — converts an [`AnnotatedIr`] into [`GeneratedFiles`].
pub struct Generator {
    /// The annotated intermediate representation
    annotated: AnnotatedIr,
    /// Generator configuration
    config: GeneratorConfig,
}

impl Generator {
    /// Create a new generator from an annotated IR and configuration.
    pub fn new(annotated: AnnotatedIr, config: GeneratorConfig) -> Self {
        Self { annotated, config }
    }

    /// Run the full generation pipeline: produce Rust glue, Python stubs, and shim.
    ///
    /// # Returns
    ///
    /// A [`GeneratedFiles`] struct containing all output file contents.
    ///
    /// # Errors
    ///
    /// Returns [`GenError::EmptyCrate`] if there are no items to generate.
    pub fn generate(&self) -> Result<GeneratedFiles, GenError> {
        if self.annotated.items.is_empty() {
            return Err(GenError::EmptyCrate(self.annotated.crate_name.clone()));
        }

        let package_name = if self.config.package_name.is_empty() {
            self.annotated.crate_name.replace('-', "_")
        } else {
            self.config.package_name.clone()
        };

        // Collect methods grouped by their parent type (sequential — shared state)
        let methods_by_type = self.collect_methods_by_type();

        // Generate Rust glue code
        let lib_rs = self.generate_rust_code(&package_name, &methods_by_type);

        // Generate Python type stubs
        let pyi = self.generate_pyi(&package_name, &methods_by_type);

        // Generate Python __init__.py
        let init_py = self.generate_init_py(&package_name);

        Ok(GeneratedFiles {
            lib_rs,
            pyi,
            init_py,
        })
    }

    /// Collect all methods grouped by the parent type name.
    ///
    /// Methods come from three sources:
    /// - `StructItem.methods` — methods defined directly on a struct
    /// - `EnumItem.methods` — methods defined directly on an enum
    /// - `ImplItem` — separate impl blocks for a type
    fn collect_methods_by_type(&self) -> HashMap<String, Vec<(FunctionItem, PyO3Strategy)>> {
        let mut by_type: HashMap<String, Vec<(FunctionItem, PyO3Strategy)>> = HashMap::new();

        for item in &self.annotated.items {
            match &item.original {
                IrItem::Struct(s) => {
                    // Collect methods directly on the struct
                    let entry = by_type.entry(s.name.clone()).or_default();
                    for method in &s.methods {
                        let strategy =
                            determine_method_strategy(method, &item.mapping.pyo3_strategy);
                        entry.push((method.clone(), strategy));
                    }
                }
                IrItem::Enum(e) => {
                    // Collect methods directly on the enum
                    let entry = by_type.entry(e.name.clone()).or_default();
                    for method in &e.methods {
                        let strategy =
                            determine_method_strategy(method, &item.mapping.pyo3_strategy);
                        entry.push((method.clone(), strategy));
                    }
                }
                IrItem::Impl(impl_item) => {
                    // Methods from impl blocks
                    let entry = by_type.entry(impl_item.type_name.clone()).or_default();
                    for method in &impl_item.methods {
                        let strategy =
                            determine_method_strategy(method, &item.mapping.pyo3_strategy);
                        entry.push((method.clone(), strategy));
                    }
                }
                _ => {}
            }
        }

        by_type
    }

    /// Generate the Rust/PyO3 glue code.
    fn generate_rust_code(
        &self,
        package_name: &str,
        methods_by_type: &HashMap<String, Vec<(FunctionItem, PyO3Strategy)>>,
    ) -> String {
        let mut code = String::new();

        // Module header
        code.push_str(
            "// Auto-generated by Vertumnus v0.3.0 — https://github.com/Zikithezikit/Vertumnus\n",
        );
        code.push_str("// DO NOT EDIT MANUALLY. Changes will be overwritten on re-generation.\n\n");

        // Preamble: imports and module attributes
        code.push_str("#![allow(unused_imports)]\n");
        code.push_str("#![allow(non_camel_case_types)]\n");
        code.push_str("#![allow(non_snake_case)]\n\n");

        code.push_str("use pyo3::prelude::*;\n");
        code.push_str("use pyo3::exceptions::PyRuntimeError;\n");
        // Emit only the imports actually needed by the generated bindings
        for import in self.needed_imports() {
            code.push_str(&format!("use {};\n", import));
        }
        // Use ::crate_name to avoid ambiguity with #[pymodule] function name
        code.push_str(&format!(
            "use ::{} as _crate;\n\n",
            self.annotated.crate_name
        ));

        // Build sets of wrapper type names (sequential for simplicity — cheap ops)
        let struct_wrappers: HashSet<String> = self
            .annotated
            .items
            .iter()
            .filter_map(|item| match &item.original {
                IrItem::Struct(s) if item.mapping.pyo3_strategy == PyO3Strategy::PyClass => {
                    Some(s.name.clone())
                }
                _ => None,
            })
            .collect();

        let enum_wrappers: HashSet<String> = self
            .annotated
            .items
            .iter()
            .filter_map(|item| match &item.original {
                IrItem::Enum(e)
                    if item.mapping.pyo3_strategy == PyO3Strategy::PyEnum
                        || item.mapping.pyo3_strategy == PyO3Strategy::PyClass =>
                {
                    Some(e.name.clone())
                }
                _ => None,
            })
            .collect();

        // Combined set for backward compatibility
        let wrapper_types: HashSet<String> = struct_wrappers
            .union(&enum_wrappers)
            .cloned()
            .collect();

        // ===================================================================
        // MODULE LEVEL: Item definitions (functions, structs, enums)
        // Process items in parallel — each item is independent.
        // ===================================================================

        let results: Vec<ItemCodegenResult> = self
            .annotated
            .items
            .par_iter()
            .map(|item| {
                let code = match &item.original {
                    IrItem::Function(func_item) => {
                        codegen::generate_function_wrapper(
                            func_item,
                            &item.mapping,
                            &wrapper_types,
                        )
                    }
                    IrItem::Struct(struct_item) => {
                        let methods = methods_by_type
                            .get(&struct_item.name)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]);
                        codegen::generate_struct_wrapper(
                            struct_item,
                            methods,
                            &item.mapping,
                            self.config.derive_debug,
                            self.config.derive_eq,
                            &wrapper_types,
                            &enum_wrappers,
                        )
                    }
                    IrItem::Enum(enum_item) => {
                        let methods = methods_by_type
                            .get(&enum_item.name)
                            .map(|v| v.as_slice())
                            .unwrap_or(&[]);
                        codegen::generate_enum_wrapper(
                            enum_item,
                            methods,
                            &item.mapping,
                            &wrapper_types,
                        )
                    }
                    IrItem::Trait(trait_item) => codegen::generate_trait_stub(trait_item),
                    IrItem::Impl(_) => String::new(), // handled via methods_by_type
                };

                let (fn_reg, class_reg, enum_reg) = match &item.original {
                    IrItem::Function(func_item) => {
                        (Some(func_item.name.clone()), None, None)
                    }
                    IrItem::Struct(struct_item) => {
                        let class_reg = if item.mapping.pyo3_strategy != PyO3Strategy::ManualStub {
                            Some(struct_item.name.clone())
                        } else {
                            None
                        };
                        (None, class_reg, None)
                    }
                    IrItem::Enum(enum_item) => {
                        let enum_reg = if item.mapping.pyo3_strategy != PyO3Strategy::ManualStub {
                            Some(enum_item.name.clone())
                        } else {
                            None
                        };
                        (None, None, enum_reg)
                    }
                    _ => (None, None, None),
                };

                ItemCodegenResult {
                    code,
                    fn_registration: fn_reg,
                    class_registration: class_reg,
                    enum_registration: enum_reg,
                }
            })
            .collect();

        // Merge results in original order (sequential — cheap)
        let mut fn_registrations = Vec::new();
        let mut class_registrations = Vec::new();
        let mut enum_registrations = Vec::new();

        for result in results {
            code.push_str(&result.code);
            if let Some(name) = result.fn_registration {
                fn_registrations.push(name);
            }
            if let Some(name) = result.class_registration {
                class_registrations.push(name);
            }
            if let Some(name) = result.enum_registration {
                enum_registrations.push(name);
            }
        }

        // ===================================================================
        // PYMODULE FUNCTION: Registration code only
        // ===================================================================

        let native_mod_name = if self.config.native_module_name.is_empty() {
            package_name
        } else {
            &self.config.native_module_name
        };

        code.push_str(&format!(
            "/// Vertumnus-generated Python bindings for `{}` v{}\n",
            self.annotated.crate_name, self.annotated.crate_version
        ));
        code.push_str("#[pymodule]\n");
        code.push_str(&format!(
            "fn {}(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {{\n",
            native_mod_name
        ));

        // Get the crate doc string
        let crate_doc = self.get_crate_doc();
        if !crate_doc.is_empty() {
            let escaped = crate_doc.lines().next().unwrap_or("").replace('\"', "\\\"");
            code.push_str(&format!("    m.setattr(\"__doc__\", \"{}\")?;\n", escaped));
        }

        // Register functions
        if !fn_registrations.is_empty() {
            code.push_str("\n    // --- Register functions ---\n");
            for name in &fn_registrations {
                code.push_str(&format!(
                    "    m.add_function(wrap_pyfunction!({}, m)?)?;\n",
                    name
                ));
            }
        }

        // Register classes (structs + enums)
        let all_classes: Vec<&String> = class_registrations
            .iter()
            .chain(enum_registrations.iter())
            .collect();

        if !all_classes.is_empty() {
            code.push_str("\n    // --- Register classes ---\n");
            for name in &all_classes {
                code.push_str(&format!("    m.add_class::<{}>()?;\n", name));
            }
        }

        code.push_str("\n    Ok(())\n}");
        code
    }

    /// Generate the `.pyi` type stub file.
    fn generate_pyi(
        &self,
        package_name: &str,
        methods_by_type: &HashMap<String, Vec<(FunctionItem, PyO3Strategy)>>,
    ) -> String {
        stubs::generate_pyi(&self.annotated, package_name, methods_by_type)
    }

    /// Generate the `__init__.py` shim file.
    fn generate_init_py(&self, package_name: &str) -> String {
        let native_mod_name = if self.config.native_module_name.is_empty() {
            package_name
        } else {
            &self.config.native_module_name
        };
        stubs::generate_init_py(&self.annotated, package_name, native_mod_name)
    }

    /// Detect which `use` imports are needed based on the annotated IR.
    ///
    /// Scans all type strings in the IR and collects the set of types that appear
    /// as annotations in the generated code but are not in Rust's standard prelude.
    fn needed_imports(&self) -> Vec<String> {
        /// Map a raw type base name to its full import path.
        /// Only types NOT in the standard prelude need an explicit import.
        fn type_to_import(type_str: &str) -> Option<&'static str> {
            // Extract the base type name (strip generics, refs, etc.)
            let clean = type_str.trim();
            // Take everything before '<' if generic
            let base = clean.split('<').next().unwrap_or(clean);
            // Strip leading '&', '&mut ', etc.
            let base = base
                .trim_start_matches('&')
                .trim_start_matches("mut ")
                .trim();
            // Strip trailing reference suffixes (unlikely but safe)
            let base = base.trim_end_matches('>').trim();

            match base {
                "Ordering" => Some("std::cmp::Ordering"),
                "Duration" => Some("std::time::Duration"),
                "SystemTime" => Some("std::time::SystemTime"),
                "Path" => Some("std::path::Path"),
                "PathBuf" => Some("std::path::PathBuf"),
                "OsString" => Some("std::ffi::OsString"),
                "OsStr" => Some("std::ffi::OsStr"),
                "HashMap" => Some("std::collections::HashMap"),
                "HashSet" => Some("std::collections::HashSet"),
                "BTreeMap" => Some("std::collections::BTreeMap"),
                "BTreeSet" => Some("std::collections::BTreeSet"),
                "LinkedList" => Some("std::collections::LinkedList"),
                "VecDeque" => Some("std::collections::VecDeque"),
                "Arc" => Some("std::sync::Arc"),
                "Mutex" => Some("std::sync::Mutex"),
                "RwLock" => Some("std::sync::RwLock"),
                "Rc" => Some("std::rc::Rc"),
                "Cell" => Some("std::cell::Cell"),
                "RefCell" => Some("std::cell::RefCell"),
                "Cow" => Some("std::borrow::Cow"),
                _ => None,
            }
        }

        // Types that are already in scope (standard prelude, no import needed)
        let prelude_types: HashSet<&str> = [
            "Option",
            "Result",
            "String",
            "Vec",
            "Box",
            "Iterator",
            "IntoIterator",
            "Clone",
            "Copy",
            "Debug",
            "Display",
            "Eq",
            "PartialEq",
            "Ord",
            "PartialOrd",
            "Hash",
            "Default",
            "Sized",
            "ToString",
            "AsRef",
            "AsMut",
            "Into",
            "From",
            "TryFrom",
            "TryInto",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "isize",
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "usize",
            "f32",
            "f64",
            "bool",
            "char",
            "str",
            "Self", // reserved keyword, not a type that needs import
        ]
        .into_iter()
        .collect();

        // PyO3 types that are already imported at the top of the file
        let pyo3_imported: HashSet<&str> = [
            "PyResult",
            "Python",
            "Bound",
            "PyModule",
            "PyAny",
            "PyDict",
            "PyList",
            "PyTuple",
            "PyString",
            "PyBytes",
            "PyInt",
            "PyFloat",
            "PyBool",
            "PySet",
            "PyFrozenSet",
            "PyComplex",
            "PyIterator",
            "PyMapping",
            "PySequence",
            "PySlice",
            "PyType",
            "PyErr",
            "PyRuntimeError",
            "PyValueError",
            "PyTypeError",
            "PyKeyError",
            "PyStopIteration",
            "PyNotImplementedError",
            "OverflowError",
        ]
        .into_iter()
        .collect();

        let mut needed: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for item in &self.annotated.items {
            let type_strings = item.original.type_strings();
            for ts in &type_strings {
                if let Some(import) = type_to_import(ts) {
                    let ts_str: &str = ts.as_str();
                    if !prelude_types.contains(ts_str)
                        && !pyo3_imported.contains(ts_str)
                        && seen.insert(import.to_string())
                    {
                        needed.push(import.to_string());
                    }
                }
            }
        }

        needed.sort();
        needed
    }

    /// Extract a crate-level doc string.
    fn get_crate_doc(&self) -> String {
        // Look for a doc on crate-level items
        for item in &self.annotated.items {
            let doc = item.original.doc().to_string();
            if !doc.is_empty() {
                // Return the first non-empty doc as representative
                return doc.lines().next().unwrap_or("").to_string();
            }
        }
        format!(
            "Python bindings for {} v{}",
            self.annotated.crate_name, self.annotated.crate_version
        )
    }
}

/// Determine the PyO3 strategy for a method based on its function item.
fn determine_method_strategy(func: &FunctionItem, parent_strategy: &PyO3Strategy) -> PyO3Strategy {
    if func.is_unsafe || func.is_async || func.has_generics {
        return PyO3Strategy::ManualStub;
    }
    // Check if the method's return type is Result<T, E> — if so, use MapErr
    // regardless of the parent's strategy
    let output = func.output.type_str.trim();
    if output.starts_with("Result<") && output.ends_with('>') {
        return PyO3Strategy::MapErr;
    }
    parent_strategy.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vertumnus_inspector::ir::{
        EnumItem, EnumVariant, FieldVisibility, FunctionParameter, IntermediateRepresentation,
        IrItem, IrItemKind, IrType, StructItem,
    };
    use vertumnus_mapper::map_ir;

    fn make_test_annotated() -> AnnotatedIr {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "simple_math".to_string(),
            crate_version: "0.1.0".to_string(),
            items: vec![
                IrItem::Function(FunctionItem {
                    kind: IrItemKind::Function,
                    name: "add".to_string(),
                    doc: "Adds two integers.".to_string(),
                    inputs: vec![
                        FunctionParameter {
                            name: "a".to_string(),
                            type_str: "i64".to_string(),
                        },
                        FunctionParameter {
                            name: "b".to_string(),
                            type_str: "i64".to_string(),
                        },
                    ],
                    output: IrType {
                        type_str: "i64".to_string(),
                    },
                    is_unsafe: false,
                    is_async: false,
                    has_generics: false,
                    visibility: "public".to_string(),
                }),
                IrItem::Struct(StructItem {
                    kind: IrItemKind::Struct,
                    name: "Point".to_string(),
                    doc: "A 2D point.".to_string(),
                    fields: vec![
                        vertumnus_inspector::ir::StructField {
                            name: "x".to_string(),
                            type_str: "f64".to_string(),
                            visibility: FieldVisibility::Public,
                        },
                        vertumnus_inspector::ir::StructField {
                            name: "y".to_string(),
                            type_str: "f64".to_string(),
                            visibility: FieldVisibility::Public,
                        },
                    ],
                    methods: vec![FunctionItem {
                        kind: IrItemKind::Function,
                        name: "distance".to_string(),
                        doc: "Distance between points.".to_string(),
                        inputs: vec![
                            FunctionParameter {
                                name: "self".to_string(),
                                type_str: "&Point".to_string(),
                            },
                            FunctionParameter {
                                name: "other".to_string(),
                                type_str: "&Point".to_string(),
                            },
                        ],
                        output: IrType {
                            type_str: "f64".to_string(),
                        },
                        is_unsafe: false,
                        is_async: false,
                        has_generics: false,
                        visibility: "public".to_string(),
                    }],
                    has_lifetimes: false,
                    has_generics: false,
                }),
                IrItem::Enum(EnumItem {
                    kind: IrItemKind::Enum,
                    name: "Direction".to_string(),
                    doc: "Cardinal directions.".to_string(),
                    variants: vec![
                        EnumVariant {
                            name: "North".to_string(),
                            fields: vec![],
                            discriminant: None,
                        },
                        EnumVariant {
                            name: "South".to_string(),
                            fields: vec![],
                            discriminant: None,
                        },
                    ],
                    methods: vec![],
                    has_lifetimes: false,
                    has_generics: false,
                }),
            ],
        };
        map_ir(&ir).unwrap()
    }

    #[test]
    fn test_generator_creates_output() {
        let annotated = make_test_annotated();
        let config = GeneratorConfig {
            package_name: "simple_math".to_string(),
            ..Default::default()
        };
        let gen = Generator::new(annotated, config);
        let files = gen.generate().unwrap();

        assert!(!files.lib_rs.is_empty(), "lib.rs should not be empty");
        assert!(!files.pyi.is_empty(), ".pyi should not be empty");
        assert!(!files.init_py.is_empty(), "__init__.py should not be empty");

        // Verify key components in the generated Rust code
        assert!(
            files.lib_rs.contains("use pyo3::prelude::*;"),
            "Should import pyo3"
        );
        assert!(
            files.lib_rs.contains("#[pyfunction]"),
            "Should have pyfunction"
        );
        assert!(files.lib_rs.contains("#[pyclass]"), "Should have pyclass");
        assert!(files.lib_rs.contains("fn add"), "Should have add function");
        assert!(
            files.lib_rs.contains("struct Point"),
            "Should have Point struct"
        );
        assert!(
            files.lib_rs.contains("enum Direction"),
            "Should have Direction enum"
        );
        assert!(files.lib_rs.contains("#[pymodule]"), "Should have pymodule");
        assert!(
            files.lib_rs.contains("wrap_pyfunction!"),
            "Should register functions"
        );
        assert!(
            files.lib_rs.contains("add_class::"),
            "Should register classes"
        );
    }

    #[test]
    fn test_generated_stubs_contain_items() {
        let annotated = make_test_annotated();
        let config = GeneratorConfig {
            package_name: "simple_math".to_string(),
            ..Default::default()
        };
        let gen = Generator::new(annotated, config);
        let files = gen.generate().unwrap();

        assert!(files.pyi.contains("def add"));
        assert!(files.pyi.contains("class Point"));
        assert!(files.pyi.contains("class Direction"));

        assert!(files.init_py.contains("add"));
        assert!(files.init_py.contains("Point"));
        assert!(files.init_py.contains("Direction"));
    }

    #[test]
    fn test_empty_crate_errors() {
        let annotated = AnnotatedIr::new("empty".into(), "0.1.0".into());
        let config = GeneratorConfig::default();
        let gen = Generator::new(annotated, config);
        assert!(gen.generate().is_err(), "Empty crate should error");
    }

    #[test]
    fn test_method_collection() {
        let annotated = make_test_annotated();
        let gen = Generator::new(annotated, GeneratorConfig::default());
        let methods = gen.collect_methods_by_type();

        // Point should have a "distance" method
        assert!(
            methods.contains_key("Point"),
            "Should have methods for Point"
        );
        let point_methods = methods.get("Point").unwrap();
        assert!(
            point_methods.iter().any(|(m, _)| m.name == "distance"),
            "Point should have distance method"
        );
    }

    #[test]
    fn test_package_name_used() {
        let annotated = make_test_annotated();
        let config = GeneratorConfig {
            package_name: "my_package".to_string(),
            native_module_name: "my_package".to_string(), // match old behavior
            ..Default::default()
        };
        let gen = Generator::new(annotated, config);
        let files = gen.generate().unwrap();

        // The module function should use the package name (since native_module_name == package_name)
        assert!(
            files.lib_rs.contains("fn my_package("),
            "Should use package name"
        );
        assert!(
            files.pyi.contains("def add"),
            ".pyi should contain function stubs"
        );
    }

    #[test]
    fn test_crate_doc_extraction() {
        let annotated = make_test_annotated();
        let gen = Generator::new(annotated, GeneratorConfig::default());
        let doc = gen.get_crate_doc();
        assert!(!doc.is_empty(), "Should extract crate doc");
    }
}
