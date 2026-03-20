use crate::ir::{ClassIR, FieldIR, ResolvedKind, AnnotationIR, IR_VERSION};
use crate::pipeline::parser::ParsedFile;
use std::collections::HashMap;
use std::fmt;
use std::error::Error;
use tree_sitter::Node;

/// Errors that can occur during AST analysis.
#[derive(Debug)]
pub enum AnalyzeError {
    UnexpectedStructure(String),
}

impl fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnalyzeError::UnexpectedStructure(msg) => write!(f, "Unexpected AST structure: {}", msg),
        }
    }
}

impl Error for AnalyzeError {}

/// Default values injected when @Veltro() arguments are absent.
const VELTRO_DEFAULTS: &[(&str, &str)] = &[
    ("json",          "true"),
    ("fieldRename",   "none"),
    ("includeIfNull", "true"),
    ("copyWith",      "true"),
];

/// Analyzes a parsed file and extracts all annotated ClassIRs.
pub fn analyze(parsed: &ParsedFile) -> Result<Vec<ClassIR>, AnalyzeError> {
    let mut results = Vec::new();
    find_all_annotated_classes(parsed.tree.root_node(), &parsed.source, &parsed.path, &mut results)?;
    Ok(results)
}

fn find_all_annotated_classes(
    node: Node,
    source: &str,
    path: &std::path::PathBuf,
    results: &mut Vec<ClassIR>,
) -> Result<(), AnalyzeError> {
    if node.kind() == "class_definition" || node.kind() == "class_declaration" {
        let annotations = collect_annotations(node, source);
        if !annotations.is_empty() {
            results.push(extract_class(node, source, path.clone(), annotations)?);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_all_annotated_classes(child, source, path, results)?;
    }

    Ok(())
}

/// Collects all annotations on a class node.
fn collect_annotations(node: Node, source: &str) -> Vec<AnnotationIR> {
    let mut annotations = Vec::new();
    let mut seen_names = std::collections::HashSet::new();

    // Check own children (new grammar style)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotation" || child.kind() == "metadata" {
            let text = child.utf8_text(source.as_bytes()).unwrap_or("").trim().to_string();
            if let Some(ann) = parse_annotation_text(&text) {
                if seen_names.insert(ann.name.clone()) {
                    annotations.push(ann);
                }
            }
        }
    }

    // Check previous siblings (some grammar styles place annotations before the class node)
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "annotation" || p.kind() == "metadata" {
            let text = p.utf8_text(source.as_bytes()).unwrap_or("").trim().to_string();
            if let Some(ann) = parse_annotation_text(&text) {
                if seen_names.insert(ann.name.clone()) {
                    annotations.push(ann);
                }
            }
        } else if p.kind().contains("definition") || p.kind().contains("declaration") {
            break;
        }
        prev = p.prev_sibling();
    }

    // Check parent for metadata wrappers
    if let Some(parent) = node.parent() {
        let mut cursor = parent.walk();
        for child in parent.children(&mut cursor) {
            if child.kind() == "annotation" || child.kind() == "metadata" {
                let text = child.utf8_text(source.as_bytes()).unwrap_or("").trim().to_string();
                if let Some(ann) = parse_annotation_text(&text) {
                    if seen_names.insert(ann.name.clone()) {
                        annotations.push(ann);
                    }
                }
            }
        }
    }

    annotations
}

/// Parses annotation text like "@Veltro(json: false)" into an AnnotationIR.
fn parse_annotation_text(text: &str) -> Option<AnnotationIR> {
    if !text.starts_with('@') {
        return None;
    }
    let without_at = &text[1..];
    let (name, args_text) = if let Some(paren_idx) = without_at.find('(') {
        let name = without_at[..paren_idx].trim().to_string();
        let after_paren = &without_at[paren_idx + 1..];
        let close_idx = after_paren.rfind(')')?;
        let args = &after_paren[..close_idx];
        (name, args.to_string())
    } else {
        (without_at.trim().to_string(), String::new())
    };

    if name.is_empty() {
        return None;
    }

    let mut arguments: HashMap<String, String> = HashMap::new();

    if name == "Veltro" {
        if !args_text.trim().is_empty() {
            for pair in args_text.split(',') {
                let pair = pair.trim();
                if pair.is_empty() {
                    continue;
                }
                if let Some(colon_idx) = pair.find(':') {
                    let key = pair[..colon_idx].trim().to_string();
                    let raw_value = pair[colon_idx + 1..].trim().to_string();

                    // For FieldRename.snake → extract only the variant: "snake"
                    let value = if key == "fieldRename" {
                        if let Some(dot_idx) = raw_value.rfind('.') {
                            raw_value[dot_idx + 1..].to_string()
                        } else {
                            raw_value
                        }
                    } else {
                        raw_value
                    };

                    arguments.insert(key, value);
                }
            }
        }

        // Inject defaults for missing keys
        for (key, default_val) in VELTRO_DEFAULTS {
            arguments
                .entry(key.to_string())
                .or_insert_with(|| default_val.to_string());
        }
    }

    Some(AnnotationIR { name, arguments })
}

/// Extracts the raw value from @Default(value), e.g. "false", "ThemeMode.dark".
fn extract_default_value(text: &str) -> Option<String> {
    let prefix = "@Default(";
    let start = text.find(prefix)?;
    let after = &text[start + prefix.len()..];

    let mut depth = 1usize;
    let mut end = 0;
    for (i, ch) in after.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth == 0 {
        let val = after[..end].trim().to_string();
        if val.is_empty() {
            None
        } else {
            Some(val)
        }
    } else {
        None
    }
}

fn extract_class(
    node: Node,
    source: &str,
    path: std::path::PathBuf,
    annotations: Vec<AnnotationIR>,
) -> Result<ClassIR, AnalyzeError> {
    let name_node = node
        .child_by_field_name("name")
        .ok_or_else(|| AnalyzeError::UnexpectedStructure("Class name not found".into()))?;
    let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();

    let mut generics = Vec::new();
    if let Some(type_params_list) = node.child_by_field_name("type_parameters") {
        let mut cursor = type_params_list.walk();
        for param in type_params_list.children(&mut cursor) {
            if param.kind() == "type_parameter" {
                if let Some(id_node) = param.child_by_field_name("name") {
                    generics.push(id_node.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                }
            }
        }
    }

    let expected_mixin = format!("_${}", name);
    let class_text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let has_with_mixin = check_has_with_mixin(class_text, &expected_mixin);

    let body = node
        .child_by_field_name("body")
        .ok_or_else(|| AnalyzeError::UnexpectedStructure("Class body not found".into()))?;

    let has_from_json = has_from_json_factory(body, source, &name);

    let mut fields = Vec::new();
    if has_with_mixin {
        let mut cursor = body.walk();
        for member in body.children(&mut cursor) {
            if let Some(f) = try_find_fields(member, source, &name)? {
                fields = f;
                break;
            }
        }
    }

    Ok(ClassIR {
        ir_version: IR_VERSION,
        name,
        generics,
        fields,
        annotations,
        source_file: path,
        has_with_mixin,
        has_from_json,
    })
}

fn check_has_with_mixin(class_text: &str, expected_mixin: &str) -> bool {
    if let Some(with_idx) = class_text.find("with") {
        let after_with = &class_text[with_idx + 4..];
        if let Some(brace_idx) = after_with.find('{') {
            let mixin_section = &after_with[..brace_idx];
            return mixin_section.contains(expected_mixin);
        }
        return after_with.contains(expected_mixin);
    }
    false
}

fn has_from_json_factory(node: Node, source: &str, class_name: &str) -> bool {
    let class_text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let pattern = format!("{}.fromJson", class_name);
    class_text.contains(&pattern)
}

fn try_find_fields(
    node: Node,
    source: &str,
    class_name: &str,
) -> Result<Option<Vec<FieldIR>>, AnalyzeError> {
    let kind = node.kind();
    if kind == "constructor_signature"
        || kind == "factory_constructor_declaration"
        || kind == "redirecting_factory_constructor_signature"
        || kind.contains("constructor")
    {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        if text.contains("factory") {
            let from_json_pattern = format!("{}.fromJson", class_name);
            if text.contains(&from_json_pattern) {
                return Ok(None);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "formal_parameter_list" {
                    return Ok(Some(recursive_extract_fields(child, source)?));
                }
            }
            if let Some(params_list) = node.child_by_field_name("parameters") {
                return Ok(Some(recursive_extract_fields(params_list, source)?));
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(f) = try_find_fields(child, source, class_name)? {
            return Ok(Some(f));
        }
    }

    Ok(None)
}

fn recursive_extract_fields(node: Node, source: &str) -> Result<Vec<FieldIR>, AnalyzeError> {
    let mut fields = Vec::new();
    let kind = node.kind();
    if kind == "formal_parameter" || kind == "normal_formal_parameter" || kind == "default_formal_parameter" {
        fields.push(parse_parameter(node, source)?);
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            fields.extend(recursive_extract_fields(child, source)?);
        }
    }
    Ok(fields)
}

fn parse_parameter(node: Node, source: &str) -> Result<FieldIR, AnalyzeError> {
    let start = node.start_byte();
    // Scan back to the previous ',' or '{' to capture annotations like @Default()
    // that precede this parameter in the source but may be outside the node span.
    let scan_start = {
        let prefix = &source[..start];
        prefix.rfind([',', '{'])
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    let context_text = &source[scan_start..node.end_byte()];
    let default_value = extract_default_value(context_text);
    let is_required = context_text.contains("required");

    let simple_param = if node.kind() == "default_formal_parameter" {
        node.child_by_field_name("parameter").ok_or_else(|| {
            AnalyzeError::UnexpectedStructure("Missing parameter in default_formal_parameter".into())
        })?
    } else {
        node
    };

    let mut name = "".to_string();
    if let Some(n) = simple_param.child_by_field_name("name") {
        name = n.utf8_text(source.as_bytes()).unwrap_or("").to_string();
    }

    let mut type_name = "dynamic".to_string();
    let mut is_nullable = false;
    let mut generic_args = Vec::new();

    if let Some(t) = simple_param.child_by_field_name("type") {
        let t_text = t.utf8_text(source.as_bytes()).unwrap_or("");
        is_nullable = t_text.ends_with('?');
        type_name = t_text.trim_end_matches('?').trim().to_string();

        if let Some(user_type) = t.child_by_field_name("type") {
            if let Some(tn) = user_type.child_by_field_name("name") {
                type_name = tn.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            }
            if let Some(ta) = user_type.child_by_field_name("type_arguments") {
                let mut cursor = ta.walk();
                for arg in ta.children(&mut cursor) {
                    if arg.kind() == "type_annotation" {
                        generic_args.push(arg.utf8_text(source.as_bytes()).unwrap_or("").to_string());
                    }
                }
            }
        }
    } else {
        let mut cursor = simple_param.walk();
        for child in simple_param.children(&mut cursor) {
            match child.kind() {
                "type_identifier" | "type_annotation" | "user_type" => {
                    let t_text = child.utf8_text(source.as_bytes()).unwrap_or("");
                    is_nullable = t_text.ends_with('?');
                    type_name = t_text.trim_end_matches('?').trim().to_string();
                }
                "identifier" if name.is_empty() => {
                    name = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                }
                _ => {}
            }
        }
    }

    if name.is_empty() {
        let mut cursor = simple_param.walk();
        let children: Vec<Node> = simple_param.children(&mut cursor).collect();
        if let Some(last) = children.iter().rev().find(|n| n.kind() == "identifier") {
            name = last.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        }
    }

    // Fallback: use context text to detect nullable types the AST may not expose
    // (e.g. when tree-sitter puts the '?' outside the type node).
    if !is_nullable && !type_name.is_empty() && type_name != "dynamic" {
        let nullable_pattern = format!("{}?", type_name);
        if context_text.contains(&nullable_pattern) {
            is_nullable = true;
        }
    }

    if name.is_empty() {
        return Err(AnalyzeError::UnexpectedStructure(format!(
            "Could not parse parameter name: {}",
            context_text
        )));
    }

    Ok(FieldIR {
        name,
        type_name,
        generic_args,
        is_required,
        is_nullable,
        resolved_kind: ResolvedKind::External,
        is_generic_param: false,
        default_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::parser::parse_file;
    use std::path::Path;

    #[test]
    fn test_analyze_simple() {
        let path = Path::new("testdata/simple.dart");
        let parsed = parse_file(path).unwrap();
        let irs = analyze(&parsed).unwrap();
        assert_eq!(irs.len(), 1);
        let ir = &irs[0];

        assert_eq!(ir.name, "User");
        assert!(!ir.has_from_json);
        assert!(ir.has_with_mixin);
        assert_eq!(ir.fields.len(), 3);
        assert_eq!(ir.fields[0].name, "id");
        assert_eq!(ir.fields[0].type_name, "String");
        assert!(ir.fields[0].is_required);
        assert_eq!(ir.annotations.len(), 1);
        assert_eq!(ir.annotations[0].name, "Veltro");
        assert_eq!(ir.annotations[0].arguments.get("json").map(|s| s.as_str()), Some("true"));
        assert_eq!(ir.annotations[0].arguments.get("fieldRename").map(|s| s.as_str()), Some("none"));
        assert_eq!(ir.annotations[0].arguments.get("includeIfNull").map(|s| s.as_str()), Some("true"));
        assert_eq!(ir.annotations[0].arguments.get("copyWith").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_analyze_nested() {
        let path = Path::new("testdata/nested.dart");
        let parsed = parse_file(path).unwrap();
        let irs = analyze(&parsed).unwrap();
        assert_eq!(irs.len(), 2);
    }

    #[test]
    fn test_veltro_defaults_injected() {
        let ann = parse_annotation_text("@Veltro()").unwrap();
        assert_eq!(ann.name, "Veltro");
        assert_eq!(ann.arguments.get("json").map(|s| s.as_str()), Some("true"));
        assert_eq!(ann.arguments.get("fieldRename").map(|s| s.as_str()), Some("none"));
        assert_eq!(ann.arguments.get("includeIfNull").map(|s| s.as_str()), Some("true"));
        assert_eq!(ann.arguments.get("copyWith").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_veltro_json_false_copy_with_false() {
        let ann = parse_annotation_text("@Veltro(json: false, copyWith: false)").unwrap();
        assert_eq!(ann.arguments.get("json").map(|s| s.as_str()), Some("false"));
        assert_eq!(ann.arguments.get("copyWith").map(|s| s.as_str()), Some("false"));
        assert_eq!(ann.arguments.get("fieldRename").map(|s| s.as_str()), Some("none"));
        assert_eq!(ann.arguments.get("includeIfNull").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_veltro_field_rename_snake() {
        let ann = parse_annotation_text("@Veltro(fieldRename: FieldRename.snake)").unwrap();
        assert_eq!(ann.arguments.get("fieldRename").map(|s| s.as_str()), Some("snake"));
        assert_eq!(ann.arguments.get("json").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_extract_default_value_bool() {
        assert_eq!(extract_default_value("@Default(false) bool isLoading"), Some("false".to_string()));
        assert_eq!(extract_default_value("@Default(true) bool flag"), Some("true".to_string()));
    }

    #[test]
    fn test_extract_default_value_enum() {
        assert_eq!(
            extract_default_value("@Default(ThemeMode.dark) ThemeMode themeMode"),
            Some("ThemeMode.dark".to_string())
        );
        assert_eq!(
            extract_default_value("@Default(ConnectivityStatus.disconnected) ConnectivityStatus connectivity"),
            Some("ConnectivityStatus.disconnected".to_string())
        );
    }

    #[test]
    fn test_extract_default_value_list() {
        assert_eq!(extract_default_value("@Default([]) List<String> tags"), Some("[]".to_string()));
    }

    #[test]
    fn test_extract_default_value_none() {
        assert_eq!(extract_default_value("required String id"), None);
        assert_eq!(extract_default_value("String? nickname"), None);
    }

    #[test]
    fn test_analyze_state() {
        let path = Path::new("testdata/state.dart");
        let parsed = parse_file(path).unwrap();
        let irs = analyze(&parsed).unwrap();
        assert_eq!(irs.len(), 1);
        let ir = &irs[0];
        assert_eq!(ir.name, "AppState");
        assert!(!ir.has_from_json);
        assert!(ir.has_with_mixin);

        let ann = &ir.annotations[0];
        assert_eq!(ann.name, "Veltro");
        assert_eq!(ann.arguments.get("json").map(|s| s.as_str()), Some("false"));

        let is_loading = ir.fields.iter().find(|f| f.name == "isLoading").unwrap();
        assert_eq!(is_loading.default_value, Some("false".to_string()));

        let connectivity = ir.fields.iter().find(|f| f.name == "connectivity").unwrap();
        assert_eq!(connectivity.default_value, Some("ConnectivityStatus.disconnected".to_string()));

        let error = ir.fields.iter().find(|f| f.name == "error").unwrap();
        assert_eq!(error.default_value, None);
    }
}
