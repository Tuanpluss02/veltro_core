use super::{GeneratedFile, VeltroPlugin};
use crate::ir::{ClassIR, FieldIR, ResolvedKind};
use std::collections::HashMap;

pub struct DataPlugin;

impl VeltroPlugin for DataPlugin {
    fn annotation(&self) -> &str {
        "Data"
    }

    fn generate(
        &self,
        ir: &ClassIR,
        _options: &HashMap<String, String>,
    ) -> Vec<GeneratedFile> {
        if !ir.has_with_mixin {
            eprintln!(
                "  ⚠ {} skipped — missing `with _${}`. Add it to enable generation.",
                ir.name, ir.name
            );
            return vec![];
        }

        let output_path = ir
            .source_file
            .with_extension("g.dart")
            .to_string_lossy()
            .to_string();

        vec![GeneratedFile {
            path: output_path,
            content: generate_dart(ir),
        }]
    }
}

/// Generates the class body for a single ClassIR.
/// Does NOT include the file header or `part of` directive — the pipeline adds those.
/// Starts with `\n` to match the blank-line separator the old pipeline loop produced.
fn generate_dart(ir: &ClassIR) -> String {
    let mut code = String::new();

    // Blank line before this class (mirrors the old `code.push('\n')` in the loop)
    code.push('\n');

    let generics_decl = if ir.generics.is_empty() {
        "".to_string()
    } else {
        format!("<{}>", ir.generics.join(", "))
    };

    // ── 3. Mixin ──────────────────────────────────────────────────────────────
    code.push_str(&format!("\nmixin _${}{} {{\n", ir.name, generics_decl));

    for field in &ir.fields {
        let type_str = get_type_string(field);
        code.push_str(&format!("  {} get {};\n", type_str, field.name));
    }
    code.push('\n');

    code.push_str(&format!("  {}{} copyWith({{", ir.name, generics_decl));
    for field in &ir.fields {
        let type_str = get_type_string(field);
        let type_name_no_q = type_str.trim_end_matches('?');
        code.push_str(&format!("{}? {}, ", type_name_no_q, field.name));
    }
    code.push_str("});\n");

    code.push_str("  Map<String, dynamic> toJson();\n");
    code.push('\n');
    code.push_str("  @override\n");
    code.push_str("  String toString();\n");
    code.push_str("}\n");

    // ── 4. Concrete class ─────────────────────────────────────────────────────
    code.push_str(&format!(
        "\nclass _{}{} with _${}{} implements {}{} {{\n",
        ir.name, generics_decl, ir.name, generics_decl, ir.name, generics_decl
    ));

    for field in &ir.fields {
        let type_str = get_type_string(field);
        code.push_str(&format!("  @override final {} {};\n", type_str, field.name));
    }
    code.push('\n');

    code.push_str(&format!("  const _{}({{\n", ir.name));
    for field in &ir.fields {
        let required = if field.is_required { "required " } else { "" };
        code.push_str(&format!("    {}this.{},\n", required, field.name));
    }
    code.push_str("  });\n");

    let from_json_params = if ir.generics.is_empty() {
        "".to_string()
    } else {
        ir.generics
            .iter()
            .map(|g| format!(", {} Function(Object?) fromJson{}", g, g))
            .collect::<Vec<_>>()
            .join("")
    };

    code.push_str(&format!(
        "\n  factory _{}.fromJson(Map<String, dynamic> json{}) => _{}(\n",
        ir.name, from_json_params, ir.name
    ));
    for field in &ir.fields {
        let line = generate_from_json_field(field);
        code.push_str(&format!("    {},\n", line));
    }
    code.push_str("  );\n");

    code.push_str(&format!("\n  @override\n  {}{} copyWith({{\n", ir.name, generics_decl));
    for field in &ir.fields {
        let type_str = get_type_string(field);
        let type_name_no_q = type_str.trim_end_matches('?');
        code.push_str(&format!("    {}? {},\n", type_name_no_q, field.name));
    }
    code.push_str("  }) => _{}(\n".replace("{}", &ir.name).as_str());
    for field in &ir.fields {
        code.push_str(&format!("    {}: {} ?? this.{},\n", field.name, field.name, field.name));
    }
    code.push_str("  );\n");

    code.push_str("\n  @override\n  Map<String, dynamic> toJson() => {\n");
    for field in &ir.fields {
        let val = match field.resolved_kind {
            ResolvedKind::AnnotatedClass => format!("{}.toJson()", field.name),
            ResolvedKind::Enum => format!("{}.name", field.name),
            _ => field.name.clone(),
        };
        code.push_str(&format!("    '{}': {},\n", field.name, val));
    }
    code.push_str("  };\n");

    code.push_str(&format!(
        "\n  @override\n  bool operator ==(Object other) =>\n    identical(this, other) ||\n    (other is _{}{}",
        ir.name, generics_decl
    ));
    for field in &ir.fields {
        code.push_str(&format!(" && other.{} == {}", field.name, field.name));
    }
    code.push_str(");\n");

    code.push_str("\n  @override\n  int get hashCode => Object.hashAll([");
    let field_names: Vec<&str> = ir.fields.iter().map(|f| f.name.as_str()).collect();
    code.push_str(&field_names.join(", "));
    code.push_str("]);\n");

    let fields_str: Vec<String> = ir
        .fields
        .iter()
        .map(|f| format!("{}: ${}", f.name, f.name))
        .collect();
    code.push_str(&format!(
        "\n  @override\n  String toString() => '{}({})';",
        ir.name,
        fields_str.join(", ")
    ));
    code.push_str("\n}\n");

    // ── 5. Top-level helper (only if has_from_json) ───────────────────────────
    if ir.has_from_json {
        let args = if ir.generics.is_empty() {
            "".to_string()
        } else {
            ir.generics
                .iter()
                .map(|g| format!(", fromJson{}", g))
                .collect::<Vec<_>>()
                .join("")
        };
        code.push_str(&format!(
            "\n{}{} _${}FromJson{}(Map<String, dynamic> json{}) =>\n  _{}.fromJson(json{});\n",
            ir.name, generics_decl, ir.name, generics_decl, from_json_params, ir.name, args
        ));
    }

    code
}

fn get_type_string(field: &FieldIR) -> String {
    let mut s = field.type_name.clone();
    if !field.generic_args.is_empty() {
        s.push_str(&format!("<{}>", field.generic_args.join(", ")));
    }
    if field.is_nullable {
        s.push('?');
    }
    s
}

fn generate_from_json_field(field: &FieldIR) -> String {
    if field.is_generic_param {
        return format!(
            "{}: fromJson{}(json['{}'])",
            field.name, field.type_name, field.name
        );
    }

    match field.resolved_kind {
        ResolvedKind::AnnotatedClass => format!(
            "{}: {}.fromJson(json['{}'] as Map<String, dynamic>)",
            field.name, field.type_name, field.name
        ),
        ResolvedKind::Enum => format!(
            "{}: {}.values.byName(json['{}'] as String)",
            field.name, field.type_name, field.name
        ),
        ResolvedKind::External => {
            let type_str = get_type_string(field);
            format!("{}: json['{}'] as {}", field.name, field.name, type_str)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ClassIR, FieldIR, ResolvedKind, AnnotationIR, IR_VERSION};
    use std::path::PathBuf;

    fn data_annotation() -> AnnotationIR {
        AnnotationIR { name: "Data".to_string(), arguments: Default::default() }
    }

    fn make_class(name: &str, generics: Vec<String>, fields: Vec<FieldIR>, has_from_json: bool) -> ClassIR {
        ClassIR {
            ir_version: IR_VERSION,
            name: name.to_string(),
            generics,
            fields,
            annotations: vec![data_annotation()],
            source_file: PathBuf::from(format!("{}.dart", name.to_lowercase())),
            has_with_mixin: true,
            has_from_json,
        }
    }

    fn make_field(name: &str, type_name: &str, kind: ResolvedKind, is_generic_param: bool) -> FieldIR {
        FieldIR {
            name: name.to_string(),
            type_name: type_name.to_string(),
            generic_args: vec![],
            is_required: true,
            is_nullable: false,
            resolved_kind: kind,
            is_generic_param,
        }
    }

    #[test]
    fn test_generate_simple() {
        let ir = make_class(
            "User",
            vec![],
            vec![make_field("id", "String", ResolvedKind::External, false)],
            true,
        );

        let output = generate_dart(&ir);

        assert!(output.contains("mixin _$User {"), "Missing mixin declaration");
        assert!(output.contains("String get id;"), "Missing abstract getter");
        assert!(output.contains("User copyWith("), "Missing copyWith signature in mixin");
        assert!(output.contains("Map<String, dynamic> toJson();"), "Missing toJson signature in mixin");
        assert!(output.contains("class _User with _$User implements User {"), "Missing concrete class");
        assert!(output.contains("@override final String id;"), "Missing @override field");
        assert!(output.contains("factory _User.fromJson("), "Missing fromJson factory");
        assert!(output.contains("User _$UserFromJson("), "Missing top-level helper");
    }

    #[test]
    fn test_generate_no_from_json() {
        let ir = make_class(
            "User",
            vec![],
            vec![make_field("id", "String", ResolvedKind::External, false)],
            false,
        );

        let output = generate_dart(&ir);
        assert!(!output.contains("_$UserFromJson"), "Top-level helper should not be emitted");
        assert!(output.contains("mixin _$User {"), "Missing mixin");
        assert!(output.contains("class _User with _$User implements User {"), "Missing concrete class");
    }

    #[test]
    fn test_generate_annotated_class_field() {
        let ir = make_class(
            "Person",
            vec![],
            vec![make_field("address", "Address", ResolvedKind::AnnotatedClass, false)],
            true,
        );

        let output = generate_dart(&ir);
        assert!(
            output.contains("address: Address.fromJson(json['address'] as Map<String, dynamic>)"),
            "AnnotatedClass fromJson should use .fromJson()"
        );
        assert!(output.contains("'address': address.toJson()"), "AnnotatedClass toJson should use .toJson()");
    }

    #[test]
    fn test_generate_enum_field() {
        let ir = make_class(
            "User",
            vec![],
            vec![make_field("status", "Status", ResolvedKind::Enum, false)],
            true,
        );

        let output = generate_dart(&ir);
        assert!(
            output.contains("Status.values.byName(json['status'] as String)"),
            "Enum fromJson should use .values.byName()"
        );
        assert!(output.contains("'status': status.name"), "Enum toJson should use .name");
    }

    #[test]
    fn test_generate_generic() {
        let ir = make_class(
            "ApiResponse",
            vec!["T".to_string()],
            vec![
                make_field("success", "bool", ResolvedKind::External, false),
                make_field("data", "T", ResolvedKind::External, true),
            ],
            true,
        );

        let output = generate_dart(&ir);
        assert!(output.contains("mixin _$ApiResponse<T>"), "Missing generic mixin");
        assert!(
            output.contains("class _ApiResponse<T> with _$ApiResponse<T> implements ApiResponse<T>"),
            "Missing generic concrete class"
        );
        assert!(
            output.contains("data: fromJsonT(json['data'])"),
            "Missing generic fromJson parameter usage"
        );
        assert!(
            output.contains("ApiResponse<T> _$ApiResponseFromJson<T>"),
            "Missing generic top-level helper"
        );
    }
}
