use std::cell::RefCell;
use std::rc::Rc;

#[cfg(test)]
use writ_vm::WritStruct;
use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    // typeof(value) -> string
    vm.register_fn("typeof", 1, |args| {
        Ok(Value::Str(Rc::new(args[0].type_name_owned())))
    });

    // instanceof(value, typeName) -> bool
    vm.register_fn("instanceof", 2, |args| {
        let type_name = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("instanceof expects a string type name".to_string()),
        };
        let actual = args[0].type_name_owned();
        Ok(Value::Bool(actual == type_name))
    });

    // hasField(obj, name) -> bool
    vm.register_fn("hasField", 2, |args| {
        let name = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("hasField expects a string field name".to_string()),
        };
        let result = match &args[0] {
            Value::Struct(s) => s.public_fields.contains(&name),
            Value::Dict(d) => d.borrow().contains_key(&name),
            Value::Object(obj) => obj.borrow().get_field(&name).is_ok(),
            _ => false,
        };
        Ok(Value::Bool(result))
    });

    // getField(obj, name) -> value
    vm.register_fn("getField", 2, |args| {
        let name = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("getField expects a string field name".to_string()),
        };
        match &args[0] {
            Value::Struct(s) => {
                if !s.public_fields.contains(&name) {
                    return Err(format!("field '{}' not found on '{}'", name, s.type_name));
                }
                s.get_field(&name)
                    .cloned()
                    .ok_or_else(|| format!("field '{}' not found on '{}'", name, s.type_name))
            }
            Value::Dict(d) => Ok(d.borrow().get(&name).cloned().unwrap_or(Value::Null)),
            Value::Object(obj) => obj.borrow().get_field(&name),
            other => Err(format!("cannot get field on {}", other.type_name())),
        }
    });

    // setField(obj, name, value) -> null
    // Note: Does not work on Value::Struct (value semantics — copy is modified, not original)
    vm.register_fn("setField", 3, |args| {
        let name = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("setField expects a string field name".to_string()),
        };
        let value = args[2].clone();
        match &args[0] {
            Value::Dict(d) => {
                d.borrow_mut().insert(name, value);
                Ok(Value::Null)
            }
            Value::Object(obj) => {
                obj.borrow_mut().set_field(&name, value)?;
                Ok(Value::Null)
            }
            Value::Struct(_) => Err(
                "setField cannot modify structs (value types); use direct field assignment instead"
                    .to_string(),
            ),
            other => Err(format!("cannot set field on {}", other.type_name())),
        }
    });

    // fields(obj) -> Array<string>
    vm.register_fn("fields", 1, |args| {
        let names: Vec<Value> = match &args[0] {
            Value::Struct(s) => s
                .public_field_names()
                .into_iter()
                .map(|n| Value::Str(Rc::new(n)))
                .collect(),
            Value::Dict(d) => d
                .borrow()
                .keys()
                .map(|k| Value::Str(Rc::new(k.clone())))
                .collect(),
            _ => Vec::new(),
        };
        Ok(Value::Array(Rc::new(RefCell::new(names))))
    });

    // methods(obj) -> Array<string>
    vm.register_fn("methods", 1, |args| {
        let names: Vec<Value> = match &args[0] {
            Value::Struct(s) => s
                .public_method_names()
                .into_iter()
                .map(|n| Value::Str(Rc::new(n)))
                .collect(),
            _ => Vec::new(),
        };
        Ok(Value::Array(Rc::new(RefCell::new(names))))
    });

    // hasMethod(obj, name) -> bool
    vm.register_fn("hasMethod", 2, |args| {
        let name = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err("hasMethod expects a string method name".to_string()),
        };
        let result = match &args[0] {
            Value::Struct(s) => s.public_methods.contains(&name),
            _ => false,
        };
        Ok(Value::Bool(result))
    });

    // invoke(obj, methodName, ...args) is handled as a VM built-in
    // because it needs VM access to dispatch method calls. The VM intercepts
    // calls to "invoke" before they reach native function dispatch.
}

/// Constructs a WritStruct from the given fields (used by reflection tests).
#[cfg(test)]
pub fn make_test_struct(
    name: &str,
    fields: Vec<(&str, Value)>,
    public: Vec<&str>,
    methods: Vec<&str>,
) -> WritStruct {
    use std::collections::{HashMap, HashSet};
    let field_order: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
    let field_map: HashMap<String, Value> = fields
        .into_iter()
        .map(|(n, v)| (n.to_string(), v))
        .collect();
    let public_fields: HashSet<String> = public.into_iter().map(|s| s.to_string()).collect();
    let public_methods: HashSet<String> = methods.into_iter().map(|s| s.to_string()).collect();
    WritStruct {
        type_name: name.to_string(),
        fields: field_map,
        field_order,
        public_fields,
        public_methods,
    }
}
