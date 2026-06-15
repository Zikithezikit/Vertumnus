//! Rust type string parser and mapper.
//!
//! Parses the type strings stored in the IR (e.g., "Option<Vec<f64>>") and
//! produces [`MappedType`] results with Python type equivalents, PyO3
//! strategies, and any warnings for unsupported types.

use crate::annotated_ir::{MappingWarning, PyO3Strategy};
use crate::config::VertumnusConfig;

/// Result of mapping a single Rust type string to Python.
#[derive(Debug, Clone, PartialEq)]
pub struct MappedType {
    /// The Python type string (e.g., "int", "list[float]", "Point")
    pub python_type: String,
    /// The PyO3 strategy for this type
    pub pyo3_strategy: PyO3Strategy,
    /// Warnings encountered while mapping
    pub warnings: Vec<MappingWarning>,
}

impl MappedType {
    fn new(python_type: impl Into<String>, pyo3_strategy: PyO3Strategy) -> Self {
        Self {
            python_type: python_type.into(),
            pyo3_strategy,
            warnings: Vec::new(),
        }
    }

    fn with_warning(
        python_type: impl Into<String>,
        pyo3_strategy: PyO3Strategy,
        message: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        Self {
            python_type: python_type.into(),
            pyo3_strategy,
            warnings: vec![MappingWarning {
                message: message.into(),
                location: location.into(),
            }],
        }
    }

    fn merge_warnings(&mut self, other: &[MappingWarning]) {
        self.warnings.extend_from_slice(other);
    }
}

// ---------------------------------------------------------------------------
// Primitive type set
// ---------------------------------------------------------------------------

/// Returns true if `s` is a Rust primitive type name.
fn is_primitive(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "str"
            | "()"
            | "!"
    )
}

/// Map a Rust primitive type to its Python equivalent and PyO3 strategy.
fn map_primitive(s: &str) -> MappedType {
    match s {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => MappedType::new("int", PyO3Strategy::Native),
        "f32" | "f64" => MappedType::new("float", PyO3Strategy::Native),
        "bool" => MappedType::new("bool", PyO3Strategy::Native),
        "char" => MappedType::new("str", PyO3Strategy::Native),
        "str" => MappedType::new("str", PyO3Strategy::Native),
        "()" => MappedType::new("None", PyO3Strategy::Native),
        "!" => MappedType::new("typing.NoReturn", PyO3Strategy::Native),
        _ => MappedType::with_warning(
            s,
            PyO3Strategy::ManualStub,
            format!("Unknown primitive type '{s}' — manual binding required"),
            "map_primitive",
        ),
    }
}

// ---------------------------------------------------------------------------
// Type string parser
// ---------------------------------------------------------------------------

/// Find the index of the matching closing delimiter, starting from `start`.
/// Returns `None` if not found or unbalanced.
fn find_matching(s: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0u32;
    for (i, c) in s[start..].char_indices() {
        match c {
            c if c == open => depth += 1,
            c if c == close => {
                if depth == 0 {
                    return None; // unbalanced
                }
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split comma-separated top-level type arguments, respecting nested brackets.
/// E.g., "i64, MathError" -> ["i64", "MathError"]
///        "Vec<i32>, String" -> ["Vec<i32>", "String"]
fn split_type_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth_angle = 0u32;
    let mut depth_paren = 0u32;
    let mut depth_bracket = 0u32;
    let mut start = 0usize;

    for (i, c) in s.char_indices() {
        match c {
            '<' => depth_angle += 1,
            '>' => depth_angle = depth_angle.saturating_sub(1),
            '(' => depth_paren += 1,
            ')' => depth_paren = depth_paren.saturating_sub(1),
            '[' => depth_bracket += 1,
            ']' => depth_bracket = depth_bracket.saturating_sub(1),
            ',' if depth_angle == 0 && depth_paren == 0 && depth_bracket == 0 => {
                let trimmed = s[start..i].trim();
                if !trimmed.is_empty() {
                    args.push(trimmed);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let remaining = s[start..].trim();
    if !remaining.is_empty() {
        args.push(remaining);
    }

    args
}

/// Strip whitespace around a type string.
fn trim_type(s: &str) -> &str {
    s.trim()
}

/// Check if a string is a bare generic parameter name (e.g., `T`, `U`, `E`).
///
/// In Rust, generic parameters are typically single uppercase letters.
/// Longer uppercase-starting names are treated as named types (structs, enums).
fn is_generic_param(s: &str) -> bool {
    let s = s.trim();
    // Only single uppercase letters are bare generic parameters
    if s.len() == 1 {
        let c = s
            .chars()
            .next()
            .expect("len == 1 implies at least one char");
        return c.is_ascii_uppercase() && c != '_';
    }
    // Two or more characters: could be a named type or a known std type.
    // We do NOT treat these as generic parameters — they might be structs/enums.
    false
}

/// Check if a type string contains a lifetime annotation (`'a`).
fn has_lifetime(s: &str) -> bool {
    s.contains('\'')
}

/// Check if a type string contains `dyn`.
fn is_dyn_trait(s: &str) -> bool {
    s.trim().starts_with("dyn ")
}

/// Check if a type string contains `impl Trait`.
fn is_impl_trait(s: &str) -> bool {
    s.trim().starts_with("impl ")
}

// ---------------------------------------------------------------------------
// Main mapping function
// ---------------------------------------------------------------------------

/// Map a Rust type string to its Python equivalent (without config).
///
/// Convenience wrapper that delegates to [`map_type_with_config`] with no config.
///
/// See [`map_type_with_config`] for full documentation.
pub fn map_type(type_str: &str, location: &str) -> MappedType {
    map_type_with_config(type_str, location, None)
}

/// Map a Rust type string to its Python equivalent, consulting the optional
/// user config for custom type mappings.
///
/// This is the primary entry point for type mapping. It handles:
/// - Primitives
/// - Standard library types (String, Vec, Option, Result, HashMap, etc.)
/// - User-defined type mappings from config (checked first for named types)
/// - References (`&T`, `&mut T`)
/// - Tuples
/// - Slices and arrays
/// - Named types (structs, enums)
/// - `dyn Trait` and `impl Trait`
///
/// # Arguments
/// * `type_str` - The Rust type string from the IR
/// * `location` - A human-readable location for warning messages
/// * `config` - Optional user config for custom type mappings
pub fn map_type_with_config(
    type_str: &str,
    location: &str,
    config: Option<&VertumnusConfig>,
) -> MappedType {
    let trimmed = trim_type(type_str);

    // Handle unit type and never type
    if trimmed == "()" {
        return MappedType::new("None", PyO3Strategy::Native);
    }
    if trimmed == "!" {
        return MappedType::new("typing.NoReturn", PyO3Strategy::Native);
    }

    // Primitives
    if is_primitive(trimmed) {
        return map_primitive(trimmed);
    }

    // Known standard types
    if trimmed == "String" || trimmed == "&str" {
        return MappedType::new("str", PyO3Strategy::Native);
    }

    // Check for lifetimes — warn but still try to parse
    if has_lifetime(trimmed) && !trimmed.starts_with("&'") {
        // Lifetime in a non-reference position (e.g., type with lifetime param)
        return MappedType::with_warning(
            "typing.Any",
            PyO3Strategy::ManualStub,
            format!("Type '{trimmed}' contains lifetimes which are not supported in v1. Skipping."),
            location,
        );
    }

    // Reference types: &T, &mut T, &'a T, &'a mut T
    if let Some(result) = try_parse_reference(trimmed, location, config) {
        return result;
    }

    // dyn Trait
    if is_dyn_trait(trimmed) {
        return MappedType::with_warning(
            "typing.Any",
            PyO3Strategy::ManualStub,
            format!(
                "'dyn Trait' type '{trimmed}' has limited support in v1. Manual binding required."
            ),
            location,
        );
    }

    // impl Trait
    if is_impl_trait(trimmed) {
        return MappedType::with_warning(
            "typing.Any",
            PyO3Strategy::ManualStub,
            format!(
                "'impl Trait' type '{trimmed}' cannot be mapped automatically. Manual binding required."
            ),
            location,
        );
    }

    // Tuple types: (A, B, ...)
    if trimmed.starts_with('(') {
        return parse_tuple(trimmed, location, config);
    }

    // Slice types: [T] or [T; N]
    if trimmed.starts_with('[') {
        return parse_slice_or_array(trimmed, location, config);
    }

    // Fn pointer types: fn(...) -> ...
    if trimmed.starts_with("fn(") {
        return parse_fn_pointer(trimmed, location, config);
    }

    // Generic type: Vec<T>, Option<T>, Result<T,E>, HashMap<K,V>, etc.
    if let Some(result) = try_parse_generic(trimmed, location, config) {
        return result;
    }

    // Check for bare generic parameters (T, U, etc.)
    if is_generic_param(trimmed) {
        return MappedType::with_warning(
            "typing.Any",
            PyO3Strategy::ManualStub,
            format!("Generic parameter '{trimmed}' cannot be resolved without monomorphization."),
            location,
        );
    }

    // Raw pointers
    if trimmed.starts_with("*const ") || trimmed.starts_with("*mut ") {
        return MappedType::with_warning(
            "typing.Any",
            PyO3Strategy::ManualStub,
            format!("Raw pointer '{trimmed}' cannot be safely represented in Python."),
            location,
        );
    }

    // Function item types like `fn(usize) -> usize {main}`
    if trimmed.contains("{") && trimmed.starts_with("fn(") {
        return parse_fn_pointer(trimmed, location, config);
    }

    // NEW: Check config registry before falling back to default PyClass
    if let Some(cfg) = config {
        if let Some(entry) = cfg.lookup(trimmed) {
            let strategy = VertumnusConfig::parse_strategy(&entry.strategy);
            return MappedType {
                python_type: entry.python.clone(),
                pyo3_strategy: strategy,
                warnings: Vec::new(),
            };
        }
    }

    // Fallback: treat as a named type (struct or enum) — will be #[pyclass]
    MappedType::new(trimmed.to_string(), PyO3Strategy::PyClass)
}

// ---------------------------------------------------------------------------
// Specific type parsers
// ---------------------------------------------------------------------------

/// Try to parse a reference type (`&T`, `&mut T`, `&'a T`, `&'a mut T`).
fn try_parse_reference(
    type_str: &str,
    location: &str,
    config: Option<&VertumnusConfig>,
) -> Option<MappedType> {
    let s = trim_type(type_str);
    if !s.starts_with('&') {
        return None;
    }

    let inner = &s[1..].trim();

    // Check for lifetime: &'a ...
    if let Some(after_quote) = inner.strip_prefix('\'') {
        // Find the end of the lifetime name
        let lifetime_end = after_quote
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(after_quote.len());
        let _lifetime = &after_quote[..lifetime_end];
        let after_lifetime = after_quote[lifetime_end..].trim();

        // Check for mut
        let (_, rest) = if let Some(stripped) = after_lifetime.strip_prefix("mut ") {
            (true, stripped.trim())
        } else {
            (false, after_lifetime)
        };

        let inner_mapped = map_type_with_config(rest, location, config);
        let mut result = MappedType::new(inner_mapped.python_type.clone(), PyO3Strategy::Native);
        result.merge_warnings(&inner_mapped.warnings);
        result.warnings.push(MappingWarning {
            message: format!(
                "Reference with lifetime '{}' in '{}' — lifetime elided for Python binding.",
                _lifetime, type_str
            ),
            location: location.to_string(),
        });
        return Some(result);
    }

    // &mut T
    if let Some(rest) = inner.strip_prefix("mut ") {
        let rest = rest.trim();
        let inner_mapped = map_type_with_config(rest, location, config);
        let mut result = MappedType::new(inner_mapped.python_type.clone(), PyO3Strategy::Native);
        result.merge_warnings(&inner_mapped.warnings);
        return Some(result);
    }

    // &T (shared reference, including &str which is handled above)
    let inner_mapped = map_type_with_config(inner, location, config);
    let mut result = MappedType::new(inner_mapped.python_type.clone(), PyO3Strategy::Native);
    result.merge_warnings(&inner_mapped.warnings);
    Some(result)
}

/// Parse a tuple type string: `(A, B, ...)`.
fn parse_tuple(
    type_str: &str,
    location: &str,
    config: Option<&VertumnusConfig>,
) -> MappedType {
    let s = trim_type(type_str);
    debug_assert!(s.starts_with('(') && s.ends_with(')'));

    let inner = &s[1..s.len() - 1].trim();
    if inner.is_empty() {
        return MappedType::new("None", PyO3Strategy::Native);
    }

    let args = split_type_args(inner);
    let mut mapped_args = Vec::new();
    let mut all_warnings = Vec::new();

    for arg in args {
        let mapped = map_type_with_config(arg, location, config);
        all_warnings.extend(mapped.warnings);
        mapped_args.push(mapped.python_type);
    }

    let py_tuple = format!("tuple[{}]", mapped_args.join(", "));
    MappedType {
        python_type: py_tuple,
        pyo3_strategy: PyO3Strategy::Native,
        warnings: all_warnings,
    }
}

/// Parse a slice or array type: `[T]` or `[T; N]`.
fn parse_slice_or_array(
    type_str: &str,
    location: &str,
    config: Option<&VertumnusConfig>,
) -> MappedType {
    let s = trim_type(type_str);
    debug_assert!(s.starts_with('['));
    debug_assert!(s.ends_with(']'));

    let inner = &s[1..s.len() - 1].trim();

    // Check if it's an array [T; N] or slice [T]
    if let Some(semi_pos) = inner.rfind("; ") {
        // It's an array: [T; N]
        let element_type = &inner[..semi_pos].trim();
        let _len = &inner[semi_pos + 2..].trim();
        let mapped = map_type_with_config(element_type, location, config);
        let mut result = MappedType::new(
            format!("list[{}]", mapped.python_type),
            PyO3Strategy::Native,
        );
        result.merge_warnings(&mapped.warnings);
        result.warnings.push(MappingWarning {
            message: format!(
                "Array type '{}' mapped as list — fixed-size array semantics lost.",
                type_str
            ),
            location: location.to_string(),
        });
        result
    } else {
        // It's a slice: [T]
        let mapped = map_type_with_config(inner, location, config);
        let mut result = MappedType::new(
            format!("list[{}]", mapped.python_type),
            PyO3Strategy::Native,
        );
        result.merge_warnings(&mapped.warnings);
        result
    }
}

/// Parse a function pointer type: `fn(...)` or `fn(...) -> ...`.
fn parse_fn_pointer(
    type_str: &str,
    location: &str,
    config: Option<&VertumnusConfig>,
) -> MappedType {
    let s = trim_type(type_str);

    // Strip everything after the return type (rustdoc sometimes adds {name})
    let s = if let Some(brace_pos) = s.find('{') {
        s[..brace_pos].trim()
    } else {
        s
    };

    // Find the opening paren
    let paren_start = s.find('(').unwrap_or(0);
    // After "fn" prefix, find the parens
    let after_fn = &s[paren_start..];

    // Find closing paren
    let paren_end = find_matching(after_fn, 0, '(', ')').unwrap_or(after_fn.len() - 1);

    let args_str = &after_fn[1..paren_end];
    let return_str = &s[paren_start + paren_end + 1..].trim();

    let mut all_warnings = Vec::new();

    let mapped_args: Vec<String> = if args_str.trim().is_empty() {
        Vec::new()
    } else {
        split_type_args(args_str)
            .iter()
            .map(|a| {
                let mapped = map_type_with_config(a.trim(), location, config);
                all_warnings.extend(mapped.warnings);
                mapped.python_type
            })
            .collect()
    };

    let py_return = if let Some(ret) = return_str.strip_prefix("-> ") {
        let mapped = map_type_with_config(ret.trim(), location, config);
        all_warnings.extend(mapped.warnings);
        mapped.python_type
    } else {
        "None".to_string()
    };

    let py_fn = format!(
        "typing.Callable[[{}], {}]",
        mapped_args.join(", "),
        py_return
    );

    MappedType {
        python_type: py_fn,
        pyo3_strategy: PyO3Strategy::Native,
        warnings: all_warnings,
    }
}

/// Try to parse a generic type like `Vec<T>`, `Option<T>`, `Result<T,E>`, etc.
fn try_parse_generic(
    type_str: &str,
    location: &str,
    config: Option<&VertumnusConfig>,
) -> Option<MappedType> {
    let s = trim_type(type_str);

    // Find the opening angle bracket
    let angle_start = s.find('<')?;
    let base_name = s[..angle_start].trim();

    // Find matching closing bracket
    let close_pos = find_matching(s, angle_start, '<', '>')?;
    let args_str = &s[angle_start + 1..close_pos].trim();

    let args = split_type_args(args_str);
    let mapped_args: Vec<MappedType> = args.iter().map(|a| map_type_with_config(a.trim(), location, config)).collect();
    let mut all_warnings: Vec<MappingWarning> = mapped_args
        .iter()
        .flat_map(|m| m.warnings.clone())
        .collect();

    match base_name {
        // Fully-qualified paths like alloc::vec::Vec
        "Vec" | "alloc::vec::Vec" | "std::vec::Vec" => {
            if let Some(inner) = mapped_args.first() {
                let py = format!("list[{}]", inner.python_type);
                Some(MappedType {
                    python_type: py,
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                })
            } else {
                Some(MappedType::with_warning(
                    "list[typing.Any]",
                    PyO3Strategy::Native,
                    "Vec without type parameter".to_string(),
                    location,
                ))
            }
        }
        "Option" => {
            if let Some(inner) = mapped_args.first() {
                let py = if inner.python_type == "None" {
                    "None".to_string()
                } else {
                    format!("{} | None", inner.python_type)
                };
                Some(MappedType {
                    python_type: py,
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                })
            } else {
                Some(MappedType {
                    python_type: "None".to_string(),
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                })
            }
        }
        "Result" => {
            if !mapped_args.is_empty() {
                let ok_type = &mapped_args[0];
                // The PyO3 strategy for Result is MapErr: wraps `?` operator
                // and raises Python exception on Err
                let mut result = MappedType {
                    python_type: ok_type.python_type.clone(),
                    pyo3_strategy: PyO3Strategy::MapErr,
                    warnings: all_warnings,
                };
                // Add a note about the error type
                if mapped_args.len() >= 2 {
                    let err_type = &mapped_args[1];
                    result.warnings.push(MappingWarning {
                        message: format!(
                            "Result type: error variant '{}' will be converted to Python RuntimeError",
                            err_type.python_type
                        ),
                        location: location.to_string(),
                    });
                }
                Some(result)
            } else {
                Some(MappedType::with_warning(
                    "typing.Any",
                    PyO3Strategy::MapErr,
                    "Result without type parameters".to_string(),
                    location,
                ))
            }
        }
        "HashMap" => {
            if mapped_args.len() >= 2 {
                let py = format!(
                    "dict[{}, {}]",
                    mapped_args[0].python_type, mapped_args[1].python_type
                );
                Some(MappedType {
                    python_type: py,
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                })
            } else {
                Some(MappedType::with_warning(
                    "dict[typing.Any, typing.Any]",
                    PyO3Strategy::Native,
                    "HashMap without type parameters".to_string(),
                    location,
                ))
            }
        }
        "HashSet" => {
            if let Some(inner) = mapped_args.first() {
                let py = format!("set[{}]", inner.python_type);
                Some(MappedType {
                    python_type: py,
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                })
            } else {
                Some(MappedType::with_warning(
                    "set[typing.Any]",
                    PyO3Strategy::Native,
                    "HashSet without type parameter".to_string(),
                    location,
                ))
            }
        }
        "Box" => {
            if let Some(inner) = mapped_args.first() {
                let mut result = MappedType {
                    python_type: inner.python_type.clone(),
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                };
                result.warnings.push(MappingWarning {
                    message: format!(
                        "Box<{}> unwrapped to {}",
                        inner.python_type, inner.python_type
                    ),
                    location: location.to_string(),
                });
                Some(result)
            } else {
                Some(MappedType::with_warning(
                    "typing.Any",
                    PyO3Strategy::Native,
                    "Box without type parameter".to_string(),
                    location,
                ))
            }
        }
        "Rc" | "Arc" => {
            if let Some(inner) = mapped_args.first() {
                let mut result = MappedType {
                    python_type: inner.python_type.clone(),
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                };
                result.warnings.push(MappingWarning {
                    message: format!(
                        "{}<{}> unwrapped to {} — reference counting semantics lost in Python",
                        base_name, inner.python_type, inner.python_type
                    ),
                    location: location.to_string(),
                });
                Some(result)
            } else {
                Some(MappedType::with_warning(
                    "typing.Any",
                    PyO3Strategy::Native,
                    format!("{} without type parameter", base_name),
                    location,
                ))
            }
        }
        "Cow" => {
            if let Some(inner) = mapped_args.first() {
                let mut result = MappedType {
                    python_type: inner.python_type.clone(),
                    pyo3_strategy: PyO3Strategy::Native,
                    warnings: all_warnings,
                };
                result.warnings.push(MappingWarning {
                    message: format!(
                        "Cow<{}> unwrapped to {} — copy-on-write semantics lost in Python",
                        inner.python_type, inner.python_type
                    ),
                    location: location.to_string(),
                });
                Some(result)
            } else {
                Some(MappedType::with_warning(
                    "typing.Any",
                    PyO3Strategy::Native,
                    "Cow without type parameter".to_string(),
                    location,
                ))
            }
        }
        // Range types
        "Range" | "RangeInclusive" | "RangeFrom" | "RangeTo" | "RangeFull" | "RangeToInclusive" => {
            Some(MappedType {
                python_type: "range".to_string(),
                pyo3_strategy: PyO3Strategy::Native,
                warnings: all_warnings,
            })
        }
        // Generic named type: e.g., MyStruct<T> — treat as the base name with pyclass
        _ => {
            // Check if any args are unresolved generics
            for arg in &mapped_args {
                if arg.pyo3_strategy == PyO3Strategy::ManualStub {
                    all_warnings.push(MappingWarning {
                        message: format!(
                            "Generic parameter in '{type_str}' requires manual binding"
                        ),
                        location: location.to_string(),
                    });
                }
            }

            // Check config registry for base name before falling back to PyClass
            if let Some(cfg) = config {
                if let Some(entry) = cfg.lookup(base_name) {
                    let strategy = VertumnusConfig::parse_strategy(&entry.strategy);
                    let mut result = MappedType {
                        python_type: entry.python.clone(),
                        pyo3_strategy: strategy,
                        warnings: all_warnings,
                    };
                    if !mapped_args.is_empty() {
                        result.warnings.push(MappingWarning {
                            message: format!(
                                "Generic type '{type_str}' has type parameters; generated binding will not be generic."
                            ),
                            location: location.to_string(),
                        });
                    }
                    return Some(result);
                }
            }

            // For named generics, use the base name as the Python type
            // with a warning about unresolved generic params
            let mut result = MappedType {
                python_type: base_name.to_string(),
                pyo3_strategy: PyO3Strategy::PyClass,
                warnings: all_warnings,
            };

            if !mapped_args.is_empty() {
                result.warnings.push(MappingWarning {
                    message: format!(
                        "Generic type '{type_str}' has type parameters; generated binding will not be generic."
                    ),
                    location: location.to_string(),
                });
            }

            Some(result)
        }
    }
}

// ---------------------------------------------------------------------------
// High-level item mapping
// ---------------------------------------------------------------------------

/// Map a type string AND determine if the item should be [`PyO3Strategy::PyClass`]
/// or [`PyO3Strategy::PyEnum`] based on context.
///
/// This is used when mapping named types that are structs or enums defined in the crate.
pub fn map_named_type(name: &str, is_enum: bool) -> MappedType {
    if is_enum {
        MappedType::new(name.to_string(), PyO3Strategy::PyEnum)
    } else {
        MappedType::new(name.to_string(), PyO3Strategy::PyClass)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Primitive type tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_primitives() {
        let cases = [
            ("i8", "int"),
            ("i16", "int"),
            ("i32", "int"),
            ("i64", "int"),
            ("i128", "int"),
            ("isize", "int"),
            ("u8", "int"),
            ("u16", "int"),
            ("u32", "int"),
            ("u64", "int"),
            ("u128", "int"),
            ("usize", "int"),
            ("f32", "float"),
            ("f64", "float"),
            ("bool", "bool"),
            ("char", "str"),
            ("str", "str"),
            ("()", "None"),
            ("!", "typing.NoReturn"),
        ];
        for (rust, py) in &cases {
            let result = map_type(rust, "test");
            assert_eq!(
                &result.python_type, py,
                "Expected {rust} -> {py}, got {}",
                result.python_type
            );
            assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
            assert!(
                result.warnings.is_empty(),
                "Unexpected warnings: {:?}",
                result.warnings
            );
        }
    }

    // -----------------------------------------------------------------------
    // Standard type tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_string() {
        let result = map_type("String", "test");
        assert_eq!(result.python_type, "str");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);

        let result = map_type("&str", "test");
        assert_eq!(result.python_type, "str");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_vec() {
        let result = map_type("Vec<f64>", "test");
        assert_eq!(result.python_type, "list[float]");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_nested_vec() {
        let result = map_type("Vec<Vec<i64>>", "test");
        assert_eq!(result.python_type, "list[list[int]]");
    }

    #[test]
    fn test_map_option() {
        let result = map_type("Option<i64>", "test");
        assert_eq!(result.python_type, "int | None");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_option_none_unit() {
        let result = map_type("Option<()>", "test");
        assert_eq!(result.python_type, "None");
    }

    #[test]
    fn test_map_nested_option_vec() {
        let result = map_type("Option<Vec<String>>", "test");
        assert_eq!(result.python_type, "list[str] | None");
    }

    #[test]
    fn test_map_result() {
        let result = map_type("Result<i64, String>", "test");
        assert_eq!(result.python_type, "int");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::MapErr);
        assert!(
            !result.warnings.is_empty(),
            "Should have warning about error type"
        );
    }

    #[test]
    fn test_map_result_no_err_warning() {
        let result = map_type("Result<i64, MathError>", "safe_div.return_type");
        assert_eq!(result.python_type, "int");
        assert!(result
            .warnings
            .iter()
            .any(|w| w.message.contains("RuntimeError")));
    }

    #[test]
    fn test_map_hashmap() {
        let result = map_type("HashMap<String, i64>", "test");
        assert_eq!(result.python_type, "dict[str, int]");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_hashset() {
        let result = map_type("HashSet<String>", "test");
        assert_eq!(result.python_type, "set[str]");
    }

    #[test]
    fn test_map_box() {
        let result = map_type("Box<f64>", "test");
        assert_eq!(result.python_type, "float");
        assert!(!result.warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Reference tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_reference() {
        let result = map_type("&i64", "test");
        assert_eq!(result.python_type, "int");
    }

    #[test]
    fn test_map_mut_reference() {
        let result = map_type("&mut i64", "test");
        assert_eq!(result.python_type, "int");
    }

    #[test]
    fn test_map_ref_with_lifetime() {
        let result = map_type("&'a str", "test");
        assert_eq!(result.python_type, "str");
        assert!(!result.warnings.is_empty(), "Should warn about lifetime");
    }

    #[test]
    fn test_map_ref_lifetime_mut() {
        let result = map_type("&'a mut Point", "test");
        assert_eq!(result.python_type, "Point");
        assert!(!result.warnings.is_empty(), "Should warn about lifetime");
    }

    // -----------------------------------------------------------------------
    // Tuple tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_tuple() {
        let result = map_type("(i32, f64)", "test");
        assert_eq!(result.python_type, "tuple[int, float]");
    }

    #[test]
    fn test_map_unit() {
        let result = map_type("()", "test");
        assert_eq!(result.python_type, "None");
    }

    #[test]
    fn test_map_nested_tuple() {
        let result = map_type("(i32, (f64, bool))", "test");
        assert_eq!(result.python_type, "tuple[int, tuple[float, bool]]");
    }

    // -----------------------------------------------------------------------
    // Slice / array tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_slice() {
        let result = map_type("[u8]", "test");
        assert_eq!(result.python_type, "list[int]");
    }

    #[test]
    fn test_map_array() {
        let result = map_type("[u8; 32]", "test");
        assert_eq!(result.python_type, "list[int]");
        assert!(!result.warnings.is_empty(), "Should warn about array size");
    }

    // -----------------------------------------------------------------------
    // Fn pointer tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_fn_pointer() {
        let result = map_type("fn(i32, i32) -> i32", "test");
        assert_eq!(result.python_type, "typing.Callable[[int, int], int]");
    }

    #[test]
    fn test_map_fn_pointer_no_return() {
        let result = map_type("fn()", "test");
        assert_eq!(result.python_type, "typing.Callable[[], None]");
    }

    // -----------------------------------------------------------------------
    // dyn / impl Trait tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_dyn_trait() {
        let result = map_type("dyn Display", "test");
        assert_eq!(result.python_type, "typing.Any");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::ManualStub);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_map_impl_trait() {
        let result = map_type("impl Display", "test");
        assert_eq!(result.python_type, "typing.Any");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::ManualStub);
    }

    // -----------------------------------------------------------------------
    // Generic parameter tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_generic_param() {
        let result = map_type("T", "test");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::ManualStub);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_map_known_generics_not_treated_as_params() {
        // These are standard types, not generic params
        assert_eq!(map_type("Option<i32>", "test").python_type, "int | None");
        assert_eq!(map_type("Result<i32, String>", "test").python_type, "int");
        assert_eq!(map_type("Vec<String>", "test").python_type, "list[str]");
        assert_eq!(
            map_type("HashMap<String, String>", "test").python_type,
            "dict[str, str]"
        );
    }

    // -----------------------------------------------------------------------
    // Named type tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_named_struct() {
        let result = map_type("Point", "test");
        assert_eq!(result.python_type, "Point");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::PyClass);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_map_named_enum() {
        let result = map_type("Direction", "test");
        assert_eq!(result.python_type, "Direction");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::PyClass);
    }

    #[test]
    fn test_map_named_with_generics() {
        let result = map_type("Wrapper<T>", "test");
        assert_eq!(result.python_type, "Wrapper");
        // Should have a warning about generic
        assert!(!result.warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Lifetime tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_type_with_lifetimes() {
        let result = map_type("Ref<'a>", "test");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::ManualStub);
        assert!(!result.warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_find_matching() {
        // Vec<i64> — find the matching > starting at the < at index 3
        assert_eq!(find_matching("Vec<i64>", 3, '<', '>'), Some(7));
        // Option<Vec<i64>> — find the OUTER matching > starting at the < at index 6
        // String: O p t i o n < V e c < i 6 4 > >
        //         0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5
        // The outer > is at index 15
        assert_eq!(find_matching("Option<Vec<i64>>", 6, '<', '>'), Some(15));
        assert_eq!(find_matching("no angle", 0, '<', '>'), None);
    }

    #[test]
    fn test_split_type_args() {
        assert_eq!(split_type_args("i64, f64"), vec!["i64", "f64"]);
        assert_eq!(split_type_args("i64"), vec!["i64"]);
        assert_eq!(
            split_type_args("Vec<i64>, String"),
            vec!["Vec<i64>", "String"]
        );
        assert_eq!(
            split_type_args("HashMap<String, i64>, bool"),
            vec!["HashMap<String, i64>", "bool"]
        );
    }

    #[test]
    fn test_is_generic_param() {
        assert!(is_generic_param("T"));
        assert!(is_generic_param("U"));
        assert!(!is_generic_param("i64"));
        assert!(!is_generic_param("String"));
        assert!(!is_generic_param("Vec"));
        assert!(!is_generic_param(""));
    }

    #[test]
    fn test_map_named_type_enum() {
        let result = map_named_type("Direction", true);
        assert_eq!(result.python_type, "Direction");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::PyEnum);

        let result = map_named_type("Point", false);
        assert_eq!(result.python_type, "Point");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::PyClass);
    }

    #[test]
    fn test_map_complex_nested() {
        // Result<Option<Vec<f64>>, String>
        let result = map_type("Result<Option<Vec<f64>>, String>", "test");
        assert_eq!(result.python_type, "list[float] | None");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::MapErr);
    }

    #[test]
    fn test_map_arc_rc() {
        let result = map_type("Arc<Mutex<i32>>", "test");
        // Arc unwraps to Mutex<i32>, which is a named generic -> "Mutex"
        assert_eq!(result.python_type, "Mutex");
        // Should have warning about lost semantics
        assert!(!result.warnings.is_empty());
    }

    // -----------------------------------------------------------------------
    // Config-aware mapping tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_map_type_with_config_named_type() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "bytes::Bytes".to_string(),
            TypeMappingEntry {
                python: "bytes".to_string(),
                strategy: "native".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        // Named type that's in config should use the configured mapping
        let result = map_type_with_config("bytes::Bytes", "test", Some(&config));
        assert_eq!(result.python_type, "bytes");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_map_type_with_config_simple_name() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "std::time::Duration".to_string(),
            TypeMappingEntry {
                python: "float".to_string(),
                strategy: "native".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        // Simple name should match the fully-qualified key
        let result = map_type_with_config("Duration", "test", Some(&config));
        assert_eq!(result.python_type, "float");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_type_with_config_inner_generic() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "url::Url".to_string(),
            TypeMappingEntry {
                python: "str".to_string(),
                strategy: "native".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        // Inner type in a generic should be resolved via config
        let result = map_type_with_config("Option<url::Url>", "test", Some(&config));
        assert_eq!(result.python_type, "str | None");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_type_with_config_vec_config_type() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "bytes::Bytes".to_string(),
            TypeMappingEntry {
                python: "bytes".to_string(),
                strategy: "native".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        // Vec of a configured type
        let result = map_type_with_config("Vec<bytes::Bytes>", "test", Some(&config));
        assert_eq!(result.python_type, "list[bytes]");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::Native);
    }

    #[test]
    fn test_map_type_with_config_no_match_falls_back() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "bytes::Bytes".to_string(),
            TypeMappingEntry {
                python: "bytes".to_string(),
                strategy: "native".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        // Types not in config fall back to default behavior
        let result = map_type_with_config("SomeUnknownType", "test", Some(&config));
        assert_eq!(result.python_type, "SomeUnknownType");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::PyClass);
    }

    #[test]
    fn test_map_type_with_config_manual_strategy() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "UnsupportedType".to_string(),
            TypeMappingEntry {
                python: "typing.Any".to_string(),
                strategy: "manual".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        let result = map_type_with_config("UnsupportedType", "test", Some(&config));
        assert_eq!(result.python_type, "typing.Any");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::ManualStub);
    }

    #[test]
    fn test_map_type_with_config_generic_base_name() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "MyWrapper".to_string(),
            TypeMappingEntry {
                python: "MyWrapped".to_string(),
                strategy: "pyclass".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        // Generic type where the base name is in the config
        let result = map_type_with_config("MyWrapper<String>", "test", Some(&config));
        assert_eq!(result.python_type, "MyWrapped");
        assert_eq!(result.pyo3_strategy, PyO3Strategy::PyClass);
        // Should still have warning about type parameters being lost
        assert!(result.warnings.iter().any(|w| w.message.contains("type parameters")));
    }

    #[test]
    fn test_map_type_with_config_maperr_strategy() {
        use crate::config::TypeMappingEntry;
        use std::collections::HashMap;

        let mut mappings = HashMap::new();
        mappings.insert(
            "AppResult".to_string(),
            TypeMappingEntry {
                python: "T".to_string(),
                strategy: "maperr".to_string(),
            },
        );
        let config = VertumnusConfig {
            type_mappings: mappings,
        };

        let result = map_type_with_config("AppResult", "test", Some(&config));
        assert_eq!(result.pyo3_strategy, PyO3Strategy::MapErr);
    }
}
