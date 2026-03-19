use std::path::PathBuf;

/// Intermediate representation of a Dart class annotated with @Data().
#[derive(Debug, Clone)]
pub struct DataClassIR {
    /// The name of the class.
    pub name: String,
    /// Generic type parameters, e.g. ["T"].
    pub generics: Vec<String>,
    /// All fields in the factory constructor.
    pub fields: Vec<FieldIR>,
    /// Path to the source file where this class is defined.
    pub source_file: PathBuf,
    /// Whether the user declared `factory ClassName.fromJson(...)` in the class body.
    pub has_from_json: bool,
}

/// Intermediate representation of a class field.
#[derive(Debug, Clone)]
pub struct FieldIR {
    /// The name of the field.
    pub name: String,
    /// The type name, e.g. "String" or "List".
    pub type_name: String,
    /// Generic type arguments, e.g. ["String"] for "List<String>".
    pub generic_args: Vec<String>,
    /// Whether the field is marked as required.
    pub is_required: bool,
    /// Whether the type is nullable (ends with ?).
    pub is_nullable: bool,
    /// The resolved kind of the type (populated in Pass 2).
    pub resolved_kind: TypeKind,
    /// Whether the type is one of the class's generic parameters.
    pub is_generic_param: bool,
}

/// The kind of a type, used to determine how to generate code.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TypeKind {
    /// A class annotated with @Data() within the project.
    DataClass,
    /// An enum within the project.
    Enum,
    /// An external type or primitive.
    #[default]
    External,
}
