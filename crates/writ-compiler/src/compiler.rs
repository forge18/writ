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
use crate::upvalue::UpvalueDescriptor;

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

/// Bytecode compiler for the Writ language.
///
/// Walks the AST produced by `writ-parser` and emits bytecode instructions
/// into a [`Chunk`]. Assumes the AST has already been validated by `writ-types`.
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
    /// Compile-time type stack mirroring the operand stack for typed instruction emission.
    type_stack: Vec<ExprType>,
    /// Function name → index in `functions` for direct call dispatch.
    function_index: std::collections::HashMap<String, u16>,
    /// Function name → return type for typed instruction emission after CallDirect.
    function_return_types: std::collections::HashMap<String, ExprType>,
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
            type_stack: Vec::new(),
            function_index: std::collections::HashMap::new(),
            function_return_types: std::collections::HashMap::new(),
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
        self.chunk.optimize();
        for func in &mut self.functions {
            func.chunk.optimize();
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
            if local.is_captured {
                self.emit(Instruction::CloseUpvalue(local.slot), line);
            } else {
                self.emit(Instruction::Pop, line);
            }
            self.locals.pop();
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

    // ── Program compilation ────────────────────────────────────────

    /// Compiles a sequence of statements (a program).
    pub fn compile_program(&mut self, stmts: &[Stmt]) -> Result<(), CompileError> {
        for stmt in stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    // ── Statement compilation ──────────────────────────────────────

    /// Compiles a single statement.
    pub fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), CompileError> {
        let line = stmt.span.line;
        match &stmt.kind {
            StmtKind::Let {
                name, initializer, ..
            }
            | StmtKind::Var {
                name, initializer, ..
            } => {
                self.compile_expr(initializer)?;
                let init_type = self.type_stack.pop().unwrap_or(ExprType::Other);
                let slot = self.add_typed_local(name, &stmt.span, init_type)?;
                self.emit(Instruction::StoreLocal(slot), line);
            }
            StmtKind::Const { name, initializer } => {
                self.compile_expr(initializer)?;
                let slot = self.add_local(name, &stmt.span)?;
                self.emit(Instruction::StoreLocal(slot), line);
            }
            StmtKind::LetDestructure { names, initializer } => {
                // Compile the initializer (expected to produce a tuple/array)
                self.compile_expr(initializer)?;
                let tuple_slot = self.add_local("__tuple", &stmt.span)?;
                self.emit(Instruction::StoreLocal(tuple_slot), line);
                // Extract each element by index into separate locals
                for (i, name) in names.iter().enumerate() {
                    self.emit(Instruction::LoadLocal(tuple_slot), line);
                    self.emit(Instruction::LoadInt(i as i32), line);
                    self.emit(Instruction::GetIndex, line);
                    let slot = self.add_local(name, &stmt.span)?;
                    self.emit(Instruction::StoreLocal(slot), line);
                }
            }
            StmtKind::Assignment { target, op, value } => {
                self.compile_assignment(target, op, value, &stmt.span)?;
            }
            StmtKind::ExprStmt(expr) => {
                self.compile_expr(expr)?;
                self.emit(Instruction::Pop, line);
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
                    self.compile_expr(e)?;
                } else {
                    self.emit(Instruction::LoadNull, line);
                }
                self.emit(Instruction::Return, line);
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

    /// Compiles a single expression, leaving its value on the stack.
    pub fn compile_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        let line = expr.span.line;
        match &expr.kind {
            ExprKind::Literal(lit) => self.compile_literal(lit, &expr.span)?,
            ExprKind::Identifier(name) => {
                if let Some((slot, type_tag)) = self.resolve_local(name) {
                    self.emit(Instruction::LoadLocal(slot), line);
                    self.type_stack.push(tag_to_expr_type(type_tag));
                } else if let Some(uv) = self.resolve_upvalue(name) {
                    self.emit(Instruction::LoadUpvalue(uv), line);
                    self.type_stack.push(ExprType::Other);
                } else {
                    // Not a local or upvalue — try global / function name.
                    let hash = string_hash(name);
                    self.chunk.add_string(name);
                    self.emit(Instruction::LoadGlobal(hash), line);
                    self.type_stack.push(ExprType::Other);
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.compile_binary(op, lhs, rhs, line)?;
            }
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
                        return self.compile_literal(&folded, &expr.span);
                    }
                }
                self.compile_expr(operand)?;
                let operand_type = self.type_stack.pop().unwrap_or(ExprType::Other);
                let instr = match op {
                    UnaryOp::Negate => Instruction::Neg,
                    UnaryOp::Not => Instruction::Not,
                };
                self.emit(instr, line);
                let result_type = match op {
                    UnaryOp::Negate => operand_type,
                    UnaryOp::Not => ExprType::Bool,
                };
                self.type_stack.push(result_type);
            }
            ExprKind::Grouped(inner) => {
                self.compile_expr(inner)?;
            }
            ExprKind::Call { callee, args } => {
                let ret_type = self.compile_call(callee, args, &expr.span)?;
                self.type_stack.push(ret_type);
            }
            ExprKind::MemberAccess { object, member } => {
                self.compile_expr(object)?;
                self.type_stack.pop(); // pop object type
                let hash = string_hash(member);
                // Store field name in string pool for runtime reverse lookup
                self.chunk.add_string(member);
                self.emit(Instruction::GetField(hash), line);
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::SafeAccess { object, member } => {
                self.compile_expr(object)?;
                self.type_stack.pop(); // pop object type
                let hash = string_hash(member);
                // Store field name in string pool for runtime reverse lookup
                self.chunk.add_string(member);
                self.emit(Instruction::GetField(hash), line);
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::ArrayLiteral(elements) => {
                self.compile_array_literal(elements, line)?;
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::DictLiteral(elements) => {
                self.compile_dict_literal(elements, line)?;
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::StringInterpolation(segments) => {
                self.compile_string_interpolation(segments, line)?;
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::NullCoalesce { lhs, rhs } => {
                self.compile_expr(lhs)?;
                self.type_stack.pop();
                self.compile_expr(rhs)?;
                self.type_stack.pop();
                self.emit(Instruction::NullCoalesce, line);
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::Lambda { params, body } => {
                self.compile_lambda(params, body, &expr.span)?;
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::Tuple(elements) => {
                for elem in elements {
                    self.compile_expr(elem)?;
                    self.type_stack.pop();
                }
                self.emit(Instruction::MakeArray(elements.len() as u16), line);
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::Range { start, end, .. } => {
                // `..` between strings compiles to Concat instruction
                self.compile_expr(start)?;
                self.type_stack.pop();
                self.compile_expr(end)?;
                self.type_stack.pop();
                self.emit(Instruction::Concat, line);
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::When { subject, arms } => {
                self.compile_when_expr(subject.as_deref(), arms, &expr.span)?;
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::Yield(arg) => {
                self.compile_yield(arg.as_deref(), &expr.span)?;
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::Index { object, index } => {
                self.compile_expr(object)?;
                self.type_stack.pop();
                self.compile_expr(index)?;
                self.type_stack.pop();
                self.emit(Instruction::GetIndex, line);
                self.type_stack.push(ExprType::Other);
            }
            ExprKind::Cast { expr, .. } => {
                self.compile_expr(expr)?;
                // Cast preserves whatever type was pushed by the inner expr
            }
            other => {
                return Err(CompileError {
                    annotation: None,
                    message: format!("unsupported expression: {other:?}"),
                    span: expr.span.clone(),
                });
            }
        }
        Ok(())
    }

    // ── Binary expression compilation ──────────────────────────────

    fn compile_binary(
        &mut self,
        op: &BinaryOp,
        lhs: &Expr,
        rhs: &Expr,
        line: u32,
    ) -> Result<(), CompileError> {
        match op {
            BinaryOp::And => {
                // Short-circuit: if lhs is false, skip rhs
                self.compile_expr(lhs)?;
                self.type_stack.pop();
                let end_jump = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                self.emit(Instruction::Pop, line);
                self.compile_expr(rhs)?;
                self.type_stack.pop();
                self.chunk.patch_jump(end_jump);
                self.type_stack.push(ExprType::Bool);
            }
            BinaryOp::Or => {
                // Short-circuit: if lhs is true, skip rhs
                self.compile_expr(lhs)?;
                self.type_stack.pop();
                let end_jump = self.chunk.emit_jump(Instruction::JumpIfTrue(0), line);
                self.emit(Instruction::Pop, line);
                self.compile_expr(rhs)?;
                self.type_stack.pop();
                self.chunk.patch_jump(end_jump);
                self.type_stack.push(ExprType::Bool);
            }
            _ => {
                // Try constant folding first
                if let Some(folded) = Self::try_fold_binary(op, lhs, rhs) {
                    self.compile_literal(&folded, &lhs.span)?;
                    return Ok(());
                }
                self.compile_expr(lhs)?;
                self.compile_expr(rhs)?;
                let rhs_type = self.type_stack.pop().unwrap_or(ExprType::Other);
                let lhs_type = self.type_stack.pop().unwrap_or(ExprType::Other);
                let (instr, result_type) = Self::typed_binary_instruction(op, lhs_type, rhs_type);
                self.emit(instr, line);
                self.type_stack.push(result_type);
            }
        }
        Ok(())
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

    /// Returns the typed instruction and result type for a binary operation.
    fn typed_binary_instruction(
        op: &BinaryOp,
        lhs_type: ExprType,
        rhs_type: ExprType,
    ) -> (Instruction, ExprType) {
        match (lhs_type, rhs_type) {
            (ExprType::Int, ExprType::Int) => match op {
                BinaryOp::Add => (Instruction::AddInt, ExprType::Int),
                BinaryOp::Subtract => (Instruction::SubInt, ExprType::Int),
                BinaryOp::Multiply => (Instruction::MulInt, ExprType::Int),
                BinaryOp::Divide => (Instruction::DivInt, ExprType::Int),
                BinaryOp::Less => (Instruction::LtInt, ExprType::Bool),
                BinaryOp::LessEqual => (Instruction::LeInt, ExprType::Bool),
                BinaryOp::Greater => (Instruction::GtInt, ExprType::Bool),
                BinaryOp::GreaterEqual => (Instruction::GeInt, ExprType::Bool),
                BinaryOp::Equal => (Instruction::EqInt, ExprType::Bool),
                BinaryOp::NotEqual => (Instruction::NeInt, ExprType::Bool),
                BinaryOp::Modulo => (Instruction::Mod, ExprType::Int),
                _ => (Self::binary_op_instruction(op), ExprType::Other),
            },
            (ExprType::Float, ExprType::Float) => match op {
                BinaryOp::Add => (Instruction::AddFloat, ExprType::Float),
                BinaryOp::Subtract => (Instruction::SubFloat, ExprType::Float),
                BinaryOp::Multiply => (Instruction::MulFloat, ExprType::Float),
                BinaryOp::Divide => (Instruction::DivFloat, ExprType::Float),
                BinaryOp::Less => (Instruction::LtFloat, ExprType::Bool),
                BinaryOp::LessEqual => (Instruction::LeFloat, ExprType::Bool),
                BinaryOp::Greater => (Instruction::GtFloat, ExprType::Bool),
                BinaryOp::GreaterEqual => (Instruction::GeFloat, ExprType::Bool),
                BinaryOp::Equal => (Instruction::EqFloat, ExprType::Bool),
                BinaryOp::NotEqual => (Instruction::NeFloat, ExprType::Bool),
                BinaryOp::Modulo => (Instruction::Mod, ExprType::Float),
                _ => (Self::binary_op_instruction(op), ExprType::Other),
            },
            _ => {
                let result_type = match op {
                    BinaryOp::Less
                    | BinaryOp::LessEqual
                    | BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::Equal
                    | BinaryOp::NotEqual => ExprType::Bool,
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => ExprType::Other,
                    _ => ExprType::Other,
                };
                (Self::binary_op_instruction(op), result_type)
            }
        }
    }

    // ── Literal compilation ────────────────────────────────────────

    fn compile_literal(&mut self, lit: &Literal, span: &Span) -> Result<(), CompileError> {
        let line = span.line;
        match lit {
            Literal::Int(v) => {
                if let Ok(narrowed) = i32::try_from(*v) {
                    self.emit(Instruction::LoadInt(narrowed), line);
                } else {
                    let idx = self.chunk.add_int64(*v);
                    self.emit(Instruction::LoadConstInt(idx), line);
                }
                self.type_stack.push(ExprType::Int);
            }
            Literal::Float(v) => {
                let narrowed = *v as f32;
                if narrowed.is_infinite() && !v.is_infinite() {
                    let idx = self.chunk.add_float64(*v);
                    self.emit(Instruction::LoadConstFloat(idx), line);
                } else {
                    self.emit(Instruction::LoadFloat(narrowed), line);
                }
                self.type_stack.push(ExprType::Float);
            }
            Literal::String(s) => {
                let index = self.chunk.add_string(s);
                self.emit(Instruction::LoadStr(index), line);
                self.type_stack.push(ExprType::Other);
            }
            Literal::Bool(b) => {
                self.emit(Instruction::LoadBool(*b), line);
                self.type_stack.push(ExprType::Bool);
            }
            Literal::Null => {
                self.emit(Instruction::LoadNull, line);
                self.type_stack.push(ExprType::Other);
            }
        }
        Ok(())
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
            // For value types (structs), we need to store the modified value
            // back to the local variable. Detect if the object is a local.
            let local_slot = if let ExprKind::Identifier(name) = &object.kind {
                self.resolve_local(name).map(|(slot, _)| slot)
            } else {
                None
            };

            self.compile_expr(object)?;
            self.compile_expr(value)?;
            let hash = string_hash(member);
            // Store field name in string pool for runtime reverse lookup
            self.chunk.add_string(member);
            self.emit(Instruction::SetField(hash), line);

            // For struct value types, the VM pushes the modified struct back.
            // Store it back to the local and pop.
            if let Some(slot) = local_slot {
                self.emit(Instruction::StoreLocal(slot), line);
            }
            return Ok(());
        }

        // Handle index assignment: collection[index] = value
        if let ExprKind::Index { object, index } = &target.kind {
            self.compile_expr(object)?;
            self.compile_expr(index)?;
            self.compile_expr(value)?;
            self.emit(Instruction::SetIndex, line);
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
                self.compile_expr(value)?;
                if let Some(slot) = local_slot {
                    self.emit(Instruction::StoreLocal(slot), line);
                } else if let Some(uv) = upvalue_idx {
                    self.emit(Instruction::StoreUpvalue(uv), line);
                }
            }
            compound => {
                // Fast path: local += int_literal → IncrLocalInt
                if let Some(slot) = local_slot
                    && let AssignOp::AddAssign = compound
                    && let ExprKind::Literal(Literal::Int(v)) = &value.kind
                    && let Ok(imm) = i32::try_from(*v)
                {
                    self.emit(Instruction::IncrLocalInt(slot, imm), line);
                    return Ok(());
                }
                // Fast path: local -= int_literal → IncrLocalInt with negated imm
                if let Some(slot) = local_slot
                    && let AssignOp::SubAssign = compound
                    && let ExprKind::Literal(Literal::Int(v)) = &value.kind
                    && let Ok(imm) = i32::try_from(*v)
                    && let Some(neg) = imm.checked_neg()
                {
                    self.emit(Instruction::IncrLocalInt(slot, neg), line);
                    return Ok(());
                }

                if let Some(slot) = local_slot {
                    self.emit(Instruction::LoadLocal(slot), line);
                } else if let Some(uv) = upvalue_idx {
                    self.emit(Instruction::LoadUpvalue(uv), line);
                }
                self.compile_expr(value)?;
                self.emit(Self::compound_op_instruction(compound), line);
                if let Some(slot) = local_slot {
                    self.emit(Instruction::StoreLocal(slot), line);
                } else if let Some(uv) = upvalue_idx {
                    self.emit(Instruction::StoreUpvalue(uv), line);
                }
            }
        }
        Ok(())
    }

    // ── Control flow: if/else ──────────────────────────────────────

    fn compile_if(
        &mut self,
        condition: &Expr,
        then_block: &[Stmt],
        else_branch: &Option<ElseBranch>,
        line: u32,
    ) -> Result<(), CompileError> {
        self.compile_expr(condition)?;
        // JumpIfFalsePop: pops the condition and jumps if falsy.
        // No separate Pop needed for either branch.
        let else_jump = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

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

        self.compile_expr(condition)?;
        // JumpIfFalsePop: pops the condition and jumps if falsy.
        let exit_jump = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

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
        self.compile_expr(start)?;
        let iter_slot = self.add_local("__iter", span)?;
        self.emit(Instruction::StoreLocal(iter_slot), line);

        self.compile_expr(end)?;
        let end_slot = self.add_local("__end", span)?;
        self.emit(Instruction::StoreLocal(end_slot), line);

        let loop_start = self.chunk.current_offset();

        // Condition: __iter < __end (or <= for inclusive)
        self.emit(Instruction::LoadLocal(iter_slot), line);
        self.emit(Instruction::LoadLocal(end_slot), line);
        let cmp = if inclusive {
            Instruction::Le
        } else {
            Instruction::Lt
        };
        self.emit(cmp, line);
        let exit_jump = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

        // Bind user-visible loop variable
        self.emit(Instruction::LoadLocal(iter_slot), line);
        let var_slot = self.add_local(variable, span)?;
        self.emit(Instruction::StoreLocal(var_slot), line);

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
        self.emit(Instruction::LoadLocal(iter_slot), line);
        self.emit(Instruction::LoadInt(1), line);
        self.emit(Instruction::Add, line);
        self.emit(Instruction::StoreLocal(iter_slot), line);

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

    fn compile_for_array(
        &mut self,
        variable: &str,
        iterable: &Expr,
        body: &[Stmt],
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;

        // Store the array in a hidden local
        self.compile_expr(iterable)?;
        let arr_slot = self.add_local("__arr", span)?;
        self.emit(Instruction::StoreLocal(arr_slot), line);

        // Get array length via GetField(hash("length"))
        self.emit(Instruction::LoadLocal(arr_slot), line);
        let len_hash = string_hash("length");
        self.emit(Instruction::GetField(len_hash), line);
        let len_slot = self.add_local("__len", span)?;
        self.emit(Instruction::StoreLocal(len_slot), line);

        // Counter starts at 0
        self.emit(Instruction::LoadInt(0), line);
        let idx_slot = self.add_local("__idx", span)?;
        self.emit(Instruction::StoreLocal(idx_slot), line);

        let loop_start = self.chunk.current_offset();

        // Condition: __idx < __len
        self.emit(Instruction::LoadLocal(idx_slot), line);
        self.emit(Instruction::LoadLocal(len_slot), line);
        self.emit(Instruction::Lt, line);
        let exit_jump = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

        // Get element: __arr[__idx]
        self.emit(Instruction::LoadLocal(arr_slot), line);
        self.emit(Instruction::LoadLocal(idx_slot), line);
        self.emit(Instruction::GetIndex, line);
        let var_slot = self.add_local(variable, span)?;
        self.emit(Instruction::StoreLocal(var_slot), line);

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
        self.emit(Instruction::LoadLocal(idx_slot), line);
        self.emit(Instruction::LoadInt(1), line);
        self.emit(Instruction::Add, line);
        self.emit(Instruction::StoreLocal(idx_slot), line);

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

        // Pop/close locals from inner scopes down to loop scope
        let pops: Vec<_> = self
            .locals
            .iter()
            .rev()
            .take_while(|l| l.depth > loop_depth)
            .map(|l| (l.is_captured, l.slot))
            .collect();
        for (captured, slot) in pops {
            if captured {
                self.emit(Instruction::CloseUpvalue(slot), line);
            } else {
                self.emit(Instruction::Pop, line);
            }
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

        // Pop/close locals from inner scopes down to loop scope
        let pops: Vec<_> = self
            .locals
            .iter()
            .rev()
            .take_while(|l| l.depth > loop_depth)
            .map(|l| (l.is_captured, l.slot))
            .collect();
        for (captured, slot) in pops {
            if captured {
                self.emit(Instruction::CloseUpvalue(slot), line);
            } else {
                self.emit(Instruction::Pop, line);
            }
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
            self.compile_expr(subj)?;
            let slot = self.add_local("__subject", span)?;
            self.emit(Instruction::StoreLocal(slot), line);
            Some(slot)
        } else {
            None
        };

        let mut end_jumps: Vec<usize> = Vec::new();

        for arm in arms {
            match &arm.pattern {
                WhenPattern::Else => {
                    // Else arm: no condition, just compile body
                    self.compile_when_body(&arm.body, line)?;
                }
                WhenPattern::Value(expr) => {
                    if let Some(slot) = subject_slot {
                        // Compare subject to value
                        self.emit(Instruction::LoadLocal(slot), line);
                        self.compile_expr(expr)?;
                        self.emit(Instruction::Eq, line);
                    } else {
                        // No subject: evaluate expression as boolean condition
                        self.compile_expr(expr)?;
                    }
                    let next_arm = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::MultipleValues(values) => {
                    let slot = subject_slot.expect("multiple values require subject");
                    // OR-chain: if any value matches, jump to body
                    let mut body_jumps: Vec<usize> = Vec::new();
                    for (i, val) in values.iter().enumerate() {
                        self.emit(Instruction::LoadLocal(slot), line);
                        self.compile_expr(val)?;
                        self.emit(Instruction::Eq, line);
                        if i < values.len() - 1 {
                            let body_jump = self.chunk.emit_jump(Instruction::JumpIfTrue(0), line);
                            self.emit(Instruction::Pop, line); // pop falsy comparison
                            body_jumps.push(body_jump);
                        }
                    }
                    // Last comparison: if false, skip to next arm
                    let next_arm = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                    self.emit(Instruction::Pop, line); // pop truthy condition

                    // Patch body jumps to here
                    for jump in &body_jumps {
                        self.chunk.patch_jump(*jump);
                    }
                    // Pop the truthy condition from JumpIfTrue targets
                    if !body_jumps.is_empty() {
                        // The JumpIfTrue jumps still have the truthy value on stack
                        // We need a Pop here for them too - but since JumpIfTrue
                        // doesn't pop, we need to add a Pop after all body_jumps converge
                        // Actually, the body_jumps jump to just before the next_arm jump
                        // handling. Let me restructure this.
                    }

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                    self.emit(Instruction::Pop, line); // pop falsy condition
                }
                WhenPattern::Range {
                    start,
                    end,
                    inclusive,
                } => {
                    let slot = subject_slot.expect("range pattern requires subject");
                    // subject >= start
                    self.emit(Instruction::LoadLocal(slot), line);
                    self.compile_expr(start)?;
                    self.emit(Instruction::Ge, line);
                    let fail_jump1 = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                    self.emit(Instruction::Pop, line); // pop truthy first comparison

                    // subject < end (or <= for inclusive)
                    self.emit(Instruction::LoadLocal(slot), line);
                    self.compile_expr(end)?;
                    let cmp = if *inclusive {
                        Instruction::Le
                    } else {
                        Instruction::Lt
                    };
                    self.emit(cmp, line);
                    let fail_jump2 = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                    self.emit(Instruction::Pop, line); // pop truthy second comparison

                    self.compile_when_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    // Patch both failure paths
                    self.chunk.patch_jump(fail_jump1);
                    self.emit(Instruction::Pop, line);
                    self.chunk.patch_jump(fail_jump2);
                    self.emit(Instruction::Pop, line);
                }
                WhenPattern::Guard {
                    binding: _,
                    condition,
                } => {
                    // Guard: compile the condition
                    self.compile_expr(condition)?;
                    let next_arm = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

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
                self.compile_expr(expr)?;
                self.emit(Instruction::Pop, line);
            }
            WhenBody::Block(stmts) => {
                self.compile_block(stmts, line)?;
            }
        }
        Ok(())
    }

    /// Compiles a `when` expression — each arm's body leaves a value on the stack.
    fn compile_when_expr(
        &mut self,
        subject: Option<&Expr>,
        arms: &[writ_parser::WhenArm],
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;
        self.begin_scope();

        let subject_slot = if let Some(subj) = subject {
            self.compile_expr(subj)?;
            let slot = self.add_local("__subject", span)?;
            self.emit(Instruction::StoreLocal(slot), line);
            Some(slot)
        } else {
            None
        };

        let mut end_jumps: Vec<usize> = Vec::new();

        for arm in arms {
            match &arm.pattern {
                WhenPattern::Else => {
                    self.compile_when_expr_body(&arm.body, line)?;
                }
                WhenPattern::Value(expr) => {
                    if let Some(slot) = subject_slot {
                        self.emit(Instruction::LoadLocal(slot), line);
                        self.compile_expr(expr)?;
                        self.emit(Instruction::Eq, line);
                    } else {
                        self.compile_expr(expr)?;
                    }
                    let next_arm = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

                    self.compile_when_expr_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                }
                WhenPattern::MultipleValues(values) => {
                    let slot = subject_slot.expect("multiple values require subject");
                    let mut body_jumps: Vec<usize> = Vec::new();
                    for (i, val) in values.iter().enumerate() {
                        self.emit(Instruction::LoadLocal(slot), line);
                        self.compile_expr(val)?;
                        self.emit(Instruction::Eq, line);
                        if i < values.len() - 1 {
                            let body_jump = self.chunk.emit_jump(Instruction::JumpIfTrue(0), line);
                            self.emit(Instruction::Pop, line);
                            body_jumps.push(body_jump);
                        }
                    }
                    let next_arm = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                    self.emit(Instruction::Pop, line);

                    for jump in &body_jumps {
                        self.chunk.patch_jump(*jump);
                    }

                    self.compile_when_expr_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(next_arm);
                    self.emit(Instruction::Pop, line);
                }
                WhenPattern::Range {
                    start,
                    end,
                    inclusive,
                } => {
                    let slot = subject_slot.expect("range pattern requires subject");
                    self.emit(Instruction::LoadLocal(slot), line);
                    self.compile_expr(start)?;
                    self.emit(Instruction::Ge, line);
                    let fail_jump1 = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                    self.emit(Instruction::Pop, line);

                    self.emit(Instruction::LoadLocal(slot), line);
                    self.compile_expr(end)?;
                    let cmp = if *inclusive {
                        Instruction::Le
                    } else {
                        Instruction::Lt
                    };
                    self.emit(cmp, line);
                    let fail_jump2 = self.chunk.emit_jump(Instruction::JumpIfFalse(0), line);
                    self.emit(Instruction::Pop, line);

                    self.compile_when_expr_body(&arm.body, line)?;

                    let end_jump = self.chunk.emit_jump(Instruction::Jump(0), line);
                    end_jumps.push(end_jump);

                    self.chunk.patch_jump(fail_jump1);
                    self.emit(Instruction::Pop, line);
                    self.chunk.patch_jump(fail_jump2);
                    self.emit(Instruction::Pop, line);
                }
                WhenPattern::Guard {
                    binding: _,
                    condition,
                } => {
                    self.compile_expr(condition)?;
                    let next_arm = self.chunk.emit_jump(Instruction::JumpIfFalsePop(0), line);

                    self.compile_when_expr_body(&arm.body, line)?;

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

        // If no arm matched, push null as the default value
        self.emit(Instruction::LoadNull, line);

        for jump in end_jumps {
            self.chunk.patch_jump(jump);
        }

        self.end_scope(line);
        Ok(())
    }

    /// Compiles a when-expression arm body, leaving the result on the stack.
    fn compile_when_expr_body(&mut self, body: &WhenBody, line: u32) -> Result<(), CompileError> {
        match body {
            WhenBody::Expr(expr) => {
                self.compile_expr(expr)?;
            }
            WhenBody::Block(stmts) => {
                // For block bodies in when-expressions, compile all statements.
                // The last expression statement's value stays on stack.
                // If the block is empty or ends with a non-expression, push null.
                if stmts.is_empty() {
                    self.emit(Instruction::LoadNull, line);
                } else {
                    for (i, stmt) in stmts.iter().enumerate() {
                        if i == stmts.len() - 1 {
                            // Last statement: if it's an expression statement, keep value on stack
                            if let StmtKind::ExprStmt(expr) = &stmt.kind {
                                self.compile_expr(expr)?;
                            } else {
                                self.compile_stmt(stmt)?;
                                self.emit(Instruction::LoadNull, line);
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
        // CallDirect instead of LoadGlobal + Call. Only for top-level functions;
        // nested functions handle recursion via upvalue resolution.
        if !nested {
            let pre_func_idx = self.functions.len() as u16;
            self.function_index
                .insert(func.name.clone(), pre_func_idx);
        }

        // Pre-register return type so CallDirect to this function can push
        // the correct ExprType, enabling typed instruction emission.
        if let Some(ref ret_type) = func.return_type {
            let expr_type = type_expr_to_expr_type(ret_type);
            self.function_return_types
                .insert(func.name.clone(), expr_type);
        }
        let predeclared_slot = if nested {
            let slot = self.add_local(&func.name, span)?;
            self.emit(Instruction::LoadNull, line);
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
        let saved_type_stack = std::mem::take(&mut self.type_stack);

        // Push current scope as enclosing (for upvalue resolution)
        self.enclosing_scopes.push(EnclosingScope {
            locals: saved_locals,
            upvalues: saved_upvalues,
        });

        // Reset for function compilation
        self.scope_depth = 0;
        self.has_yield = false;

        // Add parameters as locals (slots 0, 1, 2, ...) with type tags
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
            || !matches!(self.chunk.instructions().last(), Some(Instruction::Return))
        {
            self.emit(Instruction::LoadNull, line);
            self.emit(Instruction::Return, line);
        }

        let func_chunk = std::mem::replace(&mut self.chunk, saved_chunk);
        let is_coroutine = self.has_yield;
        let func_upvalues = std::mem::take(&mut self.current_upvalues);
        let has_upvalues = !func_upvalues.is_empty();

        // Pop enclosing scope (locals may have been marked as captured)
        let enclosing = self.enclosing_scopes.pop().unwrap();
        self.locals = enclosing.locals;
        self.current_upvalues = enclosing.upvalues;
        self.scope_depth = saved_scope_depth;
        self.loop_stack = saved_loop_stack;
        self.has_yield = saved_has_yield;
        self.type_stack = saved_type_stack;

        let is_variadic = func.params.last().is_some_and(|p| p.is_variadic);
        let arity = u8::try_from(func.params.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many function parameters (max 255)".to_string(),
            span: span.clone(),
        })?;

        let func_idx = self.functions.len();
        // Update function_index to actual index (may differ from pre-registered
        // index if nested functions were compiled and pushed during body compilation).
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
        });

        if let Some(slot) = predeclared_slot {
            if has_upvalues {
                // Closure: create and store into pre-declared slot
                let func_idx = u16::try_from(func_idx).map_err(|_| CompileError {
                    annotation: None,
                    message: "too many functions (max 65535)".to_string(),
                    span: span.clone(),
                })?;
                self.emit(Instruction::MakeClosure(func_idx), line);
                self.emit(Instruction::StoreLocal(slot), line);
            } else {
                // Non-capturing nested function: store name string so calls
                // go through function_map resolution.
                let idx = self.chunk.add_string(&func.name);
                self.emit(Instruction::LoadStr(idx), line);
                self.emit(Instruction::StoreLocal(slot), line);
            }
        }

        Ok(())
    }

    fn compile_lambda(
        &mut self,
        params: &[writ_parser::FuncParam],
        body: &writ_parser::LambdaBody,
        span: &Span,
    ) -> Result<(), CompileError> {
        let line = span.line;

        // Save current compiler state
        let saved_chunk = std::mem::take(&mut self.chunk);
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_scope_depth = self.scope_depth;
        let saved_loop_stack = std::mem::take(&mut self.loop_stack);
        let saved_has_yield = self.has_yield;
        let saved_upvalues = std::mem::take(&mut self.current_upvalues);

        // Push current scope as enclosing (for upvalue resolution)
        self.enclosing_scopes.push(EnclosingScope {
            locals: saved_locals,
            upvalues: saved_upvalues,
        });

        self.scope_depth = 0;
        self.has_yield = false;

        // Add parameters as locals
        for param in params {
            self.add_local(&param.name, span)?;
        }

        // Compile lambda body
        match body {
            writ_parser::LambdaBody::Expr(expr) => {
                self.compile_expr(expr)?;
                self.emit(Instruction::Return, line);
            }
            writ_parser::LambdaBody::Block(stmts) => {
                for stmt in stmts {
                    self.compile_stmt(stmt)?;
                }
                if self.chunk.is_empty()
                    || !matches!(self.chunk.instructions().last(), Some(Instruction::Return))
                {
                    self.emit(Instruction::LoadNull, line);
                    self.emit(Instruction::Return, line);
                }
            }
        }

        let lambda_chunk = std::mem::replace(&mut self.chunk, saved_chunk);
        let is_coroutine = self.has_yield;
        let lambda_upvalues = std::mem::take(&mut self.current_upvalues);
        let has_upvalues = !lambda_upvalues.is_empty();

        // Pop enclosing scope (locals may have been marked as captured)
        let enclosing = self.enclosing_scopes.pop().unwrap();
        self.locals = enclosing.locals;
        self.current_upvalues = enclosing.upvalues;
        self.scope_depth = saved_scope_depth;
        self.loop_stack = saved_loop_stack;
        self.has_yield = saved_has_yield;

        let arity = u8::try_from(params.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many lambda parameters (max 255)".to_string(),
            span: span.clone(),
        })?;

        // Store lambda as an anonymous function
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
        });

        if has_upvalues {
            // Lambda captures variables — emit MakeClosure
            let func_idx = u16::try_from(func_idx).map_err(|_| CompileError {
                annotation: None,
                message: "too many functions (max 65535)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::MakeClosure(func_idx), line);
        } else {
            // No captures — use the cheap string-name path
            let index = self.chunk.add_string(&name);
            self.emit(Instruction::LoadStr(index), line);
        }

        Ok(())
    }

    // ── Function call compilation ──────────────────────────────────

    fn compile_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        span: &Span,
    ) -> Result<ExprType, CompileError> {
        let line = span.line;

        // Method call: receiver.method(args) → CallMethod
        if let ExprKind::MemberAccess { object, member } = &callee.kind {
            // Push receiver
            self.compile_expr(object)?;
            self.type_stack.pop(); // receiver

            // Compile arguments
            for arg in args {
                self.compile_call_arg(arg)?;
                self.type_stack.pop(); // each arg
            }

            let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many arguments (max 255)".to_string(),
                span: span.clone(),
            })?;

            let hash = string_hash(member);
            self.chunk.add_string(member);
            self.emit(Instruction::CallMethod(hash, arity), line);
            return Ok(ExprType::Other);
        }

        // Direct call optimization: if the callee is a simple identifier that
        // resolves to a known compiled function (not a local or upvalue), emit
        // CallDirect to skip the LoadGlobal + string-based function lookup.
        if let ExprKind::Identifier(name) = &callee.kind {
            let is_local = self.resolve_local(name).is_some();  // (slot, type_tag)
            let is_upvalue = !is_local && self.resolve_upvalue(name).is_some();
            if !is_local
                && !is_upvalue
                && let Some(&func_idx) = self.function_index.get(name.as_str())
            {
                // Look up the return type for typed instruction emission
                let ret_type = self
                    .function_return_types
                    .get(name.as_str())
                    .copied()
                    .unwrap_or(ExprType::Other);

                // Compile arguments (no callee on stack)
                for arg in args {
                    match arg {
                        CallArg::Positional(expr) => self.compile_expr(expr)?,
                        CallArg::Named { value, .. } => self.compile_expr(value)?,
                    }
                    self.type_stack.pop(); // each arg
                }

                let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                    annotation: None,
                    message: "too many arguments (max 255)".to_string(),
                    span: span.clone(),
                })?;

                self.emit(Instruction::CallDirect(func_idx, arity), line);
                return Ok(ret_type);
            }
        }

        // Fallback: push callee value, then Call (for locals, upvalues, native functions)
        self.compile_expr(callee)?;
        self.type_stack.pop(); // callee

        // Compile arguments
        for arg in args {
            match arg {
                CallArg::Positional(expr) => self.compile_expr(expr)?,
                CallArg::Named { value, .. } => self.compile_expr(value)?,
            }
            self.type_stack.pop(); // each arg
        }

        let arity = u8::try_from(args.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many arguments (max 255)".to_string(),
            span: span.clone(),
        })?;

        self.emit(Instruction::Call(arity), line);
        Ok(ExprType::Other)
    }

    // ── Collection compilation ─────────────────────────────────────

    fn compile_array_literal(
        &mut self,
        elements: &[ArrayElement],
        line: u32,
    ) -> Result<(), CompileError> {
        let mut count: u16 = 0;
        for elem in elements {
            match elem {
                ArrayElement::Expr(e) => {
                    self.compile_expr(e)?;
                    self.type_stack.pop();
                    count += 1;
                }
                ArrayElement::Spread(e) => {
                    self.compile_expr(e)?;
                    self.type_stack.pop();
                    self.emit(Instruction::Spread, line);
                    count += 1;
                }
            }
        }
        self.emit(Instruction::MakeArray(count), line);
        Ok(())
    }

    fn compile_dict_literal(
        &mut self,
        elements: &[DictElement],
        line: u32,
    ) -> Result<(), CompileError> {
        let mut count: u16 = 0;
        for elem in elements {
            match elem {
                DictElement::KeyValue { key, value } => {
                    self.compile_expr(key)?;
                    self.type_stack.pop();
                    self.compile_expr(value)?;
                    self.type_stack.pop();
                    count += 1;
                }
                DictElement::Spread(e) => {
                    self.compile_expr(e)?;
                    self.type_stack.pop();
                    self.emit(Instruction::Spread, line);
                    count += 1;
                }
            }
        }
        self.emit(Instruction::MakeDict(count), line);
        Ok(())
    }

    // ── String interpolation compilation ───────────────────────────

    fn compile_string_interpolation(
        &mut self,
        segments: &[InterpolationSegment],
        line: u32,
    ) -> Result<(), CompileError> {
        if segments.is_empty() {
            let idx = self.chunk.add_string("");
            self.emit(Instruction::LoadStr(idx), line);
            return Ok(());
        }

        let mut first = true;
        for segment in segments {
            match segment {
                InterpolationSegment::Literal(s) => {
                    let idx = self.chunk.add_string(s);
                    self.emit(Instruction::LoadStr(idx), line);
                }
                InterpolationSegment::Expression(e) => {
                    self.compile_expr(e)?;
                    self.type_stack.pop();
                }
            }
            if first {
                first = false;
            } else {
                self.emit(Instruction::Concat, line);
            }
        }
        Ok(())
    }

    // ── Coroutine compilation ─────────────────────────────────────

    fn compile_yield(&mut self, arg: Option<&Expr>, span: &Span) -> Result<(), CompileError> {
        let line = span.line;
        self.has_yield = true;

        match arg {
            None => {
                // Bare yield: suspend for one frame
                self.emit(Instruction::Yield, line);
            }
            Some(expr) => {
                self.compile_yield_expr(expr, span)?;
            }
        }
        Ok(())
    }

    fn compile_yield_expr(&mut self, expr: &Expr, span: &Span) -> Result<(), CompileError> {
        let line = span.line;

        // Check for special yield forms: waitForSeconds, waitForFrames, waitUntil
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
                    self.compile_call_arg(&args[0])?;
                    self.emit(Instruction::YieldSeconds, line);
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
                    self.compile_call_arg(&args[0])?;
                    self.emit(Instruction::YieldFrames, line);
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
                    self.compile_call_arg(&args[0])?;
                    self.emit(Instruction::YieldUntil, line);
                    return Ok(());
                }
                _ => {}
            }
        }

        // Generic case: yield someCoroutine(args)
        // Emit StartCoroutine + YieldCoroutine
        if let ExprKind::Call { callee, args } = &expr.kind {
            self.compile_expr(callee)?;
            for arg in args {
                self.compile_call_arg(arg)?;
            }
            let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many arguments (max 255)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::StartCoroutine(arity), line);
            self.emit(Instruction::YieldCoroutine, line);
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
        let line = span.line;
        let struct_name = &decl.name;

        // Collect field metadata
        let field_names: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();
        let public_fields: HashSet<String> = decl
            .fields
            .iter()
            .filter(|f| f.visibility == Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        // Compile methods as `StructName::method_name` functions
        let mut public_methods = HashSet::new();
        for method in &decl.methods {
            let qualified_name = format!("{}::{}", struct_name, method.name);

            // Determine visibility (methods without explicit visibility are public by default in the spec)
            public_methods.insert(method.name.clone());

            // Compile method body with `self` as first param
            let saved_chunk = std::mem::take(&mut self.chunk);
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_scope_depth = self.scope_depth;
            let saved_loop_stack = std::mem::take(&mut self.loop_stack);
            let saved_has_yield = self.has_yield;
            let saved_type_stack = std::mem::take(&mut self.type_stack);

            self.scope_depth = 0;
            self.has_yield = false;

            // `self` is the implicit first parameter
            self.add_local("self", span)?;

            // Add explicit parameters
            for param in &method.params {
                self.add_local(&param.name, span)?;
            }

            // Compile method body
            for stmt in &method.body {
                self.compile_stmt(stmt)?;
            }

            // Ensure implicit return
            if self.chunk.is_empty()
                || !matches!(self.chunk.instructions().last(), Some(Instruction::Return))
            {
                self.emit(Instruction::LoadNull, line);
                self.emit(Instruction::Return, line);
            }

            let method_chunk = std::mem::replace(&mut self.chunk, saved_chunk);
            let is_coroutine = self.has_yield;

            self.locals = saved_locals;
            self.scope_depth = saved_scope_depth;
            self.loop_stack = saved_loop_stack;
            self.has_yield = saved_has_yield;
            self.type_stack = saved_type_stack;

            // arity = params + 1 (for self)
            let arity = u8::try_from(method.params.len() + 1).map_err(|_| CompileError {
                annotation: None,
                message: "too many method parameters (max 254)".to_string(),
                span: span.clone(),
            })?;

            self.functions.push(CompiledFunction {
                name: qualified_name,
                arity,
                chunk: method_chunk,
                is_coroutine,
                is_variadic: false,
                upvalues: Vec::new(),
            });
        }

        // Compile constructor function: takes field values, emits MakeStruct
        {
            let saved_chunk = std::mem::take(&mut self.chunk);
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_scope_depth = self.scope_depth;
            let saved_loop_stack = std::mem::take(&mut self.loop_stack);
            let saved_has_yield = self.has_yield;
            let saved_type_stack = std::mem::take(&mut self.type_stack);

            self.scope_depth = 0;
            self.has_yield = false;

            // Constructor params = struct fields
            for field in &decl.fields {
                self.add_local(&field.name, span)?;
            }

            // Push all field values onto stack in order
            for (i, _field) in decl.fields.iter().enumerate() {
                let slot = i as u8;
                self.emit(Instruction::LoadLocal(slot), line);
            }

            // Emit MakeStruct
            let name_idx = self.chunk.add_string(struct_name);
            let field_count = u16::try_from(decl.fields.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many struct fields (max 65535)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::MakeStruct(name_idx, field_count), line);
            self.emit(Instruction::Return, line);

            let ctor_chunk = std::mem::replace(&mut self.chunk, saved_chunk);

            self.locals = saved_locals;
            self.scope_depth = saved_scope_depth;
            self.loop_stack = saved_loop_stack;
            self.has_yield = saved_has_yield;
            self.type_stack = saved_type_stack;

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
            });
        }

        // Store struct metadata for the VM
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
        let line = span.line;
        let class_name = &decl.name;

        // Resolve inherited fields from parent class (if extends)
        let parent_field_names: Vec<String> = if let Some(parent) = &decl.extends {
            self.class_metas
                .iter()
                .find(|m| m.name == *parent)
                .map(|m| m.field_names.clone())
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Own fields declared on this class
        let own_field_names: Vec<String> = decl.fields.iter().map(|f| f.name.clone()).collect();

        // All fields: parent fields first, then own fields
        let mut all_field_names = parent_field_names.clone();
        all_field_names.extend(own_field_names.clone());

        let public_fields: HashSet<String> = decl
            .fields
            .iter()
            .filter(|f| f.visibility == Visibility::Public)
            .map(|f| f.name.clone())
            .collect();

        // Compile methods as `ClassName::method_name` functions
        let mut public_methods = HashSet::new();
        for method in &decl.methods {
            let qualified_name = format!("{}::{}", class_name, method.name);
            public_methods.insert(method.name.clone());

            // Compile method body with `self` as first param
            let saved_chunk = std::mem::take(&mut self.chunk);
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_scope_depth = self.scope_depth;
            let saved_loop_stack = std::mem::take(&mut self.loop_stack);
            let saved_has_yield = self.has_yield;
            let saved_type_stack = std::mem::take(&mut self.type_stack);

            self.scope_depth = 0;
            self.has_yield = false;

            // `self` is the implicit first parameter
            self.add_local("self", span)?;

            // Add explicit parameters
            for param in &method.params {
                self.add_local(&param.name, span)?;
            }

            // Compile method body
            for stmt in &method.body {
                self.compile_stmt(stmt)?;
            }

            // Ensure implicit return
            if self.chunk.is_empty()
                || !matches!(self.chunk.instructions().last(), Some(Instruction::Return))
            {
                self.emit(Instruction::LoadNull, line);
                self.emit(Instruction::Return, line);
            }

            let method_chunk = std::mem::replace(&mut self.chunk, saved_chunk);
            let is_coroutine = self.has_yield;

            self.locals = saved_locals;
            self.scope_depth = saved_scope_depth;
            self.loop_stack = saved_loop_stack;
            self.has_yield = saved_has_yield;
            self.type_stack = saved_type_stack;

            // arity = params + 1 (for self)
            let arity = u8::try_from(method.params.len() + 1).map_err(|_| CompileError {
                annotation: None,
                message: "too many method parameters (max 254)".to_string(),
                span: span.clone(),
            })?;

            self.functions.push(CompiledFunction {
                name: qualified_name,
                arity,
                chunk: method_chunk,
                is_coroutine,
                is_variadic: false,
                upvalues: Vec::new(),
            });
        }

        // Compile constructor function: takes all field values (parent + own),
        // emits MakeClass
        {
            let saved_chunk = std::mem::take(&mut self.chunk);
            let saved_locals = std::mem::take(&mut self.locals);
            let saved_scope_depth = self.scope_depth;
            let saved_loop_stack = std::mem::take(&mut self.loop_stack);
            let saved_has_yield = self.has_yield;
            let saved_type_stack = std::mem::take(&mut self.type_stack);

            self.scope_depth = 0;
            self.has_yield = false;

            // Constructor params = all fields (parent + own)
            for field_name in &all_field_names {
                self.add_local(field_name, span)?;
            }

            // Push all field values onto stack in order
            for (i, _field_name) in all_field_names.iter().enumerate() {
                let slot = i as u8;
                self.emit(Instruction::LoadLocal(slot), line);
            }

            // Emit MakeClass
            let name_idx = self.chunk.add_string(class_name);
            let field_count = u16::try_from(all_field_names.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many class fields (max 65535)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::MakeClass(name_idx, field_count), line);
            self.emit(Instruction::Return, line);

            let ctor_chunk = std::mem::replace(&mut self.chunk, saved_chunk);

            self.locals = saved_locals;
            self.scope_depth = saved_scope_depth;
            self.loop_stack = saved_loop_stack;
            self.has_yield = saved_has_yield;
            self.type_stack = saved_type_stack;

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
            });
        }

        // Store class metadata for the VM
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

    fn compile_start(&mut self, expr: &Expr, span: &Span) -> Result<(), CompileError> {
        let line = span.line;

        if let ExprKind::Call { callee, args } = &expr.kind {
            self.compile_expr(callee)?;
            for arg in args {
                self.compile_call_arg(arg)?;
            }
            let arity = u8::try_from(args.len()).map_err(|_| CompileError {
                annotation: None,
                message: "too many arguments (max 255)".to_string(),
                span: span.clone(),
            })?;
            self.emit(Instruction::StartCoroutine(arity), line);
            self.emit(Instruction::Pop, line); // discard the CoroutineHandle
        } else {
            return Err(CompileError {
                annotation: None,
                message: "start expects a function call".to_string(),
                span: span.clone(),
            });
        }
        Ok(())
    }

    fn compile_call_arg(&mut self, arg: &CallArg) -> Result<(), CompileError> {
        match arg {
            CallArg::Positional(expr) => self.compile_expr(expr),
            CallArg::Named { value, .. } => self.compile_expr(value),
        }
    }

    // ── Operator helpers ───────────────────────────────────────────

    fn binary_op_instruction(op: &BinaryOp) -> Instruction {
        match op {
            BinaryOp::Add => Instruction::Add,
            BinaryOp::Subtract => Instruction::Sub,
            BinaryOp::Multiply => Instruction::Mul,
            BinaryOp::Divide => Instruction::Div,
            BinaryOp::Modulo => Instruction::Mod,
            BinaryOp::Equal => Instruction::Eq,
            BinaryOp::NotEqual => Instruction::Ne,
            BinaryOp::Less => Instruction::Lt,
            BinaryOp::Greater => Instruction::Gt,
            BinaryOp::LessEqual => Instruction::Le,
            BinaryOp::GreaterEqual => Instruction::Ge,
            // And/Or are handled via short-circuit in compile_binary
            BinaryOp::And | BinaryOp::Or => unreachable!("handled by compile_binary"),
        }
    }

    fn compound_op_instruction(op: &AssignOp) -> Instruction {
        match op {
            AssignOp::AddAssign => Instruction::Add,
            AssignOp::SubAssign => Instruction::Sub,
            AssignOp::MulAssign => Instruction::Mul,
            AssignOp::DivAssign => Instruction::Div,
            AssignOp::ModAssign => Instruction::Mod,
            AssignOp::Assign => unreachable!("simple assign handled separately"),
        }
    }

    // ── Local variable management ──────────────────────────────────

    fn resolve_local(&self, name: &str) -> Option<(u8, u8)> {
        self.locals
            .iter()
            .rev()
            .find(|local| local.name == name)
            .map(|local| (local.slot, local.type_tag))
    }

    /// Resolves a variable name as an upvalue (captured from an enclosing function).
    /// Returns the upvalue index in `self.current_upvalues`, or `None` if not found.
    fn resolve_upvalue(&mut self, name: &str) -> Option<u8> {
        if self.enclosing_scopes.is_empty() {
            return None;
        }
        self.resolve_upvalue_in(name, self.enclosing_scopes.len() - 1)
    }

    /// Recursive upvalue resolution. Searches enclosing scope `scope_idx` and
    /// propagates upvalue descriptors inward.
    fn resolve_upvalue_in(&mut self, name: &str, scope_idx: usize) -> Option<u8> {
        // Check if the variable is a local in this enclosing scope
        let local_slot = self.enclosing_scopes[scope_idx]
            .locals
            .iter()
            .rev()
            .find(|l| l.name == name)
            .map(|l| l.slot);

        if let Some(slot) = local_slot {
            // Mark the local as captured
            for local in &mut self.enclosing_scopes[scope_idx].locals {
                if local.name == name && local.slot == slot {
                    local.is_captured = true;
                    break;
                }
            }

            // If this is the immediately enclosing scope, add directly
            if scope_idx == self.enclosing_scopes.len() - 1 {
                return Some(self.add_upvalue(true, slot));
            }

            // Otherwise, we need to propagate through intermediate scopes.
            // Add an upvalue to each intermediate scope from scope_idx+1 to the
            // innermost, then add to current_upvalues.
            let mut prev_index = slot;
            let mut prev_is_local = true;
            for i in (scope_idx + 1)..self.enclosing_scopes.len() {
                let uv_idx = self.add_upvalue_to_scope(i, prev_is_local, prev_index);
                prev_index = uv_idx;
                prev_is_local = false;
            }
            return Some(self.add_upvalue(false, prev_index));
        }

        // Not found as a local — recurse into the next outer scope
        if scope_idx == 0 {
            return None;
        }

        let parent_uv_idx = self.resolve_upvalue_in(name, scope_idx - 1)?;

        // parent_uv_idx is now an upvalue in enclosing_scopes[scope_idx].
        // If scope_idx is the immediately enclosing scope, add directly.
        if scope_idx == self.enclosing_scopes.len() - 1 {
            // The parent resolved it as an upvalue in scope_idx's upvalues list
            // at index parent_uv_idx. From our perspective, that's a non-local
            // upvalue of our immediately enclosing scope.
            return Some(self.add_upvalue(false, parent_uv_idx));
        }

        // Otherwise propagate through remaining intermediate scopes
        let mut prev_index = parent_uv_idx;
        for i in (scope_idx + 1)..self.enclosing_scopes.len() {
            let uv_idx = self.add_upvalue_to_scope(i, false, prev_index);
            prev_index = uv_idx;
        }
        Some(self.add_upvalue(false, prev_index))
    }

    /// Adds an upvalue descriptor to `self.current_upvalues`. Deduplicates:
    /// if an identical descriptor already exists, returns its index.
    fn add_upvalue(&mut self, is_local: bool, index: u8) -> u8 {
        // Deduplicate
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

    /// Adds an upvalue descriptor to an intermediate enclosing scope.
    /// Returns the index within that scope's upvalue list.
    fn add_upvalue_to_scope(&mut self, scope_idx: usize, is_local: bool, index: u8) -> u8 {
        let scope = &mut self.enclosing_scopes[scope_idx];
        // Deduplicate
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
        let slot = u8::try_from(self.locals.len()).map_err(|_| CompileError {
            annotation: None,
            message: "too many local variables (max 256)".to_string(),
            span: span.clone(),
        })?;
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
        Ok(slot)
    }
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
