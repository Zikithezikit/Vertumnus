//! # Vertumnus Binding Generator
//!
//! Phase 3 of the Vertumnus pipeline: emits PyO3-annotated Rust glue code
//! and Python `.pyi` stubs from the annotated IR.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use vertumnus_generator::{Generator, GeneratorConfig};
//! use vertumnus_mapper::annotated_ir::AnnotatedIr;
//!
//! let annotated = AnnotatedIr::new("my_crate".into(), "1.0.0".into());
//! let config = GeneratorConfig {
//!     package_name: "my_package".to_string(),
//!     ..Default::default()
//! };
//! let gen = Generator::new(annotated, config);
//! let files = gen.generate().unwrap();
//!
//! // Write generated files
//! std::fs::write("src/lib.rs", &files.lib_rs).unwrap();
//! std::fs::write("my_package.pyi", &files.pyi).unwrap();
//! std::fs::write("python/my_package/__init__.py", &files.init_py).unwrap();
//! ```

pub mod codegen;
pub mod generator;
pub mod stubs;

pub use codegen::GeneratedRust;
pub use generator::{GenError, GeneratedFiles, Generator, GeneratorConfig};
pub use stubs::GeneratedStubs;

/// Convenience function: generate bindings from an annotated IR with a package name.
///
/// This is the simplest entry point for Phase 3.
///
/// # Arguments
/// * `annotated` - The annotated IR from Phase 2
/// * `package_name` - The Python package name to use
///
/// # Returns
/// Generated files containing Rust glue code, Python stubs, and a shim module.
pub fn generate(
    annotated: &vertumnus_mapper::annotated_ir::AnnotatedIr,
    package_name: &str,
) -> Result<GeneratedFiles, GenError> {
    let config = GeneratorConfig {
        package_name: package_name.to_string(),
        native_module_name: "_core".to_string(),
        ..Default::default()
    };
    let gen = Generator::new(annotated.clone(), config);
    gen.generate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vertumnus_inspector::ir::{
        EnumItem, EnumVariant, FieldVisibility, FunctionItem, IntermediateRepresentation, IrItem,
        IrItemKind, IrType, StructField, StructItem,
    };
    use vertumnus_mapper::map_ir;

    fn make_test_annotated() -> vertumnus_mapper::annotated_ir::AnnotatedIr {
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
                        ::vertumnus_inspector::ir::FunctionParameter {
                            name: "a".to_string(),
                            type_str: "i64".to_string(),
                        },
                        ::vertumnus_inspector::ir::FunctionParameter {
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
                    generic_params: vec![],
                }),
                IrItem::Struct(StructItem {
                    kind: IrItemKind::Struct,
                    name: "Point".to_string(),
                    doc: "A 2D point.".to_string(),
                    fields: vec![
                        StructField {
                            name: "x".to_string(),
                            type_str: "f64".to_string(),
                            visibility: FieldVisibility::Public,
                        },
                        StructField {
                            name: "y".to_string(),
                            type_str: "f64".to_string(),
                            visibility: FieldVisibility::Public,
                        },
                    ],
                    methods: vec![],
                    has_lifetimes: false,
                    has_generics: false,
                    generic_params: vec![],
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
                    generic_params: vec![],
                }),
            ],
        };
        map_ir(&ir).unwrap()
    }

    #[test]
    fn test_generate_convenience_function() {
        let annotated = make_test_annotated();
        let files = generate(&annotated, "simple_math").unwrap();
        assert!(!files.lib_rs.is_empty());
        assert!(!files.pyi.is_empty());
        assert!(!files.init_py.is_empty());
    }

    #[test]
    fn test_generator_crate_name_as_package() {
        let annotated = make_test_annotated();
        let config = GeneratorConfig::default();
        let gen = Generator::new(annotated, config);
        let files = gen.generate().unwrap();
        // Default should use crate name
        assert!(files.lib_rs.contains("simple_math"));
    }
}
