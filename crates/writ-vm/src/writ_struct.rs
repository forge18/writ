use std::collections::{HashMap, HashSet};

use crate::value::Value;

/// A Writ struct instance — a value-type collection of named fields.
///
/// Structs are copied on assignment (value semantics). They support
/// field access and method calls but have no inheritance.
#[derive(Debug, Clone)]
pub struct WritStruct {
    /// The struct type name (e.g., "Point").
    pub type_name: String,
    /// Field values, keyed by field name.
    pub fields: HashMap<String, Value>,
    /// Ordered field names (preserves declaration order).
    pub field_order: Vec<String>,
    /// Set of public field names (for reflection visibility).
    pub public_fields: HashSet<String>,
    /// Set of public method names (for reflection visibility).
    pub public_methods: HashSet<String>,
}

impl WritStruct {
    /// Gets a field value by name.
    pub fn get_field(&self, name: &str) -> Option<&Value> {
        self.fields.get(name)
    }

    /// Sets a field value by name. Returns `Err` if the field doesn't exist.
    pub fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        if self.fields.contains_key(name) {
            self.fields.insert(name.to_string(), value);
            Ok(())
        } else {
            Err(format!("'{}' has no field '{}'", self.type_name, name))
        }
    }

    /// Returns all public field names in declaration order.
    pub fn public_field_names(&self) -> Vec<String> {
        self.field_order
            .iter()
            .filter(|f| self.public_fields.contains(f.as_str()))
            .cloned()
            .collect()
    }

    /// Returns all public method names.
    pub fn public_method_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.public_methods.iter().cloned().collect();
        names.sort();
        names
    }
}

impl PartialEq for WritStruct {
    fn eq(&self, other: &Self) -> bool {
        if self.type_name != other.type_name {
            return false;
        }
        self.fields == other.fields
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn make_point(x: i32, y: i32) -> WritStruct {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Value::I32(x));
        fields.insert("y".to_string(), Value::I32(y));
        let mut public_fields = HashSet::new();
        public_fields.insert("x".to_string());
        public_fields.insert("y".to_string());
        WritStruct {
            type_name: "Point".to_string(),
            fields,
            field_order: vec!["x".to_string(), "y".to_string()],
            public_fields,
            public_methods: HashSet::new(),
        }
    }

    #[test]
    fn get_field() {
        let p = make_point(3, 4);
        assert_eq!(p.get_field("x"), Some(&Value::I32(3)));
        assert_eq!(p.get_field("y"), Some(&Value::I32(4)));
        assert_eq!(p.get_field("z"), None);
    }

    #[test]
    fn set_field() {
        let mut p = make_point(3, 4);
        assert!(p.set_field("x", Value::I32(10)).is_ok());
        assert_eq!(p.get_field("x"), Some(&Value::I32(10)));
        assert!(p.set_field("z", Value::I32(0)).is_err());
    }

    #[test]
    fn structural_equality() {
        let a = make_point(3, 4);
        let b = make_point(3, 4);
        let c = make_point(5, 6);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn value_semantics() {
        let a = make_point(3, 4);
        let mut b = a.clone();
        b.set_field("x", Value::I32(99)).unwrap();
        assert_eq!(a.get_field("x"), Some(&Value::I32(3)));
        assert_eq!(b.get_field("x"), Some(&Value::I32(99)));
    }

    #[test]
    fn public_field_names_in_order() {
        let mut p = make_point(3, 4);
        // Make only "y" public
        p.public_fields.clear();
        p.public_fields.insert("y".to_string());
        assert_eq!(p.public_field_names(), vec!["y".to_string()]);
    }
}
