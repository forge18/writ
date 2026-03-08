use std::cell::RefCell;
use std::rc::Rc;

use crate::vm::binding::{fn1, fn2, fn3};
use crate::vm::sequence::{NativeResult, Sequence, SequenceAction};
use crate::vm::{VM, Value};

pub fn register(vm: &mut VM) {
    vm.register_fn(
        "typeof",
        fn1(|v: Value| -> Result<Value, String> {
            Ok(Value::Str(Rc::from(v.type_name_owned().as_str())))
        }),
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
                    .map(|n| Value::Str(Rc::from(n.as_str())))
                    .collect(),
                Value::Dict(d) => d
                    .borrow()
                    .keys()
                    .map(|k| Value::Str(Rc::from(k.as_str())))
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
                    .map(|n| Value::Str(Rc::from(n.as_str())))
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

    vm.register_seq_fn(
        "invoke",
        Rc::new(|args: &[Value]| -> Result<NativeResult, String> {
            let obj = args.first().ok_or("invoke: missing object")?.clone();
            let method_name = match args.get(1) {
                Some(Value::Str(s)) => s.to_string(),
                Some(other) => {
                    return Err(format!(
                        "invoke: method name must be a string, got {}",
                        other.type_name()
                    ));
                }
                None => return Err("invoke: missing method name".into()),
            };
            let call_args: Vec<Value> = if args.len() > 2 {
                args[2..].to_vec()
            } else {
                Vec::new()
            };

            // Native objects: dispatch directly via WritObject::call_method
            if let Value::Object(ref obj_rc) = obj {
                let result = obj_rc.borrow_mut().call_method(&method_name, &call_args)?;
                return Ok(NativeResult::Value(result));
            }

            // Script-defined structs: build qualified name and call via sequence
            if let Value::Struct(ref s) = obj {
                let qualified = format!("{}::{}", s.layout.type_name, method_name);
                let mut all_args = Vec::with_capacity(1 + call_args.len());
                all_args.push(obj.clone());
                all_args.extend(call_args);
                return Ok(NativeResult::Sequence(Box::new(InvokeSequence {
                    callee: Value::Str(Rc::from(qualified.as_str())),
                    args: Some(all_args),
                })));
            }

            Err(format!(
                "invoke not supported on type '{}'",
                obj.type_name()
            ))
        }),
    );
}

/// One-shot sequence that calls a single function and returns its result.
struct InvokeSequence {
    callee: Value,
    args: Option<Vec<Value>>,
}

impl Sequence for InvokeSequence {
    fn poll(&mut self, last_result: Option<Value>) -> SequenceAction {
        if let Some(result) = last_result {
            return SequenceAction::Done(result);
        }
        match self.args.take() {
            Some(args) => SequenceAction::Call {
                callee: self.callee.clone(),
                args,
            },
            None => SequenceAction::Error("invoke: internal error — no args".into()),
        }
    }
}
