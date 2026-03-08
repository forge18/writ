use super::*;

impl Compiler {
    pub(super) fn compile_func_decl(&mut self, func: &FuncDecl, span: &Span) -> Result<(), CompileError> {
        let line = span.line;

        // Determine if this is a nested (closure) function or top-level.
        let nested = !self.enclosing_scopes.is_empty() || self.scope_depth > 0;

        // Pre-register the function index so recursive self-calls can use
        // CallDirect instead of LoadGlobal + Call.
        if !nested {
            let pre_func_idx = self.functions.len() as u16;
            self.function_index.insert(func.name.clone(), pre_func_idx);
        }

        // Pre-register return type for typed instruction emission
        if let Some(ref ret_type) = func.return_type {
            let expr_type = type_expr_to_expr_type(ret_type);
            self.function_return_types
                .insert(func.name.clone(), expr_type);
        }
        let predeclared_slot = if nested {
            let slot = self.add_local(&func.name, span)?;
            self.emit(Instruction::LoadNull(slot), line);
            Some(slot)
        } else {
            None
        };

        // Save current compiler state
        let saved_chunk = std::mem::take(&mut self.chunk);
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_scope_depth = self.scope_depth;
        let saved_loop_stack = std::mem::take(&mut self.loop_stack);
        let saved_has_yield = self.has_yield;
        let saved_upvalues = std::mem::take(&mut self.current_upvalues);
        let saved_reg_types = std::mem::take(&mut self.reg_types);
        let saved_next_reg = self.next_reg;
        let saved_max_reg = self.max_reg;

        // Push current scope as enclosing (for upvalue resolution)
        self.enclosing_scopes.push(EnclosingScope {
            locals: saved_locals,
            upvalues: saved_upvalues,
        });

        // Reset for function compilation
        self.scope_depth = 0;
        self.has_yield = false;
        self.next_reg = 0;
        self.max_reg = 0;

        // Add parameters as locals (regs 0, 1, 2, ...) with type tags
        for param in &func.params {
            let param_type = type_expr_to_expr_type(&param.type_annotation);
            self.add_typed_local(&param.name, span, param_type)?;
        }

        // Compile function body
        for stmt in &func.body {
            self.compile_stmt(stmt)?;
        }

        // Ensure implicit return if body doesn't end with Return
        if self.chunk.is_empty()
            || !matches!(
                self.chunk.last_opcode(),
                Some(op::Return) | Some(op::ReturnNull)
            )
        {
            self.emit(Instruction::ReturnNull, line);
        }

        let func_chunk = std::mem::replace(&mut self.chunk, saved_chunk);
        let is_coroutine = self.has_yield;
        let func_upvalues = std::mem::take(&mut self.current_upvalues);
        let has_upvalues = !func_upvalues.is_empty();
        let func_max_registers = self.max_reg;
        let func_has_rc_values = !func_upvalues.is_empty()
            || self.reg_types[..func_max_registers as usize].contains(&ExprType::Other);

        // Pop enclosing scope (locals may have been marked as captured)
        let enclosing = self.enclosing_scopes.pop().unwrap();
        self.locals = enclosing.locals;
        self.current_upvalues = enclosing.upvalues;
        self.scope_depth = saved_scope_depth;
        self.loop_stack = saved_loop_stack;
        self.has_yield = saved_has_yield;
        self.reg_types = saved_reg_types;
        self.next_reg = saved_next_reg;
        self.max_reg = saved_max_reg;

        let is_variadic = func.params.last().is_some_and(|p| p.is_variadic);
        let arity = u8::try_from(func.params.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many function parameters (max 255)".to_string(),
            span: span.clone(),
        })?;

        let func_idx = self.functions.len();
        if !nested {
            self.function_index
                .insert(func.name.clone(), func_idx as u16);
        }
        self.functions.push(CompiledFunction {
            name: func.name.clone(),
            arity,
            chunk: func_chunk,
            is_coroutine,
            is_variadic,
            upvalues: func_upvalues,
            max_registers: func_max_registers,
            has_rc_values: func_has_rc_values,
        });

        if let Some(slot) = predeclared_slot {
            if has_upvalues {
                let func_idx_u16 = u16::try_from(func_idx).map_err(|_| CompileError {
                    annotation: None,
                    message: "too many functions (max 65535)".to_string(),
                    span: span.clone(),
                })?;
                self.emit(Instruction::MakeClosure(slot, func_idx_u16), line);
            } else {
                let index = self.chunk.add_string(&func.name);
                self.emit(Instruction::LoadStr(slot, index), line);
            }
        } else if has_upvalues {
            // Top-level function captures upvalues — emit MakeClosure into a
            // scratch register so the VM populates closure_map while the main
            // chunk's stack is still live.
            let scratch = self.alloc_temp(span)?;
            let func_idx_u16 = u16::try_from(func_idx).map_err(|_| CompileError {
                annotation: None,
                message: "too many functions (max 65535)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::MakeClosure(scratch, func_idx_u16), line);
            self.free_temp(scratch);
        }

        Ok(())
    }

    pub(super) fn compile_lambda(
        &mut self,
        params: &[crate::parser::FuncParam],
        body: &crate::parser::LambdaBody,
        span: &Span,
        dst: u8,
    ) -> Result<(), CompileError> {
        let line = span.line;

        // Save current compiler state
        let saved_chunk = std::mem::take(&mut self.chunk);
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_scope_depth = self.scope_depth;
        let saved_loop_stack = std::mem::take(&mut self.loop_stack);
        let saved_has_yield = self.has_yield;
        let saved_upvalues = std::mem::take(&mut self.current_upvalues);
        let saved_reg_types = std::mem::take(&mut self.reg_types);
        let saved_next_reg = self.next_reg;
        let saved_max_reg = self.max_reg;

        self.enclosing_scopes.push(EnclosingScope {
            locals: saved_locals,
            upvalues: saved_upvalues,
        });

        self.scope_depth = 0;
        self.has_yield = false;
        self.next_reg = 0;
        self.max_reg = 0;

        // Add parameters as locals
        for param in params {
            self.add_local(&param.name, span)?;
        }

        // Compile lambda body
        match body {
            crate::parser::LambdaBody::Expr(expr) => {
                let reg = self.compile_expr(expr, None)?;
                self.emit(Instruction::Return(reg), line);
            }
            crate::parser::LambdaBody::Block(stmts) => {
                for stmt in stmts {
                    self.compile_stmt(stmt)?;
                }
                if self.chunk.is_empty()
                    || !matches!(
                        self.chunk.last_opcode(),
                        Some(op::Return) | Some(op::ReturnNull)
                    )
                {
                    self.emit(Instruction::ReturnNull, line);
                }
            }
        }

        let lambda_chunk = std::mem::replace(&mut self.chunk, saved_chunk);
        let is_coroutine = self.has_yield;
        let lambda_upvalues = std::mem::take(&mut self.current_upvalues);
        let has_upvalues = !lambda_upvalues.is_empty();
        let lambda_max_registers = self.max_reg;
        let lambda_has_rc_values = has_upvalues
            || self.reg_types[..lambda_max_registers as usize].contains(&ExprType::Other);

        let enclosing = self.enclosing_scopes.pop().unwrap();
        self.locals = enclosing.locals;
        self.current_upvalues = enclosing.upvalues;
        self.scope_depth = saved_scope_depth;
        self.loop_stack = saved_loop_stack;
        self.has_yield = saved_has_yield;
        self.reg_types = saved_reg_types;
        self.next_reg = saved_next_reg;
        self.max_reg = saved_max_reg;

        let arity = u8::try_from(params.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many lambda parameters (max 255)".to_string(),
            span: span.clone(),
        })?;

        let func_idx = self.functions.len();
        let name = format!("__lambda_{}", func_idx);
        let is_variadic = params.last().is_some_and(|p| p.is_variadic);
        self.functions.push(CompiledFunction {
            name: name.clone(),
            arity,
            chunk: lambda_chunk,
            is_coroutine,
            is_variadic,
            upvalues: lambda_upvalues,
            max_registers: lambda_max_registers,
            has_rc_values: lambda_has_rc_values,
        });

        if has_upvalues {
            let func_idx = u16::try_from(func_idx).map_err(|_| CompileError {
                annotation: None,
                message: "too many functions (max 65535)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::MakeClosure(dst, func_idx), line);
        } else {
            let index = self.chunk.add_string(&name);
            self.emit(Instruction::LoadStr(dst, index), line);
        }

        Ok(())
    }

    // ── Function call compilation ──────────────────────────────────

    pub(super) fn save_state(&mut self) -> SavedState {
        SavedState {
            chunk: std::mem::take(&mut self.chunk),
            locals: std::mem::take(&mut self.locals),
            scope_depth: self.scope_depth,
            loop_stack: std::mem::take(&mut self.loop_stack),
            has_yield: self.has_yield,
            reg_types: std::mem::take(&mut self.reg_types),
            next_reg: self.next_reg,
            max_reg: self.max_reg,
        }
    }

    /// Restore compiler state from a saved state.
    /// The saved chunk is restored to `self.chunk`. Callers that need to
    /// keep the compiled chunk should `std::mem::take` it before calling this.
    pub(super) fn restore_state(&mut self, saved: SavedState) {
        self.chunk = saved.chunk;
        self.locals = saved.locals;
        self.scope_depth = saved.scope_depth;
        self.loop_stack = saved.loop_stack;
        self.has_yield = saved.has_yield;
        self.reg_types = saved.reg_types;
        self.next_reg = saved.next_reg;
        self.max_reg = saved.max_reg;
    }

    // ── Local variable management ──────────────────────────────────

    pub(super) fn resolve_local(&self, name: &str) -> Option<(u8, u8)> {
        self.locals
            .iter()
            .rev()
            .find(|local| local.name == name)
            .map(|local| (local.slot, local.type_tag))
    }

    pub(super) fn resolve_upvalue(&mut self, name: &str) -> Option<u8> {
        if self.enclosing_scopes.is_empty() {
            return None;
        }
        self.resolve_upvalue_in(name, self.enclosing_scopes.len() - 1)
    }

    pub(super) fn resolve_upvalue_in(&mut self, name: &str, scope_idx: usize) -> Option<u8> {
        let local_slot = self.enclosing_scopes[scope_idx]
            .locals
            .iter()
            .rev()
            .find(|l| l.name == name)
            .map(|l| l.slot);

        if let Some(slot) = local_slot {
            for local in &mut self.enclosing_scopes[scope_idx].locals {
                if local.name == name && local.slot == slot {
                    local.is_captured = true;
                    break;
                }
            }

            if scope_idx == self.enclosing_scopes.len() - 1 {
                return Some(self.add_upvalue(true, slot));
            }

            let mut prev_index = slot;
            let mut prev_is_local = true;
            for i in (scope_idx + 1)..self.enclosing_scopes.len() {
                let uv_idx = self.add_upvalue_to_scope(i, prev_is_local, prev_index);
                prev_index = uv_idx;
                prev_is_local = false;
            }
            return Some(self.add_upvalue(false, prev_index));
        }

        if scope_idx == 0 {
            return None;
        }

        let parent_uv_idx = self.resolve_upvalue_in(name, scope_idx - 1)?;

        if scope_idx == self.enclosing_scopes.len() - 1 {
            return Some(self.add_upvalue(false, parent_uv_idx));
        }

        let mut prev_index = parent_uv_idx;
        for i in (scope_idx + 1)..self.enclosing_scopes.len() {
            let uv_idx = self.add_upvalue_to_scope(i, false, prev_index);
            prev_index = uv_idx;
        }
        Some(self.add_upvalue(false, prev_index))
    }

    pub(super) fn add_upvalue(&mut self, is_local: bool, index: u8) -> u8 {
        for (i, uv) in self.current_upvalues.iter().enumerate() {
            if uv.is_local == is_local && uv.index == index {
                return i as u8;
            }
        }
        let idx = self.current_upvalues.len() as u8;
        self.current_upvalues
            .push(UpvalueDescriptor { is_local, index });
        idx
    }

    pub(super) fn add_upvalue_to_scope(&mut self, scope_idx: usize, is_local: bool, index: u8) -> u8 {
        let scope = &mut self.enclosing_scopes[scope_idx];
        for (i, uv) in scope.upvalues.iter().enumerate() {
            if uv.is_local == is_local && uv.index == index {
                return i as u8;
            }
        }
        let idx = scope.upvalues.len() as u8;
        scope.upvalues.push(UpvalueDescriptor { is_local, index });
        idx
    }

    pub(super) fn add_local(&mut self, name: &str, span: &Span) -> Result<u8, CompileError> {
        let slot = self.alloc_local(span)?;
        self.locals.push(Local {
            name: name.to_string(),
            slot,
            depth: self.scope_depth,
            is_captured: false,
            type_tag: 0,
        });
        Ok(slot)
    }

    pub(super) fn add_local_with_slot(
        &mut self,
        name: &str,
        span: &Span,
        slot: u8,
    ) -> Result<u8, CompileError> {
        let _ = span;
        self.locals.push(Local {
            name: name.to_string(),
            slot,
            depth: self.scope_depth,
            is_captured: false,
            type_tag: 0,
        });
        Ok(slot)
    }

    pub(super) fn add_typed_local(
        &mut self,
        name: &str,
        span: &Span,
        expr_type: ExprType,
    ) -> Result<u8, CompileError> {
        let slot = self.add_local(name, span)?;
        self.locals.last_mut().unwrap().type_tag = expr_type_to_tag(expr_type);
        self.set_reg_type(slot, expr_type);
        Ok(slot)
    }
}
