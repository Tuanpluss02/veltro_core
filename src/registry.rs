use std::collections::HashMap;
use crate::ir::{DataClassIR, TypeKind};
use crate::pipeline::parser::ParsedFile;
use rayon::prelude::*;
use dashmap::DashMap;
use tree_sitter::Node;

/// Registry to store and look up type kinds.
pub struct TypeRegistry {
    /// Internal map from type name to TypeKind.
    pub types: HashMap<String, TypeKind>,
}

impl TypeRegistry {
    /// Creates a new, empty TypeRegistry.
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
        }
    }

    /// Inserts a new type kind into the registry.
    #[cfg(test)]
    pub fn insert(&mut self, name: String, kind: TypeKind) {
        self.types.insert(name, kind);
    }

    /// Gets the type kind for a given name.
    /// Returns TypeKind::External if the type is not found.
    pub fn get(&self, name: &str) -> TypeKind {
        self.types.get(name).cloned().unwrap_or(TypeKind::External)
    }

    /// Builds a TypeRegistry from all discovered DataClasses and all parsed files.
    pub fn build(ir_list: &[DataClassIR], parsed_files: &[ParsedFile]) -> Self {
        let registry = DashMap::new();

        // Pass 1: Collect DataClasses
        ir_list.par_iter().for_each(|ir| {
            registry.insert(ir.name.clone(), TypeKind::DataClass);
        });

        // Pass 1: Collect Enums and @IsEnum
        parsed_files.par_iter().for_each(|parsed| {
            collect_enums(parsed.tree.root_node(), &parsed.source, &registry);
        });

        Self {
            types: registry.into_iter().collect(),
        }
    }
}

fn collect_enums(node: Node, source: &str, registry: &DashMap<String, TypeKind>) {
    let kind = node.kind();
    if kind == "enum_declaration" || ((kind == "class_definition" || kind == "class_declaration") && has_is_enum_annotation(node, source)) {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            if !name.is_empty() {
                registry.insert(name, TypeKind::Enum);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_enums(child, source, registry);
    }
}

fn has_is_enum_annotation(node: Node, source: &str) -> bool {
    // Check own children (new grammar style)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "annotation" || child.kind() == "metadata") && 
           child.utf8_text(source.as_bytes()).unwrap_or("").trim() == "@IsEnum()" {
            return true;
        }
    }

    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "annotation" || p.kind() == "metadata" {
            let text = p.utf8_text(source.as_bytes()).unwrap_or("");
            if text.trim() == "@IsEnum()" {
                return true;
            }
        }
        if p.kind().contains("definition") {
            break;
        }
        prev = p.prev_sibling();
    }
    
    // Check parent for metadata wrapper
    if let Some(parent) = node.parent() {
        for i in 0..parent.child_count() {
            let child = parent.child(i as u32).unwrap();
            if (child.kind() == "annotation" || child.kind() == "metadata") && 
               child.utf8_text(source.as_bytes()).unwrap_or("").trim() == "@IsEnum()" {
                return true;
            }
        }
    }
    
    false
}
