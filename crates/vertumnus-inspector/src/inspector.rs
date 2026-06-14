//! Rustdoc JSON parser for Vertumnus.
//!
//! Inspects a Rust crate's public API by running `cargo +nightly rustdoc`
//! with `--output-format json` and parsing the resulting structure into
//! Vertumnus's Intermediate Representation (IR).

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

use crate::ir::{
    EnumItem, EnumVariant, FieldVisibility, FunctionItem, FunctionParameter,
    ImplItem, IntermediateRepresentation, IrItem, IrItemKind, IrType, StructField,
    StructItem, TraitItem,
};

/// Errors that can occur during crate inspection.
#[derive(Debug, thiserror::Error)]
pub enum InspectError {
    #[error("Failed to run cargo rustdoc: {0}")]
    CargoRustdocFailed(String),
    #[error("Failed to read rustdoc output: {0}")]
    ReadOutputFailed(#[from] std::io::Error),
    #[error("Failed to parse rustdoc JSON: {0}")]
    JsonParseFailed(#[from] serde_json::Error),
    #[error("Rustdoc JSON missing expected structure: {0}")]
    MissingStructure(String),
    #[error("Failed to find rustdoc JSON output file at {path}: {err}")]
    OutputFileNotFound { path: String, err: std::io::Error },
    #[error("Crate has no public API items")]
    NoPublicItems,
}

/// Inspect a Rust crate and produce an Intermediate Representation.
pub fn inspect_crate(crate_path: &Path) -> Result<IntermediateRepresentation, InspectError> {
    let rustdoc_json = run_rustdoc_json(crate_path)?;
    let parsed: Value = serde_json::from_str(&rustdoc_json)?;
    convert_rustdoc_to_ir(crate_path, &parsed)
}

// ---------------------------------------------------------------------------
// Running rustdoc
// ---------------------------------------------------------------------------

/// Run `cargo +nightly rustdoc -- -Z unstable-options --output-format json` and
/// return the raw JSON string of the primary crate.
fn run_rustdoc_json(crate_path: &Path) -> Result<String, InspectError> {
    let canonical = crate_path.canonicalize().map_err(|e| {
        InspectError::CargoRustdocFailed(format!("Cannot resolve path: {e}"))
    })?;

    let output = Command::new("cargo")
        .args([
            "+nightly",
            "rustdoc",
            "--",
            "-Z",
            "unstable-options",
            "--output-format",
            "json",
        ])
        .current_dir(&canonical)
        .output()
        .map_err(|e| {
            InspectError::CargoRustdocFailed(format!("Failed to execute cargo rustdoc: {e}"))
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(InspectError::CargoRustdocFailed(format!(
            "cargo rustdoc failed:\n{stderr}"
        )));
    }

    // The JSON output is at target/doc/<crate_name>.json
    let doc_dir = canonical.join("target").join("doc");
    if !doc_dir.exists() {
        return Err(InspectError::OutputFileNotFound {
            path: doc_dir.to_string_lossy().to_string(),
            err: std::io::Error::new(std::io::ErrorKind::NotFound, "doc directory not created"),
        });
    }

    // Guess the crate name from the directory and find the matching JSON file
    let crate_name_guess = canonical
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.replace('-', "_"));

    let expected_file = doc_dir.join(format!(
        "{}.json",
        crate_name_guess.as_deref().unwrap_or("unknown")
    ));

    if expected_file.exists() {
        return std::fs::read_to_string(&expected_file).map_err(InspectError::ReadOutputFailed);
    }

    // Fallback: list all JSON files for debugging
    let available: Vec<String> = std::fs::read_dir(&doc_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            e.ok().and_then(|e| {
                let p = e.path();
                if p.extension()? == "json" {
                    p.file_name()?.to_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
        })
        .collect();

    Err(InspectError::OutputFileNotFound {
        path: doc_dir.to_string_lossy().to_string(),
        err: std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "No JSON file found for crate. Available files: {:?}",
                available
            ),
        ),
    })
}

// ---------------------------------------------------------------------------
// Rustdoc JSON deserialization helpers
// ---------------------------------------------------------------------------

/// Top-level rustdoc JSON structure.
#[derive(Debug, Deserialize)]
struct RustdocCrate {
    /// Root item ID (integer stored as JSON number).
    root: serde_json::Number,
    #[serde(default)]
    crate_version: Option<String>,
    /// The index maps string IDs to items.
    index: HashMap<String, RustdocItem>,
}

/// A single item in the rustdoc JSON index.
#[derive(Debug, Deserialize)]
struct RustdocItem {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    docs: Option<String>,
    /// The item's inner data — keyed by kind (e.g. "function", "struct", etc.)
    inner: HashMap<String, Value>,
}

// ---------------------------------------------------------------------------
// IR conversion
// ---------------------------------------------------------------------------

/// Convert the parsed rustdoc JSON into our IR.
fn convert_rustdoc_to_ir(
    crate_path: &Path,
    root_value: &Value,
) -> Result<IntermediateRepresentation, InspectError> {
    let rustdoc: RustdocCrate = serde_json::from_value(root_value.clone())?;

    let crate_name = rustdoc
        .index
        .get(&rustdoc.root.to_string())
        .and_then(|item| item.name.as_deref())
        .unwrap_or_else(|| {
            crate_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        })
        .to_string();

    let crate_version = rustdoc
        .crate_version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_string());

    let mut ir = IntermediateRepresentation::new(crate_name, crate_version);

    // Start from the root module
    let root_id = rustdoc.root.to_string();
    let root_item = rustdoc.index.get(&root_id).ok_or_else(|| {
        InspectError::MissingStructure(format!("Root item '{}' not found in index", root_id))
    })?;

    // Get the module's item list
    let module_items = root_item
        .inner
        .get("module")
        .and_then(|m| m.get("items"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| {
            InspectError::MissingStructure("Root item is not a module or has no items".to_string())
        })?;

    // Collect item IDs (handling both integer and string representations)
    let item_ids: Vec<String> = module_items
        .iter()
        .map(|v| match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .collect();

    collect_items(&rustdoc.index, &item_ids, &mut ir);

    if ir.items.is_empty() {
        // Still ok if no top-level items — could be all in submodules
    }

    Ok(ir)
}

/// Recursively collect public items from the rustdoc index.
fn collect_items(
    index: &HashMap<String, RustdocItem>,
    item_ids: &[String],
    ir: &mut IntermediateRepresentation,
) {
    for id_str in item_ids {
        let Some(item) = index.get(id_str) else {
            continue;
        };

        // Skip non-public items
        if item.visibility.as_deref() != Some("public") {
            continue;
        }

        let Some(inner) = resolve_inner(item) else {
            continue;
        };

        match inner {
            InnerItem::Module { items } => {
                let child_ids: Vec<String> = items
                    .iter()
                    .map(|v| match v {
                        Value::Number(n) => n.to_string(),
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                    .collect();
                collect_items(index, &child_ids, ir);
            }
            InnerItem::Function(func) => {
                if let Some(ir_fn) = convert_function(item, &func) {
                    ir.items.push(IrItem::Function(ir_fn));
                }
            }
            InnerItem::Struct(s) => {
                if let Some(ir_struct) = convert_struct(item, &s, index) {
                    ir.items.push(IrItem::Struct(ir_struct));
                }
            }
            InnerItem::Enum(e) => {
                if let Some(ir_enum) = convert_enum(item, &e, index) {
                    ir.items.push(IrItem::Enum(ir_enum));
                }
            }
            InnerItem::Trait(t) => {
                if let Some(ir_trait) = convert_trait(item, &t, index) {
                    ir.items.push(IrItem::Trait(ir_trait));
                }
            }
            InnerItem::Impl(imp) => {
                // We skip synthetic impls (auto-derived, e.g. Send/Sync)
                if imp.is_synthetic.unwrap_or(false) {
                    return;
                }
                if let Some(ir_impl) = convert_impl(item, &imp, index) {
                    ir.items.push(IrItem::Impl(ir_impl));
                }
            }
            // Other item kinds (struct_field, variant, etc.) are handled
            // via their parent items, not as top-level items.
            InnerItem::Other => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Inner item resolution
// ---------------------------------------------------------------------------

/// Resolved inner item kind extracted from an item's `inner` map.
enum InnerItem {
    Module { items: Vec<Value> },
    Function(RustdocFunction),
    Struct(RustdocStructInfo),
    Enum(RustdocEnumInfo),
    Trait(RustdocTraitInfo),
    Impl(RustdocImplInfo),
    Other,
}

struct RustdocFunction {
    inputs: Vec<(String, Value)>,
    output: Option<Value>,
    generic_params: Vec<Value>,
    is_unsafe: bool,
    is_async: bool,
}

struct RustdocStructInfo {
    field_ids: Vec<String>,
    generic_params: Vec<Value>,
    impl_ids: Vec<String>,
}

struct RustdocEnumInfo {
    variant_ids: Vec<String>,
    generic_params: Vec<Value>,
    impl_ids: Vec<String>,
}

struct RustdocTraitInfo {
    method_ids: Vec<String>,
    generic_params: Vec<Value>,
}

struct RustdocImplInfo {
    for_type: Value,
    trait_type: Option<Value>,
    method_ids: Vec<String>,
    is_synthetic: Option<bool>,
}

/// Extract the inner item data from an item's `inner` map, matching the kind.
fn resolve_inner(item: &RustdocItem) -> Option<InnerItem> {
    // The inner map has a single key like "function", "struct", "module", etc.
    let (kind_key, inner_value) = item.inner.iter().next()?;

    match kind_key.as_str() {
        "module" => {
            let items = inner_value.get("items")?.as_array()?.clone();
            Some(InnerItem::Module { items })
        }
        "function" => {
            let sig = inner_value.get("sig")?;
            let inputs_raw = sig.get("inputs")?.as_array()?;
            let mut inputs = Vec::new();
            for input in inputs_raw {
                if let Some(arr) = input.as_array() {
                    if arr.len() >= 2 {
                        let name = arr[0].as_str().unwrap_or("arg").to_string();
                        let type_val = arr[1].clone();
                        inputs.push((name, type_val));
                    }
                }
            }
            let output = sig.get("output").cloned();
            let generics = inner_value.get("generics");
            let generic_params = generics
                .and_then(|g| g.get("params"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let header = inner_value.get("header");
            let is_unsafe = header
                .and_then(|h| h.get("is_unsafe"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let is_async = header
                .and_then(|h| h.get("is_async"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            Some(InnerItem::Function(RustdocFunction {
                inputs,
                output,
                generic_params,
                is_unsafe,
                is_async,
            }))
        }
        "struct" => {
            let kind = inner_value.get("kind")?;
            let field_ids = kind
                .get("plain")
                .and_then(|p| p.get("fields"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| match v {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let generics = inner_value.get("generics");
            let generic_params = generics
                .and_then(|g| g.get("params"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let impl_ids = inner_value
                .get("impls")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| match v {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            Some(InnerItem::Struct(RustdocStructInfo {
                field_ids,
                generic_params,
                impl_ids,
            }))
        }
        "enum" => {
            let variant_ids = inner_value
                .get("variants")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| match v {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let generics = inner_value.get("generics");
            let generic_params = generics
                .and_then(|g| g.get("params"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let impl_ids = inner_value
                .get("impls")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| match v {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();

            Some(InnerItem::Enum(RustdocEnumInfo {
                variant_ids,
                generic_params,
                impl_ids,
            }))
        }
        "trait" => {
            let method_ids = inner_value
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| match v {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let generics = inner_value.get("generics");
            let generic_params = generics
                .and_then(|g| g.get("params"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            Some(InnerItem::Trait(RustdocTraitInfo {
                method_ids,
                generic_params,
            }))
        }
        "impl" => {
            let for_type = inner_value.get("for")?.clone();
            let trait_type = inner_value.get("trait").and_then(|v| {
                if v.is_null() {
                    None
                } else {
                    Some(v.clone())
                }
            });
            let method_ids = inner_value
                .get("items")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| match v {
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.clone(),
                            other => other.to_string(),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let is_synthetic = inner_value
                .get("is_synthetic")
                .and_then(|v| v.as_bool());

            Some(InnerItem::Impl(RustdocImplInfo {
                for_type,
                trait_type,
                method_ids,
                is_synthetic,
            }))
        }
        _ => Some(InnerItem::Other),
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn convert_function(item: &RustdocItem, func: &RustdocFunction) -> Option<FunctionItem> {
    let name = item.name.as_deref()?;

    let inputs: Vec<FunctionParameter> = func
        .inputs
        .iter()
        .map(|(name, type_val)| FunctionParameter {
            name: name.clone(),
            type_str: resolve_type(type_val),
        })
        .collect();

    let output = func
        .output
        .as_ref()
        .map(|o| {
            let s = resolve_type(o);
            if s.is_empty() { "()".to_string() } else { s }
        })
        .unwrap_or_else(|| "()".to_string());

    let has_generics = !func.generic_params.is_empty();

    Some(FunctionItem {
        kind: IrItemKind::Function,
        name: name.to_string(),
        doc: item.docs.as_deref().unwrap_or("").to_string(),
        inputs,
        output: IrType { type_str: output },
        is_unsafe: func.is_unsafe,
        is_async: func.is_async,
        has_generics,
        visibility: "public".to_string(),
    })
}

fn convert_struct(
    item: &RustdocItem,
    s: &RustdocStructInfo,
    index: &HashMap<String, RustdocItem>,
) -> Option<StructItem> {
    let name = item.name.as_deref()?;

    // Resolve fields from their separate items
    let fields: Vec<StructField> = s
        .field_ids
        .iter()
        .filter_map(|fid| {
            let field_item = index.get(fid)?;
            let field_type = field_item.inner.get("struct_field")?;
            let vis = match field_item.visibility.as_deref() {
                Some("public") => FieldVisibility::Public,
                _ => FieldVisibility::Private,
            };
            Some(StructField {
                name: field_item.name.as_deref()?.to_string(),
                type_str: resolve_type(field_type),
                visibility: vis,
            })
        })
        .collect();

    let has_generics = !s.generic_params.is_empty();
    let has_lifetimes = s.generic_params.iter().any(|p| {
        p.get("kind")
            .and_then(|k| k.as_object())
            .and_then(|k| k.get("lifetime"))
            .is_some()
    });

    // Fetch inherent methods from impl blocks
    let methods = collect_type_methods(name, &s.impl_ids, index, false);

    Some(StructItem {
        kind: IrItemKind::Struct,
        name: name.to_string(),
        doc: item.docs.as_deref().unwrap_or("").to_string(),
        fields,
        methods,
        has_lifetimes,
        has_generics,
    })
}

/// Resolve enum variant fields from rustdoc JSON.
///
/// Handles three variant kinds:
/// - `"plain"` — no fields
/// - `{"struct": {"fields": [field_ids]}}` — struct variant (named fields)
/// - `{"tuple": [field_ids]}` — tuple variant (unnamed fields)
fn resolve_enum_variant_fields(
    kind_val: Option<&Value>,
    index: &HashMap<String, RustdocItem>,
) -> Vec<StructField> {
    let kind_val = match kind_val {
        Some(v) => v,
        None => return Vec::new(),
    };

    // Plain variant — no fields
    if let Some(s) = kind_val.as_str() {
        if s == "plain" {
            return Vec::new();
        }
    }

    // Struct variant: kind is {"struct": {"fields": [field_ids]}}
    if let Some(struct_obj) = kind_val.get("struct") {
        if let Some(fields_arr) = struct_obj.get("fields").and_then(|v| v.as_array()) {
            return fields_arr
                .iter()
                .filter_map(|f_id| {
                    let fid = f_id.as_u64()?.to_string();
                    let field_item = index.get(&fid)?;
                    let field_inner = field_item.inner.get("struct_field")?;
                    let fname = field_item.name.as_deref().unwrap_or("unnamed");
                    Some(StructField {
                        name: fname.to_string(),
                        type_str: resolve_type(field_inner),
                        visibility: FieldVisibility::Public,
                    })
                })
                .collect();
        }
    }

    // Tuple variant: kind is {"tuple": [field_ids]}
    if let Some(tuple_arr) = kind_val.get("tuple").and_then(|v| v.as_array()) {
        return tuple_arr
            .iter()
            .enumerate()
            .filter_map(|(i, f_id)| {
                let fid = f_id.as_u64()?.to_string();
                let field_item = index.get(&fid)?;
                let field_inner = field_item.inner.get("struct_field")?;
                let fname = field_item.name.as_deref().unwrap_or("");
                // For tuple variants, the name is positional ("0", "1", etc.)
                let name = if fname == i.to_string() || fname.is_empty() {
                    format!("_{}", i)
                } else {
                    fname.to_string()
                };
                Some(StructField {
                    name,
                    type_str: resolve_type(field_inner),
                    visibility: FieldVisibility::Public,
                })
            })
            .collect();
    }

    Vec::new()
}

fn convert_enum(
    item: &RustdocItem,
    e: &RustdocEnumInfo,
    index: &HashMap<String, RustdocItem>,
) -> Option<EnumItem> {
    let name = item.name.as_deref()?;

    let variants: Vec<EnumVariant> = e
        .variant_ids
        .iter()
        .filter_map(|vid| {
            let variant_item = index.get(vid)?;
            let variant_inner = variant_item.inner.get("variant")?;
            let vname = variant_item.name.as_deref()?;

            // Resolve variant fields from rustdoc JSON.
            // Variant kinds:
            //   - "plain" → no fields
            //   - {"struct": {"fields": [field_ids]}} → struct variant (named fields)
            //   - {"tuple": [field_ids]} → tuple variant (unnamed fields)
            let kind_val = variant_inner.get("kind");
            let fields: Vec<StructField> = resolve_enum_variant_fields(kind_val, index);

            Some(EnumVariant {
                name: vname.to_string(),
                fields,
                discriminant: variant_inner
                    .get("discriminant")
                    .and_then(|d| d.as_str())
                    .map(|s| s.to_string()),
            })
        })
        .collect();

    let has_generics = !e.generic_params.is_empty();
    let has_lifetimes = e.generic_params.iter().any(|p| {
        p.get("kind")
            .and_then(|k| k.as_object())
            .and_then(|k| k.get("lifetime"))
            .is_some()
    });

    let methods = collect_type_methods(name, &e.impl_ids, index, false);

    Some(EnumItem {
        kind: IrItemKind::Enum,
        name: name.to_string(),
        doc: item.docs.as_deref().unwrap_or("").to_string(),
        variants,
        methods,
        has_lifetimes,
        has_generics,
    })
}

fn convert_trait(
    item: &RustdocItem,
    t: &RustdocTraitInfo,
    index: &HashMap<String, RustdocItem>,
) -> Option<TraitItem> {
    let name = item.name.as_deref()?;

    let methods: Vec<FunctionItem> = t
        .method_ids
        .iter()
        .filter_map(|mid| {
            let method_item = index.get(mid)?;
            let inner = resolve_inner(method_item)?;
            match inner {
                InnerItem::Function(func) => convert_function(method_item, &func),
                _ => None,
            }
        })
        .collect();

    let has_lifetimes = t.generic_params.iter().any(|p| {
        p.get("kind")
            .and_then(|k| k.as_object())
            .and_then(|k| k.get("lifetime"))
            .is_some()
    });

    Some(TraitItem {
        kind: IrItemKind::Trait,
        name: name.to_string(),
        doc: item.docs.as_deref().unwrap_or("").to_string(),
        methods,
        has_lifetimes,
    })
}

fn convert_impl(
    item: &RustdocItem,
    imp: &RustdocImplInfo,
    index: &HashMap<String, RustdocItem>,
) -> Option<ImplItem> {
    let type_name = resolve_type(&imp.for_type);

    let trait_name = imp.trait_type.as_ref().map(resolve_type);

    let methods: Vec<FunctionItem> = imp
        .method_ids
        .iter()
        .filter_map(|mid| {
            let method_item = index.get(mid)?;
            let inner = resolve_inner(method_item)?;
            match inner {
                InnerItem::Function(func) => convert_function(method_item, &func),
                _ => None,
            }
        })
        .collect();

    Some(ImplItem {
        kind: IrItemKind::Impl,
        type_name,
        methods,
        trait_name,
        doc: item.docs.as_deref().unwrap_or("").to_string(),
    })
}

/// Collect methods for a type by following its impl block IDs.
fn collect_type_methods(
    type_name: &str,
    impl_ids: &[String],
    index: &HashMap<String, RustdocItem>,
    include_trait_impls: bool,
) -> Vec<FunctionItem> {
    let mut methods = Vec::new();

    for impl_id in impl_ids {
        let Some(impl_item) = index.get(impl_id) else {
            continue;
        };
        let Some(inner) = resolve_inner(impl_item) else {
            continue;
        };
        let InnerItem::Impl(imp) = inner else {
            continue;
        };

        // Skip trait impls unless explicitly included
        if !include_trait_impls && imp.trait_type.is_some() {
            continue;
        }

        // Verify this impl is indeed for our type (defensive check)
        let for_type_str = resolve_type(&imp.for_type);
        if for_type_str != type_name {
            continue;
        }

        for mid in &imp.method_ids {
            let Some(method_item) = index.get(mid) else {
                continue;
            };
            let Some(minner) = resolve_inner(method_item) else {
                continue;
            };
            if let InnerItem::Function(func) = minner {
                if let Some(ir_fn) = convert_function(method_item, &func) {
                    methods.push(ir_fn);
                }
            }
        }
    }

    methods
}

// ---------------------------------------------------------------------------
// Type resolution
// ---------------------------------------------------------------------------

/// Resolve a rustdoc JSON type value to a simple type string.
///
/// The actual rustdoc JSON format uses single-key discriminator objects:
/// - `{"primitive": "i64"}`                 -> "i64"
/// - `{"generic": "T"}`                     -> "T"
/// - `{"resolved_path": {"path":"Vec",...}}` -> "Vec<T>"
/// - `{"borrowed_ref": {"type":...,...}}`   -> "&T"
/// - `{"tuple": [T1, T2]}`                  -> "(T1, T2)"
fn resolve_type(type_val: &Value) -> String {
    match type_val {
        Value::Null => "()".to_string(),
        Value::Object(obj) => {
            // Single-key discriminator — find the first key
            let (kind, inner) = match obj.iter().next() {
                Some((k, v)) => (k.as_str(), v),
                None => return "<empty>".to_string(),
            };

            match kind {
                "primitive" => inner
                    .as_str()
                    .map(|s| {
                        // Map rustdoc's unit type representation
                        if s == "()" { "()".to_string() } else { s.to_string() }
                    })
                    .unwrap_or_else(|| "unknown_primitive".to_string()),

                "generic" => {
                    let name = inner.as_str().unwrap_or("T");
                    // Lifetime names already include the leading quote (e.g. "'a")
                    name.to_string()
                }

                "resolved_path" => {
                    let path = inner.get("path").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let args = inner.get("args").and_then(|a| {
                        // Skip null args
                        if a.is_null() { None } else { Some(a) }
                    });

                    let args_str = args
                        .map(resolve_angle_bracketed_args)
                        .unwrap_or_default();

                    if args_str.is_empty() {
                        path.to_string()
                    } else {
                        format!("{}<{}>", path, args_str)
                    }
                }

                "borrowed_ref" => {
                    let lifetime = inner
                        .get("lifetime")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.trim_start_matches('\''));
                    let is_mut = inner
                        .get("is_mutable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let inner_type = inner.get("type").map(resolve_type);

                    let mut_prefix = if is_mut { "mut " } else { "" };
                    let inner_str = inner_type.as_deref().unwrap_or("unknown");

                    match lifetime {
                        Some(lt) => format!("&'{} {}{}", lt, mut_prefix, inner_str),
                        None => format!("&{}{}", mut_prefix, inner_str),
                    }
                }

                "tuple" => {
                    let elements: Vec<String> = inner
                        .as_array()
                        .map(|arr| arr.iter().map(resolve_type).collect())
                        .unwrap_or_default();
                    if elements.is_empty() {
                        "()".to_string()
                    } else {
                        format!("({})", elements.join(", "))
                    }
                }

                "slice" => {
                    let inner_type = inner
                        .get("inner")
                        .map(resolve_type)
                        .unwrap_or_else(|| "unknown".to_string());
                    format!("[{}]", inner_type)
                }

                "array" => {
                    let inner_type = inner
                        .get("inner")
                        .map(resolve_type)
                        .unwrap_or_else(|| "unknown".to_string());
                    let len = inner
                        .get("len")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    format!("[{}; {}]", inner_type, len)
                }

                "fn_pointer" => {
                    let decl = inner.get("decl");
                    let inputs_str = decl
                        .and_then(|d| d.get("inputs"))
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|input| {
                                    // Inputs can be [name, type] tuples or just types
                                    match input {
                                        Value::Array(pair) if pair.len() >= 2 => {
                                            resolve_type(&pair[1])
                                        }
                                        other => resolve_type(other),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default();
                    let output = decl
                        .and_then(|d| d.get("output"))
                        .map(resolve_type)
                        .filter(|s| s != "()");
                    match output {
                        Some(ret) => format!("fn({}) -> {}", inputs_str, ret),
                        None => format!("fn({})", inputs_str),
                    }
                }

                "dyn_trait" => {
                    let bounds = inner
                        .get("bounds")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(resolve_type)
                                .collect::<Vec<_>>()
                                .join(" + ")
                        })
                        .unwrap_or_default();
                    format!("dyn {}", bounds)
                }

                "impl_trait" => {
                    let bounds = inner
                        .get("bounds")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(resolve_type)
                                .collect::<Vec<_>>()
                                .join(" + ")
                        })
                        .unwrap_or_default();
                    format!("impl {}", bounds)
                }

                "raw_pointer" => {
                    let inner_type = inner
                        .get("type")
                        .map(resolve_type)
                        .unwrap_or_else(|| "unknown".to_string());
                    let is_mut = inner
                        .get("is_mutable")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let prefix = if is_mut { "mut" } else { "const" };
                    format!("*{} {}", prefix, inner_type)
                }

                "never" => "!".to_string(),

                "qualified_path" => {
                    let path = inner.get("path").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let args = inner.get("args");
                    let args_str = args
                        .map(resolve_angle_bracketed_args)
                        .unwrap_or_default();
                    if args_str.is_empty() {
                        path.to_string()
                    } else {
                        format!("{}<{}>", path, args_str)
                    }
                }

                "infer" => "_".to_string(),

                other => {
                    // Try to extract a name field, or string representation
                    inner
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("<unknown_type:{}>", other))
                }
            }
        }
        _ => type_val.to_string(),
    }
}

/// Resolve generic arguments in angle brackets from a rustdoc JSON value.
///
/// Handles the format:
/// ```json
/// {"angle_bracketed": {"args": [{"type": {...}}, ...], "constraints": []}}
/// ```
fn resolve_angle_bracketed_args(args_val: &Value) -> String {
    if args_val.is_null() {
        return String::new();
    }

    // Try `angle_bracketed` wrapper first
    if let Some(ab) = args_val.get("angle_bracketed") {
        if let Some(args_arr) = ab.get("args").and_then(|v| v.as_array()) {
            return args_arr
                .iter()
                .map(|a| {
                    // Each arg has {"type": {...}} or is directly a type
                    if let Some(type_val) = a.get("type") {
                        resolve_type(type_val)
                    } else {
                        resolve_type(a)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
        }
    }

    // Direct args array
    if let Some(args_arr) = args_val.get("args").and_then(|v| v.as_array()) {
        return args_arr
            .iter()
            .map(|a| {
                if let Some(type_val) = a.get("type") {
                    resolve_type(type_val)
                } else {
                    resolve_type(a)
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
    }

    // Fallback: try to resolve the whole value as a type
    resolve_type(args_val)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_resolve_primitive_types() {
        assert_eq!(resolve_type(&json!({"primitive": "i64"})), "i64");
        assert_eq!(resolve_type(&json!({"primitive": "f64"})), "f64");
        assert_eq!(resolve_type(&json!({"primitive": "bool"})), "bool");
        assert_eq!(resolve_type(&json!({"primitive": "str"})), "str");
        assert_eq!(resolve_type(&json!({"primitive": "()"})), "()");
    }

    #[test]
    fn test_resolve_generic_types() {
        assert_eq!(resolve_type(&json!({"generic": "T"})), "T");
        assert_eq!(resolve_type(&json!({"generic": "'a"})), "'a");
    }

    #[test]
    fn test_resolve_vec_type() {
        // Vec<i64> in actual rustdoc JSON format
        let vec_type = json!({
            "resolved_path": {
                "path": "Vec",
                "id": 42,
                "args": {
                    "angle_bracketed": {
                        "args": [
                            {"type": {"primitive": "i64"}}
                        ],
                        "constraints": []
                    }
                }
            }
        });
        assert_eq!(resolve_type(&vec_type), "Vec<i64>");
    }

    #[test]
    fn test_resolve_option_type() {
        let opt_type = json!({
            "resolved_path": {
                "path": "Option",
                "id": 2,
                "args": {
                    "angle_bracketed": {
                        "args": [
                            {"type": {"primitive": "f64"}}
                        ],
                        "constraints": []
                    }
                }
            }
        });
        assert_eq!(resolve_type(&opt_type), "Option<f64>");
    }

    #[test]
    fn test_resolve_result_type() {
        let result_type = json!({
            "resolved_path": {
                "path": "Result",
                "id": 39,
                "args": {
                    "angle_bracketed": {
                        "args": [
                            {"type": {"primitive": "i64"}},
                            {"type": {"resolved_path": { "path": "MathError", "id": 93, "args": null }}}
                        ],
                        "constraints": []
                    }
                }
            }
        });
        assert_eq!(resolve_type(&result_type), "Result<i64, MathError>");
    }

    #[test]
    fn test_resolve_tuple_type() {
        let tuple_type = json!({
            "tuple": [
                {"primitive": "i32"},
                {"primitive": "f64"}
            ]
        });
        assert_eq!(resolve_type(&tuple_type), "(i32, f64)");
    }

    #[test]
    fn test_resolve_reference_type() {
        let ref_type = json!({
            "borrowed_ref": {
                "lifetime": null,
                "is_mutable": false,
                "type": {"primitive": "str"}
            }
        });
        assert_eq!(resolve_type(&ref_type), "&str");

        let mut_ref_type = json!({
            "borrowed_ref": {
                "lifetime": null,
                "is_mutable": true,
                "type": {"primitive": "i32"}
            }
        });
        assert_eq!(resolve_type(&mut_ref_type), "&mut i32");
    }

    #[test]
    fn test_resolve_null() {
        assert_eq!(resolve_type(&Value::Null), "()");
    }

    #[test]
    fn test_resolve_empty_tuple() {
        let empty = json!({"tuple": []});
        assert_eq!(resolve_type(&empty), "()");
    }

    #[test]
    fn test_resolve_fn_pointer() {
        let fn_ptr = json!({
            "fn_pointer": {
                "decl": {
                    "inputs": [
                        ["x", {"primitive": "i32"}],
                        ["y", {"primitive": "i32"}]
                    ],
                    "output": {"primitive": "i32"}
                }
            }
        });
        assert_eq!(resolve_type(&fn_ptr), "fn(i32, i32) -> i32");
    }

    #[test]
    fn test_ir_roundtrip() {
        let ir = IntermediateRepresentation::new("test".to_string(), "1.0.0".to_string());
        let json = ir.to_json_pretty().unwrap();
        let parsed = IntermediateRepresentation::from_json(&json).unwrap();
        assert_eq!(ir, parsed);
    }
}
