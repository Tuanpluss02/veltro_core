use super::{GeneratedFile, VeltroPlugin};
use crate::ir::{ClassIR, FieldIR, ResolvedKind};
use std::collections::HashMap;

pub struct VeltroCodePlugin;

#[derive(Debug)]
enum FieldRename {
    None,
    Snake,
    Kebab,
    ScreamingSnake,
    Pascal,
}

struct VeltroOptions {
    json: bool,
    field_rename: FieldRename,
    include_if_null: bool,
    create_copy_with: bool,
}

impl VeltroOptions {
    fn from_arguments(args: &HashMap<String, String>) -> Self {
        Self {
            json: args.get("json").map(|s| s != "false").unwrap_or(true),
            field_rename: match args.get("fieldRename").map(|s| s.as_str()) {
                Some("snake")          => FieldRename::Snake,
                Some("kebab")          => FieldRename::Kebab,
                Some("screamingSnake") => FieldRename::ScreamingSnake,
                Some("pascal")         => FieldRename::Pascal,
                _                      => FieldRename::None,
            },
            include_if_null: args.get("includeIfNull").map(|s| s != "false").unwrap_or(true),
            create_copy_with: args.get("copyWith").map(|s| s != "false").unwrap_or(true),
        }
    }
}

impl VeltroPlugin for VeltroCodePlugin {
    fn annotation(&self) -> &str {
        "Veltro"
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

        let veltro_ann = ir.annotations.iter().find(|a| a.name == "Veltro");
        let opts = if let Some(ann) = veltro_ann {
            VeltroOptions::from_arguments(&ann.arguments)
        } else {
            VeltroOptions::from_arguments(&HashMap::new())
        };

        let output_path = ir
            .source_file
            .with_extension("g.dart")
            .to_string_lossy()
            .to_string();

        vec![GeneratedFile {
            path: output_path,
            content: generate_dart(ir, &opts),
        }]
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.char_indices() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap());
    }
    result
}

fn to_json_key(field_name: &str, rename: &FieldRename) -> String {
    match rename {
        FieldRename::None          => field_name.to_string(),
        FieldRename::Snake         => to_snake_case(field_name),
        FieldRename::Kebab         => to_snake_case(field_name).replace('_', "-"),
        FieldRename::ScreamingSnake => to_snake_case(field_name).to_uppercase(),
        FieldRename::Pascal        => {
            let mut c = field_name.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str())
             .unwrap_or_default()
        }
    }
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

fn generate_from_json_field(field: &FieldIR, rename: &FieldRename) -> String {
    let json_key = to_json_key(&field.name, rename);

    if field.is_generic_param {
        return format!(
            "{}: fromJson{}(json['{}'])",
            field.name, field.type_name, json_key
        );
    }

    match field.resolved_kind {
        ResolvedKind::AnnotatedClass => {
            if field.is_nullable {
                format!(
                    "{}: json['{}'] != null ? {}.fromJson(json['{}'] as Map<String, dynamic>) : null",
                    field.name, json_key, field.type_name, json_key
                )
            } else {
                format!(
                    "{}: {}.fromJson(json['{}'] as Map<String, dynamic>)",
                    field.name, field.type_name, json_key
                )
            }
        }
        ResolvedKind::Enum => {
            if field.is_nullable {
                format!(
                    "{}: json['{}'] != null ? {}.values.byName(json['{}'] as String) : null",
                    field.name, json_key, field.type_name, json_key
                )
            } else {
                format!(
                    "{}: {}.values.byName(json['{}'] as String)",
                    field.name, field.type_name, json_key
                )
            }
        }
        ResolvedKind::External => {
            let type_str = get_type_string(field);
            format!("{}: json['{}'] as {}", field.name, json_key, type_str)
        }
    }
}

fn generate_dart(ir: &ClassIR, opts: &VeltroOptions) -> String {
    let mut code = String::new();
    code.push('\n');

    let generics_decl = if ir.generics.is_empty() {
        "".to_string()
    } else {
        format!("<{}>", ir.generics.join(", "))
    };

    // ── Mixin ──────────────────────────────────────────────────────────────
    code.push_str(&format!("\nmixin _${}{} {{\n", ir.name, generics_decl));

    for field in &ir.fields {
        let type_str = get_type_string(field);
        code.push_str(&format!("  {} get {};\n", type_str, field.name));
    }
    code.push('\n');

    if opts.create_copy_with {
        code.push_str(&format!("  {}{} copyWith({{", ir.name, generics_decl));
        for field in &ir.fields {
            let type_str = get_type_string(field);
            let type_name_no_q = type_str.trim_end_matches('?');
            code.push_str(&format!("{}? {}, ", type_name_no_q, field.name));
        }
        code.push_str("});\n");
    }

    if opts.json {
        code.push_str("  Map<String, dynamic> toJson();\n");
    }

    code.push('\n');
    code.push_str("  @override\n");
    code.push_str("  String toString();\n");
    code.push_str("}\n");

    // ── Concrete class ─────────────────────────────────────────────────────
    code.push_str(&format!(
        "\nclass _{}{} with _${}{} implements {}{} {{\n",
        ir.name, generics_decl, ir.name, generics_decl, ir.name, generics_decl
    ));

    for field in &ir.fields {
        let type_str = get_type_string(field);
        code.push_str(&format!("  @override final {} {};\n", type_str, field.name));
    }
    code.push('\n');

    // Constructor with @Default() support
    code.push_str(&format!("  const _{}({{\n", ir.name));
    for field in &ir.fields {
        if let Some(ref default_val) = field.default_value {
            code.push_str(&format!("    this.{} = {},\n", field.name, default_val));
        } else if field.is_required {
            code.push_str(&format!("    required this.{},\n", field.name));
        } else {
            code.push_str(&format!("    this.{},\n", field.name));
        }
    }
    code.push_str("  });\n");

    // fromJson factory (only when json: true)
    if opts.json {
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
            let line = generate_from_json_field(field, &opts.field_rename);
            code.push_str(&format!("    {},\n", line));
        }
        code.push_str("  );\n");
    }

    // copyWith (only when copyWith: true)
    if opts.create_copy_with {
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
    }

    // toJson (only when json: true)
    if opts.json {
        if opts.include_if_null {
            code.push_str("\n  @override\n  Map<String, dynamic> toJson() => {\n");
            for field in &ir.fields {
                let json_key = to_json_key(&field.name, &opts.field_rename);
                let val = if field.is_nullable {
                    match field.resolved_kind {
                        ResolvedKind::AnnotatedClass => format!("{}?.toJson()", field.name),
                        ResolvedKind::Enum => format!("{}?.name", field.name),
                        _ => field.name.clone(),
                    }
                } else {
                    match field.resolved_kind {
                        ResolvedKind::AnnotatedClass => format!("{}.toJson()", field.name),
                        ResolvedKind::Enum => format!("{}.name", field.name),
                        _ => field.name.clone(),
                    }
                };
                code.push_str(&format!("    '{}': {},\n", json_key, val));
            }
            code.push_str("  };\n");
        } else {
            // if-guard pattern for nullable fields
            let non_nullable: Vec<&FieldIR> = ir.fields.iter().filter(|f| !f.is_nullable).collect();
            let nullable: Vec<&FieldIR> = ir.fields.iter().filter(|f| f.is_nullable).collect();

            code.push_str("\n  @override\n  Map<String, dynamic> toJson() {\n");
            code.push_str("    final map = <String, dynamic>{\n");
            for field in &non_nullable {
                let json_key = to_json_key(&field.name, &opts.field_rename);
                let val = match field.resolved_kind {
                    ResolvedKind::AnnotatedClass => format!("{}.toJson()", field.name),
                    ResolvedKind::Enum => format!("{}.name", field.name),
                    _ => field.name.clone(),
                };
                code.push_str(&format!("      '{}': {},\n", json_key, val));
            }
            code.push_str("    };\n");
            for field in &nullable {
                let json_key = to_json_key(&field.name, &opts.field_rename);
                let val = match field.resolved_kind {
                    ResolvedKind::AnnotatedClass => format!("{}?.toJson()", field.name),
                    ResolvedKind::Enum => format!("{}?.name", field.name),
                    _ => field.name.clone(),
                };
                code.push_str(&format!(
                    "    if ({} != null) map['{}'] = {};\n",
                    field.name, json_key, val
                ));
            }
            code.push_str("    return map;\n");
            code.push_str("  }\n");
        }
    }

    // == operator
    code.push_str(&format!(
        "\n  @override\n  bool operator ==(Object other) =>\n    identical(this, other) ||\n    (other is _{}{}",
        ir.name, generics_decl
    ));
    for field in &ir.fields {
        code.push_str(&format!(" && other.{} == {}", field.name, field.name));
    }
    code.push_str(");\n");

    // hashCode
    code.push_str("\n  @override\n  int get hashCode => Object.hashAll([");
    let field_names: Vec<&str> = ir.fields.iter().map(|f| f.name.as_str()).collect();
    code.push_str(&field_names.join(", "));
    code.push_str("]);\n");

    // toString
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

    // Extension for fromJson (only when json: true)
    if opts.json {
        code.push_str(&generate_json_extension(ir));
    }

    code
}

fn generate_json_extension(ir: &ClassIR) -> String {
    let mut code = String::new();

    if ir.generics.is_empty() {
        code.push_str(&format!(
            "\nextension _${}JsonExtension on {} {{\n",
            ir.name, ir.name
        ));
        code.push_str("  // ignore: unused_element\n");
        code.push_str(&format!(
            "  static {} fromJson(Map<String, dynamic> json) =>\n      _{}.fromJson(json);\n",
            ir.name, ir.name
        ));
        code.push_str("}\n");
    } else {
        let generics_decl = format!("<{}>", ir.generics.join(", "));
        let extra_params: String = ir.generics
            .iter()
            .map(|g| format!(", {} Function(Object?) fromJson{}", g, g))
            .collect::<Vec<_>>()
            .join("");
        let extra_args: String = ir.generics
            .iter()
            .map(|g| format!(", fromJson{}", g))
            .collect::<Vec<_>>()
            .join("");

        code.push_str(&format!(
            "\nextension _${}JsonExtension{} on {}{} {{\n",
            ir.name, generics_decl, ir.name, generics_decl
        ));
        code.push_str("  // ignore: unused_element\n");
        code.push_str(&format!(
            "  static {}{} fromJson{}(Map<String, dynamic> json{}) =>\n      _{}{}.fromJson(json{});\n",
            ir.name, generics_decl,
            generics_decl,
            extra_params,
            ir.name, generics_decl,
            extra_args
        ));
        code.push_str("}\n");
    }

    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ClassIR, FieldIR, ResolvedKind, AnnotationIR, IR_VERSION};
    use std::path::PathBuf;

    fn veltro_annotation(args: &[(&str, &str)]) -> AnnotationIR {
        let mut arguments = HashMap::new();
        // inject defaults
        for (k, v) in &[("json", "true"), ("fieldRename", "none"), ("includeIfNull", "true"), ("copyWith", "true")] {
            arguments.insert(k.to_string(), v.to_string());
        }
        for (k, v) in args {
            arguments.insert(k.to_string(), v.to_string());
        }
        AnnotationIR { name: "Veltro".to_string(), arguments }
    }

    fn make_class(name: &str, generics: Vec<String>, fields: Vec<FieldIR>, ann: AnnotationIR) -> ClassIR {
        ClassIR {
            ir_version: IR_VERSION,
            name: name.to_string(),
            generics,
            fields,
            annotations: vec![ann],
            source_file: PathBuf::from(format!("{}.dart", name.to_lowercase())),
            has_with_mixin: true,
            has_from_json: false,
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
            default_value: None,
        }
    }

    fn make_nullable_field(name: &str, type_name: &str, kind: ResolvedKind) -> FieldIR {
        FieldIR {
            name: name.to_string(),
            type_name: type_name.to_string(),
            generic_args: vec![],
            is_required: false,
            is_nullable: true,
            resolved_kind: kind,
            is_generic_param: false,
            default_value: None,
        }
    }

    fn make_default_field(name: &str, type_name: &str, default_val: &str) -> FieldIR {
        FieldIR {
            name: name.to_string(),
            type_name: type_name.to_string(),
            generic_args: vec![],
            is_required: false,
            is_nullable: false,
            resolved_kind: ResolvedKind::External,
            is_generic_param: false,
            default_value: Some(default_val.to_string()),
        }
    }

    #[test]
    fn test_generate_json_true_default() {
        let ir = make_class(
            "User",
            vec![],
            vec![make_field("id", "String", ResolvedKind::External, false)],
            veltro_annotation(&[]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("mixin _$User {"), "Missing mixin");
        assert!(output.contains("String get id;"), "Missing getter");
        assert!(output.contains("User copyWith("), "Missing copyWith in mixin");
        assert!(output.contains("Map<String, dynamic> toJson();"), "Missing toJson in mixin");
        assert!(output.contains("class _User with _$User implements User {"), "Missing concrete class");
        assert!(output.contains("factory _User.fromJson("), "Missing fromJson factory");
        assert!(output.contains("extension _$UserJsonExtension on User {"), "Missing extension");
        assert!(output.contains("static User fromJson(Map<String, dynamic> json) =>"), "Missing static fromJson");
        assert!(!output.contains("_$UserFromJson"), "Old top-level helper should not be emitted");
    }

    #[test]
    fn test_generate_json_false() {
        let ir = make_class(
            "AppState",
            vec![],
            vec![make_field("count", "int", ResolvedKind::External, false)],
            veltro_annotation(&[("json", "false")]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(!output.contains("fromJson"), "No fromJson when json: false");
        assert!(!output.contains("toJson"), "No toJson when json: false");
        assert!(!output.contains("extension"), "No extension when json: false");
        assert!(output.contains("mixin _$AppState {"), "Missing mixin");
        assert!(output.contains("AppState copyWith("), "copyWith still generated");
    }

    #[test]
    fn test_generate_copy_with_false() {
        let ir = make_class(
            "IncrementEvent",
            vec![],
            vec![make_field("amount", "int", ResolvedKind::External, false)],
            veltro_annotation(&[("json", "false"), ("copyWith", "false")]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(!output.contains("copyWith"), "No copyWith when copyWith: false");
        assert!(!output.contains("fromJson"), "No fromJson when json: false");
        assert!(output.contains("mixin _$IncrementEvent {"), "Missing mixin");
        assert!(output.contains("class _IncrementEvent"), "Missing concrete class");
    }

    #[test]
    fn test_generate_field_rename_snake() {
        let ir = make_class(
            "UserDto",
            vec![],
            vec![make_field("userId", "String", ResolvedKind::External, false)],
            veltro_annotation(&[("fieldRename", "snake")]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("json['user_id']"), "fromJson should use snake_case key");
        assert!(output.contains("'user_id': userId"), "toJson should use snake_case key");
    }

    #[test]
    fn test_generate_include_if_null_false() {
        let ir = make_class(
            "UserDto",
            vec![],
            vec![
                make_field("userId", "String", ResolvedKind::External, false),
                make_nullable_field("nickname", "String", ResolvedKind::External),
            ],
            veltro_annotation(&[("includeIfNull", "false")]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("final map = <String, dynamic>"), "Should use if-guard pattern");
        assert!(output.contains("if (nickname != null) map['nickname'] = nickname"), "Nullable field should be guarded");
        assert!(output.contains("'userId': userId"), "Non-nullable field always included");
    }

    #[test]
    fn test_generate_default_value() {
        let fields = vec![
            make_default_field("isLoading", "bool", "false"),
            make_default_field("themeMode", "ThemeMode", "ThemeMode.dark"),
            make_nullable_field("error", "String", ResolvedKind::External),
        ];
        let ir = make_class("AppState", vec![], fields, veltro_annotation(&[("json", "false")]));
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("this.isLoading = false,"), "@Default(false) → = false");
        assert!(output.contains("this.themeMode = ThemeMode.dark,"), "@Default(ThemeMode.dark) → = ThemeMode.dark");
        assert!(output.contains("this.error,"), "Nullable field without default");
        assert!(!output.contains("required this.isLoading"), "Default field must not be required");
        assert!(!output.contains("required this.themeMode"), "Default field must not be required");
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
            veltro_annotation(&[]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("mixin _$ApiResponse<T>"), "Missing generic mixin");
        assert!(output.contains("class _ApiResponse<T> with _$ApiResponse<T> implements ApiResponse<T>"), "Missing generic class");
        assert!(output.contains("data: fromJsonT(json['data'])"), "Missing generic fromJson usage");
        assert!(output.contains("extension _$ApiResponseJsonExtension<T> on ApiResponse<T>"), "Missing generic extension");
        assert!(output.contains("static ApiResponse<T> fromJson<T>(Map<String, dynamic> json, T Function(Object?) fromJsonT)"), "Missing generic static method");
    }

    #[test]
    fn test_generate_annotated_class_field() {
        let ir = make_class(
            "Person",
            vec![],
            vec![make_field("address", "Address", ResolvedKind::AnnotatedClass, false)],
            veltro_annotation(&[]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("address: Address.fromJson(json['address'] as Map<String, dynamic>)"));
        assert!(output.contains("'address': address.toJson()"));
    }

    #[test]
    fn test_generate_enum_field() {
        let ir = make_class(
            "User",
            vec![],
            vec![make_field("status", "Status", ResolvedKind::Enum, false)],
            veltro_annotation(&[]),
        );
        let opts = VeltroOptions::from_arguments(&ir.annotations[0].arguments);
        let output = generate_dart(&ir, &opts);

        assert!(output.contains("Status.values.byName(json['status'] as String)"));
        assert!(output.contains("'status': status.name"));
    }
}
