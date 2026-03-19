use std::collections::HashMap;
use crate::ir::{ClassIR, ResolvedKind};
use crate::pipeline::parser::ParsedFile;
use rayon::prelude::*;
use dashmap::DashMap;
use tree_sitter::Node;

/// Registry to store and look up resolved type kinds.
pub struct TypeRegistry {
    pub types: HashMap<String, ResolvedKind>,
}

impl TypeRegistry {
    /// Creates a new, empty TypeRegistry (test only).
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
        }
    }

    /// Inserts a type kind into the registry (test only).
    #[cfg(test)]
    pub fn insert(&mut self, name: String, kind: ResolvedKind) {
        self.types.insert(name, kind);
    }

    /// Gets the resolved kind for a given name.
    /// Returns ResolvedKind::External if the type is not found.
    pub fn get(&self, name: &str) -> ResolvedKind {
        self.types.get(name).cloned().unwrap_or(ResolvedKind::External)
    }

    /// Builds a TypeRegistry from all discovered ClassIRs and all parsed files.
    pub fn build(ir_list: &[ClassIR], parsed_files: &[ParsedFile]) -> Self {
        let registry: DashMap<String, ResolvedKind> = DashMap::new();

        // Pass 1: Register annotated classes; @IsEnum() annotation → Enum kind
        ir_list.par_iter().for_each(|ir| {
            let has_is_enum = ir.annotations.iter().any(|a| a.name == "IsEnum");
            if has_is_enum {
                registry.insert(ir.name.clone(), ResolvedKind::Enum);
            } else {
                registry.insert(ir.name.clone(), ResolvedKind::AnnotatedClass);
            }
        });

        // Pass 1: Also collect native Dart enum declarations from parsed files
        parsed_files.par_iter().for_each(|parsed| {
            collect_dart_enums(parsed.tree.root_node(), &parsed.source, &registry);
        });

        Self {
            types: registry.into_iter().collect(),
        }
    }
}

fn collect_dart_enums(node: Node, source: &str, registry: &DashMap<String, ResolvedKind>) {
    if node.kind() == "enum_declaration" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = name_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
            if !name.is_empty() {
                registry.insert(name, ResolvedKind::Enum);
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_dart_enums(child, source, registry);
    }
}
