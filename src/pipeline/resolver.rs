use crate::ir::{DataClassIR, TypeKind};
use crate::registry::TypeRegistry;

/// Resolves field types using the TypeRegistry.
/// Mark fields as generic if they match class generic parameters.
pub fn resolve(mut ir: DataClassIR, registry: &TypeRegistry) -> DataClassIR {
    for field in &mut ir.fields {
        if ir.generics.contains(&field.type_name) {
            field.resolved_kind = TypeKind::External;
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
    use crate::ir::{FieldIR, DataClassIR};
    use std::path::PathBuf;

    #[test]
    fn test_resolve_dataclass() {
        let mut registry = TypeRegistry::new();
        registry.insert("Address".to_string(), TypeKind::DataClass);

        let ir = DataClassIR {
            name: "Person".to_string(),
            generics: vec![],
            fields: vec![FieldIR {
                name: "address".to_string(),
                type_name: "Address".to_string(),
                generic_args: vec![],
                is_required: true,
                is_nullable: false,
                resolved_kind: TypeKind::External,
                is_generic_param: false,
            }],
            source_file: PathBuf::from("person.dart"),
            has_from_json: true,
        };

        let resolved = resolve(ir, &registry);
        assert_eq!(resolved.fields[0].resolved_kind, TypeKind::DataClass);
    }

    #[test]
    fn test_resolve_generic() {
        let registry = TypeRegistry::new();
        let ir = DataClassIR {
            name: "Box".to_string(),
            generics: vec!["T".to_string()],
            fields: vec![FieldIR {
                name: "value".to_string(),
                type_name: "T".to_string(),
                generic_args: vec![],
                is_required: true,
                is_nullable: false,
                resolved_kind: TypeKind::External,
                is_generic_param: false,
            }],
            source_file: PathBuf::from("box.dart"),
            has_from_json: false,
        };

        let resolved = resolve(ir, &registry);
        assert_eq!(resolved.fields[0].resolved_kind, TypeKind::External);
        assert!(resolved.fields[0].is_generic_param);
    }
}
