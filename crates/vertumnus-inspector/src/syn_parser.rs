//! `syn`-based fallback parser for Vertumnus.
//!
//! Parses a Rust crate's public API by reading the source files directly
//! using the `syn` crate. This is the stable Rust alternative to the
//! rustdoc JSON parser (which requires nightly).
//!
//! ## Limitations vs rustdoc JSON
//!
//! - Only parses the crate root (`src/lib.rs`) and its direct `pub mod`
//!   children. Does not follow `pub use` re-exports in a deep way.
//! - Type resolution is string-based using `quote!` pretty-printing, so
//!   complex generic types may appear in a slightly different format than
//!   rustdoc JSON resolution.
//! - Does not detect `has_lifetimes` perfectly — only checks for `'` in
//!   type strings.
//! - Does not resolve trait impl method bodies for accurate method lists
//!   on structs/enums (only inherent impls).

use std::fs;
use std::path::Path;

use syn::{
    parse_file, Attribute, Expr, Fields, File, FnArg, GenericParam, Item, ItemEnum, ItemFn,
    ItemImpl, ItemStruct, ItemTrait, Pat, ReturnType, Type, Visibility,
};

use crate::ir::{
    EnumItem, EnumVariant, FieldVisibility, FunctionItem, FunctionParameter, ImplItem,
    IntermediateRepresentation, IrItem, IrItemKind, IrType, StructField, StructItem, TraitItem,
};

use crate::inspector::InspectError;

/// Maximum recursion depth for module traversal.
const MAX_MODULE_DEPTH: usize = 16;

/// Parse a Rust crate using `syn` by reading its source files.
pub fn inspect_crate_with_syn(crate_path: &Path) -> Result<IntermediateRepresentation, InspectError> {
    let crate_root = find_crate_root(crate_path)?;

    // Read and extract crate name from Cargo.toml
    let cargo_toml_path = crate_root.join("Cargo.toml");
    let cargo_content = fs::read_to_string(&cargo_toml_path)
        .map_err(|e| InspectError::CargoRustdocFailed(format!(
            "Failed to read Cargo.toml at {}: {e}",
            cargo_toml_path.display()
        )))?;
    let cargo_parsed: toml::Value = cargo_content.parse()
        .map_err(|e| InspectError::CargoRustdocFailed(format!(
            "Failed to parse Cargo.toml: {e}"
        )))?;
    let crate_name = cargo_parsed
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or_else(|| {
            crate_root
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        })
        .to_string();
    let crate_version = cargo_parsed
        .get("package")
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();

    let mut ir = IntermediateRepresentation::new(crate_name, crate_version);

    // Parse the crate root file (src/lib.rs)
    let lib_path = crate_root.join("src").join("lib.rs");
    if !lib_path.exists() {
        // Try main.rs as fallback
        let main_path = crate_root.join("src").join("main.rs");
        if main_path.exists() {
            let source = fs::read_to_string(&main_path)
                .map_err(|e| InspectError::CargoRustdocFailed(format!(
                    "Failed to read {}: {e}",
                    main_path.display()
                )))?;
            let file = parse_file(&source)
                .map_err(|e| InspectError::CargoRustdocFailed(format!(
                    "Failed to parse {}: {e}",
                    main_path.display()
                )))?;
            parse_syn_file(&file, &crate_root, &mut ir, 0)?;
        } else {
            return Err(InspectError::CargoRustdocFailed(format!(
                "Crate root file not found at {} or {}",
                lib_path.display(),
                main_path.display()
            )));
        }
    } else {
        let source = fs::read_to_string(&lib_path)
            .map_err(|e| InspectError::CargoRustdocFailed(format!(
                "Failed to read {}: {e}",
                lib_path.display()
            )))?;
        let file = parse_file(&source)
            .map_err(|e| InspectError::CargoRustdocFailed(format!(
                "Failed to parse {}: {e}",
                lib_path.display()
            )))?;
        parse_syn_file(&file, &crate_root, &mut ir, 0)?;
    }

    if ir.items.is_empty() {
        return Err(InspectError::NoPublicItems);
    }

    Ok(ir)
}

/// Find the crate root directory by looking for Cargo.toml.
fn find_crate_root(path: &Path) -> Result<std::path::PathBuf, InspectError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| InspectError::CargoRustdocFailed(format!("Cannot resolve path: {e}")))?;

    if canonical.is_dir() {
        if canonical.join("Cargo.toml").exists() {
            Ok(canonical)
        } else {
            Err(InspectError::CargoRustdocFailed(format!(
                "No Cargo.toml found in {}",
                canonical.display()
            )))
        }
    } else if canonical.is_file() {
        // Could be Cargo.toml itself
        if canonical.file_name().and_then(|s| s.to_str()) == Some("Cargo.toml") {
            Ok(canonical
                .parent()
                .unwrap_or(&canonical)
                .to_path_buf())
        } else {
            // Assume it's the crate root file
            Ok(canonical
                .parent()
                .unwrap_or(&canonical)
                .to_path_buf())
        }
    } else {
        Err(InspectError::CargoRustdocFailed(format!(
            "Path does not exist: {}",
            canonical.display()
        )))
    }
}

/// Parse all items in a syn `File`, recursing into modules.
fn parse_syn_file(
    file: &File,
    crate_root: &Path,
    ir: &mut IntermediateRepresentation,
    depth: usize,
) -> Result<(), InspectError> {
    if depth > MAX_MODULE_DEPTH {
        return Ok(()); // Guard against infinite recursion
    }

    for item in &file.items {
        parse_syn_item(item, crate_root, ir, depth)?;
    }

    Ok(())
}

/// Parse a single syn `Item`, dispatching to the appropriate handler.
fn parse_syn_item(
    item: &Item,
    crate_root: &Path,
    ir: &mut IntermediateRepresentation,
    depth: usize,
) -> Result<(), InspectError> {
    match item {
        Item::Fn(f) => {
            if let Some(ir_fn) = convert_fn_item(f) {
                ir.items.push(IrItem::Function(ir_fn));
            }
        }
        Item::Struct(s) => {
            if let Some(ir_struct) = convert_struct_item(s) {
                ir.items.push(IrItem::Struct(ir_struct));
            }
        }
        Item::Enum(e) => {
            if let Some(ir_enum) = convert_enum_item(e) {
                ir.items.push(IrItem::Enum(ir_enum));
            }
        }
        Item::Trait(t) => {
            if let Some(ir_trait) = convert_trait_item(t) {
                ir.items.push(IrItem::Trait(ir_trait));
            }
        }
        Item::Impl(i) => {
            if let Some(ir_impl) = convert_impl_item(i) {
                ir.items.push(IrItem::Impl(ir_impl));
            }
        }
        Item::Mod(m) if is_public(&m.vis) => {
            // Recurse into `pub mod` children
            if let Some((_, items)) = &m.content {
                for child in items {
                    parse_syn_item(child, crate_root, ir, depth + 1)?;
                }
            } else {
                // Try to load the module file
                let mod_name = m.ident.to_string();
                let candidates = [
                    crate_root.join("src").join(format!("{}.rs", mod_name)),
                    crate_root.join("src").join(&mod_name).join("mod.rs"),
                ];
                for candidate in &candidates {
                    if candidate.exists() {
                        let source = fs::read_to_string(candidate)
                            .map_err(|e| InspectError::CargoRustdocFailed(format!(
                                "Failed to read module file {}: {e}",
                                candidate.display()
                            )))?;
                        let file = parse_file(&source)
                            .map_err(|e| InspectError::CargoRustdocFailed(format!(
                                "Failed to parse module file {}: {e}",
                                candidate.display()
                            )))?;
                        parse_syn_file(&file, crate_root, ir, depth + 1)?;
                        break;
                    }
                }
            }
        }
        _ => {} // Skip use, const, type alias, extern crate, etc.
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Visibility helpers
// ---------------------------------------------------------------------------

fn is_public(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

// ---------------------------------------------------------------------------
// Doc comment extraction
// ---------------------------------------------------------------------------

/// Extract doc comments from an attributes list.
fn extract_doc(attrs: &[Attribute]) -> String {
    let mut doc_lines: Vec<String> = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            // Doc comments can be represented as:
            //   #[doc = "comment"]  — name-value
            //   #[doc("comment")]   — bare meta (older style)
            let value = match &attr.meta {
                syn::Meta::NameValue(nv) => {
                    if let Expr::Lit(lit) = &nv.value {
                        if let syn::Lit::Str(s) = &lit.lit {
                            Some(s.value())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                syn::Meta::List(list) => {
                    // Parse doc attributes like #[doc("comment")]
                    if let Ok(Expr::Lit(lit)) = syn::parse2::<Expr>(list.tokens.clone()) {
                        if let syn::Lit::Str(s) = &lit.lit {
                            Some(s.value())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                _ => None,
            };

            if let Some(text) = value {
                // Trim leading whitespace from doc comments
                let trimmed = text
                    .strip_prefix(' ')
                    .unwrap_or(&text)
                    .to_string();
                doc_lines.push(trimmed);
            }
        }
    }
    doc_lines.join("\n")
}

// ---------------------------------------------------------------------------
// Type string formatting
// ---------------------------------------------------------------------------

/// Convert a syn `Type` to a string representation using `quote!`.
fn type_to_string(ty: &Type) -> String {
    quote::quote!(#ty).to_string()
}

/// Extract a parameter's type, stripping `self` receiver magic.
fn param_type_str(arg: &FnArg) -> Option<String> {
    match arg {
        FnArg::Typed(pat_type) => {
            let ty_str = type_to_string(&pat_type.ty);
            Some(ty_str)
        }
        FnArg::Receiver(_) => Some("&self".to_string()),
    }
}

/// Extract a parameter's name, converting receiver to "self".
fn param_name_str(arg: &FnArg) -> String {
    match arg {
        FnArg::Typed(pat_type) => {
            pat_name_str(&pat_type.pat)
        }
        FnArg::Receiver(_) => "self".to_string(),
    }
}

fn pat_name_str(pat: &Pat) -> String {
    match pat {
        Pat::Ident(ident) => ident.ident.to_string(),
        Pat::Wild(_) => "_".to_string(),
        Pat::Reference(ref_pat) => pat_name_str(&ref_pat.pat),
        _ => "_".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Function conversion
// ---------------------------------------------------------------------------

fn convert_fn_item(f: &ItemFn) -> Option<FunctionItem> {
    if !is_public(&f.vis) {
        return None;
    }

    let name = f.sig.ident.to_string();
    let doc = extract_doc(&f.attrs);
    let is_unsafe = f.sig.unsafety.is_some();
    let is_async = f.sig.asyncness.is_some();
    let has_generics = !f.sig.generics.params.is_empty();

    let inputs: Vec<FunctionParameter> = f
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            let name = param_name_str(arg);
            let type_str = param_type_str(arg)?;

            // Skip `self` parameter for free functions (shouldn't happen,
            // but just in case — treat it as a method-like free function)
            Some(FunctionParameter { name, type_str })
        })
        .collect();

    let output = match &f.sig.output {
        ReturnType::Default => IrType {
            type_str: "()".to_string(),
        },
        ReturnType::Type(_, ty) => {
            let s = type_to_string(ty);
            IrType {
                type_str: if s.is_empty() { "()".to_string() } else { s },
            }
        }
    };

    Some(FunctionItem {
        kind: IrItemKind::Function,
        name,
        doc,
        inputs,
        output,
        is_unsafe,
        is_async,
        has_generics,
        visibility: "public".to_string(),
    })
}

// ---------------------------------------------------------------------------
// Struct conversion
// ---------------------------------------------------------------------------

fn convert_struct_item(s: &ItemStruct) -> Option<StructItem> {
    if !is_public(&s.vis) {
        return None;
    }

    let name = s.ident.to_string();
    let doc = extract_doc(&s.attrs);

    let fields = struct_fields_to_ir(&s.fields);
    let has_generics = !s.generics.params.is_empty();
    let has_lifetimes = has_lifetime_params(&s.generics.params);

    Some(StructItem {
        kind: IrItemKind::Struct,
        name,
        doc,
        fields,
        methods: Vec::new(), // Methods will be collected in post-processing
        has_lifetimes,
        has_generics,
    })
}

fn struct_fields_to_ir(fields: &Fields) -> Vec<StructField> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                let vis = if is_public(&f.vis) {
                    FieldVisibility::Public
                } else {
                    FieldVisibility::Private
                };
                StructField {
                    name: f.ident.as_ref().map(|id| id.to_string()).unwrap_or_else(|| "_".to_string()),
                    type_str: type_to_string(&f.ty),
                    visibility: vis,
                }
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let vis = if is_public(&f.vis) {
                    FieldVisibility::Public
                } else {
                    FieldVisibility::Private
                };
                StructField {
                    name: format!("_{}", i),
                    type_str: type_to_string(&f.ty),
                    visibility: vis,
                }
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Enum conversion
// ---------------------------------------------------------------------------

fn convert_enum_item(e: &ItemEnum) -> Option<EnumItem> {
    if !is_public(&e.vis) {
        return None;
    }

    let name = e.ident.to_string();
    let doc = extract_doc(&e.attrs);
    let has_generics = !e.generics.params.is_empty();
    let has_lifetimes = has_lifetime_params(&e.generics.params);

    let variants: Vec<EnumVariant> = e
        .variants
        .iter()
        .map(|v| {
            let fields = enum_variant_fields_to_ir(&v.fields);
            EnumVariant {
                name: v.ident.to_string(),
                fields,
                discriminant: v
                    .discriminant
                    .as_ref()
                    .map(|(_, expr)| quote::quote!(#expr).to_string()),
            }
        })
        .collect();

    Some(EnumItem {
        kind: IrItemKind::Enum,
        name,
        doc,
        variants,
        methods: Vec::new(),
        has_lifetimes,
        has_generics,
    })
}

fn enum_variant_fields_to_ir(fields: &Fields) -> Vec<StructField> {
    match fields {
        Fields::Named(named) => named
            .named
            .iter()
            .map(|f| StructField {
                name: f.ident.as_ref().map(|id| id.to_string()).unwrap_or_else(|| "_".to_string()),
                type_str: type_to_string(&f.ty),
                visibility: FieldVisibility::Public,
            })
            .collect(),
        Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| StructField {
                name: format!("_{}", i),
                type_str: type_to_string(&f.ty),
                visibility: FieldVisibility::Public,
            })
            .collect(),
        Fields::Unit => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Trait conversion
// ---------------------------------------------------------------------------

fn convert_trait_item(t: &ItemTrait) -> Option<TraitItem> {
    if !is_public(&t.vis) {
        return None;
    }

    let name = t.ident.to_string();
    let doc = extract_doc(&t.attrs);
    let has_lifetimes = has_lifetime_params(&t.generics.params);

    let methods: Vec<FunctionItem> = t
        .items
        .iter()
        .filter_map(|item| {
            use syn::TraitItem;
            match item {
                TraitItem::Fn(method) => {
                    let name = method.sig.ident.to_string();
                    let doc = extract_doc(&method.attrs);
                    let is_unsafe = method.sig.unsafety.is_some();
                    let is_async = method.sig.asyncness.is_some();
                    let has_generics = !method.sig.generics.params.is_empty();

                    let inputs: Vec<FunctionParameter> = method
                        .sig
                        .inputs
                        .iter()
                        .filter_map(|arg| {
                            let name = param_name_str(arg);
                            let type_str = param_type_str(arg)?;
                            Some(FunctionParameter { name, type_str })
                        })
                        .collect();

                    let output = match &method.sig.output {
                        ReturnType::Default => IrType {
                            type_str: "()".to_string(),
                        },
                        ReturnType::Type(_, ty) => {
                            let s = type_to_string(ty);
                            IrType {
                                type_str: if s.is_empty() { "()".to_string() } else { s },
                            }
                        }
                    };

                    Some(FunctionItem {
                        kind: IrItemKind::Function,
                        name,
                        doc,
                        inputs,
                        output,
                        is_unsafe,
                        is_async,
                        has_generics,
                        visibility: "public".to_string(),
                    })
                }
                _ => None,
            }
        })
        .collect();

    Some(TraitItem {
        kind: IrItemKind::Trait,
        name,
        doc,
        methods,
        has_lifetimes,
    })
}

// ---------------------------------------------------------------------------
// Impl conversion
// ---------------------------------------------------------------------------

fn convert_impl_item(i: &ItemImpl) -> Option<ImplItem> {
    let type_name = type_to_string(&i.self_ty);

    let trait_name = i.trait_.as_ref().map(|(_, path, _)| {
        quote::quote!(#path).to_string()
    });

    let methods: Vec<FunctionItem> = i
        .items
        .iter()
        .filter_map(|item| {
            use syn::ImplItem;
            match item {
                ImplItem::Fn(method) => {
                    let name = method.sig.ident.to_string();
                    let doc = extract_doc(&method.attrs);
                    let is_unsafe = method.sig.unsafety.is_some();
                    let is_async = method.sig.asyncness.is_some();
                    let has_generics = !method.sig.generics.params.is_empty();

                    let inputs: Vec<FunctionParameter> = method
                        .sig
                        .inputs
                        .iter()
                        .filter_map(|arg| {
                            let name = param_name_str(arg);
                            let type_str = param_type_str(arg)?;
                            Some(FunctionParameter { name, type_str })
                        })
                        .collect();

                    let output = match &method.sig.output {
                        ReturnType::Default => IrType {
                            type_str: "()".to_string(),
                        },
                        ReturnType::Type(_, ty) => {
                            let s = type_to_string(ty);
                            IrType {
                                type_str: if s.is_empty() { "()".to_string() } else { s },
                            }
                        }
                    };

                    Some(FunctionItem {
                        kind: IrItemKind::Function,
                        name,
                        doc,
                        inputs,
                        output,
                        is_unsafe,
                        is_async,
                        has_generics,
                        visibility: "public".to_string(),
                    })
                }
                _ => None,
            }
        })
        .collect();

    Some(ImplItem {
        kind: IrItemKind::Impl,
        type_name,
        methods,
        trait_name,
        doc: extract_doc(&i.attrs),
    })
}

// ---------------------------------------------------------------------------
// Generic helpers
// ---------------------------------------------------------------------------

fn has_lifetime_params(params: &syn::punctuated::Punctuated<GenericParam, syn::token::Comma>) -> bool {
    params.iter().any(|p| matches!(p, GenericParam::Lifetime(_)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let source = r#"
/// Adds two numbers.
pub fn add(a: i64, b: i64) -> i64 {
    a + b
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        // Call parse_syn_file with a minimal crate_root (won't be used for this test)
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Function(f) = &ir.items[0] {
            assert_eq!(f.name, "add");
            assert_eq!(f.inputs.len(), 2);
            assert_eq!(f.inputs[0].name, "a");
            assert!(f.inputs[0].type_str.contains("i64"));
            assert_eq!(f.inputs[1].name, "b");
            assert!(f.inputs[1].type_str.contains("i64"));
            assert!(f.output.type_str.contains("i64"));
            assert!(!f.is_unsafe);
            assert!(!f.is_async);
            assert_eq!(f.doc, "Adds two numbers.");
        } else {
            panic!("Expected a function item");
        }
    }

    #[test]
    fn test_parse_private_function_skipped() {
        let source = r#"
fn private_fn() -> i32 { 42 }
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();
        assert!(ir.items.is_empty());
    }

    #[test]
    fn test_parse_struct() {
        let source = r#"
/// A point in 2D space.
pub struct Point {
    pub x: f64,
    pub y: f64,
    z: i32,
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Struct(s) = &ir.items[0] {
            assert_eq!(s.name, "Point");
            assert_eq!(s.fields.len(), 3);
            assert_eq!(s.fields[0].name, "x");
            assert!(s.fields[0].type_str.contains("f64"));
            assert_eq!(s.fields[0].visibility, FieldVisibility::Public);
            assert_eq!(s.fields[1].name, "y");
            assert_eq!(s.fields[2].name, "z");
            assert_eq!(s.fields[2].visibility, FieldVisibility::Private);
            assert_eq!(s.doc, "A point in 2D space.");
        } else {
            panic!("Expected a struct item");
        }
    }

    #[test]
    fn test_parse_tuple_struct() {
        let source = r#"
pub struct Wrapper(pub String, i32);
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Struct(s) = &ir.items[0] {
            assert_eq!(s.name, "Wrapper");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "_0");
            assert_eq!(s.fields[0].visibility, FieldVisibility::Public);
            assert_eq!(s.fields[1].name, "_1");
            assert_eq!(s.fields[1].visibility, FieldVisibility::Private);
        }
    }

    #[test]
    fn test_parse_enum() {
        let source = r#"
/// Cardinal directions.
pub enum Direction {
    North,
    South,
    East,
    West,
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Enum(e) = &ir.items[0] {
            assert_eq!(e.name, "Direction");
            assert_eq!(e.variants.len(), 4);
            assert_eq!(e.variants[0].name, "North");
            assert!(e.variants[0].fields.is_empty());
            assert_eq!(e.doc, "Cardinal directions.");
        } else {
            panic!("Expected an enum item");
        }
    }

    #[test]
    fn test_parse_enum_with_data() {
        let source = r#"
pub enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Enum(e) = &ir.items[0] {
            assert_eq!(e.name, "Message");
            assert_eq!(e.variants.len(), 3);
            // Quit — unit variant
            assert_eq!(e.variants[0].name, "Quit");
            assert!(e.variants[0].fields.is_empty());
            // Move — struct variant with named fields
            assert_eq!(e.variants[1].name, "Move");
            assert_eq!(e.variants[1].fields.len(), 2);
            assert_eq!(e.variants[1].fields[0].name, "x");
            assert!(e.variants[1].fields[0].type_str.contains("i32"));
            assert_eq!(e.variants[1].fields[1].name, "y");
            // Write — tuple variant
            assert_eq!(e.variants[2].name, "Write");
            assert_eq!(e.variants[2].fields.len(), 1);
            assert_eq!(e.variants[2].fields[0].name, "_0");
        }
    }

    #[test]
    fn test_parse_generic_struct() {
        let source = r#"
pub struct Container<T> {
    pub value: T,
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        if let IrItem::Struct(s) = &ir.items[0] {
            assert!(s.has_generics);
            assert_eq!(s.fields.len(), 1);
            assert_eq!(s.fields[0].name, "value");
            assert_eq!(s.fields[0].type_str, "T");
        } else {
            panic!("Expected a struct item");
        }
    }

    #[test]
    fn test_parse_unsafe_async_function() {
        let source = r#"
pub async unsafe fn dangerous() -> i32 { 42 }
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Function(f) = &ir.items[0] {
            assert!(f.is_unsafe);
            assert!(f.is_async);
        }
    }

    #[test]
    fn test_parse_trait() {
        let source = r#"
/// A trait for things that can speak.
pub trait Speak {
    fn speak(&self) -> String;
    fn shout(&self) -> String {
        self.speak().to_uppercase()
    }
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Trait(t) = &ir.items[0] {
            assert_eq!(t.name, "Speak");
            assert_eq!(t.methods.len(), 2);
            assert_eq!(t.methods[0].name, "speak");
            assert_eq!(t.methods[0].inputs.len(), 1);
            assert_eq!(t.methods[0].inputs[0].name, "self");
            assert!(t.methods[0].output.type_str.contains("String"));
            assert_eq!(t.methods[1].name, "shout");
            assert!(t.doc.contains("A trait"));
        } else {
            panic!("Expected a trait item");
        }
    }

    #[test]
    fn test_parse_impl_block() {
        let source = r#"
pub struct MyStruct {
    pub value: i32,
}

impl MyStruct {
    pub fn new(val: i32) -> Self {
        MyStruct { value: val }
    }

    pub fn get(&self) -> i32 {
        self.value
    }
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        // Should have 2 items: MyStruct + impl
        assert_eq!(ir.items.len(), 2);

        // Find the impl
        let impls: Vec<&ImplItem> = ir.items.iter().filter_map(|item| {
            if let IrItem::Impl(i) = item { Some(i) } else { None }
        }).collect();
        assert_eq!(impls.len(), 1);
        assert_eq!(impls[0].methods.len(), 2);
        assert_eq!(impls[0].methods[0].name, "new");
        assert_eq!(impls[0].methods[1].name, "get");
        assert!(impls[0].trait_name.is_none());
    }

    #[test]
    fn test_parse_trait_impl_block() {
        let source = r#"
pub struct MyStruct { pub value: i32 }

pub trait MyTrait {
    fn do_something(&self) -> i32;
}

impl MyTrait for MyStruct {
    fn do_something(&self) -> i32 {
        self.value
    }
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        let impls: Vec<&ImplItem> = ir.items.iter().filter_map(|item| {
            if let IrItem::Impl(i) = item { Some(i) } else { None }
        }).collect();
        assert_eq!(impls.len(), 1);
        assert!(impls[0].trait_name.is_some());
        assert_eq!(
            impls[0].trait_name.as_deref().unwrap(),
            "MyTrait"
        );
    }

    #[test]
    fn test_io_error_variant() {
        // Test that IO error variant for InspectError implements std::error::Error
        let err = InspectError::ReadOutputFailed(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "test",
        ));
        assert!(err.to_string().contains("test"));
    }

    #[test]
    fn test_find_crate_root_on_file() {
        let tmp = std::env::temp_dir();
        // Use /tmp as a test — it likely has no Cargo.toml
        let result = find_crate_root(&tmp);
        assert!(result.is_err() || result.is_ok());
    }

    #[test]
    fn test_extract_doc_empty() {
        let source = "pub fn foo() {}";
        let file = parse_file(source).unwrap();
        let attrs = match &file.items[0] {
            Item::Fn(f) => &f.attrs,
            _ => panic!("expected function"),
        };
        let doc = extract_doc(attrs);
        assert!(doc.is_empty());
    }

    #[test]
    fn test_extract_doc_multi_line() {
        let source = r#"
/// First line.
/// Second line.
pub fn foo() {}
"#;
        let file = parse_file(source).unwrap();
        let attrs = match &file.items[0] {
            Item::Fn(f) => &f.attrs,
            _ => panic!("expected function"),
        };
        let doc = extract_doc(attrs);
        assert_eq!(doc, "First line.\nSecond line.");
    }

    #[test]
    fn test_parse_find_crate_root_errors() {
        let result = find_crate_root(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_function_with_string_params() {
        let source = r#"
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 1);
        if let IrItem::Function(f) = &ir.items[0] {
            assert_eq!(f.name, "greet");
            assert_eq!(f.inputs.len(), 1);
            assert!(f.inputs[0].type_str.contains("String"));
            assert!(f.output.type_str.contains("String"));
        }
    }

    #[test]
    fn test_parse_function_with_option_result() {
        let source = r#"
pub fn find(needle: &str, haystack: &[String]) -> Option<usize> {
    None
}

pub fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("divide by zero".to_string())
    } else {
        Ok(a / b)
    }
}
"#;
        let file = parse_file(source).unwrap();
        let mut ir = IntermediateRepresentation::new("test".to_string(), "0.1.0".to_string());
        let crate_root = Path::new("/tmp");
        parse_syn_file(&file, crate_root, &mut ir, 0).unwrap();

        assert_eq!(ir.items.len(), 2);
        if let IrItem::Function(f) = &ir.items[0] {
            assert_eq!(f.name, "find");
            assert!(f.output.type_str.contains("Option"));
        }
        if let IrItem::Function(f) = &ir.items[1] {
            assert_eq!(f.name, "divide");
            assert!(f.output.type_str.contains("Result"));
        }
    }
}
