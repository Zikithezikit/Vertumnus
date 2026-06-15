//! Auto-detect monomorphization from public API.
//!
//! Scans the IR for concrete usages of generic types and generates concrete
//! wrapper items so that e.g. `Container<String>` becomes a concrete Python
//! class rather than a `ManualStub`.
//!
//! Approach:
//! 1. Scan all items for concrete generic usages (e.g., `Container<String>`)
//! 2. Match against generic structs/enums defined in the crate
//! 3. Generate new concrete wrapper items with substituted types

use std::collections::{HashMap, HashSet};

use vertumnus_inspector::ir::{
    EnumItem, EnumVariant, IntermediateRepresentation, IrItem, IrItemKind, StructField, StructItem,
};

use crate::annotated_ir::{AnnotatedItem, MappingWarning, PyO3Strategy, TypeMapping};

/// Run monomorphization detection and inject concrete wrapper items.
///
/// Returns additional `AnnotatedItem`s for each detected concrete instantiation
/// of a generic type.
///
/// `exclude_keys` — a set of `"Container<String>"`-style keys to skip, used
/// to avoid duplicating user-provided monomorphizations (B2).
pub fn detect_and_generate_concrete_wrappers(
    ir: &IntermediateRepresentation,
    exclude_keys: &HashSet<String>,
) -> Vec<AnnotatedItem> {
    // Step 1: Collect names of generic types defined in this crate
    let generic_types = collect_generic_types(ir);
    if generic_types.is_empty() {
        return Vec::new();
    }

    // Step 2: Scan the IR for concrete usages of those generic types
    let concrete_usages = scan_concrete_usages(ir, &generic_types);
    if concrete_usages.is_empty() {
        return Vec::new();
    }

    // Step 3: For each concrete instantiation, generate a wrapper item
    let mut generated = Vec::new();

    for (type_name, args_set) in &concrete_usages {
        // Find the original generic item
        let original_item = ir.items.iter().find(|item| item.name() == type_name);

        let Some(original) = original_item else {
            continue;
        };

        for args in args_set {
            // Build the exclusion key (e.g., "Container<String>") to check
            // against user-provided monomorphization hints (B2)
            let exclusion_key = format!("{}<{}>", type_name, args.join(", "));
            if exclude_keys.contains(&exclusion_key) {
                continue;
            }

            let wrapper_name = generate_concrete_name(type_name, args);

            match original {
                IrItem::Struct(s) => {
                    if let Some(wrapper) =
                        generate_concrete_struct(s, type_name, args, &wrapper_name)
                    {
                        // Create an AnnotatedItem for this concrete wrapper
                        let annotated = AnnotatedItem {
                            original: IrItem::Struct(wrapper),
                            mapping: TypeMapping {
                                python_type: wrapper_name.clone(),
                                pyo3_strategy: PyO3Strategy::PyClass,
                                warnings: vec![MappingWarning {
                                    message: format!(
                                        "Auto-generated concrete wrapper for generic '{}' with args [{}]",
                                        type_name,
                                        args.join(", ")
                                    ),
                                    location: wrapper_name.clone(),
                                }],
                            },
                        };
                        generated.push(annotated);
                    }
                }
                IrItem::Enum(e) => {
                    if let Some(wrapper) = generate_concrete_enum(e, type_name, args, &wrapper_name)
                    {
                        let annotated = AnnotatedItem {
                            original: IrItem::Enum(wrapper),
                            mapping: TypeMapping {
                                python_type: wrapper_name.clone(),
                                pyo3_strategy: PyO3Strategy::PyEnum,
                                warnings: vec![MappingWarning {
                                    message: format!(
                                        "Auto-generated concrete wrapper for generic enum '{}' with args [{}]",
                                        type_name,
                                        args.join(", ")
                                    ),
                                    location: wrapper_name.clone(),
                                }],
                            },
                        };
                        generated.push(annotated);
                    }
                }
                _ => {}
            }
        }
    }

    generated
}

/// Process user-provided monomorphization hints from the config.
///
/// This allows users to explicitly specify concrete instantiations
/// of generic types in `.vertumnus/config.toml`, complementing the
/// auto-detection in `detect_and_generate_concrete_wrappers`.
///
/// Keys are like `"Container<String>"` and values specify the Python
/// wrapper name and strategy.
pub fn process_user_monomorphizations(
    ir: &IntermediateRepresentation,
    hints: &HashMap<String, crate::config::MonomorphizeEntry>,
) -> Vec<AnnotatedItem> {
    if hints.is_empty() {
        return Vec::new();
    }

    let mut generated: Vec<AnnotatedItem> = Vec::new();

    for (key, entry) in hints {
        // Parse the monomorphization key
        let Some((base_type, args)) = crate::config::VertumnusConfig::parse_monomorphize_key(key)
        else {
            // Invalid key — skip with a warning (will be handled by caller)
            continue;
        };

        // Find the original generic item in the IR
        let original_item = ir.items.iter().find(|item| item.name() == base_type);
        let Some(original) = original_item else {
            continue;
        };

        let wrapper_name = &entry.python;
        let strategy = crate::config::VertumnusConfig::parse_strategy(&entry.strategy);
        let wrapper_parts: Vec<String> = args.iter().map(|s| s.to_string()).collect();

        match original {
            IrItem::Struct(s) => {
                if let Some(wrapper) =
                    generate_concrete_struct(s, base_type, &wrapper_parts, wrapper_name)
                {
                    let annotated = AnnotatedItem {
                        original: IrItem::Struct(wrapper),
                        mapping: TypeMapping {
                            python_type: wrapper_name.clone(),
                            pyo3_strategy: strategy,
                            warnings: vec![MappingWarning {
                                message: format!(
                                    "User-provided concrete wrapper for generic '{}' with args [{}]",
                                    base_type,
                                    args.join(", ")
                                ),
                                location: wrapper_name.clone(),
                            }],
                        },
                    };
                    generated.push(annotated);
                }
            }
            IrItem::Enum(e) => {
                if let Some(wrapper) =
                    generate_concrete_enum(e, base_type, &wrapper_parts, wrapper_name)
                {
                    let annotated = AnnotatedItem {
                        original: IrItem::Enum(wrapper),
                        mapping: TypeMapping {
                            python_type: wrapper_name.clone(),
                            pyo3_strategy: strategy,
                            warnings: vec![MappingWarning {
                                message: format!(
                                    "User-provided concrete wrapper for generic enum '{}' with args [{}]",
                                    base_type,
                                    args.join(", ")
                                ),
                                location: wrapper_name.clone(),
                            }],
                        },
                    };
                    generated.push(annotated);
                }
            }
            _ => {}
        }
    }

    generated
}

// ---------------------------------------------------------------------------
// Step 1: Collect generic types defined in the crate
// ---------------------------------------------------------------------------

fn collect_generic_types(ir: &IntermediateRepresentation) -> HashSet<String> {
    ir.items
        .iter()
        .filter_map(|item| match item {
            IrItem::Struct(s) if s.has_generics => Some(s.name.clone()),
            IrItem::Enum(e) if e.has_generics => Some(e.name.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Step 2: Scan concrete usages
// ---------------------------------------------------------------------------

/// Scan the IR for concrete usages of the given generic types.
///
/// Returns a map from generic type name to a set of concrete type argument lists.
fn scan_concrete_usages(
    ir: &IntermediateRepresentation,
    generic_types: &HashSet<String>,
) -> HashMap<String, HashSet<Vec<String>>> {
    let mut usages: HashMap<String, HashSet<Vec<String>>> = HashMap::new();

    for item in &ir.items {
        match item {
            IrItem::Function(f) => {
                // Scan inputs
                for param in &f.inputs {
                    detect_generic_usages(&param.type_str, generic_types, &mut usages);
                }
                // Scan output
                detect_generic_usages(&f.output.type_str, generic_types, &mut usages);
            }
            IrItem::Struct(s) => {
                // Scan fields (method types are also relevant)
                for field in &s.fields {
                    detect_generic_usages(&field.type_str, generic_types, &mut usages);
                }
                for method in &s.methods {
                    for param in &method.inputs {
                        detect_generic_usages(&param.type_str, generic_types, &mut usages);
                    }
                    detect_generic_usages(&method.output.type_str, generic_types, &mut usages);
                }
            }
            IrItem::Enum(e) => {
                for variant in &e.variants {
                    for field in &variant.fields {
                        detect_generic_usages(&field.type_str, generic_types, &mut usages);
                    }
                }
                for method in &e.methods {
                    for param in &method.inputs {
                        detect_generic_usages(&param.type_str, generic_types, &mut usages);
                    }
                    detect_generic_usages(&method.output.type_str, generic_types, &mut usages);
                }
            }
            IrItem::Trait(t) => {
                for method in &t.methods {
                    for param in &method.inputs {
                        detect_generic_usages(&param.type_str, generic_types, &mut usages);
                    }
                    detect_generic_usages(&method.output.type_str, generic_types, &mut usages);
                }
            }
            IrItem::Impl(i) => {
                for method in &i.methods {
                    for param in &method.inputs {
                        detect_generic_usages(&param.type_str, generic_types, &mut usages);
                    }
                    detect_generic_usages(&method.output.type_str, generic_types, &mut usages);
                }
            }
        }
    }

    usages
}

/// Parse a type string and detect usages of known generic types with concrete args.
fn detect_generic_usages(
    type_str: &str,
    generic_types: &HashSet<String>,
    usages: &mut HashMap<String, HashSet<Vec<String>>>,
) {
    // Skip primitives and simple types
    let trimmed = type_str.trim();
    if trimmed.is_empty() || !trimmed.contains('<') {
        return;
    }

    // Walk the type string to find all generic instantiations
    // We use a simple approach: find patterns like `Name<Args>`
    let chars: Vec<char> = trimmed.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Look for '<'
        if chars[i] != '<' {
            i += 1;
            continue;
        }

        // Scan backwards to find the start of the type name
        let name_end = i;
        let mut name_start = i;
        while name_start > 0 {
            name_start -= 1;
            let c = chars[name_start];
            if !c.is_alphanumeric() && c != '_' && c != ':' {
                name_start += 1;
                break;
            }
        }
        let type_name: String = chars[name_start..name_end].iter().collect();
        let type_name = type_name.trim();

        // Skip if followed by :: (it's a path separator, not generic)
        if i + 1 < len && chars[i + 1] == ':' {
            i += 1;
            continue;
        }

        // Check against our known generic types
        if !type_name.is_empty() && generic_types.contains(type_name) {
            // Find matching '>'
            let mut depth = 1u32;
            let mut j = i + 1;
            while j < len && depth > 0 {
                match chars[j] {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }

            if depth == 0 {
                let args_str: String = chars[i + 1..j - 1].iter().collect();
                let args: Vec<String> = split_top_level_args(&args_str);
                // Filter out generic parameter placeholders (single uppercase letters)
                let concrete_args: Vec<String> = args
                    .into_iter()
                    .filter(|a| !is_single_uppercase(a))
                    .collect();
                if !concrete_args.is_empty() {
                    usages
                        .entry(type_name.to_string())
                        .or_default()
                        .insert(concrete_args);
                }
            }

            i = j; // Skip past the closing >
            continue;
        }
        i += 1;
    }
}

/// Check if a string is a single uppercase letter (generic parameter placeholder).
fn is_single_uppercase(s: &str) -> bool {
    let s = s.trim();
    if s.len() == 1 {
        let c = s.chars().next().unwrap();
        return c.is_ascii_uppercase();
    }
    false
}

/// Split top-level comma-separated arguments, respecting nested angle brackets.
fn split_top_level_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut depth = 0u32;
    let mut start = 0usize;

    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let arg = s[start..i].trim().to_string();
                if !arg.is_empty() {
                    args.push(arg);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    let remaining = s[start..].trim().to_string();
    if !remaining.is_empty() {
        args.push(remaining);
    }

    args
}

// ---------------------------------------------------------------------------
// Naming: generate a Python-safe concrete wrapper name
// ---------------------------------------------------------------------------

/// Generate a Python-safe concrete wrapper name.
///
/// Examples:
/// - `Container<[String]>` → `Container_String`
/// - `Container<[i64]>` → `Container_i64`
/// - `Map<[String, i64]>` → `Container_String_i64`
fn generate_concrete_name(base_name: &str, args: &[String]) -> String {
    let sanitized_base = sanitize_python_name(base_name);
    let sanitized_args: Vec<String> = args.iter().map(|a| sanitize_type_for_name(a)).collect();
    if sanitized_args.is_empty() {
        sanitized_base
    } else {
        format!("{}_{}", sanitized_base, sanitized_args.join("_"))
    }
}

/// Sanitize a name to be Python-safe (replace non-alphanumeric with underscores).
fn sanitize_python_name(name: &str) -> String {
    // Handle path separators: replace `::` with `_`
    let name = name.replace("::", "_");
    // Strip leading/trailing underscores
    let name = name.trim_matches('_').to_string();
    if name.is_empty() {
        "Generic".to_string()
    } else {
        name.to_string()
    }
}

/// Sanitize a type string for use in a Python identifier.
/// Takes the "simple name" — the last path component.
fn sanitize_type_for_name(type_str: &str) -> String {
    let trimmed = type_str.trim();

    // Handle types with angle brackets: take the outer name
    if let Some(angle_pos) = trimmed.find('<') {
        let base = &trimmed[..angle_pos];
        let inner = &trimmed[angle_pos + 1..trimmed.rfind('>').unwrap_or(trimmed.len())];
        let inner_args = split_top_level_args(inner);
        let sanitized_inner: Vec<String> = inner_args
            .iter()
            .map(|a| sanitize_type_for_name(a))
            .collect();
        let base_simple = simple_name(base);
        if sanitized_inner.is_empty() {
            base_simple
        } else {
            format!("{}_{}", base_simple, sanitized_inner.join("_"))
        }
    }
    // Handle tuples
    else if trimmed.starts_with('(') {
        let inner = &trimmed[1..trimmed.rfind(')').unwrap_or(trimmed.len())];
        let args = split_top_level_args(inner);
        let sanitized: Vec<String> = args.iter().map(|a| sanitize_type_for_name(a)).collect();
        format!("tuple_{}", sanitized.join("_"))
    }
    // Handle references
    else if trimmed.starts_with('&') {
        let inner = trimmed.trim_start_matches('&').trim();
        // Skip lifetime
        let inner = if inner.starts_with('\'') {
            if let Some(pos) = inner.find(|c: char| c.is_whitespace()) {
                inner[pos..].trim()
            } else {
                inner
            }
        } else {
            inner
        };
        let inner = inner.trim_start_matches("mut ").trim();
        sanitize_type_for_name(inner)
    }
    // Simple type: extract the last path segment
    else {
        simple_name(trimmed)
    }
}

/// Extract the simple name from a potentially qualified type string.
fn simple_name(s: &str) -> String {
    let s = s.trim();
    // Take the last segment after ::
    if let Some(pos) = s.rfind("::") {
        let last = &s[pos + 2..];
        // Further strip any generic suffix
        if let Some(angle) = last.find('<') {
            last[..angle].to_string()
        } else {
            last.to_string()
        }
    } else {
        // Strip generic suffix
        if let Some(angle) = s.find('<') {
            s[..angle].to_string()
        } else {
            s.to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Step 3: Generate concrete wrapper items
// ---------------------------------------------------------------------------

/// Generate a concrete struct by substituting generic params with concrete types.
fn generate_concrete_struct(
    original: &StructItem,
    type_name: &str,
    args: &[String],
    wrapper_name: &str,
) -> Option<StructItem> {
    // Determine generic param names from original struct's fields
    let param_names = extract_type_params_from_fields(&original.fields);

    if param_names.is_empty() {
        // No generic params found in fields — can't substitute
        return None;
    }

    // Build substitution map
    let sub_map: HashMap<&str, &str> = param_names
        .iter()
        .zip(args.iter())
        .map(|(param, arg)| (param.as_str(), arg.as_str()))
        .collect();

    let new_fields: Vec<StructField> = original
        .fields
        .iter()
        .map(|f| {
            let new_type = substitute_type_params(&f.type_str, &sub_map);
            StructField {
                name: f.name.clone(),
                type_str: new_type,
                visibility: f.visibility.clone(),
            }
        })
        .collect();

    Some(StructItem {
        kind: IrItemKind::Struct,
        name: wrapper_name.to_string(),
        doc: format!(
            "Auto-generated concrete wrapper for generic struct '{}<{}>'",
            type_name,
            args.join(", ")
        ),
        fields: new_fields,
        methods: Vec::new(), // Methods not transferred (they reference generic params)
        has_lifetimes: false,
        has_generics: false,
        generic_params: vec![],
    })
}

/// Generate a concrete enum by substituting generic params with concrete types.
fn generate_concrete_enum(
    original: &EnumItem,
    type_name: &str,
    args: &[String],
    wrapper_name: &str,
) -> Option<EnumItem> {
    // Extract generic param names from variant fields
    let param_names: Vec<String> = original
        .variants
        .iter()
        .flat_map(|v| extract_type_params_from_fields(&v.fields))
        .collect();

    if param_names.is_empty() {
        return None;
    }

    let sub_map: HashMap<&str, &str> = param_names
        .iter()
        .zip(args.iter().cycle())
        .map(|(param, arg)| (param.as_str(), arg.as_str()))
        .collect();

    let new_variants: Vec<EnumVariant> = original
        .variants
        .iter()
        .map(|v| {
            let new_fields: Vec<StructField> = v
                .fields
                .iter()
                .map(|f| {
                    let new_type = substitute_type_params(&f.type_str, &sub_map);
                    StructField {
                        name: f.name.clone(),
                        type_str: new_type,
                        visibility: f.visibility.clone(),
                    }
                })
                .collect();
            EnumVariant {
                name: v.name.clone(),
                fields: new_fields,
                discriminant: v.discriminant.clone(),
            }
        })
        .collect();

    Some(EnumItem {
        kind: IrItemKind::Enum,
        name: wrapper_name.to_string(),
        doc: format!(
            "Auto-generated concrete wrapper for generic enum '{}<{}>'",
            type_name,
            args.join(", ")
        ),
        variants: new_variants,
        methods: Vec::new(),
        has_lifetimes: false,
        has_generics: false,
        generic_params: vec![],
    })
}

/// Extract generic type parameter names from struct fields.
fn extract_type_params_from_fields(fields: &[StructField]) -> Vec<String> {
    let mut params: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for field in fields {
        for word in field
            .type_str
            .split(&['<', '>', ',', ' ', '(', ')', '[', ']', ':', ';'])
        {
            let word = word.trim();
            if word.len() == 1 {
                let c = word.chars().next().unwrap_or('_');
                if c.is_ascii_uppercase() && c != '_' && !seen.contains(word) {
                    // Only treat as generic param if it's not a known type
                    if !is_known_type(word) {
                        seen.insert(word.to_string());
                        params.push(word.to_string());
                    }
                }
            }
        }
    }

    params
}

/// Check if a word is a known Rust type (not a generic parameter).
fn is_known_type(s: &str) -> bool {
    matches!(
        s,
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "str"
            | "String"
            | "Vec"
            | "Option"
            | "Result"
            | "HashMap"
            | "HashSet"
            | "Box"
            | "Rc"
            | "Arc"
            | "Cow"
            | "Self"
            | "Send"
            | "Sync"
    )
}

// ---------------------------------------------------------------------------
// Type parameter substitution
// ---------------------------------------------------------------------------

/// Substitute generic type parameters in a type string.
///
/// E.g., with map {"T": "String"}, `Option<T>` becomes `Option<String>`.
fn substitute_type_params(type_str: &str, sub_map: &HashMap<&str, &str>) -> String {
    let mut result = String::new();
    let chars: Vec<char> = type_str.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Check for an identifier
        if chars[i].is_ascii_uppercase() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();

            // Check if this word is a generic param to substitute
            if let Some(replacement) = sub_map.get(word.as_str()) {
                result.push_str(replacement);
            } else {
                result.push_str(&word);
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use vertumnus_inspector::ir::{FieldVisibility, FunctionItem, FunctionParameter, IrType};

    fn make_simple_ir() -> IntermediateRepresentation {
        IntermediateRepresentation::new("test_crate".to_string(), "1.0.0".to_string())
    }

    #[test]
    fn test_collect_generic_types_empty() {
        let ir = make_simple_ir();
        let types = collect_generic_types(&ir);
        assert!(types.is_empty());
    }

    #[test]
    fn test_collect_generic_types_with_generic_struct() {
        let mut ir = make_simple_ir();
        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Container".to_string(),
            doc: "".to_string(),
            fields: vec![StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            }],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        }));
        let types = collect_generic_types(&ir);
        assert_eq!(types.len(), 1);
        assert!(types.contains("Container"));
    }

    #[test]
    fn test_collect_generic_types_skips_non_generic() {
        let mut ir = make_simple_ir();
        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Point".to_string(),
            doc: "".to_string(),
            fields: vec![],
            methods: vec![],
            has_lifetimes: false,
            has_generics: false,
            generic_params: vec![],
        }));
        let types = collect_generic_types(&ir);
        assert!(types.is_empty());
    }

    #[test]
    fn test_detect_generic_usages_simple() {
        let mut generic_types = HashSet::new();
        generic_types.insert("Container".to_string());

        let mut usages = HashMap::new();
        detect_generic_usages("Container<String>", &generic_types, &mut usages);

        assert_eq!(usages.len(), 1);
        let args = usages.get("Container").unwrap();
        assert_eq!(args.len(), 1);
        let arg_list: &Vec<String> = args.iter().next().unwrap();
        assert_eq!(arg_list[0], "String");
    }

    #[test]
    fn test_detect_generic_usages_nested() {
        let mut generic_types = HashSet::new();
        generic_types.insert("Container".to_string());

        let mut usages = HashMap::new();
        detect_generic_usages("Container<Vec<String>>", &generic_types, &mut usages);

        assert_eq!(usages.len(), 1);
        let args = usages.get("Container").unwrap();
        let arg_list: &Vec<String> = args.iter().next().unwrap();
        assert_eq!(arg_list[0], "Vec<String>");
    }

    #[test]
    fn test_detect_generic_usages_multiple_args() {
        let mut generic_types = HashSet::new();
        generic_types.insert("Map".to_string());

        let mut usages = HashMap::new();
        detect_generic_usages("Map<String, i64>", &generic_types, &mut usages);

        let args = usages.get("Map").unwrap();
        let arg_list: &Vec<String> = args.iter().next().unwrap();
        assert_eq!(arg_list.len(), 2);
        assert_eq!(arg_list[0], "String");
        assert_eq!(arg_list[1], "i64");
    }

    #[test]
    fn test_detect_generic_usages_skips_generic_params() {
        let mut generic_types = HashSet::new();
        generic_types.insert("Container".to_string());

        let mut usages = HashMap::new();
        // T is a generic param — should be filtered out
        detect_generic_usages("Container<T>", &generic_types, &mut usages);

        assert!(usages.is_empty() || usages.get("Container").unwrap().is_empty());
    }

    #[test]
    fn test_sanitize_type_for_name() {
        assert_eq!(sanitize_type_for_name("String"), "String");
        assert_eq!(sanitize_type_for_name("std::string::String"), "String");
        assert_eq!(sanitize_type_for_name("Vec<i64>"), "Vec_i64");
        assert_eq!(sanitize_type_for_name("&str"), "str");
        assert_eq!(sanitize_type_for_name("(i32, String)"), "tuple_i32_String");
    }

    #[test]
    fn test_generate_concrete_name() {
        assert_eq!(
            generate_concrete_name("Container", &["String".to_string()]),
            "Container_String"
        );
        assert_eq!(
            generate_concrete_name("Container", &["i64".to_string()]),
            "Container_i64"
        );
        assert_eq!(
            generate_concrete_name("Map", &["String".to_string(), "i64".to_string()]),
            "Map_String_i64"
        );
    }

    #[test]
    fn test_substitute_type_params_simple() {
        let mut map = HashMap::new();
        map.insert("T", "String");
        assert_eq!(substitute_type_params("T", &map), "String");
        assert_eq!(substitute_type_params("Option<T>", &map), "Option<String>");
        assert_eq!(substitute_type_params("Vec<T>", &map), "Vec<String>");
    }

    #[test]
    fn test_substitute_type_params_no_substitution() {
        let map: HashMap<&str, &str> = HashMap::new();
        assert_eq!(substitute_type_params("i64", &map), "i64");
        assert_eq!(substitute_type_params("String", &map), "String");
    }

    #[test]
    fn test_extract_type_params_from_fields() {
        let fields = vec![
            StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            },
            StructField {
                name: "name".to_string(),
                type_str: "String".to_string(),
                visibility: FieldVisibility::Public,
            },
        ];
        let params = extract_type_params_from_fields(&fields);
        assert_eq!(params, vec!["T"]);
    }

    #[test]
    fn test_generate_concrete_struct() {
        let original = StructItem {
            kind: IrItemKind::Struct,
            name: "Container".to_string(),
            doc: "A generic container.".to_string(),
            fields: vec![StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            }],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        };

        let result = generate_concrete_struct(
            &original,
            "Container",
            &["String".to_string()],
            "Container_String",
        );

        assert!(result.is_some());
        let wrapper = result.unwrap();
        assert_eq!(wrapper.name, "Container_String");
        assert_eq!(wrapper.fields.len(), 1);
        assert_eq!(wrapper.fields[0].type_str, "String");
        assert!(!wrapper.has_generics);
    }

    #[test]
    fn test_scan_concrete_usages_full_scan() {
        let mut ir = make_simple_ir();
        let mut generic_types = HashSet::new();
        generic_types.insert("Container".to_string());

        ir.items.push(IrItem::Function(FunctionItem {
            kind: IrItemKind::Function,
            name: "make_container".to_string(),
            doc: "".to_string(),
            inputs: vec![FunctionParameter {
                name: "value".to_string(),
                type_str: "String".to_string(),
            }],
            output: IrType {
                type_str: "Container<String>".to_string(),
            },
            is_unsafe: false,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
            generic_params: vec![],
        }));

        let usages = scan_concrete_usages(&ir, &generic_types);
        assert_eq!(usages.len(), 1);
        let args = usages.get("Container").unwrap();
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn test_detect_and_generate_concrete_wrappers_integration() {
        let mut ir = make_simple_ir();

        // Generic struct
        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Container".to_string(),
            doc: "".to_string(),
            fields: vec![StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            }],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        }));

        // Function that uses Container<String>
        ir.items.push(IrItem::Function(FunctionItem {
            kind: IrItemKind::Function,
            name: "make_container".to_string(),
            doc: "".to_string(),
            inputs: vec![],
            output: IrType {
                type_str: "Container<String>".to_string(),
            },
            is_unsafe: false,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
            generic_params: vec![],
        }));

        let wrappers = detect_and_generate_concrete_wrappers(&ir, &HashSet::new());
        assert_eq!(wrappers.len(), 1);
        assert_eq!(wrappers[0].mapping.python_type, "Container_String");

        // Verify the generated struct item
        if let IrItem::Struct(s) = &wrappers[0].original {
            assert_eq!(s.name, "Container_String");
            assert_eq!(s.fields[0].type_str, "String");
        } else {
            panic!("Expected a struct item");
        }
    }

    #[test]
    fn test_detect_and_generate_no_generics() {
        let ir = make_simple_ir();
        let wrappers = detect_and_generate_concrete_wrappers(&ir, &HashSet::new());
        assert!(wrappers.is_empty());
    }

    #[test]
    fn test_detect_generic_usages_in_multiple_locations() {
        let mut ir = make_simple_ir();
        let mut generic_types = HashSet::new();
        generic_types.insert("Container".to_string());
        generic_types.insert("Wrapper".to_string());

        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Container".to_string(),
            doc: "".to_string(),
            fields: vec![StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            }],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        }));

        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Wrapper".to_string(),
            doc: "".to_string(),
            fields: vec![
                StructField {
                    name: "key".to_string(),
                    type_str: "K".to_string(),
                    visibility: FieldVisibility::Public,
                },
                StructField {
                    name: "value".to_string(),
                    type_str: "V".to_string(),
                    visibility: FieldVisibility::Public,
                },
            ],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        }));

        // Multiple usages
        ir.items.push(IrItem::Function(FunctionItem {
            kind: IrItemKind::Function,
            name: "process".to_string(),
            doc: "".to_string(),
            inputs: vec![
                FunctionParameter {
                    name: "c".to_string(),
                    type_str: "Container<String>".to_string(),
                },
                FunctionParameter {
                    name: "w".to_string(),
                    type_str: "Wrapper<String, i64>".to_string(),
                },
            ],
            output: IrType {
                type_str: "Container<i64>".to_string(),
            },
            is_unsafe: false,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
            generic_params: vec![],
        }));

        let wrappers = detect_and_generate_concrete_wrappers(&ir, &HashSet::new());
        // Should generate: Container_String, Container_i64, Wrapper_String_i64
        assert_eq!(wrappers.len(), 3);

        let wrapper_names: Vec<&str> = wrappers
            .iter()
            .map(|w| w.mapping.python_type.as_str())
            .collect();
        assert!(wrapper_names.contains(&"Container_String"));
        assert!(wrapper_names.contains(&"Container_i64"));
        assert!(wrapper_names.contains(&"Wrapper_String_i64"));
    }

    #[test]
    fn test_process_user_monomorphizations_struct() {
        let mut ir = make_simple_ir();

        // Generic struct
        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Container".to_string(),
            doc: "".to_string(),
            fields: vec![StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            }],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        }));

        // User-provided hints
        let mut hints = HashMap::new();
        hints.insert(
            "Container<String>".to_string(),
            crate::config::MonomorphizeEntry {
                python: "MyStringContainer".to_string(),
                strategy: "pyclass".to_string(),
            },
        );
        hints.insert(
            "Container<i64>".to_string(),
            crate::config::MonomorphizeEntry {
                python: "MyIntContainer".to_string(),
                strategy: "pyclass".to_string(),
            },
        );

        let wrappers = process_user_monomorphizations(&ir, &hints);
        assert_eq!(wrappers.len(), 2);

        let wrapper_names: Vec<&str> = wrappers
            .iter()
            .map(|w| w.mapping.python_type.as_str())
            .collect();
        assert!(wrapper_names.contains(&"MyStringContainer"));
        assert!(wrapper_names.contains(&"MyIntContainer"));

        // Verify strategies
        for wrapper in &wrappers {
            assert_eq!(wrapper.mapping.pyo3_strategy, PyO3Strategy::PyClass);
            assert!(wrapper.mapping.warnings[0]
                .message
                .contains("User-provided"));
        }
    }

    #[test]
    fn test_user_monomorphizations_empty_hints() {
        let ir = make_simple_ir();
        let hints = HashMap::new();
        let wrappers = process_user_monomorphizations(&ir, &hints);
        assert!(wrappers.is_empty());
    }

    #[test]
    fn test_user_monomorphizations_take_priority() {
        // Test that user-provided wrappers take priority over auto-detected ones
        // by checking that `map_ir_with_full_context` prefers user wrappers.
        let mut ir = make_simple_ir();

        // Generic struct
        ir.items.push(IrItem::Struct(StructItem {
            kind: IrItemKind::Struct,
            name: "Container".to_string(),
            doc: "".to_string(),
            fields: vec![StructField {
                name: "value".to_string(),
                type_str: "T".to_string(),
                visibility: FieldVisibility::Public,
            }],
            methods: vec![],
            has_lifetimes: false,
            has_generics: true,
            generic_params: vec![],
        }));

        // Function that uses Container<String> (will trigger auto-detect)
        ir.items.push(IrItem::Function(FunctionItem {
            kind: IrItemKind::Function,
            name: "make_container".to_string(),
            doc: "".to_string(),
            inputs: vec![],
            output: IrType {
                type_str: "Container<String>".to_string(),
            },
            is_unsafe: false,
            is_async: false,
            has_generics: false,
            visibility: "public".to_string(),
            generic_params: vec![],
        }));

        // User hint with a different name for the same instantiation
        let mut hints = HashMap::new();
        hints.insert(
            "Container<String>".to_string(),
            crate::config::MonomorphizeEntry {
                python: "UserContainer".to_string(),
                strategy: "pyclass".to_string(),
            },
        );

        let config = crate::config::VertumnusConfig {
            type_mappings: HashMap::new(),
            monomorphize: hints,
        };

        let annotated = crate::mapper::map_ir_with_full_context(&ir, Some(&config), None).unwrap();

        // Should have the user's name, not the auto-detected one
        let names: Vec<&str> = annotated
            .items
            .iter()
            .map(|item| item.mapping.python_type.as_str())
            .collect();
        assert!(
            names.contains(&"UserContainer"),
            "Should contain user-provided name 'UserContainer', got: {:?}",
            names
        );
        assert!(
            !names.contains(&"Container_String"),
            "Should NOT contain auto-detected name 'Container_String'"
        );
    }
}
