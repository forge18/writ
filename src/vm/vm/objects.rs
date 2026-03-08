use std::cell::RefCell;
use std::rc::Rc;

use crate::compiler::string_hash;
use super::super::class_instance::WritClassInstance;
use super::super::error::RuntimeError;
use super::super::frame::ChunkId;
use super::super::value::{ClosureData, Value};
use super::super::writ_struct::WritStruct;
use super::VM;

impl VM {

    /// Register-based GetField.
    pub(super) fn exec_get_field_reg(
        &mut self,
        base: usize,
        dst: u8,
        obj_reg: u8,
        name_hash: u32,
    ) -> Result<(), RuntimeError> {
        let object = &self.stack[base + obj_reg as usize];
        let result = match object {
            Value::Array(arr) => {
                let length_hash = string_hash("length");
                if name_hash == length_hash {
                    Value::I32(arr.borrow().len() as i32)
                } else {
                    return Err(self.make_error(format!("unknown array field (hash {name_hash})")));
                }
            }
            Value::Dict(dict) => {
                let dict = dict.borrow();
                dict.iter()
                    .find(|(key, _)| string_hash(key) == name_hash)
                    .map(|(_, v)| v.clone())
                    .unwrap_or(Value::Null)
            }
            Value::Object(obj) => {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?;
                obj.borrow()
                    .get_field_by_hash(name_hash, field_name)
                    .map_err(|e| self.make_error(e))?
            }
            Value::Struct(s) => s.get_field_by_hash(name_hash).cloned().ok_or_else(|| {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .cloned()
                    .unwrap_or_else(|| format!("<hash:{name_hash}>"));
                self.make_error(format!(
                    "'{}' has no field '{}'",
                    s.layout.type_name, field_name
                ))
            })?,
            #[cfg(feature = "mobile-aosoa")]
            Value::AoSoA(container) => {
                let length_hash = string_hash("length");
                if name_hash == length_hash {
                    Value::I32(container.borrow().len() as i32)
                } else {
                    return Err(self.make_error(format!("unknown AoSoA field (hash {name_hash})")));
                }
            }
            _ => {
                return Err(self.make_error(format!("field access on {}", object.type_name())));
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based SetField.
    pub(super) fn exec_set_field_reg(
        &mut self,
        base: usize,
        obj_reg: u8,
        name_hash: u32,
        val_reg: u8,
    ) -> Result<(), RuntimeError> {
        let value = self.stack[base + val_reg as usize].clone();
        let object = &self.stack[base + obj_reg as usize];
        match object {
            Value::Dict(dict) => {
                let mut dict = dict.borrow_mut();
                let existing_key = dict
                    .keys()
                    .find(|key| string_hash(key) == name_hash)
                    .cloned();
                if let Some(key) = existing_key {
                    dict.insert(key, value);
                } else if let Some(name) = self.field_names.get(&name_hash) {
                    dict.insert(name.clone(), value);
                } else {
                    return Err(
                        self.make_error(format!("cannot set unknown field (hash {name_hash})"))
                    );
                }
            }
            Value::Object(obj) => {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?;
                obj.borrow_mut()
                    .set_field_by_hash(name_hash, field_name, value)
                    .map_err(|e| self.make_error(e))?;
            }
            Value::Struct(_) => {
                // For structs, we need to take ownership to mutate
                let mut s = match std::mem::replace(
                    &mut self.stack[base + obj_reg as usize],
                    Value::Null,
                ) {
                    Value::Struct(s) => s,
                    _ => unreachable!(),
                };
                s.set_field_by_hash(name_hash, value)
                    .map_err(|e| self.make_error(e))?;
                self.stack[base + obj_reg as usize] = Value::Struct(s);
            }
            _ => {
                return Err(self.make_error(format!("field assignment on {}", object.type_name())));
            }
        }
        Ok(())
    }

    /// Register-based GetIndex.
    pub(super) fn exec_get_index_reg(
        &mut self,
        base: usize,
        dst: u8,
        obj_reg: u8,
        idx_reg: u8,
    ) -> Result<(), RuntimeError> {
        let collection = &self.stack[base + obj_reg as usize];
        let index = &self.stack[base + idx_reg as usize];
        let result = match (collection, index) {
            (Value::Array(arr), idx_val @ (Value::I32(_) | Value::I64(_))) => {
                let arr = arr.borrow();
                let i = idx_val.as_i64();
                if i < 0 || i as usize >= arr.len() {
                    return Err(self.make_error(format!(
                        "array index {i} out of bounds (length {})",
                        arr.len()
                    )));
                }
                arr[i as usize].clone()
            }
            (Value::Dict(dict), Value::Str(key)) => {
                let dict = dict.borrow();
                dict.get(&**key).cloned().unwrap_or(Value::Null)
            }
            #[cfg(feature = "mobile-aosoa")]
            (Value::AoSoA(container), idx_val @ (Value::I32(_) | Value::I64(_))) => {
                let container = container.borrow();
                let i = idx_val.as_i64();
                if i < 0 || i as usize >= container.len() {
                    return Err(self.make_error(format!(
                        "array index {} out of bounds (length {})",
                        i,
                        container.len()
                    )));
                }
                let writ_struct = container.get(i as usize).unwrap();
                Value::Struct(Box::new(writ_struct))
            }
            _ => {
                return Err(self.make_error(format!(
                    "cannot index {} with {}",
                    collection.type_name(),
                    index.type_name()
                )));
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based SetIndex.
    pub(super) fn exec_set_index_reg(
        &mut self,
        base: usize,
        obj_reg: u8,
        idx_reg: u8,
        val_reg: u8,
    ) -> Result<(), RuntimeError> {
        let value = self.stack[base + val_reg as usize].clone();
        let collection = &self.stack[base + obj_reg as usize];
        let index = &self.stack[base + idx_reg as usize];
        match (collection, index) {
            (Value::Array(arr), idx_val @ (Value::I32(_) | Value::I64(_))) => {
                let mut arr = arr.borrow_mut();
                let i = idx_val.as_i64();
                if i < 0 || i as usize >= arr.len() {
                    return Err(self.make_error(format!(
                        "array index {i} out of bounds (length {})",
                        arr.len()
                    )));
                }
                arr[i as usize] = value;
            }
            (Value::Dict(dict), Value::Str(key)) => {
                let mut dict = dict.borrow_mut();
                dict.insert(key.to_string(), value);
            }
            #[cfg(feature = "mobile-aosoa")]
            (Value::AoSoA(container), idx_val @ (Value::I32(_) | Value::I64(_))) => {
                let i = idx_val.as_i64();
                let mut container = container.borrow_mut();
                if i < 0 || i as usize >= container.len() {
                    return Err(self.make_error(format!(
                        "array index {} out of bounds (length {})",
                        i,
                        container.len()
                    )));
                }
                if let Value::Struct(s) = &value {
                    container
                        .set(i as usize, s)
                        .map_err(|e| self.make_error(e))?;
                } else {
                    return Err(self.make_error(format!(
                        "cannot assign {} to AoSoA element (expected struct)",
                        value.type_name()
                    )));
                }
            }
            _ => {
                return Err(self.make_error(format!(
                    "cannot index-assign {} with {}",
                    collection.type_name(),
                    index.type_name()
                )));
            }
        }
        Ok(())
    }

    /// Register-based MakeStruct.
    pub(super) fn exec_make_struct_reg(
        &mut self,
        base: usize,
        dst: u8,
        name_idx: u32,
        start: u8,
        field_count: u8,
        chunk_id: ChunkId,
    ) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let struct_name = chunk
            .rc_strings()
            .get(name_idx as usize)
            .map(|s| s.to_string())
            .ok_or_else(|| self.make_error(format!("invalid struct name index {name_idx}")))?;

        let layout = self
            .struct_layouts
            .get(&struct_name)
            .cloned()
            .ok_or_else(|| self.make_error(format!("unknown struct type '{struct_name}'")))?;

        let n = field_count as usize;
        let mut fields = Vec::with_capacity(layout.field_count);
        for i in 0..layout.field_count {
            if i < n {
                fields.push(self.stack[base + start as usize + i].clone());
            } else {
                fields.push(Value::Null);
            }
        }

        let writ_struct = WritStruct { layout, fields };
        self.stack[base + dst as usize] = Value::Struct(Box::new(writ_struct));
        Ok(())
    }

    /// Register-based MakeClass.
    pub(super) fn exec_make_class_reg(
        &mut self,
        base: usize,
        dst: u8,
        name_idx: u32,
        start: u8,
        field_count: u8,
        chunk_id: ChunkId,
    ) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let class_name = chunk
            .rc_strings()
            .get(name_idx as usize)
            .map(|s| s.to_string())
            .ok_or_else(|| self.make_error(format!("invalid class name index {name_idx}")))?;

        let layout = self
            .class_layouts
            .get(&class_name)
            .cloned()
            .ok_or_else(|| self.make_error(format!("unknown class type '{class_name}'")))?;

        let parent_class = self
            .class_metas
            .get(&class_name)
            .and_then(|m| m.parent.clone());

        let n = field_count as usize;
        let mut fields = Vec::with_capacity(layout.field_count);
        for i in 0..layout.field_count {
            if i < n {
                fields.push(self.stack[base + start as usize + i].clone());
            } else {
                fields.push(Value::Null);
            }
        }

        let instance = WritClassInstance {
            layout,
            fields,
            parent_class,
        };

        self.stack[base + dst as usize] = Value::Object(Rc::new(RefCell::new(instance)));
        Ok(())
    }

    /// Register-based MakeClosure.
    pub(super) fn exec_make_closure_reg(
        &mut self,
        base: usize,
        dst: u8,
        func_idx: u16,
    ) -> Result<(), RuntimeError> {
        let func_idx = func_idx as usize;
        let descriptors = self.functions[func_idx].upvalues.clone();
        let mut upvalues = Vec::with_capacity(descriptors.len());

        for desc in &descriptors {
            if desc.is_local {
                let abs_slot = self.current_frame().base + desc.index as usize;
                let idx = self.capture_local(abs_slot);
                upvalues.push(idx);
            } else {
                let parent_uv = self
                    .frame_upvalues
                    .last()
                    .and_then(|o| o.as_ref())
                    .expect("transitive capture requires parent closure");
                upvalues.push(parent_uv[desc.index as usize]);
            }
        }

        // Store upvalue indices in closure_map so that call_function / CallDirect can
        // provide correct upvalues after the main chunk's stack has been cleared.
        if func_idx < self.closure_map.len() {
            self.closure_map[func_idx] = Some(upvalues.clone());
        }

        let abs_dst = base + dst as usize;
        self.stack[abs_dst] = Value::Closure(Box::new(ClosureData { func_idx, upvalues }));
        // Self-recursive closures capture their own destination slot as an upvalue;
        // close it so the upvalue store holds the completed closure value.
        if self.has_open_upvalues
            && abs_dst < self.open_upvalues.len()
            && let Some(uv_idx) = self.open_upvalues[abs_dst]
        {
            self.upvalue_store[uv_idx as usize] = self.stack[abs_dst].cheap_clone();
        }
        Ok(())
    }

}
