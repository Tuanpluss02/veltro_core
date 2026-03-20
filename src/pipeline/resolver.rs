use crate::ir::{ClassIR, ResolvedKind};
use crate::registry::TypeRegistry;

/// Resolves field types using the TypeRegistry.
/// Marks fields as generic if they match class generic parameters.
pub fn resolve(mut ir: ClassIR, registry: &TypeRegistry) -> ClassIR {
    for field in &mut ir.fields {
        if ir.generics.contains(&field.type_name) {
            field.resolved_kind = ResolvedKind::External;
            field.is_generic_param = true;
        } else {
            field.resolved_kind = registry.get(&field.type_name);
            field.is_generic_param = false;
        }
    }
    ir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{FieldIR, ClassIR, AnnotationIR, IR_VERSION};
    use std::path::PathBuf;

    fn make_class(name: &str, generics: Vec<String>, fields: Vec<FieldIR>) -> ClassIR {
        ClassIR {
            ir_version: IR_VERSION,
            name: name.to_string(),
            generics,
            fields,
            annotations: vec![AnnotationIR { name: "Data".to_string(), arguments: Default::default() }],
            source_file: PathBuf::from(format!("{}.dart", name.to_lowercase())),
            has_with_mixin: true,
            has_from_json: true,
        }
    }

    fn make_field(name: &str, type_name: &str) -> FieldIR {
        FieldIR {
            name: name.to_string(),
            type_name: type_name.to_string(),
            generic_args: vec![],
            is_required: true,
            is_nullable: false,
            resolved_kind: ResolvedKind::External,
            is_generic_param: false,
            default_value: None,
        }
    }

    #[test]
    fn test_resolve_annotated_class() {
        let mut registry = TypeRegistry::new();
        registry.insert("Address".to_string(), ResolvedKind::AnnotatedClass);

        let ir = make_class("Person", vec![], vec![make_field("address", "Address")]);
        let resolved = resolve(ir, &registry);
        assert_eq!(resolved.fields[0].resolved_kind, ResolvedKind::AnnotatedClass);
    }

    #[test]
    fn test_resolve_generic() {
        let registry = TypeRegistry::new();
        let ir = make_class("Box", vec!["T".to_string()], vec![make_field("value", "T")]);
        let resolved = resolve(ir, &registry);
        assert_eq!(resolved.fields[0].resolved_kind, ResolvedKind::External);
        assert!(resolved.fields[0].is_generic_param);
    }
}
