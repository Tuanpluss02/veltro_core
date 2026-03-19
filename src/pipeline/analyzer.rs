use crate::ir::{DataClassIR, FieldIR, TypeKind};
use crate::pipeline::parser::ParsedFile;
use std::fmt;
use std::error::Error;
use tree_sitter::Node;

/// Errors that can occur during AST analysis.
#[derive(Debug)]
pub enum AnalyzeError {
    /// The AST structure was not as expected.
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

/// Analyzes a parsed file to extract all DataClassIRs if @Data() is present.
pub fn analyze(parsed: &ParsedFile) -> Result<Vec<DataClassIR>, AnalyzeError> {
    let mut results = Vec::new();
    find_all_data_classes(parsed.tree.root_node(), &parsed.source, &parsed.path, &mut results)?;
    Ok(results)
}

fn find_all_data_classes(node: Node, source: &str, path: &std::path::PathBuf, results: &mut Vec<DataClassIR>) -> Result<(), AnalyzeError> {
    if (node.kind() == "class_definition" || node.kind() == "class_declaration") && has_data_annotation(node, source) {
        let name_node = node.child_by_field_name("name");
        let class_name = name_node
            .map(|n| n.utf8_text(source.as_bytes()).unwrap_or(""))
            .unwrap_or("");
        let expected_mixin = format!("_${}", class_name);
        let class_text = node.utf8_text(source.as_bytes()).unwrap_or("");

        if !has_with_mixin(class_text, &expected_mixin) {
            eprintln!(
                "  ⚠ {} skipped — missing `with _${}`. Add it to enable generation.",
                class_name, class_name
            );
        } else {
            results.push(extract_data_class(node, source, path.clone())?);
        }
    }
    
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_all_data_classes(child, source, path, results)?;
    }
    
    Ok(())
}

/// Checks whether the class text contains `with _$ClassName`.
fn has_with_mixin(class_text: &str, expected_mixin: &str) -> bool {
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

fn has_data_annotation(node: Node, source: &str) -> bool {
    // Check own children (new grammar style)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "annotation" || child.kind() == "metadata") && 
           child.utf8_text(source.as_bytes()).unwrap_or("").trim() == "@Data()" {
            return true;
        }
    }

    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "annotation" || p.kind() == "metadata" {
            let text = p.utf8_text(source.as_bytes()).unwrap_or("");
            if text.trim() == "@Data()" {
                return true;
            }
        }
        if p.kind().contains("definition") {
             break;
        }
        prev = p.prev_sibling();
    }
    
    if let Some(parent) = node.parent() {
        for i in 0..parent.child_count() {
            let child = parent.child(i as u32).unwrap();
            if (child.kind() == "annotation" || child.kind() == "metadata") && 
               child.utf8_text(source.as_bytes()).unwrap_or("").trim() == "@Data()" {
                return true;
            }
        }
    }
    
    false
}

/// Checks whether a class body contains a `factory ClassName.fromJson(...)` declaration.
fn has_from_json_factory(node: Node, source: &str, class_name: &str) -> bool {
    let class_text = node.utf8_text(source.as_bytes()).unwrap_or("");
    let pattern = format!("{}.fromJson", class_name);
    class_text.contains(&pattern)
}

fn extract_data_class(node: Node, source: &str, path: std::path::PathBuf) -> Result<DataClassIR, AnalyzeError> {
    let name_node = node.child_by_field_name("name")
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

    let body = node.child_by_field_name("body")
        .ok_or_else(|| AnalyzeError::UnexpectedStructure("Class body not found".into()))?;
    
    // Detect user-declared fromJson factory
    let has_from_json = has_from_json_factory(body, source, &name);

    let mut fields = Vec::new();
    let mut cursor = body.walk();
    for member in body.children(&mut cursor) {
        if let Some(f) = try_find_fields(member, source, &name)? {
            fields = f;
            break;
        }
    }

    Ok(DataClassIR {
        name,
        generics,
        fields,
        source_file: path,
        has_from_json,
    })
}

/// Tries to find fields from a constructor, skipping `fromJson` factories.
fn try_find_fields(node: Node, source: &str, class_name: &str) -> Result<Option<Vec<FieldIR>>, AnalyzeError> {
    let kind = node.kind();
    if kind == "constructor_signature" || 
       kind == "factory_constructor_declaration" || 
       kind == "redirecting_factory_constructor_signature" ||
       kind.contains("constructor") {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        if text.contains("factory") {
            // Skip fromJson factory — it's not the primary constructor
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
        node.child_by_field_name("parameter")
            .ok_or_else(|| AnalyzeError::UnexpectedStructure("Missing parameter in default_formal_parameter".into()))?
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
        return Err(AnalyzeError::UnexpectedStructure(format!("Could not parse parameter name: {}", context_text)));
    }

    Ok(FieldIR {
        name,
        type_name,
        generic_args,
        is_required,
        is_nullable,
        resolved_kind: TypeKind::External,
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
        assert_eq!(ir.fields.len(), 3);
        assert_eq!(ir.fields[0].name, "id");
        assert_eq!(ir.fields[0].type_name, "String");
        assert!(ir.fields[0].is_required);
    }
}
