use std::cell::RefCell;
use std::rc::Rc;

#[cfg(test)]
use writ_vm::WritStruct;
use writ_vm::binding::{fn1, fn2, fn3};
use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn(
        "typeof",
        fn1(|v: Value| -> Result<Value, String> { Ok(Value::Str(Rc::new(v.type_name_owned()))) }),
    );

    vm.register_fn(
        "instanceof",
        fn2(|v: Value, type_name: String| -> Result<bool, String> {
            Ok(v.type_name_owned() == type_name)
        }),
    );

    vm.register_fn(
        "hasField",
        fn2(|obj: Value, name: String| -> Result<bool, String> {
            let result = match &obj {
                Value::Struct(s) => s.layout.public_fields.contains(&name),
                Value::Dict(d) => d.borrow().contains_key(&name),
                Value::Object(o) => o.borrow().get_field(&name).is_ok(),
                _ => false,
            };
            Ok(result)
        }),
    );

    vm.register_fn(
        "getField",
        fn2(|obj: Value, name: String| -> Result<Value, String> {
            match obj {
                Value::Struct(s) => {
                    if !s.layout.public_fields.contains(&name) {
                        return Err(format!(
                            "field '{}' not found on '{}'",
                            name, s.layout.type_name
                        ));
                    }
                    s.get_field(&name).cloned().ok_or_else(|| {
                        format!("field '{}' not found on '{}'", name, s.layout.type_name)
                    })
                }
                Value::Dict(d) => Ok(d.borrow().get(&name).cloned().unwrap_or(Value::Null)),
                Value::Object(o) => o.borrow().get_field(&name),
                other => Err(format!("cannot get field on {}", other.type_name())),
            }
        }),
    );

    vm.register_fn(
        "setField",
       fn3(|obj: Value, name: String, value: Value| -> Result<(), String> {
            match &obj {
                Value::Dict(d) => {
                    d.borrow_mut().insert(name, value);
                    Ok(())
                }
                Value::Object(o) => o.borrow_mut().set_field(&name, value),
                Value::Struct(_) => Err(
                    "setField cannot modify structs (value types); use direct field assignment instead"
                        .to_string(),
                ),
                other => Err(format!("cannot set field on {}", other.type_name())),
            }
        }),
    );

    vm.register_fn(
        "fields",
        fn1(|obj: Value| -> Result<Value, String> {
            let names: Vec<Value> = match &obj {
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
        }),
    );

    vm.register_fn(
        "methods",
        fn1(|obj: Value| -> Result<Value, String> {
            let names: Vec<Value> = match &obj {
                Value::Struct(s) => s
                    .public_method_names()
                    .into_iter()
                    .map(|n| Value::Str(Rc::new(n)))
                    .collect(),
                _ => Vec::new(),
            };
            Ok(Value::Array(Rc::new(RefCell::new(names))))
        }),
    );

    vm.register_fn(
        "hasMethod",
        fn2(|obj: Value, name: String| -> Result<bool, String> {
            let result = match &obj {
                Value::Struct(s) => s.layout.public_methods.contains(&name),
                _ => false,
            };
            Ok(result)
        }),
    );
}

/// Constructs a WritStruct from the given fields (used by reflection tests).
#[cfg(test)]
pub fn make_test_struct(
    name: &str,
    fields: Vec<(&str, Value)>,
    public: Vec<&str>,
    methods: Vec<&str>,
) -> WritStruct {
    use std::collections::HashSet;
    use writ_vm::FieldLayout;

    let field_names: Vec<String> = fields.iter().map(|(n, _)| n.to_string()).collect();
    let field_values: Vec<Value> = fields.into_iter().map(|(_, v)| v).collect();
    let public_fields: HashSet<String> = public.into_iter().map(|s| s.to_string()).collect();
    let public_methods: HashSet<String> = methods.into_iter().map(|s| s.to_string()).collect();
    let layout = Rc::new(FieldLayout::new(
        name.to_string(),
        field_names,
        public_fields,
        public_methods,
    ));
    WritStruct {
        layout,
        fields: field_values,
    }
}
