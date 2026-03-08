use std::rc::Rc;

use super::super::error::RuntimeError;
use super::super::value::Value;
use super::{ArithOps, CmpOps, VM};

impl VM {
    // ── Instruction helpers ──────────────────────────────────────────

    /// Performs a checked i32 operation, promoting to i64 on overflow.
    #[inline(always)]
    pub(super) fn int_arith_i32(
        a: i32,
        b: i32,
        i32_op: fn(i32, i32) -> Option<i32>,
        i64_op: fn(i64, i64) -> Option<i64>,
        err_msg: &str,
    ) -> Result<Value, String> {
        match i32_op(a, b) {
            Some(r) => Ok(Value::I32(r)),
            None => i64_op(a as i64, b as i64)
                .map(Value::I64)
                .ok_or_else(|| err_msg.to_string()),
        }
    }

    /// Performs a checked i64 operation.
    #[inline(always)]
    pub(super) fn int_arith_i64(
        a: i64,
        b: i64,
        op: fn(i64, i64) -> Option<i64>,
        err_msg: &str,
    ) -> Result<Value, String> {
        op(a, b).map(Value::I64).ok_or_else(|| err_msg.to_string())
    }

    /// Executes integer arithmetic with automatic width promotion.
    #[inline(always)]
    pub(super) fn exec_int_arith(
        &self,
        a: &Value,
        b: &Value,
        i32_op: fn(i32, i32) -> Option<i32>,
        i64_op: fn(i64, i64) -> Option<i64>,
    ) -> Result<Value, RuntimeError> {
        let result = match (a, b) {
            (Value::I32(a), Value::I32(b)) => {
                Self::int_arith_i32(*a, *b, i32_op, i64_op, "integer overflow")
            }
            _ => Self::int_arith_i64(a.as_i64(), b.as_i64(), i64_op, "integer overflow"),
        };
        result.map_err(|msg| self.make_error(msg))
    }

    // ── Upvalue helpers ──────────────────────────────────────────

    /// Captures a local variable into the flat upvalue store.
    /// Returns the store index. Reuses existing capture if already open.
    pub(super) fn capture_local(&mut self, abs_slot: usize) -> u32 {
        if abs_slot < self.open_upvalues.len()
            && let Some(existing_idx) = self.open_upvalues[abs_slot]
        {
            return existing_idx;
        }
        let idx = self.upvalue_store.len() as u32;
        self.upvalue_store.push(self.stack[abs_slot].clone());
        if abs_slot >= self.open_upvalues.len() {
            self.open_upvalues.resize(abs_slot + 1, None);
        }
        self.open_upvalues[abs_slot] = Some(idx);
        self.has_open_upvalues = true;
        idx
    }

    /// Closes all open upvalues at or above `min_slot` by syncing the
    /// current stack value into the flat upvalue store, then removing them
    /// from the open set.
    pub(super) fn close_upvalues_above(&mut self, min_slot: usize) {
        let end = self.open_upvalues.len();
        if min_slot >= end {
            return;
        }
        let mut any_remaining = false;
        for slot in min_slot..end {
            if let Some(uv_idx) = self.open_upvalues[slot].take()
                && slot < self.stack.len()
            {
                self.upvalue_store[uv_idx as usize] = self.stack[slot].clone();
            }
        }
        for slot in 0..min_slot.min(end) {
            if self.open_upvalues[slot].is_some() {
                any_remaining = true;
                break;
            }
        }
        self.has_open_upvalues = any_remaining;
    }

    // ── Register-based instruction helpers ───────────────────────

    /// Attempts to call a named operator method (`add`, `subtract`, etc.) on
    /// an Object or Struct receiver. Returns `Some(result)` if the receiver is
    /// an Object/Struct and the method is found; `None` otherwise so the caller
    /// can fall through to its normal error path.
    pub(super) fn try_operator_method(
        &mut self,
        receiver: Value,
        rhs: Value,
        method_name: &'static str,
    ) -> Option<Result<Value, RuntimeError>> {
        match &receiver {
            Value::Object(_) => {
                let class_name = if let Value::Object(obj) = &receiver {
                    obj.borrow().type_name().to_string()
                } else {
                    unreachable!()
                };
                let qualified = format!("{class_name}::{method_name}");
                let func_idx = self.function_map.get(&qualified).copied();
                // Also walk inheritance chain
                let func_idx = func_idx.or_else(|| {
                    let mut search = self
                        .class_metas
                        .get(&class_name)
                        .and_then(|m| m.parent.clone());
                    let mut found = None;
                    while let Some(cls) = search {
                        let q = format!("{cls}::{method_name}");
                        if let Some(&fi) = self.function_map.get(&q) {
                            found = Some(fi);
                            break;
                        }
                        search = self.class_metas.get(&cls).and_then(|m| m.parent.clone());
                    }
                    found
                });
                if let Some(fi) = func_idx {
                    let args = vec![receiver, rhs];
                    Some(self.call_compiled_function(fi, &args))
                } else {
                    // Try native WritObject::call_method
                    if let Value::Object(obj) = &receiver {
                        let result = obj
                            .borrow_mut()
                            .call_method(method_name, &[rhs])
                            .map_err(|e| self.make_error(e));
                        Some(result)
                    } else {
                        None
                    }
                }
            }
            Value::Struct(s) => {
                let qualified = format!("{}::{method_name}", s.layout.type_name);
                if let Some(&fi) = self.function_map.get(&qualified) {
                    let args = vec![receiver, rhs];
                    Some(self.call_compiled_function(fi, &args))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Register-based Add: handles int, float, mixed, string concat, and operator overloading.
    pub(super) fn exec_add_reg(&mut self, base: usize, dst: u8, a: u8, b: u8) -> Result<(), RuntimeError> {
        let a_ref = &self.stack[base + a as usize];
        let b_ref = &self.stack[base + b as usize];
        let result = match (a_ref, b_ref) {
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                self.exec_int_arith(a, b, i32::checked_add, i64::checked_add)?
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::promote_float_pair_op(a, b, |x, y| x + y)
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::F64(a.as_i64() as f64 + b.as_f64())
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                Value::F64(a.as_f64() + b.as_i64() as f64)
            }
            (Value::Str(a), Value::Str(b)) => Value::Str(Rc::from(format!("{a}{b}").as_str())),
            _ => {
                let lhs_v = self.stack[base + a as usize].clone();
                let rhs_v = self.stack[base + b as usize].clone();
                if let Some(res) = self.try_operator_method(lhs_v, rhs_v, "add") {
                    res?
                } else {
                    return Err(self.make_error(format!(
                        "cannot add {} and {}",
                        self.stack[base + a as usize].type_name(),
                        self.stack[base + b as usize].type_name()
                    )));
                }
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based binary arithmetic (Sub, Mul).
    /// `dst_abs`, `a_abs`, `b_abs` are absolute stack indices (base + reg).
    pub(super) fn exec_binary_arith_reg(
        &mut self,
        dst_abs: usize,
        a_abs: usize,
        b_abs: usize,
        ops: ArithOps,
    ) -> Result<(), RuntimeError> {
        let ArithOps {
            i32_op,
            i64_op,
            f64_op,
            method_name,
        } = ops;
        let a_ref = &self.stack[a_abs];
        let b_ref = &self.stack[b_abs];
        let result = match (a_ref, b_ref) {
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                self.exec_int_arith(a, b, i32_op, i64_op)?
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::promote_float_pair_op(a, b, f64_op)
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::F64(f64_op(a.as_i64() as f64, b.as_f64()))
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                Value::F64(f64_op(a.as_f64(), b.as_i64() as f64))
            }
            _ => {
                let lhs_v = self.stack[a_abs].clone();
                let rhs_v = self.stack[b_abs].clone();
                if let Some(res) = self.try_operator_method(lhs_v, rhs_v, method_name) {
                    res?
                } else {
                    return Err(self.make_error(format!(
                        "cannot perform arithmetic on {} and {}",
                        self.stack[a_abs].type_name(),
                        self.stack[b_abs].type_name()
                    )));
                }
            }
        };
        self.stack[dst_abs] = result;
        Ok(())
    }

    /// Register-based Div with zero-check.
    pub(super) fn exec_div_reg(&mut self, base: usize, dst: u8, a: u8, b: u8) -> Result<(), RuntimeError> {
        let a_ref = &self.stack[base + a as usize];
        let b_ref = &self.stack[base + b as usize];
        let result = match (a_ref, b_ref) {
            (
                Value::I32(_) | Value::I64(_) | Value::F32(_) | Value::F64(_),
                b @ (Value::I32(_) | Value::I64(_)),
            ) if b.as_i64() == 0 => {
                return Err(self.make_error("division by zero".to_string()));
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                self.exec_int_arith(a, b, i32::checked_div, i64::checked_div)?
            }
            (_, b @ (Value::F32(_) | Value::F64(_))) if b.as_f64() == 0.0 => {
                return Err(self.make_error("division by zero".to_string()));
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::promote_float_pair_op(a, b, |x, y| x / y)
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::F64(a.as_i64() as f64 / b.as_f64())
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                Value::F64(a.as_f64() / b.as_i64() as f64)
            }
            _ => {
                let lhs_v = self.stack[base + a as usize].clone();
                let rhs_v = self.stack[base + b as usize].clone();
                if let Some(res) = self.try_operator_method(lhs_v, rhs_v, "divide") {
                    res?
                } else {
                    return Err(self.make_error(format!(
                        "cannot divide {} by {}",
                        self.stack[base + a as usize].type_name(),
                        self.stack[base + b as usize].type_name()
                    )));
                }
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based Mod with zero-check.
    pub(super) fn exec_mod_reg(&mut self, base: usize, dst: u8, a: u8, b: u8) -> Result<(), RuntimeError> {
        let a_ref = &self.stack[base + a as usize];
        let b_ref = &self.stack[base + b as usize];
        let result = match (a_ref, b_ref) {
            (Value::I32(_) | Value::I64(_), b @ (Value::I32(_) | Value::I64(_)))
                if b.as_i64() == 0 =>
            {
                return Err(self.make_error("modulo by zero".to_string()));
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                self.exec_int_arith(a, b, i32::checked_rem, i64::checked_rem)?
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::promote_float_pair_op(a, b, |x, y| x % y)
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                Value::F64(a.as_i64() as f64 % b.as_f64())
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                Value::F64(a.as_f64() % b.as_i64() as f64)
            }
            _ => {
                let lhs_v = self.stack[base + a as usize].clone();
                let rhs_v = self.stack[base + b as usize].clone();
                if let Some(res) = self.try_operator_method(lhs_v, rhs_v, "modulo") {
                    res?
                } else {
                    return Err(self.make_error(format!(
                        "cannot modulo {} by {}",
                        self.stack[base + a as usize].type_name(),
                        self.stack[base + b as usize].type_name()
                    )));
                }
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based comparison.
    pub(super) fn exec_comparison_reg(
        &mut self,
        base: usize,
        dst: u8,
        a: u8,
        b: u8,
        ops: CmpOps,
    ) -> Result<(), RuntimeError> {
        let CmpOps {
            i64_cmp,
            f64_cmp,
            method_name,
        } = ops;
        let a_ref = &self.stack[base + a as usize];
        let b_ref = &self.stack[base + b as usize];
        let result = match (a_ref, b_ref) {
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                i64_cmp(&a.as_i64(), &b.as_i64())
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                f64_cmp(&a.as_f64(), &b.as_f64())
            }
            (a @ (Value::I32(_) | Value::I64(_)), b @ (Value::F32(_) | Value::F64(_))) => {
                f64_cmp(&(a.as_i64() as f64), &b.as_f64())
            }
            (a @ (Value::F32(_) | Value::F64(_)), b @ (Value::I32(_) | Value::I64(_))) => {
                f64_cmp(&a.as_f64(), &(b.as_i64() as f64))
            }
            _ => {
                let lhs_v = self.stack[base + a as usize].clone();
                let rhs_v = self.stack[base + b as usize].clone();
                if let Some(res) = self.try_operator_method(lhs_v, rhs_v, method_name) {
                    // Operator method returns bool as Value::Bool
                    match res? {
                        Value::Bool(b) => {
                            self.stack[base + dst as usize] = Value::Bool(b);
                            return Ok(());
                        }
                        other => {
                            self.stack[base + dst as usize] = Value::Bool(!other.is_falsy());
                            return Ok(());
                        }
                    }
                } else {
                    return Err(self.make_error(format!(
                        "cannot compare {} and {}",
                        self.stack[base + a as usize].type_name(),
                        self.stack[base + b as usize].type_name()
                    )));
                }
            }
        };
        self.stack[base + dst as usize] = Value::Bool(result);
        Ok(())
    }
}
