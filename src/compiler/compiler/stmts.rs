use super::*;

impl Compiler {
    /// Compiles a type-annotated statement produced by the type checker.
    ///
    /// For `Let` / `Var` / `Const` initializers, the checker-inferred type from
    /// [`TypedStmt::expr_type`] seeds the register type table directly, replacing
    /// the post-hoc read-back from [`compile_stmt`](Self::compile_stmt). This
    /// ensures the compiler and type checker agree on every local variable's type
    /// without the compiler re-inferring independently.
    ///
    /// All other statement kinds delegate to [`compile_stmt`](Self::compile_stmt).
    pub fn compile_typed_stmt(&mut self, typed: &TypedStmt) -> Result<(), CompileError> {
        match &typed.stmt.kind {
            StmtKind::Let {
                name, initializer, ..
            }
            | StmtKind::Var {
                name, initializer, ..
            } => {
                let slot = self.add_local(name, &typed.stmt.span)?;
                self.compile_expr(initializer, Some(slot))?;
                let checked = checked_type_to_expr_type(&typed.expr_type);
                self.locals.last_mut().unwrap().type_tag = expr_type_to_tag(checked);
                self.set_reg_type(slot, checked);
            }
            StmtKind::Const { name, initializer } => {
                let slot = self.add_local(name, &typed.stmt.span)?;
                self.compile_expr(initializer, Some(slot))?;
                let checked = checked_type_to_expr_type(&typed.expr_type);
                self.set_reg_type(slot, checked);
            }
            _ => {
                self.compile_stmt(&typed.stmt)?;
            }
        }
        Ok(())
    }

    pub(super) fn compile_block(&mut self, stmts: &[Stmt], line: u32) -> Result<(), CompileError> {
        self.begin_scope();
        for stmt in stmts {
            self.compile_stmt(stmt)?;
        }
        self.end_scope(line);
        Ok(())
    }

    // --- Statement compilation ---

    pub fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        let line = stmt.span.line;
        match &stmt.kind {
            StmtKind::Let {
                name, initializer, ..
            }
            | StmtKind::Var {
                name, initializer, ..
            } => {
                let slot = self.add_local(name, &stmt.span)?;
                self.compile_expr(initializer, Some(slot))?;
                let init_type = self.reg_type(slot);
                self.locals.last_mut().unwrap().type_tag = expr_type_to_tag(init_type);
            }
            StmtKind::Const { name, initializer } => {
                let slot = self.add_local(name, &stmt.span)?;
                self.compile_expr(initializer, Some(slot))?;
            }
            StmtKind::LetDestructure { names, initializer } => {
                // Compile the initializer (expected to produce a tuple/array)
                let tuple_reg = self.alloc_temp(&stmt.span)?;
                self.compile_expr(initializer, Some(tuple_reg))?;
                let tuple_local = self.add_local_with_slot("__tuple", &stmt.span, tuple_reg)?;
                let _ = tuple_local;
                // Extract each element by index into separate locals
                for (i, name) in names.iter().enumerate() {
                    let elem_slot = self.add_local(name, &stmt.span)?;
                    let idx_reg = self.alloc_temp(&stmt.span)?;
                    self.emit(Instruction::LoadInt(idx_reg, i as i32), line);
                    self.emit(Instruction::GetIndex(elem_slot, tuple_reg, idx_reg), line);
                    self.free_temp(idx_reg);
                }
            }
            StmtKind::Assignment { target, op, value } => {
                self.compile_assignment(target, op, value, &stmt.span)?;
            }
            StmtKind::ExprStmt(expr) => {
                // Compile to a temp, then free it (no Pop needed)
                let reg = self.compile_expr(expr, None)?;
                if reg >= self.next_reg.saturating_sub(1) && reg == self.next_reg - 1 {
                    self.free_temp(reg);
                }
            }
            StmtKind::Block(stmts) => {
                self.compile_block(stmts, line)?;
            }
            StmtKind::If {
                condition,
                then_block,
                else_branch,
            } => {
                self.compile_if(condition, then_block, else_branch, line)?;
            }
            StmtKind::While { condition, body } => {
                self.compile_while(condition, body, line)?;
            }
            StmtKind::For {
                variable,
                iterable,
                body,
            } => {
                self.compile_for(variable, iterable, body, &stmt.span)?;
            }
            StmtKind::When { subject, arms } => {
                self.compile_when(subject, arms, &stmt.span)?;
            }
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    let reg = self.compile_expr(e, None)?;
                    self.emit(Instruction::Return(reg), line);
                    // Don't free temp -- we're returning
                } else {
                    self.emit(Instruction::ReturnNull, line);
                }
            }
            StmtKind::Break => {
                self.compile_break(&stmt.span)?;
            }
            StmtKind::Continue => {
                self.compile_continue(&stmt.span)?;
            }
            StmtKind::Func(decl) => {
                self.compile_func_decl(decl, &stmt.span)?;
            }
            StmtKind::Start(expr) => {
                self.compile_start(expr, &stmt.span)?;
            }
            StmtKind::Struct(decl) => {
                self.compile_struct_decl(decl, &stmt.span)?;
            }
            StmtKind::Class(decl) => {
                self.compile_class_decl(decl, &stmt.span)?;
            }
            StmtKind::Trait(_)
            | StmtKind::Enum(_)
            | StmtKind::Import(_)
            | StmtKind::WildcardImport(_) => {
                // Type-checker-only constructs; no bytecode emitted.
            }
            other => {
                return Err(CompileError {
                    annotation: None,
                    message: format!("unsupported statement: {other:?}"),
                    span: stmt.span.clone(),
                });
            }
        }
        Ok(())
    }

    // --- Expression compilation ---
    // Returns the register containing the result.
    // If `dst` is Some, the result is placed there; otherwise a temp is allocated.

    pub(super) fn compile_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_branch: &Option<ElseBranch>,
        line: u32,
    ) -> Result<(), CompileError> {
        let cond_reg = self.compile_expr(condition, None)?;
        let else_jump = self
            .chunk
            .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
        // Free condition temp
        self.maybe_free_temp(cond_reg, 0);

        self.compile_block(then_block, line)?;

        if let Some(branch) = else_branch {
            let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
            self.chunk.patch_jump(else_jump);

            match branch {
                ElseBranch::ElseBlock(stmts) => {
                    self.compile_block(stmts, line)?;
                }
                ElseBranch::ElseIf(if_stmt) => {
                    self.compile_stmt(if_stmt)?;
                }
            }
            self.chunk.patch_jump(end_jump);
        } else {
            self.chunk.patch_jump(else_jump);
        }

        Ok(())
    }

    // --- Control flow: while ---

    pub(super) fn compile_while(
        &mut self,
        condition: &Expr,
        body: &[Stmt],
        line: u32,
    ) -> Result<(), CompileError> {
        let loop_start = self.chunk.current_offset();

        let cond_reg = self.compile_expr(condition, None)?;
        let exit_jump = self
            .chunk
            .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
        self.maybe_free_temp(cond_reg, 0);

        self.loop_stack.push(LoopContext {
            start_offset: loop_start,
            break_jumps: Vec::new(),
            scope_depth: self.scope_depth,
        });

        self.compile_block(body, line)?;

        // Backward jump to loop start
        let current = self.chunk.current_offset();
        let back_offset = (loop_start as i32) - (current as i32) - 1;
        self.emit(Instruction::Jump(back_offset), line);

        self.chunk.patch_jump(exit_jump);

        // Patch break jumps
        let loop_ctx = self.loop_stack.pop().expect("loop stack underflow");
        for jump_idx in loop_ctx.break_jumps {
            self.chunk.patch_jump(jump_idx);
        }

        Ok(())
    }

    // --- Control flow: for ---

    pub(super) fn compile_for(
        &mut self,
        variable: &str,
        iterable: &Expr,
        body: &[Stmt],
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;
        self.begin_scope();

        match &iterable.kind {
            ExprKind::Range {
                start,
                end,
                inclusive,
            } => {
                self.compile_for_range(variable, start, end, *inclusive, body, span)?;
            }
            _ => {
                self.compile_for_array(variable, iterable, body, span)?;
            }
        }

        self.end_scope(line);
        Ok(())
    }

    pub(super) fn compile_for_range(
        &mut self,
        variable: &str,
        start: &Expr,
        end: &Expr,
        inclusive: bool,
        body: &[Stmt],
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;

        // Allocate hidden iterator and end locals
        let iter_slot = self.add_local("__iter", span)?;
        self.compile_expr(start, Some(iter_slot))?;

        let end_slot = self.add_local("__end", span)?;
        self.compile_expr(end, Some(end_slot))?;

        let loop_start = self.chunk.current_offset();

        // Condition: __iter < __end (or <= for inclusive)
        // Use fused TestLtInt/TestLeInt since both are int
        let iter_type = self.reg_type(iter_slot);
        let end_type = self.reg_type(end_slot);
        if iter_type == ExprType::Int && end_type == ExprType::Int {
            let exit_jump = if inclusive {
                self.chunk
                    .emit_jump(Instruction::TestLeInt(iter_slot, end_slot, 0), line)
            } else {
                self.chunk
                    .emit_jump(Instruction::TestLtInt(iter_slot, end_slot, 0), line)
            };

            // Bind user-visible loop variable
            let var_slot = self.add_local(variable, span)?;
            self.emit(Instruction::Move(var_slot, iter_slot), line);

            // Push loop context and compile body
            self.loop_stack.push(LoopContext {
                start_offset: loop_start,
                break_jumps: Vec::new(),
                scope_depth: self.scope_depth,
            });

            self.begin_scope();
            for stmt in body {
                self.compile_stmt(stmt)?;
            }
            self.end_scope(line);

            // Increment __iter
            self.emit(Instruction::AddIntImm(iter_slot, iter_slot, 1), line);

            // Backward jump
            let current = self.chunk.current_offset();
            let back_offset = (loop_start as i32) - (current as i32) - 1;
            self.emit(Instruction::Jump(back_offset), line);

            self.chunk.patch_jump(exit_jump);
        } else {
            // Fallback: generic comparison
            let cond_reg = self.alloc_temp(span)?;
            let cmp = if inclusive {
                Instruction::Le(cond_reg, iter_slot, end_slot)
            } else {
                Instruction::Lt(cond_reg, iter_slot, end_slot)
            };
            self.emit(cmp, line);
            let exit_jump = self
                .chunk
                .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
            self.free_temp(cond_reg);

            let var_slot = self.add_local(variable, span)?;
            self.emit(Instruction::Move(var_slot, iter_slot), line);

            self.loop_stack.push(LoopContext {
                start_offset: loop_start,
                break_jumps: Vec::new(),
                scope_depth: self.scope_depth,
            });

            self.begin_scope();
            for stmt in body {
                self.compile_stmt(stmt)?;
            }
            self.end_scope(line);

            // Increment __iter
            let one_reg = self.alloc_temp(span)?;
            self.emit(Instruction::LoadInt(one_reg, 1), line);
            self.emit(Instruction::Add(iter_slot, iter_slot, one_reg), line);
            self.free_temp(one_reg);

            let current = self.chunk.current_offset();
            let back_offset = (loop_start as i32) - (current as i32) - 1;
            self.emit(Instruction::Jump(back_offset), line);

            self.chunk.patch_jump(exit_jump);
        }

        // Patch break jumps
        let loop_ctx = self.loop_stack.pop().expect("loop stack underflow");
        for jump_idx in loop_ctx.break_jumps {
            self.chunk.patch_jump(jump_idx);
        }

        Ok(())
    }

    pub(super) fn compile_for_array(
        &mut self,
        variable: &str,
        iterable: &Expr,
        body: &[Stmt],
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;

        // Store the array in a hidden local
        let arr_slot = self.add_local("__arr", span)?;
        self.compile_expr(iterable, Some(arr_slot))?;

        // Get array length via GetField(hash("length"))
        let len_slot = self.add_local("__len", span)?;
        let len_hash = string_hash("length");
        self.emit(Instruction::GetField(len_slot, arr_slot, len_hash), line);

        // Counter starts at 0
        let idx_slot = self.add_local("__idx", span)?;
        self.emit(Instruction::LoadInt(idx_slot, 0), line);

        let loop_start = self.chunk.current_offset();

        // Condition: __idx < __len
        let cond_reg = self.alloc_temp(span)?;
        self.emit(Instruction::Lt(cond_reg, idx_slot, len_slot), line);
        let exit_jump = self
            .chunk
            .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
        self.free_temp(cond_reg);

        // Get element: __arr[__idx]
        let var_slot = self.add_local(variable, span)?;
        self.emit(Instruction::GetIndex(var_slot, arr_slot, idx_slot), line);

        // Push loop context and compile body
        self.loop_stack.push(LoopContext {
            start_offset: loop_start,
            break_jumps: Vec::new(),
            scope_depth: self.scope_depth,
        });

        self.begin_scope();
        for stmt in body {
            self.compile_stmt(stmt)?;
        }
        self.end_scope(line);

        // Increment __idx
        self.emit(Instruction::AddIntImm(idx_slot, idx_slot, 1), line);

        // Backward jump
        let current = self.chunk.current_offset();
        let back_offset = (loop_start as i32) - (current as i32) - 1;
        self.emit(Instruction::Jump(back_offset), line);

        self.chunk.patch_jump(exit_jump);

        // Patch break jumps
        let loop_ctx = self.loop_stack.pop().expect("loop stack underflow");
        for jump_idx in loop_ctx.break_jumps {
            self.chunk.patch_jump(jump_idx);
        }

        Ok(())
    }

    // --- Control flow: break/continue ---

    pub(super) fn compile_break(&mut self, span: &Span) -> Result<(), CompileError> {
        let line = span.line;
        let loop_depth = self
            .loop_stack
            .last()
            .ok_or_else(|| CompileError {
                annotation: None,
                message: "'break' outside of loop".to_string(),
                span: span.clone(),
            })?
            .scope_depth;

        // Close captured locals from inner scopes down to loop scope
        let close_ops: Vec<_> = self
            .locals
            .iter()
            .rev()
            .take_while(|l| l.depth > loop_depth)
            .filter(|l| l.is_captured)
            .map(|l| l.slot)
            .collect();
        for slot in close_ops {
            self.emit(Instruction::CloseUpvalue(slot), line);
        }

        let break_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
        self.loop_stack
            .last_mut()
            .expect("loop stack")
            .break_jumps
            .push(break_jump);
        Ok(())
    }

    pub(super) fn compile_continue(&mut self, span: &Span) -> Result<(), CompileError> {
        let line = span.line;
        let (loop_start, loop_depth) = {
            let ctx = self.loop_stack.last().ok_or_else(|| CompileError {
                annotation: None,
                message: "'continue' outside of loop".to_string(),
                span: span.clone(),
            })?;
            (ctx.start_offset, ctx.scope_depth)
        };

        // Close captured locals from inner scopes down to loop scope
        let close_ops: Vec<_> = self
            .locals
            .iter()
            .rev()
            .take_while(|l| l.depth > loop_depth)
            .filter(|l| l.is_captured)
            .map(|l| l.slot)
            .collect();
        for slot in close_ops {
            self.emit(Instruction::CloseUpvalue(slot), line);
        }

        let current = self.chunk.current_offset();
        let back_offset = (loop_start as i32) - (current as i32) - 1;
        self.emit(Instruction::Jump(back_offset), line);
        Ok(())
    }

    // --- When statement ---

    pub(super) fn compile_when(
        &mut self,
        subject: &Option<Expr>,
        arms: &[crate::parser::WhenArm],
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;
        self.begin_scope();

        // If there's a subject, store it in a hidden local
        let subject_slot = if let Some(subj) = subject {
            let slot = self.add_local("__subject", span)?;
            self.compile_expr(subj, Some(slot))?;
            Some(slot)
        } else {
            None
        };

        let mut end_jumps: Vec<usize> = Vec::new();

        for arm in arms {
            match &arm.pattern {
                WhenPattern::Else => {
                    self.compile_when_body(&arm.body, line)?;
                }
                WhenPattern::Value(expr) => {
                    let cond_reg = self.alloc_temp(span)?;
                    if let Some(slot) = subject_slot {
                        let val_reg = self.compile_expr(expr, None)?;
                        self.emit(Instruction::Eq(cond_reg, slot, val_reg), line);
                        self.maybe_free_temp(val_reg, cond_reg);
                    } else {
                        self.compile_expr(expr, Some(cond_reg))?;
                    }
                    let next_arm = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                    self.free_temp(cond_reg);

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::MultipleValues(values) => {
                    let slot = subject_slot.expect("multiple values require subject");
                    let cond_reg = self.alloc_temp(span)?;
                    let mut body_jumps: Vec<usize> = Vec::new();

                    for (i, val) in values.iter().enumerate() {
                        let val_reg = self.compile_expr(val, None)?;
                        self.emit(Instruction::Eq(cond_reg, slot, val_reg), line);
                        self.maybe_free_temp(val_reg, cond_reg);
                        if i < values.len() - 1 {
                            let body_jump = self
                                .chunk
                                .emit_jump(Instruction::JumpIfTruthy(cond_reg, 0), line);
                            body_jumps.push(body_jump);
                        }
                    }
                    // Last comparison: if false, skip to next arm
                    let next_arm = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                    self.free_temp(cond_reg);

                    // Patch body jumps to here
                    for jump in &body_jumps {
                        self.chunk.patch_jump(*jump);
                    }

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::Range {
                    start,
                    end,
                    inclusive,
                } => {
                    let slot = subject_slot.expect("range pattern requires subject");
                    let cond1 = self.alloc_temp(span)?;
                    // subject >= start
                    let start_reg = self.compile_expr(start, None)?;
                    self.emit(Instruction::Ge(cond1, slot, start_reg), line);
                    self.maybe_free_temp(start_reg, cond1);
                    let fail_jump1 = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond1, 0), line);

                    // subject < end (or <= for inclusive)
                    let end_reg = self.compile_expr(end, None)?;
                    let cmp = if *inclusive {
                        Instruction::Le(cond1, slot, end_reg)
                    } else {
                        Instruction::Lt(cond1, slot, end_reg)
                    };
                    self.emit(cmp, line);
                    self.maybe_free_temp(end_reg, cond1);
                    let fail_jump2 = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond1, 0), line);
                    self.free_temp(cond1);

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    // Patch both failure paths
                    self.chunk.patch_jump(fail_jump1);
                    self.chunk.patch_jump(fail_jump2);
                }
                WhenPattern::Guard {
                    binding: _,
                    condition,
                } => {
                    let cond_reg = self.compile_expr(condition, None)?;
                    let next_arm = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                    self.maybe_free_temp(cond_reg, 0);

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::TypeMatch { .. } => {
                    return Err(CompileError {
                        annotation: None,
                        message: "type match patterns not yet supported in compiler".to_string(),
                        span: span.clone(),
                    });
                }
            }
        }

        // Patch all end jumps
        for jump in end_jumps {
            self.chunk.patch_jump(jump);
        }

        self.end_scope(line);
        Ok(())
    }

    pub(super) fn compile_when_body(
        &mut self,
        body: &WhenBody,
        line: u32,
    ) -> Result<(), CompileError> {
        match body {
            WhenBody::Expr(expr) => {
                // Compile expression, discard result
                let reg = self.compile_expr(expr, None)?;
                self.maybe_free_temp(reg, 0);
            }
            WhenBody::Block(stmts) => {
                self.compile_block(stmts, line)?;
            }
        }
        Ok(())
    }

    /// Compiles a `when` expression -- each arm's body leaves a value in a register.
    pub(super) fn compile_when_expr(
        &mut self,
        subject: Option<&Expr>,
        arms: &[crate::parser::WhenArm],
        span: &Span,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let line = span.line;
        let d = self.dst_or_temp(dst, span)?;
        self.begin_scope();

        let subject_slot = if let Some(subj) = subject {
            let slot = self.add_local("__subject", span)?;
            self.compile_expr(subj, Some(slot))?;
            Some(slot)
        } else {
            None
        };

        let mut end_jumps: Vec<usize> = Vec::new();

        for arm in arms {
            match &arm.pattern {
                WhenPattern::Else => {
                    self.compile_when_expr_body(&arm.body, line, d)?;
                }
                WhenPattern::Value(expr) => {
                    let cond_reg = self.alloc_temp(span)?;
                    if let Some(slot) = subject_slot {
                        let val_reg = self.compile_expr(expr, None)?;
                        self.emit(Instruction::Eq(cond_reg, slot, val_reg), line);
                        self.maybe_free_temp(val_reg, cond_reg);
                    } else {
                        self.compile_expr(expr, Some(cond_reg))?;
                    }
                    let next_arm = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                    self.free_temp(cond_reg);

                    self.compile_when_expr_body(&arm.body, line, d)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::MultipleValues(values) => {
                    let slot = subject_slot.expect("multiple values require subject");
                    let cond_reg = self.alloc_temp(span)?;
                    let mut body_jumps: Vec<usize> = Vec::new();

                    for (i, val) in values.iter().enumerate() {
                        let val_reg = self.compile_expr(val, None)?;
                        self.emit(Instruction::Eq(cond_reg, slot, val_reg), line);
                        self.maybe_free_temp(val_reg, cond_reg);
                        if i < values.len() - 1 {
                            let body_jump = self
                                .chunk
                                .emit_jump(Instruction::JumpIfTruthy(cond_reg, 0), line);
                            body_jumps.push(body_jump);
                        }
                    }
                    let next_arm = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                    self.free_temp(cond_reg);

                    for jump in &body_jumps {
                        self.chunk.patch_jump(*jump);
                    }

                    self.compile_when_expr_body(&arm.body, line, d)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::Range {
                    start,
                    end,
                    inclusive,
                } => {
                    let slot = subject_slot.expect("range pattern requires subject");
                    let cond1 = self.alloc_temp(span)?;
                    let start_reg = self.compile_expr(start, None)?;
                    self.emit(Instruction::Ge(cond1, slot, start_reg), line);
                    self.maybe_free_temp(start_reg, cond1);
                    let fail_jump1 = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond1, 0), line);

                    let end_reg = self.compile_expr(end, None)?;
                    let cmp = if *inclusive {
                        Instruction::Le(cond1, slot, end_reg)
                    } else {
                        Instruction::Lt(cond1, slot, end_reg)
                    };
                    self.emit(cmp, line);
                    self.maybe_free_temp(end_reg, cond1);
                    let fail_jump2 = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond1, 0), line);
                    self.free_temp(cond1);

                    self.compile_when_expr_body(&arm.body, line, d)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(fail_jump1);
                    self.chunk.patch_jump(fail_jump2);
                }
                WhenPattern::Guard {
                    binding: _,
                    condition,
                } => {
                    let cond_reg = self.compile_expr(condition, None)?;
                    let next_arm = self
                        .chunk
                        .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                    self.maybe_free_temp(cond_reg, 0);

                    self.compile_when_expr_body(&arm.body, line, d)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::TypeMatch { .. } => {
                    return Err(CompileError {
                        annotation: None,
                        message: "type match patterns not yet supported in compiler".to_string(),
                        span: span.clone(),
                    });
                }
            }
        }

        // If no arm matched, load null as the default value
        self.emit(Instruction::LoadNull(d), line);

        for jump in end_jumps {
            self.chunk.patch_jump(jump);
        }

        self.end_scope(line);
        Ok(d)
    }

    /// Compiles a when-expression arm body, placing the result into `dst`.
    pub(super) fn compile_when_expr_body(
        &mut self,
        body: &WhenBody,
        line: u32,
        dst: u8,
    ) -> Result<(), CompileError> {
        match body {
            WhenBody::Expr(expr) => {
                self.compile_expr(expr, Some(dst))?;
            }
            WhenBody::Block(stmts) => {
                if stmts.is_empty() {
                    self.emit(Instruction::LoadNull(dst), line);
                } else {
                    for (i, stmt) in stmts.iter().enumerate() {
                        if i == stmts.len() - 1 {
                            if let StmtKind::ExprStmt(expr) = &stmt.kind {
                                self.compile_expr(expr, Some(dst))?;
                            } else {
                                self.compile_stmt(stmt)?;
                                self.emit(Instruction::LoadNull(dst), line);
                            }
                        } else {
                            self.compile_stmt(stmt)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // --- Function compilation ---
}
