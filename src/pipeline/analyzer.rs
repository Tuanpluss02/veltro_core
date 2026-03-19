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

/// Parses annotation text like "@Data()" or "@SlangGen(arb: 'assets/')" into an AnnotationIR.
fn parse_annotation_text(text: &str) -> Option<AnnotationIR> {
    if !text.starts_with('@') {
        return None;
    }
    let without_at = &text[1..];
    let name = if let Some(paren_idx) = without_at.find('(') {
        without_at[..paren_idx].trim().to_string()
    } else {
        without_at.trim().to_string()
    };

    if name.is_empty() {
        return None;
    }

    // Named argument parsing is best-effort; full support added in future versions
    let arguments = HashMap::new();

    Some(AnnotationIR { name, arguments })
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
    let lookback = start.saturating_sub(10);
    let context_text = &source[lookback..node.end_byte()];
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
        assert!(ir.has_from_json);
        assert!(ir.has_with_mixin);
        assert_eq!(ir.fields.len(), 3);
        assert_eq!(ir.fields[0].name, "id");
        assert_eq!(ir.fields[0].type_name, "String");
        assert!(ir.fields[0].is_required);
        assert_eq!(ir.annotations.len(), 1);
        assert_eq!(ir.annotations[0].name, "Data");
    }

    #[test]
    fn test_analyze_nested() {
        let path = Path::new("testdata/nested.dart");
        let parsed = parse_file(path).unwrap();
        let irs = analyze(&parsed).unwrap();
        assert_eq!(irs.len(), 2);
    }
}
