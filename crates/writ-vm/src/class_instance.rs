use std::rc::Rc;

use writ_compiler::string_hash;

use crate::field_layout::FieldLayout;
use crate::object::WritObject;
use crate::value::Value;

/// A runtime class instance — a reference-type object with named fields and
/// inheritance support.
///
/// Unlike [`WritStruct`](crate::writ_struct::WritStruct) (value type, copied
/// on assignment), class instances are wrapped in `Rc<RefCell<...>>` and use
/// reference semantics.
///
/// Fields are stored in a `Vec<Value>` indexed by position. The shared
/// `Rc<FieldLayout>` provides the hash-to-index mapping for field access.
#[derive(Debug, Clone)]
pub struct WritClassInstance {
    /// Shared layout descriptor.
    pub layout: Rc<FieldLayout>,
    /// Field values in declaration order (parent fields first).
    pub fields: Vec<Value>,
    /// Parent class name, if this class extends another.
    pub parent_class: Option<String>,
}

impl WritClassInstance {
    /// Gets a field value by pre-computed hash (hot path for VM).
    #[inline]
    pub fn get_field_by_hash(&self, name_hash: u32) -> Option<&Value> {
        self.layout
            .hash_to_index
            .get(&name_hash)
            .map(|&idx| &self.fields[idx])
    }

    /// Sets a field value by pre-computed hash (hot path for VM).
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
}

impl WritObject for WritClassInstance {
    fn type_name(&self) -> &str {
        &self.layout.type_name
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        let hash = string_hash(name);
        self.layout
            .hash_to_index
            .get(&hash)
            .map(|&idx| self.fields[idx].clone())
            .ok_or_else(|| format!("'{}' has no field '{name}'", self.layout.type_name))
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        let hash = string_hash(name);
        if let Some(&idx) = self.layout.hash_to_index.get(&hash) {
            self.fields[idx] = value;
            Ok(())
        } else {
            Err(format!("'{}' has no field '{name}'", self.layout.type_name))
        }
    }

    fn call_method(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
        // Method dispatch for class instances is handled by the VM via
        // compiled functions (ClassName::method_name), not here.
        Err(format!(
            "'{}' has no native method '{name}'",
            self.layout.type_name
        ))
    }

    fn get_field_by_hash(&self, hash: u32, _name: &str) -> Result<Value, String> {
        self.layout
            .hash_to_index
            .get(&hash)
            .map(|&idx| self.fields[idx].clone())
            .ok_or_else(|| format!("'{}' has no field (hash {hash})", self.layout.type_name))
    }

    fn set_field_by_hash(&mut self, hash: u32, _name: &str, value: Value) -> Result<(), String> {
        if let Some(&idx) = self.layout.hash_to_index.get(&hash) {
            self.fields[idx] = value;
            Ok(())
        } else {
            Err(format!(
                "'{}' has no field (hash {hash})",
                self.layout.type_name
            ))
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn make_layout(type_name: &str, field_names: Vec<&str>) -> Rc<FieldLayout> {
        let field_names: Vec<String> = field_names.iter().map(|s| s.to_string()).collect();
        Rc::new(FieldLayout::new(
            type_name.to_string(),
            field_names,
            HashSet::new(),
            HashSet::new(),
        ))
    }

    fn make_entity(x: f32, y: f32) -> WritClassInstance {
        let layout = make_layout("Entity", vec!["x", "y"]);
        WritClassInstance {
            layout,
            fields: vec![Value::F32(x), Value::F32(y)],
            parent_class: None,
        }
    }

    #[test]
    fn get_field() {
        let e = make_entity(1.0, 2.0);
        assert_eq!(e.get_field("x").unwrap(), Value::F32(1.0));
        assert!(e.get_field("z").is_err());
    }

    #[test]
    fn set_field() {
        let mut e = make_entity(1.0, 2.0);
        assert!(e.set_field("x", Value::F32(5.0)).is_ok());
        assert_eq!(e.get_field("x").unwrap(), Value::F32(5.0));
        assert!(e.set_field("z", Value::I32(0)).is_err());
    }

    #[test]
    fn type_name() {
        let e = make_entity(0.0, 0.0);
        assert_eq!(e.type_name(), "Entity");
    }

    #[test]
    fn inherited_fields() {
        let layout = make_layout("Player", vec!["x", "y", "health"]);
        let player = WritClassInstance {
            layout,
            fields: vec![Value::F32(0.0), Value::F32(0.0), Value::F32(100.0)],
            parent_class: Some("Entity".to_string()),
        };
        assert_eq!(player.get_field("x").unwrap(), Value::F32(0.0));
        assert_eq!(player.get_field("health").unwrap(), Value::F32(100.0));
        assert_eq!(player.parent_class.as_deref(), Some("Entity"));
    }
}
