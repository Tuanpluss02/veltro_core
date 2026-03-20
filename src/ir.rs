use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const IR_VERSION: u32 = 1;

/// A Dart class that has at least one annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassIR {
    pub ir_version: u32,
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<FieldIR>,
    /// ALL annotations on this class. Plugins filter by annotation name.
    pub annotations: Vec<AnnotationIR>,
    pub source_file: PathBuf,
    /// True if class declares `with _$ClassName`.
    pub has_with_mixin: bool,
    /// True if class declares `factory ClassName.fromJson(...)`.
    pub has_from_json: bool,
}

/// Intermediate representation of a class field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldIR {
    pub name: String,
    pub type_name: String,
    pub generic_args: Vec<String>,
    pub is_required: bool,
    pub is_nullable: bool,
    /// The resolved kind of the type (populated in Pass 2).
    pub resolved_kind: ResolvedKind,
    pub is_generic_param: bool,
    /// Raw default value string from @Default(), e.g. "false", "ThemeMode.dark".
    /// None if no @Default() annotation is present.
    pub default_value: Option<String>,
}

/// An annotation found on a Dart class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationIR {
    /// Name without @ or (), e.g. "Data", "SlangGen".
    pub name: String,
    /// Named arguments e.g. {"field_rename": "snake"}.
    pub arguments: std::collections::HashMap<String, String>,
}

/// The resolved kind of a type, used to determine how to generate code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResolvedKind {
    /// A class annotated with any veltro annotation (was TypeKind::DataClass).
    AnnotatedClass,
    /// An enum within the project.
    Enum,
    /// An external type or primitive.
    #[default]
    External,
}
