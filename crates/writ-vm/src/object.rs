use crate::value::Value;

/// Trait for host-owned types exposed to Writ scripts.
///
/// Implement this trait on Rust types that need to be accessible
/// from Writ scripts via `VM::register_type`. Scripts can read/write
/// fields and call methods on objects implementing this trait.
pub trait WritObject: std::fmt::Debug {
    /// Returns the type name as seen by scripts (e.g. `"Player"`).
    fn type_name(&self) -> &str;

    /// Gets a field value by name.
    fn get_field(&self, name: &str) -> Result<Value, String>;

    /// Sets a field value by name.
    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String>;

    /// Calls a method by name with the given arguments.
    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String>;

    /// Gets a field value by pre-computed hash (hot path for VM).
    /// Default falls back to `get_field(name)`.
    fn get_field_by_hash(&self, _hash: u32, name: &str) -> Result<Value, String> {
        self.get_field(name)
    }

    /// Sets a field value by pre-computed hash (hot path for VM).
    /// Default falls back to `set_field(name, value)`.
    fn set_field_by_hash(&mut self, _hash: u32, name: &str, value: Value) -> Result<(), String> {
        self.set_field(name, value)
    }
}

impl WritObject for Box<dyn WritObject> {
    fn type_name(&self) -> &str {
        (**self).type_name()
    }
    fn get_field(&self, name: &str) -> Result<Value, String> {
        (**self).get_field(name)
    }
    fn set_field(&mut self, name: &str, value: Value) -> Result<(), String> {
        (**self).set_field(name, value)
    }
    fn call_method(&mut self, name: &str, args: &[Value]) -> Result<Value, String> {
        (**self).call_method(name, args)
    }
    fn get_field_by_hash(&self, hash: u32, name: &str) -> Result<Value, String> {
        (**self).get_field_by_hash(hash, name)
    }
    fn set_field_by_hash(&mut self, hash: u32, name: &str, value: Value) -> Result<(), String> {
        (**self).set_field_by_hash(hash, name, value)
    }
}
