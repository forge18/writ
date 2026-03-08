use std::cell::RefCell;
use std::rc::Rc;

use super::super::error::RuntimeError;
use super::super::frame::{CallFrame, ChunkId};
use super::super::value::Value;
use super::VM;
#[cfg(feature = "mobile-aosoa")]
use crate::compiler::string_hash;

type MethodBodyPtr = *const dyn Fn(&Value, &[Value]) -> Result<Value, String>;

impl VM {
    /// Register-based Call instruction.
    ///
    /// In the register model: `Call(base_reg, arg_count)` means the callee
    /// value is in `stack[base + base_reg]`, args are in consecutive registers
    /// starting at `base + base_reg + 1`. The result will be written to
    /// `stack[base + base_reg]` (the caller's result register).
    pub(super) fn exec_call_reg(
        &mut self,
        base: usize,
        base_reg: u8,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let callee_abs = base + base_reg as usize;
        let n = arg_count as usize;

        // Handle closure calls
        if let Value::Closure(data) = &self.stack[callee_abs] {
            let func_idx = data.func_idx;
            let upvalues = data.upvalues.clone();
            let func = &self.functions[func_idx];
            let expected_arity = func.arity;
            let max_regs = func.max_registers;

            if expected_arity != arg_count {
                return Err(self.make_error(format!(
                    "closure '{}' expects {} arguments, got {}",
                    self.functions[func_idx].name, expected_arity, arg_count
                )));
            }

            let new_base = callee_abs + 1;
            let needed = new_base + max_regs as usize;
            if self.stack.len() < needed {
                self.stack.resize(needed, Value::Null);
            }

            self.frames.push(CallFrame {
                chunk_id: ChunkId::Function(func_idx),
                pc: 0,
                base: new_base,
                result_reg: callee_abs,
                max_registers: max_regs,
                has_rc_values: true, // closures may capture Rc-bearing values
            });
            self.frame_upvalues.push(Some(upvalues));

            return Ok(());
        }

        // Named function call
        let func_idx = match &self.stack[callee_abs] {
            Value::Str(s) => {
                let name: &str = s;
                if let Some(&idx) = self.function_map.get(name) {
                    Some(idx)
                } else {
                    None
                }
            }
            _ => {
                return Err(self.make_error(format!(
                    "callee is not a function: {}",
                    self.stack[callee_abs].type_name()
                )));
            }
        };

        if let Some(func_idx) = func_idx {
            let expected_arity = self.functions[func_idx].arity;
            let is_variadic = self.functions[func_idx].is_variadic;
            let max_regs = self.functions[func_idx].max_registers;
            let func_has_rc = self.functions[func_idx].has_rc_values;

            // Only look up closure_map for functions that actually have upvalue descriptors.
            let call_upvalues: Option<Vec<u32>> = if !self.functions[func_idx].upvalues.is_empty() {
                self.closure_map.get(func_idx).and_then(|e| e.clone())
            } else {
                None
            };

            if is_variadic {
                let min_args = expected_arity.saturating_sub(1);
                if arg_count < min_args {
                    return Err(self.make_error(format!(
                        "function '{}' expects at least {} arguments, got {}",
                        self.functions[func_idx].name, min_args, arg_count
                    )));
                }

                // Pack variadic args into an array in the last fixed-arg register
                let fixed_count = min_args as usize;
                let variadic_count = n - fixed_count;
                let variadic_start = callee_abs + 1 + fixed_count;
                let variadic_args: Vec<Value> = (0..variadic_count)
                    .map(|i| std::mem::replace(&mut self.stack[variadic_start + i], Value::Null))
                    .collect();
                // Place the array in the variadic register slot
                self.stack[variadic_start] = Value::Array(Rc::new(RefCell::new(variadic_args)));

                let new_base = callee_abs + 1;
                let needed = new_base + max_regs as usize;
                if self.stack.len() < needed {
                    self.stack.resize(needed, Value::Null);
                }

                self.frames.push(CallFrame {
                    chunk_id: ChunkId::Function(func_idx),
                    pc: 0,
                    base: new_base,
                    result_reg: callee_abs,
                    max_registers: max_regs,
                    has_rc_values: true, // variadic creates Array
                });
                self.frame_upvalues.push(call_upvalues);
            } else {
                if expected_arity != arg_count {
                    return Err(self.make_error(format!(
                        "function '{}' expects {} arguments, got {}",
                        self.functions[func_idx].name, expected_arity, arg_count
                    )));
                }

                let new_base = callee_abs + 1;
                let needed = new_base + max_regs as usize;
                if self.stack.len() < needed {
                    self.stack.resize(needed, Value::Null);
                }

                let has_upvalues = call_upvalues.is_some();
                self.frames.push(CallFrame {
                    chunk_id: ChunkId::Function(func_idx),
                    pc: 0,
                    base: new_base,
                    result_reg: callee_abs,
                    max_registers: max_regs,
                    has_rc_values: func_has_rc || has_upvalues,
                });
                self.frame_upvalues.push(call_upvalues);
            }

            return Ok(());
        }

        // Fall back to native functions
        let func_name = match &self.stack[callee_abs] {
            Value::Str(s) => s.to_string(),
            _ => unreachable!(),
        };

        // Built-in: invoke(obj, methodName, ...args)
        if func_name == "invoke" && arg_count >= 2 {
            return self.exec_invoke_reg(base, base_reg, arg_count);
        }

        // Native function call
        self.exec_native_call_reg(&func_name, base, base_reg, arg_count)
    }

    /// Register-based native function call.
    pub(super) fn exec_native_call_reg(
        &mut self,
        func_name: &str,
        base: usize,
        base_reg: u8,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let callee_abs = base + base_reg as usize;
        let n = arg_count as usize;

        // Check sequence-capable functions first (callback support).
        if let Some(seq_native) = self.seq_native_functions.get(func_name) {
            if let Some(ref module) = seq_native.module
                && self.disabled_modules.contains(module)
            {
                return Err(self.make_error(format!(
                    "module '{}' is disabled; cannot call '{}'",
                    module, func_name
                )));
            }
            if let Some(expected) = seq_native.arity
                && expected != arg_count
            {
                return Err(self.make_error(format!(
                    "function '{}' expects {} arguments, got {}",
                    func_name, expected, arg_count
                )));
            }
            let body = Rc::clone(&seq_native.body);
            let arg_start = callee_abs + 1;
            let native_result = {
                let args = &self.stack[arg_start..arg_start + n];
                (body)(args).map_err(|msg| self.make_error(msg))?
            };
            match native_result {
                super::super::sequence::NativeResult::Value(v) => {
                    self.stack[callee_abs] = v;
                }
                super::super::sequence::NativeResult::Sequence(seq) => {
                    self.drive_sequence(seq, callee_abs)?;
                }
            }
            return Ok(());
        }

        let native = self
            .native_functions
            .get(func_name)
            .ok_or_else(|| self.make_error(format!("undefined function '{func_name}'")))?;

        if let Some(ref module) = native.module
            && self.disabled_modules.contains(module)
        {
            return Err(self.make_error(format!(
                "module '{}' is disabled; cannot call '{}'",
                module, func_name
            )));
        }

        if let Some(expected) = native.arity
            && expected != arg_count
        {
            return Err(self.make_error(format!(
                "function '{}' expects {} arguments, got {}",
                func_name, expected, arg_count
            )));
        }

        let body = Rc::clone(&native.body);

        // Pass a direct slice of the stack -- no Vec allocation.
        // callee_abs is one slot before arg_start so the write target is
        // non-overlapping with the borrow, and the borrow ends before the write.
        let arg_start = callee_abs + 1;
        let result = {
            let args = &self.stack[arg_start..arg_start + n];
            (body)(args).map_err(|msg| self.make_error(msg))?
        };
        self.stack[callee_abs] = result;
        Ok(())
    }

    /// Register-based invoke(obj, methodName, ...args).
    pub(super) fn exec_invoke_reg(
        &mut self,
        base: usize,
        base_reg: u8,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let callee_abs = base + base_reg as usize;
        let n = arg_count as usize;

        // Args are at callee_abs+1..callee_abs+1+n
        let obj = self.stack[callee_abs + 1].clone();
        let method_name_val = self.stack[callee_abs + 2].clone();
        let method_name = match &method_name_val {
            Value::Str(s) => s.to_string(),
            _ => return Err(self.make_error("invoke: method name must be a string".to_string())),
        };

        let remaining_args: Vec<Value> = (3..=n)
            .map(|i| self.stack[callee_abs + i].clone())
            .collect();

        // Struct method dispatch
        if let Value::Struct(ref s) = obj {
            let qualified = format!("{}::{}", s.layout.type_name, method_name);
            if let Some(&func_idx) = self.function_map.get(&qualified) {
                let mut all_args = Vec::with_capacity(1 + remaining_args.len());
                all_args.push(obj.clone());
                all_args.extend_from_slice(&remaining_args);
                let result = self.call_compiled_function(func_idx, &all_args)?;
                self.stack[callee_abs] = result;
                return Ok(());
            }
            return Err(self.make_error(format!(
                "no method '{}' on type '{}'",
                method_name, s.layout.type_name
            )));
        }

        // Object method dispatch
        if let Value::Object(ref obj_rc) = obj {
            let result = obj_rc
                .borrow_mut()
                .call_method(&method_name, &remaining_args)
                .map_err(|e| self.make_error(e))?;
            self.stack[callee_abs] = result;
            return Ok(());
        }

        Err(self.make_error(format!(
            "invoke not supported on type '{}'",
            obj.type_name()
        )))
    }

    /// Register-based CallMethod.
    ///
    /// `CallMethod(base_reg, name_hash, arg_count)`: receiver is at `stack[base + base_reg]`,
    /// args at consecutive registers after it. Result written to `stack[base + base_reg]`.
    pub(super) fn exec_call_method_reg(
        &mut self,
        base: usize,
        base_reg: u8,
        name_hash: u32,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let receiver_abs = base + base_reg as usize;
        let n = arg_count as usize;
        let receiver = self.stack[receiver_abs].clone();

        // Native method dispatch
        if let Some(tag) = receiver.tag()
            && let Some(method) = self.methods.get(&(tag, name_hash))
        {
            if let Some(ref module) = method.module
                && self.disabled_modules.contains(module)
            {
                return Err(self.make_error(format!(
                    "module '{}' is disabled; cannot call '{}'",
                    module, method.name
                )));
            }

            if let Some(expected) = method.arity
                && expected != arg_count
            {
                return Err(self.make_error(format!(
                    "method '{}' expects {} arguments, got {}",
                    method.name, expected, arg_count
                )));
            }

            // Use raw pointer to body to avoid Rc::clone refcount bump.
            // SAFETY: methods table is not modified during execution.
            let body: MethodBodyPtr = &*method.body;
            let arg_start = receiver_abs + 1;
            let result = {
                let args = &self.stack[arg_start..arg_start + n];
                (unsafe { &*body })(&receiver, args).map_err(|msg| self.make_error(msg))?
            };
            self.stack[receiver_abs] = result;
            return Ok(());
        }

        // Sequence-capable method dispatch (callback support via trampoline).
        if let Some(tag) = receiver.tag()
            && let Some(seq_method) = self.seq_methods.get(&(tag, name_hash))
        {
            if let Some(ref module) = seq_method.module
                && self.disabled_modules.contains(module)
            {
                return Err(self.make_error(format!(
                    "module '{}' is disabled; cannot call '{}'",
                    module, seq_method.name
                )));
            }
            if let Some(expected) = seq_method.arity
                && expected != arg_count
            {
                return Err(self.make_error(format!(
                    "method '{}' expects {} arguments, got {}",
                    seq_method.name, expected, arg_count
                )));
            }
            let body = Rc::clone(&seq_method.body);
            let arg_start = receiver_abs + 1;
            let native_result = {
                let args = &self.stack[arg_start..arg_start + n];
                (body)(&receiver, args).map_err(|msg| self.make_error(msg))?
            };
            match native_result {
                super::super::sequence::NativeResult::Value(v) => {
                    self.stack[receiver_abs] = v;
                }
                super::super::sequence::NativeResult::Sequence(seq) => {
                    self.drive_sequence(seq, receiver_abs)?;
                }
            }
            return Ok(());
        }

        // AoSoA iter_field (non-callback, kept inline)
        #[cfg(feature = "mobile-aosoa")]
        if let Value::AoSoA(ref container) = receiver {
            let iter_field_hash = string_hash("iter_field");

            if name_hash == iter_field_hash && arg_count == 1 {
                let field_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => s.to_string(),
                    _ => {
                        return Err(
                            self.make_error("iter_field expects a string field name".to_string())
                        );
                    }
                };
                let container = container.borrow();
                let values: Vec<Value> = match container.iter_field(&field_name) {
                    Some(iter) => iter.cloned().collect(),
                    None => {
                        return Err(self.make_error(format!(
                            "'{}' has no field '{}'",
                            container.layout.type_name, field_name
                        )));
                    }
                };
                drop(container);
                self.stack[receiver_abs] = Value::Array(Rc::new(RefCell::new(values)));
                return Ok(());
            }
        }

        // Object method dispatch
        if let Value::Object(ref obj) = receiver {
            let method_name = self
                .field_names
                .get(&name_hash)
                .cloned()
                .unwrap_or_else(|| format!("<unknown:{name_hash}>"));

            // Walk inheritance chain for compiled methods
            let class_name = obj.borrow().type_name().to_string();
            let mut search_class = Some(class_name.clone());
            let mut found_func = None;
            while let Some(ref cls) = search_class {
                let qualified = format!("{cls}::{method_name}");
                if let Some(&func_idx) = self.function_map.get(&qualified) {
                    found_func = Some(func_idx);
                    break;
                }
                search_class = self.class_metas.get(cls).and_then(|m| m.parent.clone());
            }

            if let Some(func_idx) = found_func {
                let args: Vec<Value> = (0..n)
                    .map(|i| self.stack[receiver_abs + 1 + i].clone())
                    .collect();
                let mut all_args = Vec::with_capacity(1 + args.len());
                all_args.push(receiver);
                all_args.extend(args);
                let result = self.call_compiled_function(func_idx, &all_args)?;
                self.stack[receiver_abs] = result;
                return Ok(());
            }

            // Fall back to WritObject::call_method
            let args: Vec<Value> = (0..n)
                .map(|i| self.stack[receiver_abs + 1 + i].clone())
                .collect();
            let result = obj
                .borrow_mut()
                .call_method(&method_name, &args)
                .map_err(|e| self.make_error(e))?;
            self.stack[receiver_abs] = result;
            return Ok(());
        }

        // Struct method dispatch
        if let Value::Struct(ref s) = receiver {
            let method_name = self
                .field_names
                .get(&name_hash)
                .cloned()
                .unwrap_or_else(|| format!("<unknown:{name_hash}>"));

            let qualified = format!("{}::{}", s.layout.type_name, method_name);
            if let Some(&func_idx) = self.function_map.get(&qualified) {
                let args: Vec<Value> = (0..n)
                    .map(|i| self.stack[receiver_abs + 1 + i].clone())
                    .collect();
                let mut all_args = Vec::with_capacity(1 + args.len());
                all_args.push(receiver);
                all_args.extend(args);
                let result = self.call_compiled_function(func_idx, &all_args)?;
                self.stack[receiver_abs] = result;
                return Ok(());
            }
        }

        let method_name = self
            .field_names
            .get(&name_hash)
            .cloned()
            .unwrap_or_else(|| format!("<unknown:{name_hash}>"));
        Err(self.make_error(format!(
            "no method '{}' on {}",
            method_name,
            receiver.type_name()
        )))
    }
}
