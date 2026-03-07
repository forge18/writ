use crate::types::Type;

/// Maps a Writ type to its Rust type string representation.
///
/// Since the VM's `Value` enum uses flat variants (`I32`/`I64`/`F32`/`F64`),
/// generated code uses `i32`/`f32` as the default narrow types. Width promotion
/// happens at the VM level when arithmetic overflows.
pub fn writ_type_to_rust(ty: &Type) -> String {
    match ty {
        Type::Int => "i32".to_string(),
        Type::Float => "f32".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Str => "String".to_string(),
        Type::Void => "()".to_string(),
        Type::Unknown => "Value".to_string(),
        Type::Array(inner) => format!("Vec<{}>", writ_type_to_rust(inner)),
        Type::Dictionary(k, v) => {
            format!(
                "HashMap<{}, {}>",
                writ_type_to_rust(k),
                writ_type_to_rust(v)
            )
        }
        Type::Optional(inner) => format!("Option<{}>", writ_type_to_rust(inner)),
        Type::Result(inner) => format!("Result<{}, String>", writ_type_to_rust(inner)),
        Type::Tuple(types) => {
            let parts: Vec<String> = types.iter().map(writ_type_to_rust).collect();
            format!("({})", parts.join(", "))
        }
        Type::Class(name) => name.clone(),
        Type::Trait(name) => format!("Box<dyn {name}>"),
        Type::Function {
            params,
            return_type,
        } => {
            let param_types: Vec<String> = params.iter().map(writ_type_to_rust).collect();
            let ret = writ_type_to_rust(return_type);
            format!("Box<dyn Fn({}) -> {}>", param_types.join(", "), ret)
        }
        Type::Enum(name) => name.clone(),
        Type::Struct(name) => name.clone(),
    }
}

/// Maps a Writ type to a Rust type string using primitive types (i32/f32).
/// Now identical to `writ_type_to_rust` since the VM uses flat primitives.
pub fn writ_type_to_rust_primitive(ty: &Type) -> String {
    writ_type_to_rust(ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives() {
        assert_eq!(writ_type_to_rust(&Type::Int), "i32");
        assert_eq!(writ_type_to_rust(&Type::Float), "f32");
        assert_eq!(writ_type_to_rust(&Type::Bool), "bool");
        assert_eq!(writ_type_to_rust(&Type::Str), "String");
        assert_eq!(writ_type_to_rust(&Type::Void), "()");
    }

    #[test]
    fn collections() {
        assert_eq!(
            writ_type_to_rust(&Type::Array(Box::new(Type::Int))),
            "Vec<i32>"
        );
        assert_eq!(
            writ_type_to_rust(&Type::Dictionary(Box::new(Type::Str), Box::new(Type::Int))),
            "HashMap<String, i32>"
        );
    }

    #[test]
    fn optional_and_result() {
        assert_eq!(
            writ_type_to_rust(&Type::Optional(Box::new(Type::Str))),
            "Option<String>"
        );
        assert_eq!(
            writ_type_to_rust(&Type::Result(Box::new(Type::Int))),
            "Result<i32, String>"
        );
    }

    #[test]
    fn class_type() {
        assert_eq!(
            writ_type_to_rust(&Type::Class("Player".to_string())),
            "Player"
        );
    }

    #[test]
    fn primitives_mode() {
        assert_eq!(writ_type_to_rust_primitive(&Type::Int), "i32");
        assert_eq!(writ_type_to_rust_primitive(&Type::Float), "f32");
        assert_eq!(writ_type_to_rust_primitive(&Type::Bool), "bool");
    }
}
