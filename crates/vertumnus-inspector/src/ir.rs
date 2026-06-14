use serde::{Deserialize, Serialize};

/// Current version of the IR schema.
/// Must match `schemas/ir.schema.json`.
pub const IR_VERSION: &str = "0.1";

/// Top-level Intermediate Representation of a Rust crate's public API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct IntermediateRepresentation {
    /// Schema version, e.g. "0.1"
    pub vertumnus_ir_version: String,
    /// Name of the Rust crate
    pub crate_name: String,
    /// Version of the Rust crate
    pub crate_version: String,
    /// Public API items
    pub items: Vec<IrItem>,
}

/// A single type reference in the IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IrType {
    /// The Rust type string, e.g. "i64", "Option<String>", "Vec<f64>"
    #[serde(rename = "type")]
    pub type_str: String,
}

/// A parameter of a function.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub type_str: String,
}

/// A field of a struct or enum variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_str: String,
    pub visibility: FieldVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldVisibility {
    Public,
    Private,
}

/// A variant of an enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<StructField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminant: Option<String>,
}

/// A function/method item in the IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FunctionItem {
    pub kind: IrItemKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    pub inputs: Vec<FunctionParameter>,
    pub output: IrType,
    #[serde(default)]
    pub is_unsafe: bool,
    #[serde(default)]
    pub is_async: bool,
    #[serde(default)]
    pub has_generics: bool,
    #[serde(default)]
    pub visibility: String,
}

/// A struct item in the IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StructItem {
    pub kind: IrItemKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    pub fields: Vec<StructField>,
    #[serde(default)]
    pub methods: Vec<FunctionItem>,
    #[serde(default)]
    pub has_lifetimes: bool,
    #[serde(default)]
    pub has_generics: bool,
}

/// An enum item in the IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnumItem {
    pub kind: IrItemKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    pub variants: Vec<EnumVariant>,
    #[serde(default)]
    pub methods: Vec<FunctionItem>,
    #[serde(default)]
    pub has_lifetimes: bool,
    #[serde(default)]
    pub has_generics: bool,
}

/// A trait item in the IR (informational only; limited binding generation).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraitItem {
    pub kind: IrItemKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
    #[serde(default)]
    pub methods: Vec<FunctionItem>,
    #[serde(default)]
    pub has_lifetimes: bool,
}

/// An `impl` block item in the IR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImplItem {
    pub kind: IrItemKind,
    pub type_name: String,
    #[serde(default)]
    pub methods: Vec<FunctionItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trait_name: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub doc: String,
}

/// Discriminator for IR item kinds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum IrItemKind {
    Function,
    Struct,
    Enum,
    Trait,
    Impl,
}

/// A single item in the IR — can be a function, struct, enum, trait, or impl block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum IrItem {
    Function(FunctionItem),
    Struct(StructItem),
    Enum(EnumItem),
    Trait(TraitItem),
    Impl(ImplItem),
}

impl IrItem {
    pub fn name(&self) -> &str {
        match self {
            IrItem::Function(f) => &f.name,
            IrItem::Struct(s) => &s.name,
            IrItem::Enum(e) => &e.name,
            IrItem::Trait(t) => &t.name,
            IrItem::Impl(i) => &i.type_name,
        }
    }

    pub fn kind(&self) -> &IrItemKind {
        match self {
            IrItem::Function(_) => &IrItemKind::Function,
            IrItem::Struct(_) => &IrItemKind::Struct,
            IrItem::Enum(_) => &IrItemKind::Enum,
            IrItem::Trait(_) => &IrItemKind::Trait,
            IrItem::Impl(_) => &IrItemKind::Impl,
        }
    }

    pub fn doc(&self) -> &str {
        match self {
            IrItem::Function(f) => &f.doc,
            IrItem::Struct(s) => &s.doc,
            IrItem::Enum(e) => &e.doc,
            IrItem::Trait(t) => &t.doc,
            IrItem::Impl(i) => &i.doc,
        }
    }
}

impl IntermediateRepresentation {
    pub fn new(crate_name: String, crate_version: String) -> Self {
        Self {
            vertumnus_ir_version: IR_VERSION.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ir_roundtrip() {
        let ir = IntermediateRepresentation {
            vertumnus_ir_version: "0.1".to_string(),
            crate_name: "test_crate".to_string(),
            crate_version: "1.0.0".to_string(),
            items: vec![
                IrItem::Function(FunctionItem {
                    kind: IrItemKind::Function,
                    name: "add".to_string(),
                    doc: "Adds two integers.".to_string(),
                    inputs: vec![
                        FunctionParameter { name: "a".to_string(), type_str: "i64".to_string() },
                        FunctionParameter { name: "b".to_string(), type_str: "i64".to_string() },
                    ],
                    output: IrType { type_str: "i64".to_string() },
                    is_unsafe: false,
                    is_async: false,
                    has_generics: false,
                    visibility: "public".to_string(),
                }),
            ],
        };

        let json = ir.to_json_pretty().unwrap();
        let deserialized = IntermediateRepresentation::from_json(&json).unwrap();
        assert_eq!(ir, deserialized);
    }
}
