use super::*;

impl Compiler {
    pub fn compile_expr(&mut self, expr: &Expr, dst: Option<u8>) -> Result<u8, CompileError> {
        let line = expr.span.line;
        match &expr.kind {
            ExprKind::Literal(lit) => self.compile_literal(lit, &expr.span, dst),
            ExprKind::Identifier(name) => {
                if let Some((slot, type_tag)) = self.resolve_local(name) {
                    // The value is already in its register. If dst is different, Move it.
                    let ty = tag_to_expr_type(type_tag);
                    match dst {
                        Some(d) if d != slot => {
                            self.emit(Instruction::Move(d, slot), line);
                            self.set_reg_type(d, ty);
                            Ok(d)
                        }
                        Some(d) => {
                            self.set_reg_type(d, ty);
                            Ok(d)
                        }
                        None => {
                            // Return the local's register directly -- no instruction needed
                            Ok(slot)
                        }
                    }
                } else if let Some(uv) = self.resolve_upvalue(name) {
                    let d = self.dst_or_temp(dst, &expr.span)?;
                    self.emit(Instruction::LoadUpvalue(d, uv), line);
                    self.set_reg_type(d, ExprType::Other);
                    Ok(d)
                } else {
                    // Not a local or upvalue -- try global / function name.
                    let d = self.dst_or_temp(dst, &expr.span)?;
                    let hash = string_hash(name);
                    self.chunk.add_string(name);
                    self.emit(Instruction::LoadGlobal(d, hash), line);
                    self.set_reg_type(d, ExprType::Other);
                    Ok(d)
                }
            }
            ExprKind::Binary { op, lhs, rhs } => self.compile_binary(op, lhs, rhs, dst, line),
            ExprKind::Unary { op, operand } => {
                // Try folding unary on literals
                if let ExprKind::Literal(lit) = &operand.kind {
                    let folded = match (op, lit) {
                        (UnaryOp::Negate, Literal::Int(v)) => Some(Literal::Int(-v)),
                        (UnaryOp::Negate, Literal::Float(v)) => Some(Literal::Float(-v)),
                        (UnaryOp::Not, Literal::Bool(v)) => Some(Literal::Bool(!v)),
                        _ => None,
                    };
                    if let Some(folded) = folded {
                        return self.compile_literal(&folded, &expr.span, dst);
                    }
                }
                let src = self.compile_expr(operand, None)?;
                let src_type = self.reg_type(src);
                let d = self.dst_or_temp(dst, &expr.span)?;
                let instr = match op {
                    UnaryOp::Negate => Instruction::Neg(d, src),
                    UnaryOp::Not => Instruction::Not(d, src),
                };
                self.emit(instr, line);
                let result_type = match op {
                    UnaryOp::Negate => src_type,
                    UnaryOp::Not => ExprType::Bool,
                };
                self.set_reg_type(d, result_type);
                // Free src if it was a temp and not the same as dst
                self.maybe_free_temp(src, d);
                Ok(d)
            }
            ExprKind::Grouped(inner) => self.compile_expr(inner, dst),
            ExprKind::Call { callee, args } => self.compile_call(callee, args, &expr.span, dst),
            ExprKind::MemberAccess { object, member } => {
                let obj_reg = self.compile_expr(object, None)?;
                let d = self.dst_or_temp(dst, &expr.span)?;
                let hash = string_hash(member);
                self.chunk.add_string(member);
                self.emit(Instruction::GetField(d, obj_reg, hash), line);
                self.set_reg_type(d, ExprType::Other);
                self.maybe_free_temp(obj_reg, d);
                Ok(d)
            }
            ExprKind::SafeAccess { object, member } => {
                let obj_reg = self.compile_expr(object, None)?;
                let d = self.dst_or_temp(dst, &expr.span)?;
                let hash = string_hash(member);
                self.chunk.add_string(member);
                self.emit(Instruction::GetField(d, obj_reg, hash), line);
                self.set_reg_type(d, ExprType::Other);
                self.maybe_free_temp(obj_reg, d);
                Ok(d)
            }
            ExprKind::ArrayLiteral(elements) => self.compile_array_literal(elements, line, dst),
            ExprKind::DictLiteral(elements) => self.compile_dict_literal(elements, line, dst),
            ExprKind::StringInterpolation(segments) => {
                self.compile_string_interpolation(segments, line, dst)
            }
            ExprKind::NullCoalesce { lhs, rhs } => {
                let a = self.compile_expr(lhs, None)?;
                let b = self.compile_expr(rhs, None)?;
                let d = self.dst_or_temp(dst, &expr.span)?;
                self.emit(Instruction::NullCoalesce(d, a, b), line);
                self.set_reg_type(d, ExprType::Other);
                self.maybe_free_temp(b, d);
                self.maybe_free_temp(a, d);
                Ok(d)
            }
            ExprKind::Lambda { params, body } => {
                let d = self.dst_or_temp(dst, &expr.span)?;
                self.compile_lambda(params, body, &expr.span, d)?;
                self.set_reg_type(d, ExprType::Other);
                Ok(d)
            }
            ExprKind::Tuple(elements) => {
                // Compile tuple as array
                let start = self.next_reg;
                for elem in elements {
                    let r = self.alloc_temp(&expr.span)?;
                    self.compile_expr(elem, Some(r))?;
                }
                let count = elements.len() as u8;
                let d = self.dst_or_temp(dst, &expr.span)?;
                self.emit(Instruction::MakeArray(d, start, count), line);
                // Free all element temps
                self.next_reg = start;
                self.set_reg_type(d, ExprType::Other);
                if dst.is_none() {
                    // d was allocated before start was reclaimed; fix up
                    // Actually, d was from dst_or_temp which may have been before or after.
                    // Let's handle this more carefully.
                }
                Ok(d)
            }
            ExprKind::Range { start, end, .. } => {
                // `..` between strings compiles to Concat instruction
                let a = self.compile_expr(start, None)?;
                let b = self.compile_expr(end, None)?;
                let d = self.dst_or_temp(dst, &expr.span)?;
                self.emit(Instruction::Concat(d, a, b), line);
                self.set_reg_type(d, ExprType::Other);
                self.maybe_free_temp(b, d);
                self.maybe_free_temp(a, d);
                Ok(d)
            }
            ExprKind::When { subject, arms } => {
                self.compile_when_expr(subject.as_deref(), arms, &expr.span, dst)
            }
            ExprKind::Yield(arg) => {
                let d = self.dst_or_temp(dst, &expr.span)?;
                self.compile_yield(arg.as_deref(), &expr.span, d)?;
                self.set_reg_type(d, ExprType::Other);
                Ok(d)
            }
            ExprKind::Index { object, index } => {
                let obj_reg = self.compile_expr(object, None)?;
                let idx_reg = self.compile_expr(index, None)?;
                let d = self.dst_or_temp(dst, &expr.span)?;
                self.emit(Instruction::GetIndex(d, obj_reg, idx_reg), line);
                self.set_reg_type(d, ExprType::Other);
                self.maybe_free_temp(idx_reg, d);
                self.maybe_free_temp(obj_reg, d);
                Ok(d)
            }
            ExprKind::Cast { expr, .. } => self.compile_expr(expr, dst),
            ExprKind::Super { method, args } => self.compile_super(method, args, &expr.span, dst),
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                let d = self.dst_or_temp(dst, &expr.span)?;
                let cond_reg = self.compile_expr(condition, None)?;
                let else_jump = self
                    .chunk
                    .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                self.maybe_free_temp(cond_reg, d);
                self.compile_expr(then_expr, Some(d))?;
                let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                self.chunk.patch_jump(else_jump);
                self.compile_expr(else_expr, Some(d))?;
                self.chunk.patch_jump(end_jump);
                self.set_reg_type(d, ExprType::Other);
                Ok(d)
            }
            other => Err(CompileError {
                annotation: None,
                message: format!("unsupported expression: {other:?}"),
                span: expr.span.clone(),
            }),
        }
    }

    /// Free `reg` if it's a temporary (i.e., >= next_reg-1 and != keep).
    pub(super) fn maybe_free_temp(&mut self, reg: u8, keep: u8) {
        if reg != keep && reg == self.next_reg - 1 {
            // Don't free a register that belongs to a named local variable.
            if self.locals.iter().any(|l| l.slot == reg) {
                return;
            }
            self.free_temp(reg);
        }
    }

    // --- Binary expression compilation ---

    pub(super) fn compile_binary(
        &mut self,
        op: &BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        dst: Option<u8>,
        line: u32,
    ) -> Result<u8, CompileError> {
        match op {
            BinaryOp::And => {
                // Short-circuit: compile lhs into a register, jump if false
                let cond_reg = self.compile_expr(lhs, dst)?;
                let end_jump = self
                    .chunk
                    .emit_jump(Instruction::JumpIfFalsy(cond_reg, 0), line);
                // lhs was truthy -- evaluate rhs into the same register
                let rhs_reg = self.compile_expr(rhs, Some(cond_reg))?;
                let _ = rhs_reg;
                self.chunk.patch_jump(end_jump);
                self.set_reg_type(cond_reg, ExprType::Bool);
                Ok(cond_reg)
            }
            BinaryOp::Or => {
                // Short-circuit: compile lhs into a register, jump if true
                let cond_reg = self.compile_expr(lhs, dst)?;
                let end_jump = self
                    .chunk
                    .emit_jump(Instruction::JumpIfTruthy(cond_reg, 0), line);
                // lhs was falsy -- evaluate rhs into the same register
                let rhs_reg = self.compile_expr(rhs, Some(cond_reg))?;
                let _ = rhs_reg;
                self.chunk.patch_jump(end_jump);
                self.set_reg_type(cond_reg, ExprType::Bool);
                Ok(cond_reg)
            }
            _ => {
                // Try constant folding first
                if let Some(folded) = Self::try_fold_binary(op, lhs, rhs) {
                    return self.compile_literal(&folded, &lhs.span, dst);
                }
                let a = self.compile_expr(lhs, None)?;
                let b = self.compile_expr(rhs, None)?;
                let a_type = self.reg_type(a);
                let b_type = self.reg_type(b);

                // Mixed int/float -> coerce int operand to float
                let (a, a_type, b, b_type, coerced_temp) = match (a_type, b_type) {
                    (ExprType::Int, ExprType::Float) => {
                        let coerced = self.alloc_temp(&lhs.span)?;
                        self.emit(Instruction::IntToFloat(coerced, a), line);
                        (coerced, ExprType::Float, b, ExprType::Float, Some(coerced))
                    }
                    (ExprType::Float, ExprType::Int) => {
                        let coerced = self.alloc_temp(&rhs.span)?;
                        self.emit(Instruction::IntToFloat(coerced, b), line);
                        (a, ExprType::Float, coerced, ExprType::Float, Some(coerced))
                    }
                    _ => (a, a_type, b, b_type, None),
                };

                let d = self.dst_or_temp(dst, &lhs.span)?;
                let (instr, result_type) =
                    Self::typed_binary_instruction(op, a_type, b_type, d, a, b);
                self.emit(instr, line);
                self.set_reg_type(d, result_type);
                if let Some(t) = coerced_temp {
                    self.maybe_free_temp(t, d);
                }
                self.maybe_free_temp(b, d);
                self.maybe_free_temp(a, d);
                Ok(d)
            }
        }
    }

    /// Attempts to fold a binary expression on two literals at compile time.
    pub(super) fn try_fold_binary(op: &BinaryOp, lhs: &Expr, rhs: &Expr) -> Option<Literal> {
        let l = match &lhs.kind {
            ExprKind::Literal(lit) => lit,
            _ => return None,
        };
        let r = match &rhs.kind {
            ExprKind::Literal(lit) => lit,
            _ => return None,
        };
        match (l, r) {
            (Literal::Int(a), Literal::Int(b)) => Self::fold_int_op(op, *a, *b),
            (Literal::Float(a), Literal::Float(b)) => Self::fold_float_op(op, *a, *b),
            _ => None,
        }
    }

    pub(super) fn fold_int_op(op: &BinaryOp, a: i64, b: i64) -> Option<Literal> {
        match op {
            BinaryOp::Add => a.checked_add(b).map(Literal::Int),
            BinaryOp::Subtract => a.checked_sub(b).map(Literal::Int),
            BinaryOp::Multiply => a.checked_mul(b).map(Literal::Int),
            BinaryOp::Divide => {
                if b != 0 {
                    a.checked_div(b).map(Literal::Int)
                } else {
                    None
                }
            }
            BinaryOp::Modulo => {
                if b != 0 {
                    a.checked_rem(b).map(Literal::Int)
                } else {
                    None
                }
            }
            BinaryOp::Less => Some(Literal::Bool(a < b)),
            BinaryOp::LessEqual => Some(Literal::Bool(a <= b)),
            BinaryOp::Greater => Some(Literal::Bool(a > b)),
            BinaryOp::GreaterEqual => Some(Literal::Bool(a >= b)),
            BinaryOp::Equal => Some(Literal::Bool(a == b)),
            BinaryOp::NotEqual => Some(Literal::Bool(a != b)),
            _ => None,
        }
    }

    pub(super) fn fold_float_op(op: &BinaryOp, a: f64, b: f64) -> Option<Literal> {
        match op {
            BinaryOp::Add => Some(Literal::Float(a + b)),
            BinaryOp::Subtract => Some(Literal::Float(a - b)),
            BinaryOp::Multiply => Some(Literal::Float(a * b)),
            BinaryOp::Divide => {
                if b != 0.0 {
                    Some(Literal::Float(a / b))
                } else {
                    None
                }
            }
            BinaryOp::Modulo => {
                if b != 0.0 {
                    Some(Literal::Float(a % b))
                } else {
                    None
                }
            }
            BinaryOp::Less => Some(Literal::Bool(a < b)),
            BinaryOp::LessEqual => Some(Literal::Bool(a <= b)),
            BinaryOp::Greater => Some(Literal::Bool(a > b)),
            BinaryOp::GreaterEqual => Some(Literal::Bool(a >= b)),
            BinaryOp::Equal => Some(Literal::Bool(a == b)),
            BinaryOp::NotEqual => Some(Literal::Bool(a != b)),
            _ => None,
        }
    }

    pub(super) fn typed_binary_instruction(
        op: &BinaryOp,
        lhs_type: ExprType,
        rhs_type: ExprType,
        dst: u8,
        a: u8,
        b: u8,
    ) -> (Instruction, ExprType) {
        match (lhs_type, rhs_type) {
            (ExprType::Int, ExprType::Int) => match op {
                BinaryOp::Add => (Instruction::AddInt(dst, a, b), ExprType::Int),
                BinaryOp::Subtract => (Instruction::SubInt(dst, a, b), ExprType::Int),
                BinaryOp::Multiply => (Instruction::MulInt(dst, a, b), ExprType::Int),
                BinaryOp::Divide => (Instruction::DivInt(dst, a, b), ExprType::Int),
                BinaryOp::Less => (Instruction::LtInt(dst, a, b), ExprType::Bool),
                BinaryOp::LessEqual => (Instruction::LeInt(dst, a, b), ExprType::Bool),
                BinaryOp::Greater => (Instruction::GtInt(dst, a, b), ExprType::Bool),
                BinaryOp::GreaterEqual => (Instruction::GeInt(dst, a, b), ExprType::Bool),
                BinaryOp::Equal => (Instruction::EqInt(dst, a, b), ExprType::Bool),
                BinaryOp::NotEqual => (Instruction::NeInt(dst, a, b), ExprType::Bool),
                BinaryOp::Modulo => (Instruction::Mod(dst, a, b), ExprType::Int),
                _ => (
                    Self::generic_binary_instruction(op, dst, a, b),
                    ExprType::Other,
                ),
            },
            (ExprType::Float, ExprType::Float) => match op {
                BinaryOp::Add => (Instruction::AddFloat(dst, a, b), ExprType::Float),
                BinaryOp::Subtract => (Instruction::SubFloat(dst, a, b), ExprType::Float),
                BinaryOp::Multiply => (Instruction::MulFloat(dst, a, b), ExprType::Float),
                BinaryOp::Divide => (Instruction::DivFloat(dst, a, b), ExprType::Float),
                BinaryOp::Less => (Instruction::LtFloat(dst, a, b), ExprType::Bool),
                BinaryOp::LessEqual => (Instruction::LeFloat(dst, a, b), ExprType::Bool),
                BinaryOp::Greater => (Instruction::GtFloat(dst, a, b), ExprType::Bool),
                BinaryOp::GreaterEqual => (Instruction::GeFloat(dst, a, b), ExprType::Bool),
                BinaryOp::Equal => (Instruction::EqFloat(dst, a, b), ExprType::Bool),
                BinaryOp::NotEqual => (Instruction::NeFloat(dst, a, b), ExprType::Bool),
                BinaryOp::Modulo => (Instruction::Mod(dst, a, b), ExprType::Float),
                _ => (
                    Self::generic_binary_instruction(op, dst, a, b),
                    ExprType::Other,
                ),
            },
            _ => {
                let result_type = match op {
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::Equal
                    | BinaryOp::NotEqual => ExprType::Bool,
                    _ => ExprType::Other,
                };
                (Self::generic_binary_instruction(op, dst, a, b), result_type)
            }
        }
    }

    pub(super) fn generic_binary_instruction(op: &BinaryOp, dst: u8, a: u8, b: u8) -> Instruction {
        match op {
            BinaryOp::Add => Instruction::Add(dst, a, b),
            BinaryOp::Subtract => Instruction::Sub(dst, a, b),
            BinaryOp::Multiply => Instruction::Mul(dst, a, b),
            BinaryOp::Divide => Instruction::Div(dst, a, b),
            BinaryOp::Modulo => Instruction::Mod(dst, a, b),
            BinaryOp::Equal => Instruction::Eq(dst, a, b),
            BinaryOp::NotEqual => Instruction::Ne(dst, a, b),
            BinaryOp::Less => Instruction::Lt(dst, a, b),
            BinaryOp::Greater => Instruction::Gt(dst, a, b),
            BinaryOp::LessEqual => Instruction::Le(dst, a, b),
            BinaryOp::GreaterEqual => Instruction::Ge(dst, a, b),
            BinaryOp::And | BinaryOp::Or => unreachable!("handled by compile_binary"),
        }
    }

    // --- Literal compilation ---

    pub(super) fn compile_literal(
        &mut self,
        lit: &Literal,
        span: &Span,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let line = span.line;
        let d = self.dst_or_temp(dst, span)?;
        match lit {
            Literal::Int(v) => {
                if let Ok(narrowed) = i32::try_from(*v) {
                    self.emit(Instruction::LoadInt(d, narrowed), line);
                } else {
                    let idx = self.chunk.add_int64(*v);
                    self.emit(Instruction::LoadConstInt(d, idx), line);
                }
                self.set_reg_type(d, ExprType::Int);
            }
            Literal::Float(v) => {
                let narrowed = *v as f32;
                if narrowed.is_infinite() && !v.is_infinite() {
                    let idx = self.chunk.add_float64(*v);
                    self.emit(Instruction::LoadConstFloat(d, idx), line);
                } else {
                    self.emit(Instruction::LoadFloat(d, narrowed), line);
                }
                self.set_reg_type(d, ExprType::Float);
            }
            Literal::String(s) => {
                let index = self.chunk.add_string(s);
                self.emit(Instruction::LoadStr(d, index), line);
                self.set_reg_type(d, ExprType::Other);
            }
            Literal::Bool(b) => {
                self.emit(Instruction::LoadBool(d, *b), line);
                self.set_reg_type(d, ExprType::Bool);
            }
            Literal::Null => {
                self.emit(Instruction::LoadNull(d), line);
                self.set_reg_type(d, ExprType::Other);
            }
        }
        Ok(d)
    }

    // --- Assignment compilation ---

    pub(super) fn compile_assignment(
        &mut self,
        target: &Expr,
        op: &AssignOp,
        value: &Expr,
        stmt_span: &Span,
    ) -> Result<(), CompileError> {
        let line = stmt_span.line;

        // Handle field assignment: obj.field = value
        if let ExprKind::MemberAccess { object, member } = &target.kind {
            let local_slot = if let ExprKind::Identifier(name) = &object.kind {
                self.resolve_local(name).map(|(slot, _)| slot)
            } else {
                None
            };

            let obj_reg = self.compile_expr(object, None)?;
            let val_reg = self.compile_expr(value, None)?;
            let hash = string_hash(member);
            self.chunk.add_string(member);
            self.emit(Instruction::SetField(obj_reg, hash, val_reg), line);

            // For struct value types, SetField may produce a modified copy.
            // The VM handles writing back to the register directly.
            if let Some(slot) = local_slot
                && slot != obj_reg
            {
                self.emit(Instruction::Move(slot, obj_reg), line);
            }
            self.maybe_free_temp(val_reg, obj_reg);
            self.maybe_free_temp(obj_reg, 0);
            return Ok(());
        }

        // Handle index assignment: collection[index] = value
        if let ExprKind::Index { object, index } = &target.kind {
            let obj_reg = self.compile_expr(object, None)?;
            let idx_reg = self.compile_expr(index, None)?;
            let val_reg = self.compile_expr(value, None)?;
            self.emit(Instruction::SetIndex(obj_reg, idx_reg, val_reg), line);
            self.maybe_free_temp(val_reg, 0);
            self.maybe_free_temp(idx_reg, 0);
            self.maybe_free_temp(obj_reg, 0);
            return Ok(());
        }

        let name = match &target.kind {
            ExprKind::Identifier(name) => name,
            _ => {
                return Err(CompileError {
                    annotation: None,
                    message: "invalid assignment target".to_string(),
                    span: target.span.clone(),
                });
            }
        };

        let local_slot = self.resolve_local(name).map(|(slot, _)| slot);
        let upvalue_idx = if local_slot.is_none() {
            self.resolve_upvalue(name)
        } else {
            None
        };

        if local_slot.is_none() && upvalue_idx.is_none() {
            return Err(CompileError {
                annotation: None,
                message: format!("undefined variable '{name}'"),
                span: target.span.clone(),
            });
        }

        match op {
            AssignOp::Assign => {
                if let Some(slot) = local_slot {
                    self.compile_expr(value, Some(slot))?;
                } else if let Some(uv) = upvalue_idx {
                    let val = self.compile_expr(value, None)?;
                    self.emit(Instruction::StoreUpvalue(val, uv), line);
                    self.maybe_free_temp(val, 0);
                }
            }
            compound => {
                // Fast path: local += int_literal -> AddIntImm
                if let Some(slot) = local_slot
                    && let AssignOp::AddAssign = compound
                    && let ExprKind::Literal(Literal::Int(v)) = &value.kind
                    && let Ok(imm) = i32::try_from(*v)
                {
                    self.emit(Instruction::AddIntImm(slot, slot, imm), line);
                    return Ok(());
                }
                // Fast path: local -= int_literal -> SubIntImm (or AddIntImm with neg)
                if let Some(slot) = local_slot
                    && let AssignOp::SubAssign = compound
                    && let ExprKind::Literal(Literal::Int(v)) = &value.kind
                    && let Ok(imm) = i32::try_from(*v)
                    && let Some(neg) = imm.checked_neg()
                {
                    self.emit(Instruction::AddIntImm(slot, slot, neg), line);
                    return Ok(());
                }

                if let Some(slot) = local_slot {
                    let val = self.compile_expr(value, None)?;
                    let val_type = self.reg_type(val);
                    let slot_type = self.reg_type(slot);
                    let (instr, _result_type) = Self::typed_binary_instruction(
                        &Self::compound_to_binary(compound),
                        slot_type,
                        val_type,
                        slot,
                        slot,
                        val,
                    );
                    self.emit(instr, line);
                    self.maybe_free_temp(val, slot);
                } else if let Some(uv) = upvalue_idx {
                    // Load upvalue into temp, operate, store back
                    let temp = self.alloc_temp(stmt_span)?;
                    self.emit(Instruction::LoadUpvalue(temp, uv), line);
                    let val = self.compile_expr(value, None)?;
                    let (instr, _) = Self::typed_binary_instruction(
                        &Self::compound_to_binary(compound),
                        ExprType::Other,
                        self.reg_type(val),
                        temp,
                        temp,
                        val,
                    );
                    self.emit(instr, line);
                    self.emit(Instruction::StoreUpvalue(temp, uv), line);
                    self.maybe_free_temp(val, temp);
                    self.free_temp(temp);
                }
            }
        }
        Ok(())
    }

    pub(super) fn compound_to_binary(op: &AssignOp) -> BinaryOp {
        match op {
            AssignOp::AddAssign => BinaryOp::Add,
            AssignOp::SubAssign => BinaryOp::Subtract,
            AssignOp::MulAssign => BinaryOp::Multiply,
            AssignOp::DivAssign => BinaryOp::Divide,
            AssignOp::ModAssign => BinaryOp::Modulo,
            AssignOp::Assign => unreachable!("simple assign handled separately"),
        }
    }

    // --- Control flow: if/else ---

    pub(super) fn compile_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: &Span,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let line = span.line;

        // Method call: receiver.method(args) -> CallMethod
        if let ExprKind::MemberAccess { object, member } = &callee.kind {
            // Allocate consecutive registers: [receiver, arg0, arg1, ...]
            let base = self.next_reg;
            let recv_reg = self.alloc_temp(span)?;
            self.compile_expr(object, Some(recv_reg))?;

            for arg in args {
                let arg_reg = self.alloc_temp(span)?;
                self.compile_call_arg(arg, Some(arg_reg))?;
            }

            let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many arguments (max 255)".to_string(),
                span: span.clone(),
            })?;

            let hash = string_hash(member);
            self.chunk.add_string(member);
            self.emit(Instruction::CallMethod(base, hash, arity), line);

            // Free all temps used for args, result is in base
            self.next_reg = base + 1;
            self.set_reg_type(base, ExprType::Other);

            // Move result to dst if a specific destination was requested
            if let Some(d) = dst {
                if d != base {
                    self.emit(Instruction::Move(d, base), line);
                    self.next_reg = base;
                }
                return Ok(d);
            }
            return Ok(base);
        }

        // Direct call optimization
        if let ExprKind::Identifier(name) = &callee.kind {
            let is_local = self.resolve_local(name).is_some();
            let is_upvalue = !is_local && self.resolve_upvalue(name).is_some();
            if !is_local
                && !is_upvalue
                && let Some(&func_idx) = self.function_index.get(name.as_str())
            {
                let ret_type = self
                    .function_return_types
                    .get(name.as_str())
                    .copied()
                    .unwrap_or(ExprType::Other);

                // Allocate consecutive registers for args: [arg0, arg1, ...]
                let base = self.next_reg;
                for arg in args {
                    let arg_reg = self.alloc_temp(span)?;
                    self.compile_call_arg(arg, Some(arg_reg))?;
                }

                let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                    annotation: None,
                    message: "too many arguments (max 255)".to_string(),
                    span: span.clone(),
                })?;

                self.emit(Instruction::CallDirect(base, func_idx, arity), line);

                // Free all arg temps, result is in base
                self.next_reg = base + 1;
                self.set_reg_type(base, ret_type);

                // Move result to dst if a specific destination was requested
                if let Some(d) = dst {
                    if d != base {
                        self.emit(Instruction::Move(d, base), line);
                        self.next_reg = base; // release the call's base register
                    }
                    return Ok(d);
                }
                return Ok(base);
            }
        }

        // Special coroutine suspension functions
        if let ExprKind::Identifier(name) = &callee.kind {
            match name.as_str() {
                "waitForSeconds" => {
                    if args.len() != 1 {
                        return Err(CompileError {
                            annotation: None,
                            message: "waitForSeconds expects 1 argument".to_string(),
                            span: span.clone(),
                        });
                    }
                    self.has_yield = true;
                    let r = self.compile_call_arg_to_reg(&args[0], span)?;
                    let dst_reg = dst.unwrap_or(r);
                    self.emit(Instruction::YieldSeconds(r), line);
                    self.maybe_free_temp(r, dst_reg);
                    return Ok(dst_reg);
                }
                "waitForFrames" => {
                    if args.len() != 1 {
                        return Err(CompileError {
                            annotation: None,
                            message: "waitForFrames expects 1 argument".to_string(),
                            span: span.clone(),
                        });
                    }
                    self.has_yield = true;
                    let r = self.compile_call_arg_to_reg(&args[0], span)?;
                    let dst_reg = dst.unwrap_or(r);
                    self.emit(Instruction::YieldFrames(r), line);
                    self.maybe_free_temp(r, dst_reg);
                    return Ok(dst_reg);
                }
                _ => {}
            }
        }

        // Native call: known host function -> CallNative(base, idx, arity)
        if let ExprKind::Identifier(name) = &callee.kind {
            let is_local = self.resolve_local(name).is_some();
            let is_upvalue = !is_local && self.resolve_upvalue(name).is_some();
            if !is_local
                && !is_upvalue
                && let Some(&native_idx) = self.native_index.get(name.as_str())
            {
                let base = self.next_reg;
                for arg in args {
                    let arg_reg = self.alloc_temp(span)?;
                    self.compile_call_arg(arg, Some(arg_reg))?;
                }
                let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                    annotation: None,
                    message: "too many arguments (max 255)".to_string(),
                    span: span.clone(),
                })?;
                self.emit(Instruction::CallNative(base, native_idx, arity), line);
                self.next_reg = base + 1;
                self.set_reg_type(base, ExprType::Other);
                if let Some(d) = dst {
                    if d != base {
                        self.emit(Instruction::Move(d, base), line);
                        self.next_reg = base;
                    }
                    return Ok(d);
                }
                return Ok(base);
            }
        }

        // Fallback: dynamic call. [callee, arg0, arg1, ...]
        let base = self.next_reg;
        let callee_reg = self.alloc_temp(span)?;
        self.compile_expr(callee, Some(callee_reg))?;

        for arg in args {
            let arg_reg = self.alloc_temp(span)?;
            self.compile_call_arg(arg, Some(arg_reg))?;
        }

        let arity = u8::try_from(args.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many arguments (max 255)".to_string(),
            span: span.clone(),
        })?;

        self.emit(Instruction::Call(base, arity), line);

        // Free all temps, result in base
        self.next_reg = base + 1;
        self.set_reg_type(base, ExprType::Other);

        // Move result to dst if a specific destination was requested
        if let Some(d) = dst {
            if d != base {
                self.emit(Instruction::Move(d, base), line);
                self.next_reg = base;
            }
            return Ok(d);
        }
        Ok(base)
    }

    // --- Collection compilation ---

    pub(super) fn compile_array_literal(
        &mut self,
        elements: &[ArrayElement],
        line: u32,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let span = dummy_span(line);
        let start = self.next_reg;
        let mut count: u8 = 0;
        for elem in elements {
            match elem {
                ArrayElement::Expr(e) => {
                    let r = self.alloc_temp(&span)?;
                    self.compile_expr(e, Some(r))?;
                    count += 1;
                }
                ArrayElement::Spread(e) => {
                    let r = self.alloc_temp(&span)?;
                    self.compile_expr(e, Some(r))?;
                    self.emit(Instruction::Spread(r), line);
                    count += 1;
                }
            }
        }
        // Free element temps
        self.next_reg = start;
        let d = self.dst_or_temp(dst, &span)?;
        self.emit(Instruction::MakeArray(d, start, count), line);
        self.set_reg_type(d, ExprType::Other);
        Ok(d)
    }

    pub(super) fn compile_dict_literal(
        &mut self,
        elements: &[DictElement],
        line: u32,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let span = dummy_span(line);
        let start = self.next_reg;
        let mut count: u8 = 0;
        for elem in elements {
            match elem {
                DictElement::KeyValue { key, value } => {
                    let kr = self.alloc_temp(&span)?;
                    self.compile_expr(key, Some(kr))?;
                    let vr = self.alloc_temp(&span)?;
                    self.compile_expr(value, Some(vr))?;
                    count += 1;
                }
                DictElement::Spread(e) => {
                    let r = self.alloc_temp(&span)?;
                    self.compile_expr(e, Some(r))?;
                    self.emit(Instruction::Spread(r), line);
                    count += 1;
                }
            }
        }
        self.next_reg = start;
        let d = self.dst_or_temp(dst, &span)?;
        self.emit(Instruction::MakeDict(d, start, count), line);
        self.set_reg_type(d, ExprType::Other);
        Ok(d)
    }

    // --- String interpolation compilation ---

    pub(super) fn compile_string_interpolation(
        &mut self,
        segments: &[InterpolationSegment],
        line: u32,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let span = dummy_span(line);
        if segments.is_empty() {
            let d = self.dst_or_temp(dst, &span)?;
            let idx = self.chunk.add_string("");
            self.emit(Instruction::LoadStr(d, idx), line);
            self.set_reg_type(d, ExprType::Other);
            return Ok(d);
        }

        let d = self.dst_or_temp(dst, &span)?;
        let mut first = true;
        for segment in segments {
            if first {
                match segment {
                    InterpolationSegment::Literal(s) => {
                        let idx = self.chunk.add_string(s);
                        self.emit(Instruction::LoadStr(d, idx), line);
                    }
                    InterpolationSegment::Expression(e) => {
                        self.compile_expr(e, Some(d))?;
                    }
                }
                first = false;
            } else {
                let tmp = self.alloc_temp(&span)?;
                match segment {
                    InterpolationSegment::Literal(s) => {
                        let idx = self.chunk.add_string(s);
                        self.emit(Instruction::LoadStr(tmp, idx), line);
                    }
                    InterpolationSegment::Expression(e) => {
                        self.compile_expr(e, Some(tmp))?;
                    }
                }
                self.emit(Instruction::Concat(d, d, tmp), line);
                self.free_temp(tmp);
            }
        }
        self.set_reg_type(d, ExprType::Other);
        Ok(d)
    }

    // --- Coroutine compilation ---

    pub(super) fn compile_yield(
        &mut self,
        arg: Option<&Expr>,
        span: &Span,
        dst: u8,
    ) -> Result<(), CompileError> {
        let line = span.line;
        self.has_yield = true;

        match arg {
            None => {
                // Bare yield: suspend for one frame
                self.emit(Instruction::Yield, line);
            }
            Some(expr) => {
                self.compile_yield_expr(expr, span, dst)?;
            }
        }
        Ok(())
    }

    pub(super) fn compile_yield_expr(
        &mut self,
        expr: &Expr,
        span: &Span,
        dst: u8,
    ) -> Result<(), CompileError> {
        let line = span.line;

        // Check for special yield forms
        if let ExprKind::Call { callee, args } = &expr.kind
            && let ExprKind::Identifier(name) = &callee.kind
        {
            match name.as_str() {
                "waitForSeconds" => {
                    if args.len() != 1 {
                        return Err(CompileError {
                            annotation: None,
                            message: "waitForSeconds expects 1 argument".to_string(),
                            span: span.clone(),
                        });
                    }
                    let r = self.compile_call_arg_to_reg(&args[0], span)?;
                    self.emit(Instruction::YieldSeconds(r), line);
                    self.maybe_free_temp(r, dst);
                    return Ok(());
                }
                "waitForFrames" => {
                    if args.len() != 1 {
                        return Err(CompileError {
                            annotation: None,
                            message: "waitForFrames expects 1 argument".to_string(),
                            span: span.clone(),
                        });
                    }
                    let r = self.compile_call_arg_to_reg(&args[0], span)?;
                    self.emit(Instruction::YieldFrames(r), line);
                    self.maybe_free_temp(r, dst);
                    return Ok(());
                }
                "waitUntil" => {
                    if args.len() != 1 {
                        return Err(CompileError {
                            annotation: None,
                            message: "waitUntil expects 1 argument".to_string(),
                            span: span.clone(),
                        });
                    }
                    let r = self.compile_call_arg_to_reg(&args[0], span)?;
                    self.emit(Instruction::YieldUntil(r), line);
                    self.maybe_free_temp(r, dst);
                    return Ok(());
                }
                _ => {}
            }
        }

        // Generic case: yield someCoroutine(args)
        if let ExprKind::Call { callee, args } = &expr.kind {
            // Allocate consecutive registers: [callee, arg0, arg1, ...]
            let base = self.next_reg;
            let callee_reg = self.alloc_temp(span)?;
            self.compile_expr(callee, Some(callee_reg))?;
            for arg in args {
                let arg_reg = self.alloc_temp(span)?;
                self.compile_call_arg(arg, Some(arg_reg))?;
            }
            let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many arguments (max 255)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::StartCoroutine(base, arity), line);
            self.next_reg = base + 1;
            self.emit(Instruction::YieldCoroutine(dst, base), line);
            self.next_reg = base;
        } else {
            return Err(CompileError {
                annotation: None,
                message: "yield expects a function call or yield variant".to_string(),
                span: span.clone(),
            });
        }
        Ok(())
    }

    // --- Struct compilation ---

    pub(super) fn compile_start(&mut self, expr: &Expr, span: &Span) -> Result<(), CompileError> {
        let line = span.line;

        if let ExprKind::Call { callee, args } = &expr.kind {
            let base = self.next_reg;
            let callee_reg = self.alloc_temp(span)?;
            self.compile_expr(callee, Some(callee_reg))?;
            for arg in args {
                let arg_reg = self.alloc_temp(span)?;
                self.compile_call_arg(arg, Some(arg_reg))?;
            }
            let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many arguments (max 255)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::StartCoroutine(base, arity), line);
            // Discard the CoroutineHandle (free all temps)
            self.next_reg = base;
        } else {
            return Err(CompileError {
                annotation: None,
                message: "start expects a function call".to_string(),
                span: span.clone(),
            });
        }
        Ok(())
    }

    pub(super) fn compile_call_arg(
        &mut self,
        arg: &CallArg,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        match arg {
            CallArg::Positional(expr) => self.compile_expr(expr, dst),
            CallArg::Named { value, .. } => self.compile_expr(value, dst),
        }
    }

    pub(super) fn compile_call_arg_to_reg(
        &mut self,
        arg: &CallArg,
        span: &Span,
    ) -> Result<u8, CompileError> {
        let reg = self.alloc_temp(span)?;
        self.compile_call_arg(arg, Some(reg))?;
        Ok(reg)
    }

    // --- State save/restore helpers ---
}
