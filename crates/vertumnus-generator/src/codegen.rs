//! Rust/PyO3 code generation for Vertumnus bindings.
//!
//! Generates PyO3-annotated Rust code for:
//! - Free functions (`#[pyfunction]`)
//! - Structs (`#[pyclass]` with `#[pymethods]`)
//! - Enums (`#[pyclass]` or `#[pyclass]` with `#[pymethods]`)
//! - Traits (informational stubs)
//! - Methods on types (handles `self`, `&self`, `&mut self`)

use std::collections::HashSet;

use vertumnus_inspector::ir::{
    EnumItem, FieldVisibility, FunctionItem, StructField, StructItem, TraitItem,
};
use vertumnus_mapper::annotated_ir::{PyO3Strategy, TypeMapping};

/// Generated Rust code output.
#[derive(Debug, Clone)]
pub struct GeneratedRust {
    /// The complete `src/lib.rs` content
    pub lib_rs: String,
}

// ---------------------------------------------------------------------------
// Free function generation
// ---------------------------------------------------------------------------

/// Generate a `#[pyfunction]` wrapper for a free function.
pub fn generate_function_wrapper(
    func: &FunctionItem,
    mapping: &TypeMapping,
    wrapper_types: &HashSet<String>,
) -> String {
    let mut code = String::new();

    // Doc comment
    if !func.doc.is_empty() {
        for line in func.doc.lines() {
            if line.is_empty() {
                code.push_str("///\n");
            } else {
                code.push_str(&format!("/// {}\n", line));
            }
        }
    }

    // Check for ManualStub
    if mapping.pyo3_strategy == PyO3Strategy::ManualStub {
        code.push_str("// VERTUMNUS: manual binding required\n");
        for w in &mapping.warnings {
            code.push_str(&format!("// WARNING: {}\n", w.message));
        }
        // Generate a stub function signature
        code.push_str("#[allow(unused_variables)]\n");
        code.push_str("#[pyfunction]\n");
        let params: Vec<String> = func
            .inputs
            .iter()
            .filter(|p| p.name != "self")
            .map(|p| format!("{}: Bound<'_, PyAny>", p.name))
            .collect();
        code.push_str(&format!(
            "pub fn {}({}) -> PyResult<()> {{\n",
            func.name,
            params.join(", ")
        ));
        code.push_str("    todo!(\"VERTUMNUS: manual binding required\")\n");
        code.push_str("}\n\n");
        return code;
    }

    // Check for unsafe
    if func.is_unsafe {
        code.push_str("// SAFETY: This function is `unsafe` in the original Rust crate.\n");
        code.push_str("// VERTUMNUS: The caller must ensure safety invariants.\n");
    }

    // Check for async
    if func.is_async {
        code.push_str("// NOTE: This function is `async` in the original crate.\n");
        code.push_str("// VERTUMNUS: Async bindings are not supported in v1.\n");
        code.push_str("#[pyfunction]\n");
        code.push_str("#[allow(unused_variables)]\n");
        code.push_str(&format!(
            "pub fn {}_async_stub({}) -> PyResult<()> {{\n",
            func.name,
            func.inputs
                .iter()
                .map(|p| format!("{}: Bound<'_, PyAny>", p.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        code.push_str("    Err(pyo3::exceptions::PyNotImplementedError::new_err(\"Async functions are not supported in Vertumnus v1\"))\n");
        code.push_str("}\n\n");
        return code;
    }

    // Check for generics
    if func.has_generics {
        code.push_str("// NOTE: This function has generic parameters in the original crate.\n");
        code.push_str("// VERTUMNUS: Generated binding uses concrete types. Manual adjustment may be needed.\n");
    }

    // Generate the pyfunction
    code.push_str("#[pyfunction]\n");

    // Build parameter list
    let params: Vec<String> = func
        .inputs
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| {
            let rust_type = ir_type_to_pyo3_type(&p.type_str);
            format!("{}: {}", p.name, rust_type)
        })
        .collect();

    // Determine return type
    let return_type = determine_return_type(func, mapping);
    let return_annotation = if return_type == "()" {
        String::new()
    } else {
        format!(" -> {}", return_type)
    };

    code.push_str(&format!(
        "pub fn {}({}){} {{\n",
        func.name,
        params.join(", "),
        return_annotation
    ));

    // Generate the function body
    let body = generate_function_body(func, mapping, wrapper_types);
    code.push_str(&body);

    code.push_str("}\n\n");
    code
}

/// Determine the Rust return type for a PyO3 function wrapper.
fn determine_return_type(func: &FunctionItem, mapping: &TypeMapping) -> String {
    if mapping.pyo3_strategy == PyO3Strategy::MapErr {
        // Result<T, E> → PyResult<T>
        if let Some(inner) = extract_result_ok_type(&func.output.type_str) {
            format!("PyResult<{}>", ir_type_to_pyo3_type(&inner))
        } else {
            "PyResult<()>".to_string()
        }
    } else if func.output.type_str == "()" {
        "()".to_string()
    } else {
        ir_type_to_pyo3_type(&func.output.type_str)
    }
}

/// Extract the Ok type from a Result<T, E> type string.
pub(crate) fn extract_result_ok_type(type_str: &str) -> Option<String> {
    let s = type_str.trim();
    if s.starts_with("Result<") {
        let inner = &s[7..s.len() - 1]; // Strip "Result<" and ">"
        // Split on first comma at top level
        let mut depth = 0u32;
        for (i, c) in inner.char_indices() {
            match c {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    return Some(inner[..i].trim().to_string());
                }
                _ => {}
            }
        }
        // No comma means Result<T> — use the whole inner
        if !inner.is_empty() {
            return Some(inner.trim().to_string());
        }
    }
    None
}

/// Generate the body of a function wrapper.
fn generate_function_body(
    func: &FunctionItem,
    mapping: &TypeMapping,
    wrapper_types: &HashSet<String>,
) -> String {
    // Check for method (has &self, &mut self, or self)
    let is_method = func.inputs.first().map(|p| p.name.as_str()) == Some("self");

    if mapping.pyo3_strategy == PyO3Strategy::MapErr {
        // Result<T, E> → use .map_err(|e| PyRuntimeError::new_err(...))
        let call = if is_method {
            format_method_call(func, wrapper_types)
        } else {
            format_function_call(func, wrapper_types)
        };
        format!(
            "    {}.map_err(|e| PyRuntimeError::new_err(format!(\"{{:?}}\", e)))\n",
            call
        )
    } else if func.output.type_str == "()" {
        // Unit return → just call and discard
        let call = if is_method {
            format_method_call(func, wrapper_types)
        } else {
            format_function_call(func, wrapper_types)
        };
        format!("    {};\n", call)
    } else {
        // Infallible → return the value directly (PyO3 auto-converts)
        let call = if is_method {
            format_method_call(func, wrapper_types)
        } else {
            format_function_call(func, wrapper_types)
        };
        format!("    {}\n", call)
    }
}

/// Extract the base type name from a type string, stripping references, lifetimes, etc.
/// Returns the inner type name and the reference prefix substring from the original input.
fn extract_ref_prefix<'a>(type_str: &'a str) -> (&'a str, &'a str) {
    let s = type_str.trim();

    // &mut T
    if let Some(rest) = s.strip_prefix("&mut ") {
        return ("&mut ", rest.trim());
    }

    // &'lifetime mut T or &'lifetime T
    if let Some(after_ampersand) = s.strip_prefix("&") {
        if after_ampersand.starts_with('\'') {
            // Find the end of the lifetime (space after the lifetime name)
            if let Some(space_pos) = after_ampersand.find(' ') {
                let after_lifetime = after_ampersand[space_pos + 1..].trim();
                let has_mut = after_lifetime.starts_with("mut ");
                if has_mut {
                    let rest = after_lifetime.strip_prefix("mut ").unwrap().trim();
                    // "&'lifetime mut " prefix
                    let prefix_end = 1 + space_pos + 1 + 4; // & + lifetime + ' ' + "mut "
                    return if prefix_end <= s.len() {
                        (&s[..prefix_end], rest)
                    } else {
                        ("", s)
                    };
                } else {
                    // "&'lifetime " prefix
                    let prefix_end = 1 + space_pos + 1; // & + lifetime + ' '
                    return if prefix_end <= s.len() {
                        (&s[..prefix_end], after_lifetime)
                    } else {
                        ("", s)
                    };
                }
            }
        }
        // Just &T (no lifetime)
        return ("&", after_ampersand.trim());
    }

    ("", s)
}

/// Given a parameter name + type, return the expression to pass to the original crate,
/// unwrapping wrapper types by accessing `.inner` (for structs) or converting (for enums).
fn unwrap_arg_for_call(
    param_name: &str,
    param_type: &str,
    wrapper_types: &HashSet<String>,
) -> String {
    let (prefix, base) = extract_ref_prefix(param_type);
    // Strip any trailing reference on base as well (e.g. &Point -> Point)
    let base_clean = base.strip_suffix('&').map(|s| s.trim()).unwrap_or(base);
    if wrapper_types.contains(base_clean) {
        // For struct wrappers, unwrap via .inner
        // For enum wrappers, convert via ::from()
        if prefix.is_empty() || prefix == "&" || prefix == "&mut " {
            format!("{}{}.inner", prefix, param_name)
        } else {
            // With lifetimes — fall back to direct pass (will generate compile error if wrong)
            param_name.to_string()
        }
    } else {
        param_name.to_string()
    }
}

/// Format a call to the original crate function.
fn format_function_call(func: &FunctionItem, wrapper_types: &HashSet<String>) -> String {
    let args: Vec<String> = func
        .inputs
        .iter()
        .filter(|p| p.name != "self")
        .map(|p| unwrap_arg_for_call(&p.name, &p.type_str, wrapper_types))
        .collect();

    format!("_crate::{}({})", func.name, args.join(", "))
}

/// Format a call to the original crate method on `self.inner`.
fn format_method_call(func: &FunctionItem, wrapper_types: &HashSet<String>) -> String {
    let args: Vec<String> = func
        .inputs
        .iter()
        .skip(1) // skip self
        .map(|p| unwrap_arg_for_call(&p.name, &p.type_str, wrapper_types))
        .collect();

    if args.is_empty() {
        format!("self.inner.{}()", func.name)
    } else {
        format!("self.inner.{}({})", func.name, args.join(", "))
    }
}

// ---------------------------------------------------------------------------
// Struct generation
// ---------------------------------------------------------------------------

/// Generate a `#[pyclass]` wrapper for a struct, including its methods.
pub fn generate_struct_wrapper(
    s: &StructItem,
    methods: &[(FunctionItem, PyO3Strategy)],
    mapping: &TypeMapping,
    derive_debug: bool,
    _derive_eq: bool,
    wrapper_types: &HashSet<String>,
) -> String {
    let mut code = String::new();

    // Check for ManualStub
    if mapping.pyo3_strategy == PyO3Strategy::ManualStub {
        code.push_str("// VERTUMNUS: manual binding required\n");
        for w in &mapping.warnings {
            code.push_str(&format!("// WARNING: {}\n", w.message));
        }
        code.push_str("#[allow(dead_code)]\n");
        // For structs with lifetimes/generics, use PhantomData instead of inner
        if s.has_lifetimes || s.has_generics {
            code.push_str(&format!(
                "pub struct {} {{\n    _phantom: std::marker::PhantomData<{}>,\n}}\n\n",
                s.name, s.name
            ));
        } else {
            code.push_str(&format!(
                "pub struct {} {{\n    inner: _crate::{},\n}}\n\n",
                s.name, s.name
            ));
        }
        return code;
    }

    // Doc comment
    if !s.doc.is_empty() {
        for line in s.doc.lines() {
            if line.is_empty() {
                code.push_str("///\n");
            } else {
                code.push_str(&format!("/// {}\n", line));
            }
        }
    } else {
        code.push_str(&format!("/// Python wrapper for `{}`\n", s.name));
    }

    // Attributes — don't derive Debug/Clone if inner type may not support it
    let mut attrs = vec!["#[pyclass]".to_string()];
    // Only add derive macros if the inner type is not generic and has no lifetimes
    if !s.has_generics && !s.has_lifetimes {
        if derive_debug {
            attrs.push("#[derive(Debug)]".to_string());
        }
        if _derive_eq {
            // PartialEq is not auto-derived; only add if simple fields
        }
    }
    for attr in &attrs {
        code.push_str(attr);
        code.push('\n');
    }

    // Struct definition with inner wrapper
    code.push_str(&format!("pub struct {} {{\n", s.name));
    code.push_str("    inner: _crate::");
    code.push_str(&s.name);
    code.push_str(",\n}\n\n");

    // Generate property getters for public fields
    let public_fields: Vec<&StructField> = s
        .fields
        .iter()
        .filter(|f| f.visibility == FieldVisibility::Public)
        .collect();

    if !public_fields.is_empty() || !methods.is_empty() {
        code.push_str("#[pymethods]\n");
        code.push_str(&format!("impl {} {{\n", s.name));
    }

    // Generate field getters (skip generic/unsupported fields)
    for field in &public_fields {
        if is_generic_field(&field.type_str) {
            code.push_str("    // VERTUMNUS: Field '");
            code.push_str(&field.name);
            code.push_str("' has generic/unsupported type. Manual getter required.\n");
            continue;
        }
        code.push_str("    /// Getter for `");
        code.push_str(&field.name);
        code.push_str("`\n");
        code.push_str("    #[getter]\n");
        let py_return = ir_type_to_pyo3_type(&field.type_str);
        code.push_str(&format!(
            "    fn {}(&self) -> {} {{\n",
            field.name, py_return
        ));
        // Clone non-Copy types (like String) to avoid move errors
        if is_copy_type(&field.type_str) {
            code.push_str(&format!("        self.inner.{}\n", field.name));
        } else {
            code.push_str(&format!("        self.inner.{}.clone()\n", field.name));
        }
        code.push_str("    }\n\n");
    }

    // Generate methods
    for (method, strategy) in methods {
        let method_code = generate_method_wrapper(method, &s.name, strategy, true, wrapper_types);
        code.push_str(&method_code);
    }

    if !public_fields.is_empty() || !methods.is_empty() {
        code.push_str("}\n\n");
    }

    code
}

// ---------------------------------------------------------------------------
// Enum generation
// ---------------------------------------------------------------------------

/// Generate a `#[pyclass]` wrapper for an enum.
pub fn generate_enum_wrapper(
    e: &EnumItem,
    methods: &[(FunctionItem, PyO3Strategy)],
    mapping: &TypeMapping,
    wrapper_types: &HashSet<String>,
) -> String {
    let mut code = String::new();

    // Check for ManualStub
    if mapping.pyo3_strategy == PyO3Strategy::ManualStub {
        code.push_str("// VERTUMNUS: manual binding required\n");
        for w in &mapping.warnings {
            code.push_str(&format!("// WARNING: {}\n", w.message));
        }
        code.push_str("#[allow(dead_code)]\n");
        code.push_str(&format!("pub enum {} {{}}\n\n", e.name));
        return code;
    }

    // Check if this is a C-like enum (all variants have no fields)
    let is_c_like = e.variants.iter().all(|v| v.fields.is_empty());

    // Doc comment
    if !e.doc.is_empty() {
        for line in e.doc.lines() {
            if line.is_empty() {
                code.push_str("///\n");
            } else {
                code.push_str(&format!("/// {}\n", line));
            }
        }
    } else {
        code.push_str(&format!("/// Python wrapper for `{}`\n", e.name));
    }

    if is_c_like {
        // Use eq, eq_int to silence deprecated implicit equality warning
        code.push_str("#[pyclass(eq, eq_int)]\n");
        code.push_str("#[derive(Clone, PartialEq)]\n");
    } else {
        code.push_str("#[pyclass]\n");
    }

    if is_c_like {
        // C-like enum — simple variant mapping
        code.push_str(&format!("pub enum {} {{\n", e.name));
        for variant in &e.variants {
            code.push_str(&format!("    {},\n", variant.name));
        }
        code.push_str("}\n");
    } else {
        // Enum with data — limited representation
        code.push_str("// VERTUMNUS: Enum has data-carrying variants.\n");
        code.push_str("// Generated as a simple enum with only fieldless variants.\n");
        code.push_str(&format!("pub enum {} {{\n", e.name));
        for variant in &e.variants {
            if variant.fields.is_empty() {
                code.push_str(&format!("    {},\n", variant.name));
            } else {
                code.push_str(&format!(
                    "    // {}: data variant — manual binding required\n",
                    variant.name
                ));
            }
        }
        code.push_str("}\n");
    }

    code.push('\n');

    // Generate From conversion for C-like enums (for method dispatch)
    if is_c_like {
        code.push_str(&format!(
            "impl From<{}> for _crate::{} {{\n",
            e.name, e.name
        ));
        code.push_str(&format!("    fn from(val: {}) -> Self {{\n", e.name));
        code.push_str("        match val {\n");
        for variant in &e.variants {
            code.push_str(&format!(
                "            {}::{} => _crate::{}::{},\n",
                e.name, variant.name, e.name, variant.name
            ));
        }
        code.push_str("        }\n");
        code.push_str("    }\n");
        code.push_str("}\n\n");
    }

    // Generate methods if any
    if !methods.is_empty() {
        code.push_str("#[pymethods]\n");
        code.push_str(&format!("impl {} {{\n", e.name));

        for (method, strategy) in methods {
            let method_code = generate_method_wrapper(method, &e.name, strategy, false, wrapper_types);
            code.push_str(&method_code);
        }

        code.push_str("}\n\n");
    }

    code
}

// ---------------------------------------------------------------------------
// Method generation
// ---------------------------------------------------------------------------

/// Generate a method wrapper for a struct or enum method.
///
/// # Arguments
/// * `method` - The function item describing the method
/// * `parent_name` - The name of the parent type (struct/enum)
/// * `strategy` - The PyO3 strategy to use
/// * `is_struct` - Whether the parent is a struct (vs enum)
fn generate_method_wrapper(
    method: &FunctionItem,
    parent_name: &str,
    strategy: &PyO3Strategy,
    is_struct: bool,
    wrapper_types: &HashSet<String>,
) -> String {
    let mut code = String::new();

    // Check for ManualStub
    if *strategy == PyO3Strategy::ManualStub {
        code.push_str("    // VERTUMNUS: manual binding required\n");
        if method.is_unsafe {
            code.push_str("    // SAFETY: Original method is unsafe.\n");
        }
        if method.is_async {
            code.push_str("    // NOTE: Original method is async — not supported in v1.\n");
        }
        if method.has_generics {
            code.push_str("    // NOTE: Original method has generic parameters.\n");
        }
        code.push_str("    #[allow(unused_variables)]\n");
        code.push_str(&format!("    fn {}_stub(&self) -> PyResult<()> {{\n", method.name));
        code.push_str("        todo!(\"VERTUMNUS: manual binding required\")\n");
        code.push_str("    }\n\n");
        return code;
    }

    // Doc comment
    if !method.doc.is_empty() {
        for line in method.doc.lines() {
            if line.is_empty() {
                code.push_str("    ///\n");
            } else {
                code.push_str(&format!("    /// {}\n", line));
            }
        }
    }

    // Determine the type of receiver
    let receiver = determine_receiver(method);

    match receiver {
        Receiver::None => {
            // Static method — use #[staticmethod]
            if method.name == "new" {
                code.push_str("    #[new]\n");
            } else {
                code.push_str("    #[staticmethod]\n");
            }

            let params: Vec<String> = method
                .inputs
                .iter()
                .map(|p| {
                    let rt = ir_type_to_pyo3_type(&p.type_str);
                    format!("{}: {}", p.name, rt)
                })
                .collect();

            let return_type = if method.name == "new" || method.output.type_str == "Self" {
                "Self".to_string()
            } else {
                method_return_type(method, strategy)
            };
            let ret = if return_type == "()" {
                String::new()
            } else {
                format!(" -> {}", return_type)
            };

            code.push_str(&format!(
                "    pub fn {}({}){} {{\n",
                method.name,
                params.join(", "),
                ret
            ));

            if is_struct {
                // Constructor: wrap the result in the pyclass wrapper
                if method.name == "new" || method.output.type_str == "Self" {
                    // This is a constructor that returns Self
                    let arg_names: Vec<String> = method
                        .inputs
                        .iter()
                        .map(|p| unwrap_arg_for_call(&p.name, &p.type_str, wrapper_types))
                        .collect();
                    code.push_str(&format!(
                        "        {} {{ inner: _crate::{}::{}({}) }}\n",
                        parent_name, parent_name, method.name, arg_names.join(", ")
                    ));
                } else {
                    let arg_names: Vec<String> = method
                        .inputs
                        .iter()
                        .map(|p| unwrap_arg_for_call(&p.name, &p.type_str, wrapper_types))
                        .collect();
                    code.push_str(&format!(
                        "        _crate::{}::{}({})\n",
                        parent_name, method.name, arg_names.join(", ")
                    ));
                }
            } else {
                // Enum — directly delegate
                let arg_names: Vec<String> = method
                    .inputs
                    .iter()
                    .map(|p| unwrap_arg_for_call(&p.name, &p.type_str, wrapper_types))
                    .collect();
                code.push_str(&format!(
                    "        _crate::{}::{}({})\n",
                    parent_name, method.name, arg_names.join(", ")
                ));
            }
            code.push_str("    }\n\n");
        }
        Receiver::Ref | Receiver::MutRef | Receiver::Value => {
            // Instance method
            let self_param = match receiver {
                Receiver::Ref => "&self",
                Receiver::MutRef => "&mut self",
                Receiver::Value => "self",
                _ => unreachable!(),
            };

            let params: Vec<String> = method
                .inputs
                .iter()
                .filter(|p| p.name != "self")
                .map(|p| {
                    let rt = ir_type_to_pyo3_type(&p.type_str);
                    format!("{}: {}", p.name, rt)
                })
                .collect();

            let return_type = method_return_type(method, strategy);
            let ret = if return_type == "()" {
                String::new()
            } else {
                format!(" -> {}", return_type)
            };

            code.push_str(&format!(
                "    pub fn {}({}{}){} {{\n",
                method.name,
                self_param,
                if params.is_empty() {
                    String::new()
                } else {
                    format!(", {}", params.join(", "))
                },
                ret
            ));

            // Generate the body
            let call_args: Vec<String> = method
                .inputs
                .iter()
                .skip(1)
                .map(|p| unwrap_arg_for_call(&p.name, &p.type_str, wrapper_types))
                .collect();

            let call_str = if is_struct {
                if call_args.is_empty() {
                    format!("self.inner.{}()", method.name)
                } else {
                    format!("self.inner.{}({})", method.name, call_args.join(", "))
                }
            } else {
                // Enum: need to convert self to inner enum type
                let self_expr = enum_self_conversion(parent_name);
                if call_args.is_empty() {
                    format!("_crate::{}::{}({})", parent_name, method.name, self_expr)
                } else {
                    format!("_crate::{}::{}({}, {})", parent_name, method.name, self_expr, call_args.join(", "))
                }
            };

            // Map the output for Result types — no Ok() wrapping needed for PyO3
            if *strategy == PyO3Strategy::MapErr {
                code.push_str(&format!(
                    "        {}.map_err(|e| PyRuntimeError::new_err(format!(\"{{:?}}\", e)))\n",
                    call_str
                ));
            } else if method.output.type_str == "()" {
                code.push_str(&format!("        {};\n", call_str));
            } else {
                // Infallible — return the value directly (PyO3 auto-converts)
                code.push_str(&format!("        {}\n", call_str));
            }
            code.push_str("    }\n\n");
        }
    }

    code
}

/// Determine the method return type in PyO3 terms.
fn method_return_type(method: &FunctionItem, strategy: &PyO3Strategy) -> String {
    if *strategy == PyO3Strategy::MapErr {
        if let Some(ok_type) = extract_result_ok_type(&method.output.type_str) {
            format!("PyResult<{}>", ir_type_to_pyo3_type(&ok_type))
        } else {
            "PyResult<()>".to_string()
        }
    } else if method.output.type_str == "()" || method.output.type_str == "Self" {
        "()".to_string()
    } else {
        ir_type_to_pyo3_type(&method.output.type_str)
    }
}

/// The type of `self` receiver in a method.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Receiver {
    /// No `self` parameter — static method
    None,
    /// `&self`
    Ref,
    /// `&mut self`
    MutRef,
    /// `self` (consuming)
    Value,
}

/// Determine what kind of receiver a method has.
fn determine_receiver(method: &FunctionItem) -> Receiver {
    let first = match method.inputs.first() {
        Some(p) => p,
        None => return Receiver::None,
    };

    if first.name != "self" {
        return Receiver::None;
    }

    let type_str = &first.type_str;
    if type_str.contains("mut") {
        Receiver::MutRef
    } else if type_str.starts_with('&') {
        Receiver::Ref
    } else {
        Receiver::Value
    }
}

// ---------------------------------------------------------------------------
// Trait stub generation
// ---------------------------------------------------------------------------

/// Generate a stub for a trait (informational — limited binding support).
pub fn generate_trait_stub(t: &TraitItem) -> String {
    let mut code = String::new();
    code.push_str("// VERTUMNUS: Trait '");
    code.push_str(&t.name);
    code.push_str("' has limited binding support in v1.\n");
    code.push_str("// The following methods require manual wrapping:\n");
    for method in &t.methods {
        code.push_str(&format!("//   - {}\n", method.name));
    }
    code.push('\n');
    code
}

// ---------------------------------------------------------------------------
// Type conversion helpers
// ---------------------------------------------------------------------------

/// Convert an IR type string to a PyO3-compatible Rust type.
///
/// This is the inverse of the mapper's type mapping, producing Rust types
/// suitable for PyO3 function signatures.
/// Generate an expression to convert the current enum wrapper `self` to the original crate enum type.
/// For C-like enums, this clones self and converts via the generated `From` impl,
/// then borrows the result to match the original method signature.
fn enum_self_conversion(parent_name: &str) -> String {
    format!("&_crate::{}::from(self.clone())", parent_name)
}

/// Check if a Rust type implements `Copy` (primitives that PyO3 can extract by value).
fn is_copy_type(type_str: &str) -> bool {
    matches!(
        type_str.trim(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
            | "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
            | "f32" | "f64" | "bool" | "char"
    )
}

/// Check if a field type string is a bare generic parameter (single uppercase letter like `T`, `U`)
/// or contains generic type variables.
fn is_generic_field(type_str: &str) -> bool {
    let trimmed = type_str.trim();
    // Bare uppercase single-letter identifiers are generic params
    if trimmed.len() == 1 {
        let c = trimmed.chars().next().unwrap();
        return c.is_ascii_uppercase() && c.is_ascii_alphabetic();
    }
    // Multi-letter all-uppercase generic params like `TKey`, `TValue`
    if trimmed.len() <= 6 && trimmed.chars().all(|c| c.is_ascii_uppercase()) {
        return true;
    }
    // Contains generic angle brackets — treat as generic
    if trimmed.contains('<') {
        return true;
    }
    false
}

fn ir_type_to_pyo3_type(type_str: &str) -> String {
    let s = type_str.trim();

    match s {
        // Primitives
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
        | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => s.to_string(),
        "f32" | "f64" => s.to_string(),
        "bool" => "bool".to_string(),
        "char" => "char".to_string(),
        "str" => "str".to_string(),
        "&str" => "&str".to_string(),
        "()" => "()".to_string(),
        "String" => "String".to_string(),
        "Self" => "Self".to_string(),

        // Reference types: &T, &mut T, &'a T
        _ if s.starts_with("&mut ") => {
            let inner = &s[5..].trim();
            format!("&mut {}", ir_type_to_pyo3_type(inner))
        }
        _ if s.starts_with("&'") => {
            // Reference with lifetime — strip lifetime
            // e.g., &'a Point -> &Point, &'a mut Point -> &mut Point
            let after_quote = &s[2..]; // skip "&'"
            let lifetime_end = after_quote
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(after_quote.len());
            let after_lifetime = after_quote[lifetime_end..].trim();
            if let Some(rest) = after_lifetime.strip_prefix("mut ") {
                let inner = rest.trim();
                format!("&mut {}", ir_type_to_pyo3_type(inner))
            } else {
                format!("&{}", ir_type_to_pyo3_type(after_lifetime))
            }
        }
        _ if s.starts_with('&') => {
            // Shared reference: &T
            let inner = &s[1..].trim();
            let inner_type = ir_type_to_pyo3_type(inner);
            if inner_type == "str" {
                "&str".to_string()
            } else {
                format!("&{}", inner_type)
            }
        }

        // Vec<T>
        _ if s.starts_with("Vec<") && s.ends_with('>') => {
            let inner = &s[4..s.len() - 1];
            format!("Vec<{}>", ir_type_to_pyo3_type(inner))
        }

        // Option<T>
        _ if s.starts_with("Option<") && s.ends_with('>') => {
            let inner = &s[7..s.len() - 1];
            format!("Option<{}>", ir_type_to_pyo3_type(inner))
        }

        // Result<T, E> — map to just the Ok type (function returns PyResult)
        _ if s.starts_with("Result<") && s.ends_with('>') => {
            let inner = &s[7..s.len() - 1];
            let ok_type = split_type_args_once(inner)
                .map(|(first, _)| first.trim())
                .unwrap_or(inner);
            ir_type_to_pyo3_type(ok_type)
        }

        // HashMap<K, V>
        _ if s.starts_with("HashMap<") && s.ends_with('>') => {
            let inner = &s[8..s.len() - 1];
            if let Some((k, v)) = split_type_args_once(inner) {
                format!(
                    "HashMap<{}, {}>",
                    ir_type_to_pyo3_type(k.trim()),
                    ir_type_to_pyo3_type(v.trim())
                )
            } else {
                "HashMap<String, String>".to_string()
            }
        }

        // HashSet<T>
        _ if s.starts_with("HashSet<") && s.ends_with('>') => {
            let inner = &s[8..s.len() - 1];
            format!("HashSet<{}>", ir_type_to_pyo3_type(inner))
        }

        // Box<T>
        _ if s.starts_with("Box<") && s.ends_with('>') => {
            let inner = &s[4..s.len() - 1];
            ir_type_to_pyo3_type(inner)
        }

        // Arc<T>, Rc<T>
        _ if (s.starts_with("Arc<") || s.starts_with("Rc<")) && s.ends_with('>') => {
            let angle_start = s.find('<').unwrap();
            let inner = &s[angle_start + 1..s.len() - 1];
            ir_type_to_pyo3_type(inner)
        }

        // Cow<'_, T>
        _ if s.starts_with("Cow<") && s.ends_with('>') => {
            let inner = &s[4..s.len() - 1];
            // Cow has lifetime + type param, e.g., Cow<'_, str>
            // Split on comma and take the last element
            let parts: Vec<&str> = inner.split(',').map(|p| p.trim()).collect();
            let inner_type = parts.last().copied().unwrap_or(inner);
            ir_type_to_pyo3_type(inner_type)
        }

        // Tuple types: (A, B)
        _ if s.starts_with('(') && s.ends_with(')') && s.len() > 2 => {
            let inner = &s[1..s.len() - 1];
            let elems: Vec<&str> = split_top_level_commas(inner);
            let mapped: Vec<String> = elems.iter().map(|e| ir_type_to_pyo3_type(e.trim())).collect();
            format!("({})", mapped.join(", "))
        }

        // Slice/array types
        _ if s.starts_with('[') && s.ends_with(']') => {
            let inner = &s[1..s.len() - 1];
            if let Some(semi_pos) = inner.rfind("; ") {
                let elem = &inner[..semi_pos].trim();
                format!("Vec<{}>", ir_type_to_pyo3_type(elem))
            } else {
                format!("&[{}]", ir_type_to_pyo3_type(inner.trim()))
            }
        }

        // Fallback: treat as a named type reference (struct/enum)
        _ => {
            // If it looks like a generic parameter (single uppercase letter), use PyAny
            let trimmed = s.trim();
            if trimmed.len() == 1 && trimmed.chars().next().unwrap().is_ascii_uppercase() {
                "Bound<'_, PyAny>".to_string()
            } else {
                trimmed.to_string()
            }
        }
    }
}

/// Split a string by commas at the top level (respecting nested brackets).
fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;

    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    if start <= s.len() {
        parts.push(&s[start..]);
    }

    parts
}

/// Split type arguments on the first top-level comma.
fn split_type_args_once(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0u32;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                return Some((&s[..i], &s[i + 1..]));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use vertumnus_inspector::ir::{EnumVariant, FunctionParameter, IrType, IrItemKind, StructField, StructItem, EnumItem};
    use vertumnus_mapper::annotated_ir::{MappingWarning, PyO3Strategy};

    fn make_test_method(
        name: &str,
        inputs: Vec<FunctionParameter>,
        output: &str,
        strategy: PyO3Strategy,
    ) -> (FunctionItem, PyO3Strategy) {
        (
            FunctionItem {
                kind: IrItemKind::Function,
                name: name.to_string(),
                doc: String::new(),
                inputs,
                output: IrType { type_str: output.to_string() },
                is_unsafe: false,
                is_async: false,
                has_generics: false,
                visibility: "public".to_string(),
            },
            strategy,
        )
    }

    #[test]
    fn test_ir_type_to_pyo3_primitives() {
        assert_eq!(ir_type_to_pyo3_type("i32"), "i32");
        assert_eq!(ir_type_to_pyo3_type("f64"), "f64");
        assert_eq!(ir_type_to_pyo3_type("bool"), "bool");
        assert_eq!(ir_type_to_pyo3_type("String"), "String");
        assert_eq!(ir_type_to_pyo3_type("&str"), "&str");
        assert_eq!(ir_type_to_pyo3_type("()"), "()");
    }

    #[test]
    fn test_ir_type_to_pyo3_vec() {
        assert_eq!(ir_type_to_pyo3_type("Vec<f64>"), "Vec<f64>");
        assert_eq!(ir_type_to_pyo3_type("Vec<Vec<i32>>"), "Vec<Vec<i32>>");
    }

    #[test]
    fn test_ir_type_to_pyo3_option() {
        assert_eq!(ir_type_to_pyo3_type("Option<i32>"), "Option<i32>");
    }

    #[test]
    fn test_ir_type_to_pyo3_result() {
        // Result<T,E> maps to just T
        assert_eq!(ir_type_to_pyo3_type("Result<i64, String>"), "i64");
    }

    #[test]
    fn test_ir_type_to_pyo3_refs() {
        assert_eq!(ir_type_to_pyo3_type("&Point"), "&Point");
        assert_eq!(ir_type_to_pyo3_type("&mut Point"), "&mut Point");
        assert_eq!(ir_type_to_pyo3_type("&'a Point"), "&Point");
        assert_eq!(ir_type_to_pyo3_type("&'a mut Point"), "&mut Point");
    }

    #[test]
    fn test_extract_result_ok_type() {
        assert_eq!(extract_result_ok_type("Result<i64, String>"), Some("i64".to_string()));
        assert_eq!(extract_result_ok_type("Result<Vec<f64>, String>"), Some("Vec<f64>".to_string()));
        assert_eq!(extract_result_ok_type("i64"), None);
    }

    #[test]
    fn test_determine_receiver() {
        let make_func = |inputs: Vec<FunctionParameter>| -> FunctionItem {
            FunctionItem {
                kind: IrItemKind::Function,
                name: "test".to_string(),
                doc: String::new(),
                inputs,
                output: IrType { type_str: "()".to_string() },
                is_unsafe: false,
                is_async: false,
                has_generics: false,
                visibility: "public".to_string(),
            }
        };

        assert_eq!(determine_receiver(&make_func(vec![
            FunctionParameter { name: "self".to_string(), type_str: "&Point".to_string() },
        ])), Receiver::Ref);

        assert_eq!(determine_receiver(&make_func(vec![
            FunctionParameter { name: "self".to_string(), type_str: "&mut Point".to_string() },
        ])), Receiver::MutRef);

        assert_eq!(determine_receiver(&make_func(vec![
            FunctionParameter { name: "self".to_string(), type_str: "Point".to_string() },
        ])), Receiver::Value);

        assert_eq!(determine_receiver(&make_func(vec![])), Receiver::None);

        assert_eq!(determine_receiver(&make_func(vec![
            FunctionParameter { name: "x".to_string(), type_str: "i32".to_string() },
        ])), Receiver::None);
    }

    #[test]
    fn test_function_wrapper_generation() {
        let func = FunctionItem {
            kind: IrItemKind::Function,
            name: "add".to_string(),
            doc: "Adds two numbers.".to_string(),
            inputs: vec![
                FunctionParameter { name: "a".to_string(), type_str: "i64".to_string() },
                FunctionParameter { name: "b".to_string(), type_str: "i64".to_string() },
            ],
            output: IrType { type_str: "i64".to_string() },
            is_unsafe: false,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
        };

        let mapping = TypeMapping {
            python_type: "(int, int) -> int".to_string(),
            pyo3_strategy: PyO3Strategy::PyFunction,
            warnings: vec![],
        };

        let code = generate_function_wrapper(&func, &mapping, &HashSet::new());
        assert!(code.contains("#[pyfunction]"));
        assert!(code.contains("pub fn add("));
        assert!(code.contains("a: i64"));
        assert!(code.contains("b: i64"));
        assert!(code.contains("_crate::add(a, b)"));
    }

    #[test]
    fn test_function_wrapper_manual_stub() {
        let func = FunctionItem {
            kind: IrItemKind::Function,
            name: "unsafe_fn".to_string(),
            doc: String::new(),
            inputs: vec![],
            output: IrType { type_str: "()".to_string() },
            is_unsafe: true,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
        };

        let mapping = TypeMapping {
            python_type: "() -> None".to_string(),
            pyo3_strategy: PyO3Strategy::ManualStub,
            warnings: vec![
                MappingWarning {
                    message: "Function is unsafe".to_string(),
                    location: "unsafe_fn".to_string(),
                },
            ],
        };

        let code = generate_function_wrapper(&func, &mapping, &HashSet::new());
        assert!(code.contains("VERTUMNUS: manual binding required"));
        assert!(code.contains("todo!"));
    }

    #[test]
    fn test_function_wrapper_result() {
        let func = FunctionItem {
            kind: IrItemKind::Function,
            name: "safe_div".to_string(),
            doc: String::new(),
            inputs: vec![
                FunctionParameter { name: "a".to_string(), type_str: "i64".to_string() },
                FunctionParameter { name: "b".to_string(), type_str: "i64".to_string() },
            ],
            output: IrType { type_str: "Result<i64, String>".to_string() },
            is_unsafe: false,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
        };

        let mapping = TypeMapping {
            python_type: "(int, int) -> int".to_string(),
            pyo3_strategy: PyO3Strategy::MapErr,
            warnings: vec![],
        };

        let code = generate_function_wrapper(&func, &mapping, &HashSet::new());
        assert!(code.contains("PyResult<i64>"));
        assert!(code.contains("map_err"));
        assert!(code.contains("PyRuntimeError"));
    }

    #[test]
    fn test_struct_wrapper_generation() {
        let s = StructItem {
            kind: IrItemKind::Struct,
            name: "Point".to_string(),
            doc: "A 2D point.".to_string(),
            fields: vec![
                StructField { name: "x".to_string(), type_str: "f64".to_string(), visibility: FieldVisibility::Public },
                StructField { name: "y".to_string(), type_str: "f64".to_string(), visibility: FieldVisibility::Public },
            ],
            methods: vec![],
            has_lifetimes: false,
            has_generics: false,
        };

        let mapping = TypeMapping {
            python_type: "Point { x: float, y: float }".to_string(),
            pyo3_strategy: PyO3Strategy::PyClass,
            warnings: vec![],
        };

        let code = generate_struct_wrapper(&s, &[], &mapping, true, true, &HashSet::new());
        assert!(code.contains("#[pyclass]"));
        assert!(code.contains("pub struct Point {"));
        assert!(code.contains("inner: _crate::Point,"));
        assert!(code.contains("#[getter]"));
        assert!(code.contains("fn x("));
        assert!(code.contains("fn y("));
    }

    #[test]
    fn test_enum_wrapper_generation() {
        let e = EnumItem {
            kind: IrItemKind::Enum,
            name: "Direction".to_string(),
            doc: "Directions.".to_string(),
            variants: vec![
                EnumVariant { name: "North".to_string(), fields: vec![], discriminant: None },
                EnumVariant { name: "South".to_string(), fields: vec![], discriminant: None },
            ],
            methods: vec![],
            has_lifetimes: false,
            has_generics: false,
        };

        let mapping = TypeMapping {
            python_type: "Direction".to_string(),
            pyo3_strategy: PyO3Strategy::PyEnum,
            warnings: vec![],
        };

        let code = generate_enum_wrapper(&e, &[], &mapping, &HashSet::new());
        assert!(code.contains("#[pyclass(eq, eq_int)]"));
        assert!(code.contains("pub enum Direction {"));
        assert!(code.contains("North,"));
        assert!(code.contains("South,"));
    }

    #[test]
    fn test_struct_with_methods() {
        let s = StructItem {
            kind: IrItemKind::Struct,
            name: "Point".to_string(),
            doc: "A 2D point.".to_string(),
            fields: vec![
                StructField { name: "x".to_string(), type_str: "f64".to_string(), visibility: FieldVisibility::Public },
            ],
            methods: vec![],
            has_lifetimes: false,
            has_generics: false,
        };

        let methods = vec![
            make_test_method(
                "new",
                vec![
                    FunctionParameter { name: "x".to_string(), type_str: "f64".to_string() },
                    FunctionParameter { name: "y".to_string(), type_str: "f64".to_string() },
                ],
                "Self",
                PyO3Strategy::PyClass,
            ),
            make_test_method(
                "distance",
                vec![
                    FunctionParameter { name: "self".to_string(), type_str: "&Point".to_string() },
                    FunctionParameter { name: "other".to_string(), type_str: "&Point".to_string() },
                ],
                "f64",
                PyO3Strategy::PyClass,
            ),
        ];

        let mapping = TypeMapping {
            python_type: "Point".to_string(),
            pyo3_strategy: PyO3Strategy::PyClass,
            warnings: vec![],
        };

        let code = generate_struct_wrapper(&s, &methods, &mapping, true, true, &HashSet::new());
        assert!(code.contains("#[new]"));
        assert!(code.contains("fn new("));
        assert!(code.contains("fn distance(&self"));
        assert!(code.contains("other: &Point"));
    }
}
