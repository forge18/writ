use std::collections::HashSet;
use writ_lexer::Span;

use writ_parser::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClassDecl, DictElement, ElseBranch, Expr, ExprKind,
    FuncDecl, InterpolationSegment, Literal, Stmt, StmtKind, StructDecl, UnaryOp, Visibility,
    WhenBody, WhenPattern,
};

use crate::chunk::Chunk;
use crate::error::CompileError;
use crate::instruction::Instruction;
use crate::local::Local;
use crate::opcode::op;
use crate::upvalue::UpvalueDescriptor;

/// Creates a dummy Span for compiler-generated code (e.g., collection literals).
fn dummy_span(line: u32) -> Span {
    Span {
        file: String::new(),
        line,
        column: 0,
        length: 0,
    }
}

/// A compiled function, stored in the function table.
#[derive(Debug, Clone)]
pub struct CompiledFunction {
    pub name: String,
    pub arity: u8,
    pub chunk: Chunk,
    /// True if the function body contains `yield` (making it a coroutine).
    pub is_coroutine: bool,
    /// True if the last parameter is variadic (`...args: Type`).
    pub is_variadic: bool,
    /// Upvalue descriptors for this function. Empty if the function captures nothing.
    pub upvalues: Vec<UpvalueDescriptor>,
    /// Maximum number of registers this function uses (params + locals + temps).
    pub max_registers: u8,
    /// True if any register may hold an Rc-bearing value (Str, Array, Dict, Object, Closure).
    /// When false, the VM can skip `drop_in_place` on return (fast path via `set_len`).
    pub has_rc_values: bool,
}

/// Metadata about a struct type, produced by the compiler for the VM.
#[derive(Debug, Clone)]
pub struct StructMeta {
    /// Struct type name.
    pub name: String,
    /// Field names in declaration order.
    pub field_names: Vec<String>,
    /// Set of public field names.
    pub public_fields: HashSet<String>,
    /// Set of public method names.
    pub public_methods: HashSet<String>,
}

/// Metadata for a compiled class (consumed by the VM at runtime).
#[derive(Debug, Clone)]
pub struct ClassMeta {
    /// Class type name.
    pub name: String,
    /// All field names including inherited (parent fields first).
    pub field_names: Vec<String>,
    /// Only field names declared on this class (excludes inherited).
    pub own_field_names: Vec<String>,
    /// Set of public field names.
    pub public_fields: HashSet<String>,
    /// Set of public method names.
    pub public_methods: HashSet<String>,
    /// Parent class name for inheritance chain method lookup.
    pub parent: Option<String>,
}

/// Tracks the context of a loop being compiled.
struct LoopContext {
    /// Instruction offset of the loop start (for continue).
    start_offset: usize,
    /// Indices of break jump instructions to patch when loop ends.
    break_jumps: Vec<usize>,
    /// Scope depth at loop entry (for unwinding locals on break/continue).
    scope_depth: u32,
}

/// Saved state from an enclosing function scope, used for upvalue resolution.
struct EnclosingScope {
    locals: Vec<Local>,
    upvalues: Vec<UpvalueDescriptor>,
}

/// Compile-time type tag for typed instruction emission.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ExprType {
    Int,
    Float,
    Bool,
    Other,
}

/// Convert a parsed type annotation to a compile-time type tag.
fn type_expr_to_expr_type(ty: &writ_parser::TypeExpr) -> ExprType {
    match ty {
        writ_parser::TypeExpr::Simple(name) => match name.as_str() {
            "int" => ExprType::Int,
            "float" => ExprType::Float,
            "bool" => ExprType::Bool,
            _ => ExprType::Other,
        },
        _ => ExprType::Other,
    }
}

/// Bytecode compiler for the Writ language (register-based).
///
/// Walks the AST produced by `writ-parser` and emits register-based bytecode
/// instructions into a [`Chunk`]. Assumes the AST has already been validated
/// by `writ-types`.
pub struct Compiler {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: u32,
    functions: Vec<CompiledFunction>,
    loop_stack: Vec<LoopContext>,
    /// Tracks whether the current function body contains `yield`.
    has_yield: bool,
    /// Metadata for struct types (consumed by the VM).
    struct_metas: Vec<StructMeta>,
    /// Metadata for class types (consumed by the VM).
    class_metas: Vec<ClassMeta>,
    /// Stack of parent function scopes for upvalue resolution.
    enclosing_scopes: Vec<EnclosingScope>,
    /// Upvalue descriptors being built for the current function.
    current_upvalues: Vec<UpvalueDescriptor>,
    /// Per-register type tracking for typed instruction emission.
    reg_types: Vec<ExprType>,
    /// Function name → index in `functions` for direct call dispatch.
    function_index: std::collections::HashMap<String, u16>,
    /// Function name → return type for typed instruction emission after CallDirect.
    function_return_types: std::collections::HashMap<String, ExprType>,
    /// Next available register slot.
    next_reg: u8,
    /// High-water mark of registers used (becomes max_registers).
    max_reg: u8,
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

impl Compiler {
    /// Creates a new compiler with an empty chunk.
    pub fn new() -> Self {
        Self {
            chunk: Chunk::new(),
            locals: Vec::new(),
            scope_depth: 0,
            functions: Vec::new(),
            loop_stack: Vec::new(),
            has_yield: false,
            struct_metas: Vec::new(),
            class_metas: Vec::new(),
            enclosing_scopes: Vec::new(),
            current_upvalues: Vec::new(),
            reg_types: Vec::new(),
            function_index: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
            next_reg: 0,
            max_reg: 0,
        }
    }

    /// Returns a reference to the compiled chunk.
    pub fn chunk(&self) -> &Chunk {
        &self.chunk
    }

    /// Consumes the compiler and returns the compiled chunk.
    pub fn into_chunk(self) -> Chunk {
        self.chunk
    }

    /// Consumes the compiler and returns the main chunk, function table,
    /// struct metadata, and class metadata.
    ///
    /// Runs the peephole optimizer on all chunks before returning.
    pub fn into_parts(
        mut self,
    ) -> (
        Chunk,
        Vec<CompiledFunction>,
        Vec<StructMeta>,
        Vec<ClassMeta>,
    ) {
        // Run peephole optimization on main chunk and all function chunks
        self.chunk.optimize(None);
        for (idx, func) in self.functions.iter_mut().enumerate() {
            func.chunk.optimize(Some(idx as u16));
        }
        (
            self.chunk,
            self.functions,
            self.struct_metas,
            self.class_metas,
        )
    }

    /// Returns a reference to the compiled functions.
    pub fn functions(&self) -> &[CompiledFunction] {
        &self.functions
    }

    /// Returns a reference to the struct metadata.
    pub fn struct_metas(&self) -> &[StructMeta] {
        &self.struct_metas
    }

    /// Emits a single instruction at the given source line.
    pub fn emit(&mut self, instruction: Instruction, line: u32) {
        self.chunk.write(instruction, line);
    }

    // ── Register allocation ───────────────────────────────────────

    /// Allocate a register for a local variable. Returns the register slot.
    fn alloc_local(&mut self, span: &Span) -> Result<u8, CompileError> {
        let reg = self.next_reg;
        if reg == u8::MAX {
            return Err(CompileError {
                annotation: None,
                message: "too many local variables/temporaries (max 255)".to_string(),
                span: span.clone(),
            });
        }
        self.next_reg = reg + 1;
        if self.next_reg > self.max_reg {
            self.max_reg = self.next_reg;
        }
        // Extend reg_types
        while self.reg_types.len() <= reg as usize {
            self.reg_types.push(ExprType::Other);
        }
        Ok(reg)
    }

    /// Allocate a temporary register. Returns the register slot.
    fn alloc_temp(&mut self, span: &Span) -> Result<u8, CompileError> {
        self.alloc_local(span)
    }

    /// Free a temporary register. Must be the most recently allocated temp.
    fn free_temp(&mut self, reg: u8) {
        debug_assert_eq!(reg, self.next_reg - 1, "free_temp: not top of stack");
        self.next_reg -= 1;
    }

    /// Get a destination register: use the hint if provided, otherwise alloc a temp.
    fn dst_or_temp(&mut self, dst: Option<u8>, span: &Span) -> Result<u8, CompileError> {
        match dst {
            Some(d) => Ok(d),
            None => self.alloc_temp(span),
        }
    }

    /// Set the type tag for a register.
    fn set_reg_type(&mut self, reg: u8, ty: ExprType) {
        let idx = reg as usize;
        if idx >= self.reg_types.len() {
            self.reg_types.resize(idx + 1, ExprType::Other);
        }
        self.reg_types[idx] = ty;
    }

    /// Get the type tag for a register.
    fn reg_type(&self, reg: u8) -> ExprType {
        self.reg_types
            .get(reg as usize)
            .copied()
            .unwrap_or(ExprType::Other)
    }

    // ── Scope management ───────────────────────────────────────────

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self, line: u32) {
        self.scope_depth -= 1;
        while let Some(local) = self.locals.last() {
            if local.depth <= self.scope_depth {
                break;
            }
            let slot = local.slot;
            let captured = local.is_captured;
            self.locals.pop();
            if captured {
                self.emit(Instruction::CloseUpvalue(slot), line);
            }
            // No Pop needed — register is just freed
            self.next_reg = slot; // reclaim register
        }
    }

    fn compile_block(&mut self, stmts: &[Stmt], line: u32) -> Result<(), CompileError> {
        self.begin_scope();
        for stmt in stmts {
            self.compile_stmt(stmt)?;
        }
        self.end_scope(line);
        Ok(())
    }

    // ── Statement compilation ──────────────────────────────────────

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
                    // Don't free temp — we're returning
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

    // ── Expression compilation ─────────────────────────────────────
    // Returns the register containing the result.
    // If `dst` is Some, the result is placed there; otherwise a temp is allocated.

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
                            // Return the local's register directly — no instruction needed
                            Ok(slot)
                        }
                    }
                } else if let Some(uv) = self.resolve_upvalue(name) {
                    let d = self.dst_or_temp(dst, &expr.span)?;
                    self.emit(Instruction::LoadUpvalue(d, uv), line);
                    self.set_reg_type(d, ExprType::Other);
                    Ok(d)
                } else {
                    // Not a local or upvalue — try global / function name.
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
            other => Err(CompileError {
                annotation: None,
                message: format!("unsupported expression: {other:?}"),
                span: expr.span.clone(),
            }),
        }
    }

    /// Free `reg` if it's a temporary (i.e., >= next_reg-1 and != keep).
    fn maybe_free_temp(&mut self, reg: u8, keep: u8) {
        if reg != keep && reg == self.next_reg - 1 {
            self.free_temp(reg);
        }
    }

    // ── Binary expression compilation ──────────────────────────────

    fn compile_binary(
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
                // lhs was truthy — evaluate rhs into the same register
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
                // lhs was falsy — evaluate rhs into the same register
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

                // Mixed int/float → coerce int operand to float
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
    fn try_fold_binary(op: &BinaryOp, lhs: &Expr, rhs: &Expr) -> Option<Literal> {
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

    fn fold_int_op(op: &BinaryOp, a: i64, b: i64) -> Option<Literal> {
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

    fn fold_float_op(op: &BinaryOp, a: f64, b: f64) -> Option<Literal> {
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

    fn typed_binary_instruction(
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

    fn generic_binary_instruction(op: &BinaryOp, dst: u8, a: u8, b: u8) -> Instruction {
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

    // ── Literal compilation ────────────────────────────────────────

    fn compile_literal(
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

    // ── Assignment compilation ─────────────────────────────────────

    fn compile_assignment(
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
                // Fast path: local += int_literal → AddIntImm
                if let Some(slot) = local_slot
                    && let AssignOp::AddAssign = compound
                    && let ExprKind::Literal(Literal::Int(v)) = &value.kind
                    && let Ok(imm) = i32::try_from(*v)
                {
                    self.emit(Instruction::AddIntImm(slot, slot, imm), line);
                    return Ok(());
                }
                // Fast path: local -= int_literal → SubIntImm (or AddIntImm with neg)
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

    fn compound_to_binary(op: &AssignOp) -> BinaryOp {
        match op {
            AssignOp::AddAssign => BinaryOp::Add,
            AssignOp::SubAssign => BinaryOp::Subtract,
            AssignOp::MulAssign => BinaryOp::Multiply,
            AssignOp::DivAssign => BinaryOp::Divide,
            AssignOp::ModAssign => BinaryOp::Modulo,
            AssignOp::Assign => unreachable!("simple assign handled separately"),
        }
    }

    // ── Control flow: if/else ──────────────────────────────────────

    fn compile_if(
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

    // ── Control flow: while ────────────────────────────────────────

    fn compile_while(
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

    // ── Control flow: for ──────────────────────────────────────────

    fn compile_for(
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

    fn compile_for_range(
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

    fn compile_for_array(
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

    // ── Control flow: break/continue ───────────────────────────────

    fn compile_break(&mut self, span: &Span) -> Result<(), CompileError> {
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

    fn compile_continue(&mut self, span: &Span) -> Result<(), CompileError> {
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

    // ── When statement ─────────────────────────────────────────────

    fn compile_when(
        &mut self,
        subject: &Option<Expr>,
        arms: &[writ_parser::WhenArm],
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

    fn compile_when_body(&mut self, body: &WhenBody, line: u32) -> Result<(), CompileError> {
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

    /// Compiles a `when` expression — each arm's body leaves a value in a register.
    fn compile_when_expr(
        &mut self,
        subject: Option<&Expr>,
        arms: &[writ_parser::WhenArm],
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
    fn compile_when_expr_body(
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

    // ── Function compilation ───────────────────────────────────────

    fn compile_func_decl(&mut self, func: &FuncDecl, span: &Span) -> Result<(), CompileError> {
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
                let func_idx = u16::try_from(func_idx).map_err(|_| CompileError {
                    annotation: None,
                    message: "too many functions (max 65535)".to_string(),
                    span: span.clone(),
                })?;
                self.emit(Instruction::MakeClosure(slot, func_idx), line);
            } else {
                let index = self.chunk.add_string(&func.name);
                self.emit(Instruction::LoadStr(slot, index), line);
            }
        }

        Ok(())
    }

    fn compile_lambda(
        &mut self,
        params: &[writ_parser::FuncParam],
        body: &writ_parser::LambdaBody,
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
            writ_parser::LambdaBody::Expr(expr) => {
                let reg = self.compile_expr(expr, None)?;
                self.emit(Instruction::Return(reg), line);
            }
            writ_parser::LambdaBody::Block(stmts) => {
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

    fn compile_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: &Span,
        dst: Option<u8>,
    ) -> Result<u8, CompileError> {
        let line = span.line;

        // Method call: receiver.method(args) → CallMethod
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

    // ── Collection compilation ─────────────────────────────────────

    fn compile_array_literal(
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

    fn compile_dict_literal(
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

    // ── String interpolation compilation ───────────────────────────

    fn compile_string_interpolation(
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

    // ── Coroutine compilation ─────────────────────────────────────

    fn compile_yield(
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

    fn compile_yield_expr(
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

    // ── Struct compilation ───────────────────────────────────────

    fn compile_struct_decl(&mut self, decl: &StructDecl, span: &Span) -> Result<(), CompileError> {
        // Generic templates are not compiled — only their monomorphic instantiations are.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let line = span.line;
        let struct_name = &decl.name;

        let field_names: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();
        let public_fields: HashSet<String> = decl
            .fields
            .iter()
            .filter(|f| f.visibility == Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        // Compile methods
        let mut public_methods = HashSet::new();
        for method in &decl.methods {
            let qualified_name = format!("{}::{}", struct_name, method.name);
            public_methods.insert(method.name.clone());
            self.compile_method_body(&qualified_name, &method.params, &method.body, span, true)?;
        }

        // Compile constructor
        {
            let saved = self.save_state();
            self.scope_depth = 0;
            self.has_yield = false;
            self.next_reg = 0;
            self.max_reg = 0;

            for field in &decl.fields {
                self.add_local(&field.name, span)?;
            }

            let field_count = u8::try_from(decl.fields.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many struct fields (max 255)".to_string(),
                span: span.clone(),
            })?;
            let name_idx = self.chunk.add_string(struct_name);
            let result_reg = self.alloc_temp(span)?;
            self.emit(
                Instruction::MakeStruct(result_reg, name_idx, 0, field_count),
                line,
            );
            self.emit(Instruction::Return(result_reg), line);

            let ctor_chunk = std::mem::take(&mut self.chunk);
            let ctor_max = self.max_reg;

            self.restore_state(saved);

            let arity = u8::try_from(decl.fields.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many struct fields for constructor (max 255)".to_string(),
                span: span.clone(),
            })?;

            let func_idx = self.functions.len();
            self.function_index
                .insert(struct_name.clone(), func_idx as u16);
            self.functions.push(CompiledFunction {
                name: struct_name.clone(),
                arity,
                chunk: ctor_chunk,
                is_coroutine: false,
                is_variadic: false,
                upvalues: Vec::new(),
                max_registers: ctor_max,
                has_rc_values: true, // creates Struct value
            });
        }

        self.struct_metas.push(StructMeta {
            name: struct_name.clone(),
            field_names,
            public_fields,
            public_methods,
        });

        Ok(())
    }

    // ── Class compilation ──────────────────────────────────────

    fn compile_class_decl(&mut self, decl: &ClassDecl, span: &Span) -> Result<(), CompileError> {
        // Generic templates are not compiled — only their monomorphic instantiations are.
        if !decl.type_params.is_empty() {
            return Ok(());
        }

        let line = span.line;
        let class_name = &decl.name;

        let parent_field_names: Vec<String> = if let Some(parent) = &decl.extends {
            self.class_metas
                .iter()
                .find(|m| m.name == *parent)
                .map(|m| m.field_names.clone())
                .unwrap_or_default()
        } else {
            vec![]
        };

        let own_field_names: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();
        let mut all_field_names = parent_field_names;
        all_field_names.extend(own_field_names.clone());

        let public_fields: HashSet<String> = decl
            .fields
            .iter()
            .filter(|f| f.visibility == Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        // Compile methods
        let mut public_methods = HashSet::new();
        for method in &decl.methods {
            let qualified_name = format!("{}::{}", class_name, method.name);
            public_methods.insert(method.name.clone());
            self.compile_method_body(&qualified_name, &method.params, &method.body, span, true)?;
        }

        // Compile constructor
        {
            let saved = self.save_state();
            self.scope_depth = 0;
            self.has_yield = false;
            self.next_reg = 0;
            self.max_reg = 0;

            for field_name in &all_field_names {
                self.add_local(field_name, span)?;
            }

            let field_count = u8::try_from(all_field_names.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many class fields (max 255)".to_string(),
                span: span.clone(),
            })?;
            let name_idx = self.chunk.add_string(class_name);
            let result_reg = self.alloc_temp(span)?;
            self.emit(
                Instruction::MakeClass(result_reg, name_idx, 0, field_count),
                line,
            );
            self.emit(Instruction::Return(result_reg), line);

            let ctor_chunk = std::mem::take(&mut self.chunk);
            let ctor_max = self.max_reg;

            self.restore_state(saved);

            let arity = u8::try_from(all_field_names.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many class fields for constructor (max 255)".to_string(),
                span: span.clone(),
            })?;

            let func_idx = self.functions.len();
            self.function_index
                .insert(class_name.clone(), func_idx as u16);
            self.functions.push(CompiledFunction {
                name: class_name.clone(),
                arity,
                chunk: ctor_chunk,
                is_coroutine: false,
                is_variadic: false,
                upvalues: Vec::new(),
                max_registers: ctor_max,
                has_rc_values: true, // creates Object value
            });
        }

        self.class_metas.push(ClassMeta {
            name: class_name.clone(),
            field_names: all_field_names,
            own_field_names,
            public_fields,
            public_methods,
            parent: decl.extends.clone(),
        });

        Ok(())
    }

    /// Helper to compile a method body (struct or class method).
    fn compile_method_body(
        &mut self,
        qualified_name: &str,
        params: &[writ_parser::FuncParam],
        body: &[Stmt],
        span: &Span,
        has_self: bool,
    ) -> Result<(), CompileError> {
        let line = span.line;

        let saved = self.save_state();
        self.scope_depth = 0;
        self.has_yield = false;
        self.next_reg = 0;
        self.max_reg = 0;

        if has_self {
            self.add_local("self", span)?;
        }
        for param in params {
            self.add_local(&param.name, span)?;
        }

        for stmt in body {
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

        let method_chunk = std::mem::take(&mut self.chunk);
        let is_coroutine = self.has_yield;
        let method_max = self.max_reg;

        self.restore_state(saved);

        let arity = u8::try_from(params.len() + if has_self { 1 } else { 0 }).map_err(|_| {
            CompileError {
                annotation: None,
                message: "too many method parameters (max 254)".to_string(),
                span: span.clone(),
            }
        })?;

        self.functions.push(CompiledFunction {
            name: qualified_name.to_string(),
            arity,
            chunk: method_chunk,
            is_coroutine,
            is_variadic: false,
            upvalues: Vec::new(),
            max_registers: method_max,
            has_rc_values: true, // methods receive self (Object/Struct)
        });

        Ok(())
    }

    fn compile_start(&mut self, expr: &Expr, span: &Span) -> Result<(), CompileError> {
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

    fn compile_call_arg(&mut self, arg: &CallArg, dst: Option<u8>) -> Result<u8, CompileError> {
        match arg {
            CallArg::Positional(expr) => self.compile_expr(expr, dst),
            CallArg::Named { value, .. } => self.compile_expr(value, dst),
        }
    }

    fn compile_call_arg_to_reg(&mut self, arg: &CallArg, span: &Span) -> Result<u8, CompileError> {
        let reg = self.alloc_temp(span)?;
        self.compile_call_arg(arg, Some(reg))?;
        Ok(reg)
    }

    // ── State save/restore helpers ────────────────────────────────

    fn save_state(&mut self) -> SavedState {
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
    fn restore_state(&mut self, saved: SavedState) {
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

    fn resolve_local(&self, name: &str) -> Option<(u8, u8)> {
        self.locals
            .iter()
            .rev()
            .find(|local| local.name == name)
            .map(|local| (local.slot, local.type_tag))
    }

    fn resolve_upvalue(&mut self, name: &str) -> Option<u8> {
        if self.enclosing_scopes.is_empty() {
            return None;
        }
        self.resolve_upvalue_in(name, self.enclosing_scopes.len() - 1)
    }

    fn resolve_upvalue_in(&mut self, name: &str, scope_idx: usize) -> Option<u8> {
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

    fn add_upvalue(&mut self, is_local: bool, index: u8) -> u8 {
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

    fn add_upvalue_to_scope(&mut self, scope_idx: usize, is_local: bool, index: u8) -> u8 {
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

    fn add_local(&mut self, name: &str, span: &Span) -> Result<u8, CompileError> {
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

    fn add_local_with_slot(
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

    fn add_typed_local(
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

// Nested struct definition not allowed in impl block — define at module level
struct SavedState {
    chunk: Chunk,
    locals: Vec<Local>,
    scope_depth: u32,
    loop_stack: Vec<LoopContext>,
    has_yield: bool,
    reg_types: Vec<ExprType>,
    next_reg: u8,
    max_reg: u8,
}

fn expr_type_to_tag(t: ExprType) -> u8 {
    match t {
        ExprType::Other => 0,
        ExprType::Int => 1,
        ExprType::Float => 2,
        ExprType::Bool => 3,
    }
}

fn tag_to_expr_type(tag: u8) -> ExprType {
    match tag {
        1 => ExprType::Int,
        2 => ExprType::Float,
        3 => ExprType::Bool,
        _ => ExprType::Other,
    }
}

/// FNV-1a hash for field name lookups.
pub fn string_hash(s: &str) -> u32 {
    let mut hash: u32 = 2_166_136_261;
    for byte in s.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}
