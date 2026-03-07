/// Resolved types in the Writ type system.
///
/// Every expression and variable binding resolves to exactly one `Type`
/// after type checking. These correspond to the primitive types defined
/// in `.spec/language/type-system.md` Section 3.
///
/// Numeric types (`Int`, `Float`) use smart width promotion at runtime:
/// `int` starts as `i32` and promotes to `i64` on overflow; `float` starts
/// as `f32` and promotes to `f64` when precision/range demands it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// `int` — starts as `i32`, promotes to `i64` on overflow.
    Int,
    /// `float` — starts as `f32`, promotes to `f64` when needed.
    Float,
    /// `bool` — maps to Rust `bool`.
    Bool,
    /// `string` — maps to Rust `String`.
    Str,
    /// `void` — no value.
    Void,
    /// Unknown type — indicates an unresolved or error state.
    Unknown,
    /// `Result<T>` — success with value of type `T` or error with string message.
    Result(Box<Type>),
    /// `Optional<T>` — value of type `T` or no value.
    Optional(Box<Type>),
    /// `(T1, T2, ...)` — fixed-length typed tuple.
    Tuple(Vec<Type>),
    /// `Array<T>` — ordered, typed, indexed collection.
    Array(Box<Type>),
    /// `Dictionary<K, V>` — key-value typed collection.
    Dictionary(Box<Type>, Box<Type>),
    /// Function type — parameter types and return type.
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },
    /// A user-defined class type, identified by name.
    Class(String),
    /// A user-defined trait type, identified by name.
    Trait(String),
    /// A user-defined enum type, identified by name.
    Enum(String),
    /// A user-defined struct type (value type), identified by name.
    Struct(String),
}

impl Type {
    /// Returns `true` if this type supports arithmetic operations.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Str => write!(f, "string"),
            Type::Void => write!(f, "void"),
            Type::Unknown => write!(f, "unknown"),
            Type::Result(inner) => write!(f, "Result<{inner}>"),
            Type::Optional(inner) => write!(f, "Optional<{inner}>"),
            Type::Array(inner) => write!(f, "Array<{inner}>"),
            Type::Dictionary(k, v) => write!(f, "Dictionary<{k}, {v}>"),
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, ty) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ")")
            }
            Type::Function {
                params,
                return_type,
            } => {
                write!(f, "(")?;
                for (i, ty) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ") -> {return_type}")
            }
            Type::Class(name) | Type::Trait(name) | Type::Enum(name) | Type::Struct(name) => {
                write!(f, "{name}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_primitive_names() {
        assert_eq!(Type::Int.to_string(), "int");
        assert_eq!(Type::Float.to_string(), "float");
        assert_eq!(Type::Bool.to_string(), "bool");
        assert_eq!(Type::Str.to_string(), "string");
        assert_eq!(Type::Void.to_string(), "void");
        assert_eq!(Type::Unknown.to_string(), "unknown");
    }

    #[test]
    fn is_numeric_for_numeric_types() {
        assert!(Type::Int.is_numeric());
        assert!(Type::Float.is_numeric());
    }

    #[test]
    fn is_numeric_false_for_non_numeric_types() {
        assert!(!Type::Bool.is_numeric());
        assert!(!Type::Str.is_numeric());
        assert!(!Type::Void.is_numeric());
        assert!(!Type::Unknown.is_numeric());
        assert!(!Type::Class("Player".to_string()).is_numeric());
        assert!(!Type::Trait("Damageable".to_string()).is_numeric());
        assert!(!Type::Enum("Direction".to_string()).is_numeric());
    }

    #[test]
    fn display_user_defined_types() {
        assert_eq!(Type::Class("Player".to_string()).to_string(), "Player");
        assert_eq!(
            Type::Trait("Damageable".to_string()).to_string(),
            "Damageable"
        );
        assert_eq!(Type::Enum("Direction".to_string()).to_string(), "Direction");
    }
}
