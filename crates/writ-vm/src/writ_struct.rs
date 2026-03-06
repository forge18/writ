use std::rc::Rc;

use writ_compiler::string_hash;

use crate::field_layout::FieldLayout;
use crate::value::Value;

/// A Writ struct instance — a value-type collection of named fields.
///
/// Structs are copied on assignment (value semantics). They support
/// field access and method calls but have no inheritance.
///
/// Fields are stored in a `Vec<Value>` indexed by position. The shared
/// `Rc<FieldLayout>` provides the hash-to-index mapping for field access
/// and metadata for reflection.
#[derive(Debug, Clone)]
pub struct WritStruct {
    /// Shared layout descriptor (type name, field index map, reflection data).
    pub layout: Rc<FieldLayout>,
    /// Field values in declaration order. `fields[i]` corresponds to
    /// `layout.field_names[i]`.
    pub fields: Vec<Value>,
}

impl WritStruct {
    /// Gets a field value by name.
    pub fn get_field(&self, name: &str) -> Option<&Value> {
        let hash = string_hash(name);
        self.layout
            .hash_to_index
            .get(&hash)
            .map(|&idx| &self.fields[idx])
    }

    /// Gets a field value by pre-computed hash (hot path).
    #[inline]
    pub fn get_field_by_hash(&self, name_hash: u32) -> Option<&Value> {
        self.layout
            .hash_to_index
            .get(&name_hash)
            .map(|&idx| &self.fields[idx])
    }

    /// Sets a field value by name. Returns `Err` if the field doesn't exist.
    pub fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let hash = string_hash(name);
        if let Some(&idx) = self.layout.hash_to_index.get(&hash) {
            self.fields[idx] = value;
            Ok(())
        } else {
            Err(format!(
                "'{}' has no field '{}'",
                self.layout.type_name, name
            ))
        }
    }

    /// Sets a field value by pre-computed hash (hot path).
    #[inline]
    pub fn set_field_by_hash(&mut self, name_hash: u32, value: Value) -> Result<(), String> {
        if let Some(&idx) = self.layout.hash_to_index.get(&name_hash) {
            self.fields[idx] = value;
            Ok(())
        } else {
            Err(format!(
                "'{}' has no field (hash {})",
                self.layout.type_name, name_hash
            ))
        }
    }

    /// Returns the type name.
    pub fn type_name(&self) -> &str {
        &self.layout.type_name
    }

    /// Returns all public field names in declaration order.
    pub fn public_field_names(&self) -> Vec<String> {
        self.layout
            .field_names
            .iter()
            .filter(|f| self.layout.public_fields.contains(f.as_str()))
            .cloned()
            .collect()
    }

    /// Returns all public method names.
    pub fn public_method_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.layout.public_methods.iter().cloned().collect();
        names.sort();
        names
    }
}

impl PartialEq for WritStruct {
    fn eq(&self, other: &Self) -> bool {
        // Fast path: same layout pointer means same type
        if Rc::ptr_eq(&self.layout, &other.layout) {
            return self.fields == other.fields;
        }
        // Slow path: different layout objects, compare type name
        if self.layout.type_name != other.layout.type_name {
            return false;
        }
        self.fields == other.fields
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn make_layout(type_name: &str, field_names: Vec<&str>, public: Vec<&str>) -> Rc<FieldLayout> {
        let field_names: Vec<String> = field_names.iter().map(|s| s.to_string()).collect();
        let public_fields: HashSet<String> = public.iter().map(|s| s.to_string()).collect();
        Rc::new(FieldLayout::new(
            type_name.to_string(),
            field_names,
            public_fields,
            HashSet::new(),
        ))
    }

    fn make_point(x: i32, y: i32) -> WritStruct {
        let layout = make_layout("Point", vec!["x", "y"], vec!["x", "y"]);
        WritStruct {
            layout,
            fields: vec![Value::I32(x), Value::I32(y)],
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
        let layout = make_layout("Point", vec!["x", "y"], vec!["y"]);
        let p = WritStruct {
            layout,
            fields: vec![Value::I32(3), Value::I32(4)],
        };
        assert_eq!(p.public_field_names(), vec!["y".to_string()]);
    }
}
