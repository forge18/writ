use std::collections::HashMap;

use crate::object::WritObject;
use crate::value::Value;

/// A runtime class instance — a reference-type object with named fields and
/// inheritance support.
///
/// Unlike [`WritStruct`](crate::writ_struct::WritStruct) (value type, copied
/// on assignment), class instances are wrapped in `Rc<RefCell<...>>` and use
/// reference semantics.
#[derive(Debug, Clone)]
pub struct WritClassInstance {
    /// The class type name (e.g., "Player").
    pub class_name: String,
    /// Field values, keyed by field name (includes inherited fields).
    pub fields: HashMap<String, Value>,
    /// Ordered field names (preserves declaration order, parent fields first).
    pub field_order: Vec<String>,
    /// Parent class name, if this class extends another.
    pub parent_class: Option<String>,
}

impl WritObject for WritClassInstance {
    fn type_name(&self) -> &str {
        &self.class_name
    }

    fn get_field(&self, name: &str) -> Result<Value, String> {
        self.fields
            .get(name)
            .cloned()
            .ok_or_else(|| format!("'{}' has no field '{name}'", self.class_name))
    }

    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        if self.fields.contains_key(name) {
            self.fields.insert(name.to_string(), value);
            Ok(())
        } else {
            Err(format!("'{}' has no field '{name}'", self.class_name))
        }
    }

    fn call_method(&mut self, name: &str, _args: &[Value]) -> Result<Value, String> {
        // Method dispatch for class instances is handled by the VM via
        // compiled functions (ClassName::method_name), not here.
        Err(format!(
            "'{}' has no native method '{name}'",
            self.class_name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn make_entity(x: f32, y: f32) -> WritClassInstance {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Value::F32(x));
        fields.insert("y".to_string(), Value::F32(y));
        WritClassInstance {
            class_name: "Entity".to_string(),
            fields,
            field_order: vec!["x".to_string(), "y".to_string()],
            parent_class: None,
        }
    }

    #[test]
    fn get_field() {
        let e = make_entity(1.0, 2.0);
        assert_eq!(
            e.get_field("x").unwrap(),
            Value::F32(1.0)
        );
        assert!(e.get_field("z").is_err());
    }

    #[test]
    fn set_field() {
        let mut e = make_entity(1.0, 2.0);
        assert!(e.set_field("x", Value::F32(5.0)).is_ok());
        assert_eq!(
            e.get_field("x").unwrap(),
            Value::F32(5.0)
        );
        assert!(e.set_field("z", Value::I32(0)).is_err());
    }

    #[test]
    fn type_name() {
        let e = make_entity(0.0, 0.0);
        assert_eq!(e.type_name(), "Entity");
    }

    #[test]
    fn inherited_fields() {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Value::F32(0.0));
        fields.insert("y".to_string(), Value::F32(0.0));
        fields.insert("health".to_string(), Value::F32(100.0));
        let player = WritClassInstance {
            class_name: "Player".to_string(),
            fields,
            field_order: vec!["x".to_string(), "y".to_string(), "health".to_string()],
            parent_class: Some("Entity".to_string()),
        };
        assert_eq!(
            player.get_field("x").unwrap(),
            Value::F32(0.0)
        );
        assert_eq!(
            player.get_field("health").unwrap(),
            Value::F32(100.0)
        );
        assert_eq!(player.parent_class.as_deref(), Some("Entity"));
    }
}
