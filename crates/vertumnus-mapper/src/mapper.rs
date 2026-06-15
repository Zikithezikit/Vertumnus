//! Type mapper — walks IR items and produces annotated IR.
//!
//! This is the main entry point for Phase 2 of the Vertumnus pipeline.
//! It takes an [`IntermediateRepresentation`] and produces an [`AnnotatedIr`]
//! by mapping every type in every item to its Python equivalent.

use vertumnus_inspector::ir::{
    EnumItem, FunctionItem, ImplItem, IntermediateRepresentation, IrItem, StructItem, TraitItem,
};

use crate::annotated_ir::{AnnotatedIr, AnnotatedItem, MappingWarning, PyO3Strategy, TypeMapping};
use crate::config::VertumnusConfig;
use crate::type_parser::{map_type_with_config, MappedType};

/// Errors that can occur during type mapping.
#[derive(Debug, thiserror::Error)]
pub enum MapError {
    #[error("IR version {0} is not supported by this mapper version")]
    UnsupportedIrVersion(String),
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

/// Map an entire IR to an annotated IR (without config).
///
/// Convenience wrapper that delegates to [`map_ir_with_config`] with no config.
pub fn map_ir(ir: &IntermediateRepresentation) -> Result<AnnotatedIr, MapError> {
    map_ir_with_config(ir, None)
}

/// Map an entire IR to an annotated IR, with an optional user config.
///
/// This is the primary entry point for the type mapper phase.
///
/// # Arguments
/// * `ir` - The Intermediate Representation from Phase 1
/// * `config` - Optional user config with custom type mappings
///
/// # Returns
/// * `AnnotatedIr` - The annotated IR with type mappings for every item
pub fn map_ir_with_config(
    ir: &IntermediateRepresentation,
    config: Option<&VertumnusConfig>,
) -> Result<AnnotatedIr, MapError> {
    let mut annotated = AnnotatedIr::new(ir.crate_name.clone(), ir.crate_version.clone());

    for item in &ir.items {
        let annotated_item = map_item(item, config);
        annotated.items.push(annotated_item);
    }

    Ok(annotated)
}

/// Map a single IR item to an annotated item.
fn map_item(item: &IrItem, config: Option<&VertumnusConfig>) -> AnnotatedItem {
    match item {
        IrItem::Function(f) => map_function(f, config),
        IrItem::Struct(s) => map_struct(s, config),
        IrItem::Enum(e) => map_enum(e, config),
        IrItem::Trait(t) => map_trait(t, config),
        IrItem::Impl(i) => map_impl(i, config),
    }
}

/// Map a function item.
fn map_function(func: &FunctionItem, config: Option<&VertumnusConfig>) -> AnnotatedItem {
    let mut warnings = Vec::new();

    // Map inputs
    let input_mappings: Vec<MappedType> = func
        .inputs
        .iter()
        .map(|param| {
            let loc = format!("{}.{}", func.name, param.name);
            map_type_with_config(&param.type_str, &loc, config)
        })
        .collect();

    // Collect warnings from input mappings
    for (i, mapping) in input_mappings.iter().enumerate() {
        for w in &mapping.warnings {
            warnings.push(w.clone());
        }
        // Check for unsupported input types
        if mapping.pyo3_strategy == PyO3Strategy::ManualStub {
            let param_name = func.inputs.get(i).map(|p| p.name.as_str()).unwrap_or("?");
            warnings.push(MappingWarning {
                message: format!(
                    "Parameter '{}' uses type '{}' which requires manual binding",
                    param_name,
                    func.inputs
                        .get(i)
                        .map(|p| p.type_str.as_str())
                        .unwrap_or("?")
                ),
                location: format!("{}.{}", func.name, param_name),
            });
        }
    }

    // Map return type
    let output_location = format!("{}.return_type", func.name);
    let output_mapping = map_type_with_config(&func.output.type_str, &output_location, config);
    warnings.extend(output_mapping.warnings.clone());

    // Build output python type string
    let python_output = if output_mapping.python_type == "None" {
        "None".to_string()
    } else {
        output_mapping.python_type.clone()
    };

    // Build input python type strings
    let python_inputs: Vec<String> = input_mappings
        .iter()
        .map(|m| m.python_type.clone())
        .collect();

    // Determine function strategy
    let strategy = if func.is_unsafe {
        warnings.push(MappingWarning {
            message: format!(
                "Function '{}' is unsafe — generated binding will include a safety stub.",
                func.name
            ),
            location: func.name.clone(),
        });
        PyO3Strategy::ManualStub
    } else if func.is_async {
        warnings.push(MappingWarning {
            message: format!(
                "Function '{}' is async — not supported in v1. Manual binding required.",
                func.name
            ),
            location: func.name.clone(),
        });
        PyO3Strategy::ManualStub
    } else if func.has_generics {
        warnings.push(MappingWarning {
            message: format!(
                "Function '{}' has generic parameters — may require monomorphization.",
                func.name
            ),
            location: func.name.clone(),
        });
        PyO3Strategy::PyFunction
    } else if output_mapping.pyo3_strategy == PyO3Strategy::MapErr {
        // Propagate MapErr from return type to function strategy
        PyO3Strategy::MapErr
    } else {
        PyO3Strategy::PyFunction
    };

    let python_type = format!("({}) -> {}", python_inputs.join(", "), python_output);

    let mapping = TypeMapping {
        python_type,
        pyo3_strategy: strategy,
        warnings,
    };

    AnnotatedItem {
        original: IrItem::Function(func.clone()),
        mapping,
    }
}

/// Map a struct item.
fn map_struct(s: &StructItem, config: Option<&VertumnusConfig>) -> AnnotatedItem {
    let mut warnings = Vec::new();

    // Map each field
    let field_mappings: Vec<(String, MappedType)> = s
        .fields
        .iter()
        .map(|field| {
            let loc = format!("{}.{}", s.name, field.name);
            let mapped = map_type_with_config(&field.type_str, &loc, config);
            (field.name.clone(), mapped)
        })
        .collect();

    // Collect field warnings
    for (field_name, mapping) in &field_mappings {
        for w in &mapping.warnings {
            warnings.push(w.clone());
        }
        if mapping.pyo3_strategy == PyO3Strategy::ManualStub {
            warnings.push(MappingWarning {
                message: format!(
                    "Field '{}' has unsupported type — manual binding required",
                    field_name
                ),
                location: format!("{}.{}", s.name, field_name),
            });
        }
    }

    // Check for lifetimes
    if s.has_lifetimes {
        warnings.push(MappingWarning {
            message: format!(
                "Struct '{}' has lifetime parameters — not fully supported in v1. Generated binding will be a stub.",
                s.name
            ),
            location: s.name.clone(),
        });
    }

    // Check for generics
    if s.has_generics {
        warnings.push(MappingWarning {
            message: format!(
                "Struct '{}' has generic parameters — generated binding will not be generic.",
                s.name
            ),
            location: s.name.clone(),
        });
    }

    // Determine strategy
    let strategy = if s.has_lifetimes || s.has_generics {
        // Structs with lifetimes or generic parameters cannot be accurately represented
        PyO3Strategy::ManualStub
    } else {
        PyO3Strategy::PyClass
    };

    // Build Python type representation
    let field_list: Vec<String> = field_mappings
        .iter()
        .map(|(name, mapping)| format!("{}: {}", name, mapping.python_type))
        .collect();

    let python_type = if field_list.is_empty() {
        s.name.clone()
    } else {
        format!("{} {{{}}}", s.name, field_list.join(", "))
    };

    // Also map methods
    for method in &s.methods {
        let method_warnings = map_function_method_warnings(method, &s.name, config);
        warnings.extend(method_warnings);
    }

    let mapping = TypeMapping {
        python_type,
        pyo3_strategy: strategy,
        warnings,
    };

    AnnotatedItem {
        original: IrItem::Struct(s.clone()),
        mapping,
    }
}

/// Map an enum item.
fn map_enum(e: &EnumItem, config: Option<&VertumnusConfig>) -> AnnotatedItem {
    let mut warnings = Vec::new();

    // Determine if this is a C-like enum (no fields on any variant)
    let is_c_like = e.variants.iter().all(|v| v.fields.is_empty());

    // Map variant fields
    for variant in &e.variants {
        for field in &variant.fields {
            let loc = format!("{}.{}.{}", e.name, variant.name, field.name);
            let mapped = map_type_with_config(&field.type_str, &loc, config);
            for w in &mapped.warnings {
                warnings.push(w.clone());
            }
            if mapped.pyo3_strategy == PyO3Strategy::ManualStub {
                warnings.push(MappingWarning {
                    message: format!(
                        "Variant field '{}.{}' has unsupported type — manual binding required",
                        variant.name, field.name
                    ),
                    location: format!("{}.{}.{}", e.name, variant.name, field.name),
                });
            }
        }
    }

    // Check for lifetimes
    if e.has_lifetimes {
        warnings.push(MappingWarning {
            message: format!(
                "Enum '{}' has lifetime parameters — not fully supported in v1. Generated binding will be a stub.",
                e.name
            ),
            location: e.name.clone(),
        });
    }

    // Check for generics
    if e.has_generics {
        warnings.push(MappingWarning {
            message: format!(
                "Enum '{}' has generic parameters — generated binding will not be generic.",
                e.name
            ),
            location: e.name.clone(),
        });
    }

    // Determine strategy
    let strategy = if e.has_lifetimes {
        PyO3Strategy::ManualStub
    } else if !is_c_like {
        warnings.push(MappingWarning {
            message: format!(
                "Enum '{}' has data-carrying variants — requires manual binding.",
                e.name
            ),
            location: e.name.clone(),
        });
        PyO3Strategy::ManualStub
    } else {
        PyO3Strategy::PyEnum
    };

    // Build Python type representation
    let variant_list: Vec<String> = e
        .variants
        .iter()
        .map(|v| {
            if v.fields.is_empty() {
                v.name.clone()
            } else {
                let field_types: Vec<String> = v
                    .fields
                    .iter()
                    .map(|f| {
                        let loc = format!("{}.{}.{}", e.name, v.name, f.name);
                        let mapped = map_type_with_config(&f.type_str, &loc, config);
                        mapped.python_type
                    })
                    .collect();
                format!("{}({})", v.name, field_types.join(", "))
            }
        })
        .collect();

    let python_type = if is_c_like {
        e.name.clone()
    } else {
        format!("{}[{}]", e.name, variant_list.join(" | "))
    };

    // Also map methods
    for method in &e.methods {
        let method_warnings = map_function_method_warnings(method, &e.name, config);
        warnings.extend(method_warnings);
    }

    let mapping = TypeMapping {
        python_type,
        pyo3_strategy: strategy,
        warnings,
    };

    AnnotatedItem {
        original: IrItem::Enum(e.clone()),
        mapping,
    }
}

/// Map a trait item (informational — limited binding generation).
fn map_trait(t: &TraitItem, config: Option<&VertumnusConfig>) -> AnnotatedItem {
    let mut warnings = vec![MappingWarning {
        message: format!(
            "Trait '{}' has limited binding support in v1. Methods may need manual wrapping.",
            t.name
        ),
        location: t.name.clone(),
    }];

    // Map methods
    for method in &t.methods {
        let method_warnings = map_function_method_warnings(method, &t.name, config);
        warnings.extend(method_warnings);
    }

    let mapping = TypeMapping {
        python_type: t.name.clone(),
        pyo3_strategy: PyO3Strategy::ManualStub,
        warnings,
    };

    AnnotatedItem {
        original: IrItem::Trait(t.clone()),
        mapping,
    }
}

/// Map an impl block item.
fn map_impl(i: &ImplItem, config: Option<&VertumnusConfig>) -> AnnotatedItem {
    let mut warnings = Vec::new();

    // Map methods
    for method in &i.methods {
        let method_warnings = map_function_method_warnings(method, &i.type_name, config);
        warnings.extend(method_warnings);
    }

    // If this is a trait impl, add a note
    if let Some(ref trait_name) = i.trait_name {
        warnings.push(MappingWarning {
            message: format!(
                "Impl block for trait '{}' on '{}' — methods will be generated as inherent methods.",
                trait_name, i.type_name
            ),
            location: format!("impl {}", i.type_name),
        });
    }

    let mapping = TypeMapping {
        python_type: i.type_name.clone(),
        pyo3_strategy: PyO3Strategy::PyClass,
        warnings,
    };

    AnnotatedItem {
        original: IrItem::Impl(i.clone()),
        mapping,
    }
}

/// Helper: collect warnings from mapping a function's types (for methods).
fn map_function_method_warnings(
    func: &FunctionItem,
    parent_name: &str,
    config: Option<&VertumnusConfig>,
) -> Vec<MappingWarning> {
    let mut warnings = Vec::new();
    let location_prefix = format!("{}.{}", parent_name, func.name);

    for param in &func.inputs {
        let loc = format!("{}.{}", location_prefix, param.name);
        let mapped = map_type_with_config(&param.type_str, &loc, config);
        warnings.extend(mapped.warnings);
    }

    let ret_loc = format!("{}.return_type", location_prefix);
    let ret_mapped = map_type_with_config(&func.output.type_str, &ret_loc, config);
    warnings.extend(ret_mapped.warnings);

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use vertumnus_inspector::ir::{
        EnumItem, EnumVariant, FieldVisibility, FunctionParameter, ImplItem, IrItemKind, IrType,
        StructField, TraitItem,
    };

    /// Build a minimal IR with a simple function and run the mapper.
    #[test]
    fn test_map_simple_function_ir() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Function(FunctionItem {
                kind: IrItemKind::Function,
                name: "add".to_string(),
                doc: "Adds two ints.".to_string(),
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
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        assert_eq!(annotated.items.len(), 1);

        let item = &annotated.items[0];
        assert_eq!(item.mapping.pyo3_strategy, PyO3Strategy::PyFunction);
        assert!(
            item.mapping.python_type.contains("int"),
            "Expected python_type to contain 'int', got: {}",
            item.mapping.python_type
        );
        assert!(
            item.mapping.warnings.is_empty(),
            "Expected no warnings, got: {:?}",
            item.mapping.warnings
        );
    }

    #[test]
    fn test_map_struct_ir() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Struct(StructItem {
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
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        assert_eq!(item.mapping.pyo3_strategy, PyO3Strategy::PyClass);
        assert!(item.mapping.warnings.is_empty());
        assert!(item.mapping.python_type.contains("Point"));
        assert!(item.mapping.python_type.contains("x"));
        assert!(item.mapping.python_type.contains("y"));
    }

    #[test]
    fn test_map_enum_ir() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Enum(EnumItem {
                kind: IrItemKind::Enum,
                name: "Direction".to_string(),
                doc: "Directions.".to_string(),
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
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        assert_eq!(item.mapping.pyo3_strategy, PyO3Strategy::PyEnum);
        assert!(item.mapping.warnings.is_empty());
    }

    #[test]
    fn test_map_lifetime_struct_warning() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Struct(StructItem {
                kind: IrItemKind::Struct,
                name: "Ref".to_string(),
                doc: "Has lifetime.".to_string(),
                fields: vec![StructField {
                    name: "value".to_string(),
                    type_str: "&'a str".to_string(),
                    visibility: FieldVisibility::Public,
                }],
                methods: vec![],
                has_lifetimes: true,
                has_generics: false,
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        assert_eq!(item.mapping.pyo3_strategy, PyO3Strategy::ManualStub);
        assert!(
            !item.mapping.warnings.is_empty(),
            "Should have warnings about lifetime"
        );
    }

    #[test]
    fn test_map_result_function() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Function(FunctionItem {
                kind: IrItemKind::Function,
                name: "safe_div".to_string(),
                doc: "Safe division.".to_string(),
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
                    type_str: "Result<i64, String>".to_string(),
                },
                is_unsafe: false,
                is_async: false,
                has_generics: false,
                visibility: "public".to_string(),
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        // The return type of safe_div should be "int" (mapped from Result<i64, ...>)
        assert!(
            item.mapping.python_type.contains("int"),
            "Expected int in return type, got: {}",
            item.mapping.python_type
        );
        // Should have a warning about the error type
        assert!(
            item.mapping
                .warnings
                .iter()
                .any(|w| w.message.contains("Result")),
            "Expected a warning about Result type"
        );
    }

    #[test]
    fn test_map_unsafe_function() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Function(FunctionItem {
                kind: IrItemKind::Function,
                name: "unsafe_fn".to_string(),
                doc: "Unsafe.".to_string(),
                inputs: vec![],
                output: IrType {
                    type_str: "()".to_string(),
                },
                is_unsafe: true,
                is_async: false,
                has_generics: false,
                visibility: "public".to_string(),
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        assert_eq!(item.mapping.pyo3_strategy, PyO3Strategy::ManualStub);
        assert!(
            item.mapping
                .warnings
                .iter()
                .any(|w| w.message.contains("unsafe")),
            "Should have safety warning"
        );
    }

    #[test]
    fn test_map_generic_function() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Function(FunctionItem {
                kind: IrItemKind::Function,
                name: "identity".to_string(),
                doc: "Generic identity.".to_string(),
                inputs: vec![FunctionParameter {
                    name: "x".to_string(),
                    type_str: "T".to_string(),
                }],
                output: IrType {
                    type_str: "T".to_string(),
                },
                is_unsafe: false,
                is_async: false,
                has_generics: true,
                visibility: "public".to_string(),
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        assert!(
            !item.mapping.warnings.is_empty(),
            "Should have generic warning"
        );
    }

    #[test]
    fn test_annotated_ir_roundtrip() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Function(FunctionItem {
                kind: IrItemKind::Function,
                name: "foo".to_string(),
                doc: "".to_string(),
                inputs: vec![],
                output: IrType {
                    type_str: "i32".to_string(),
                },
                is_unsafe: false,
                is_async: false,
                has_generics: false,
                visibility: "public".to_string(),
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let json = annotated.to_json_pretty().unwrap();
        let parsed = AnnotatedIr::from_json(&json).unwrap();

        assert_eq!(annotated.crate_name, parsed.crate_name);
        assert_eq!(annotated.items.len(), parsed.items.len());
        assert_eq!(
            annotated.items[0].mapping.python_type,
            parsed.items[0].mapping.python_type
        );
    }

    #[test]
    fn test_map_trait() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Trait(TraitItem {
                kind: IrItemKind::Trait,
                name: "Display".to_string(),
                doc: "Display trait.".to_string(),
                methods: vec![],
                has_lifetimes: false,
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        let item = &annotated.items[0];
        assert_eq!(item.mapping.pyo3_strategy, PyO3Strategy::ManualStub);
    }

    #[test]
    fn test_map_ir_with_config_uses_custom_mappings() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        // Create a config with custom type mappings
        let mut mappings = HashMap::new();
        mappings.insert(
            "MyCustomType".to_string(),
            TypeMappingEntry {
                python: "int".to_string(),
                strategy: "native".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Function(FunctionItem {
                kind: IrItemKind::Function,
                name: "foo".to_string(),
                doc: "".to_string(),
                inputs: vec![FunctionParameter {
                    name: "x".to_string(),
                    type_str: "MyCustomType".to_string(),
                }],
                output: IrType {
                    type_str: "MyCustomType".to_string(),
                },
                is_unsafe: false,
                is_async: false,
                has_generics: false,
                visibility: "public".to_string(),
            })],
        };

        let annotated = map_ir_with_config(&ir, Some(&config)).unwrap();
        let item = &annotated.items[0];
        // The custom type should be mapped to "int" with Native strategy
        assert!(
            item.mapping.python_type.contains("int"),
            "Expected python_type to contain 'int', got: {}",
            item.mapping.python_type
        );
        // Should not have warnings about the custom type
        let custom_type_warnings: Vec<_> = item
            .mapping
            .warnings
            .iter()
            .filter(|w| w.message.contains("MyCustomType"))
            .collect();
        assert!(
            custom_type_warnings.is_empty(),
            "Expected no warnings about MyCustomType, got: {:?}",
            custom_type_warnings
        );
    }

    #[test]
    fn test_map_impl_block() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![IrItem::Impl(ImplItem {
                kind: IrItemKind::Impl,
                type_name: "Point".to_string(),
                methods: vec![FunctionItem {
                    kind: IrItemKind::Function,
                    name: "new".to_string(),
                    doc: "Constructor.".to_string(),
                    inputs: vec![
                        FunctionParameter {
                            name: "x".to_string(),
                            type_str: "f64".to_string(),
                        },
                        FunctionParameter {
                            name: "y".to_string(),
                            type_str: "f64".to_string(),
                        },
                    ],
                    output: IrType {
                        type_str: "Point".to_string(),
                    },
                    is_unsafe: false,
                    is_async: false,
                    has_generics: false,
                    visibility: "public".to_string(),
                }],
                trait_name: None,
                doc: "".to_string(),
            })],
        };

        let annotated = map_ir(&ir).unwrap();
        assert_eq!(annotated.items.len(), 1);
    }
}
