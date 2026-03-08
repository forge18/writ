pub(super) use crate::lexer::Span;
pub(super) use std::collections::{HashMap, HashSet};

pub(super) use crate::parser::{
    ArrayElement, AssignOp, BinaryOp, CallArg, ClassDecl, DictElement, ElseBranch, Expr, ExprKind,
    FuncDecl, InterpolationSegment, Literal, Stmt, StmtKind, StructDecl, UnaryOp, Visibility,
    WhenBody, WhenPattern,
};
pub(super) use crate::types::TypedStmt;

pub(super) use super::chunk::Chunk;
pub(super) use super::error::CompileError;
pub(super) use super::instruction::Instruction;
pub(super) use super::local::Local;
pub(super) use super::opcode::op;
pub(super) use super::upvalue::UpvalueDescriptor;

/// Creates a dummy Span for compiler-generated code (e.g., collection literals).
pub(super) fn dummy_span(line: u32) -> Span {
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
pub(super) struct LoopContext {
    /// Instruction offset of the loop start (for continue).
    start_offset: usize,
    /// Indices of break jump instructions to patch when loop ends.
    break_jumps: Vec<usize>,
    /// Scope depth at loop entry (for unwinding locals on break/continue).
    scope_depth: u32,
}

/// Saved state from an enclosing function scope, used for upvalue resolution.
pub(super) struct EnclosingScope {
    locals: Vec<Local>,
    upvalues: Vec<UpvalueDescriptor>,
}

/// Compile-time type tag for typed instruction emission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ExprType {
    Int,
    Float,
    Bool,
    Other,
}

/// Convert a parsed type annotation to a compile-time type tag.
pub(super) fn type_expr_to_expr_type(ty: &crate::parser::TypeExpr) -> ExprType {
    match ty {
        crate::parser::TypeExpr::Simple(name) => match name.as_str() {
            "int" => ExprType::Int,
            "float" => ExprType::Float,
            "bool" => ExprType::Bool,
            _ => ExprType::Other,
        },
        _ => ExprType::Other,
    }
}

/// Convert a checker-inferred [`crate::types::Type`] to the compiler's `ExprType` tag.
pub(super) fn checked_type_to_expr_type(ty: &crate::types::Type) -> ExprType {
    match ty {
        crate::types::Type::Int => ExprType::Int,
        crate::types::Type::Float => ExprType::Float,
        crate::types::Type::Bool => ExprType::Bool,
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
    /// Native function name → index in VM's `native_fn_vec` for `CallNative` dispatch.
    native_index: HashMap<String, u32>,
    /// Next available register slot.
    next_reg: u8,
    /// High-water mark of registers used (becomes max_registers).
    max_reg: u8,
    /// The class whose methods are currently being compiled (used for super dispatch).
    current_class: Option<String>,
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
            native_index: HashMap::new(),
            next_reg: 0,
            max_reg: 0,
            current_class: None,
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

    /// Sets the native function index map from the VM, enabling `CallNative` emission.
    pub fn set_native_index(&mut self, index: HashMap<String, u32>) {
        self.native_index = index;
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
}

mod closures;
mod exprs;
mod stmts;
mod types;

// Nested struct definition not allowed in impl block — define at module level
pub(super) struct SavedState {
    pub(super) chunk: Chunk,
    pub(super) locals: Vec<Local>,
    pub(super) scope_depth: u32,
    pub(super) loop_stack: Vec<LoopContext>,
    pub(super) has_yield: bool,
    pub(super) reg_types: Vec<ExprType>,
    pub(super) next_reg: u8,
    pub(super) max_reg: u8,
}

pub(super) fn expr_type_to_tag(t: ExprType) -> u8 {
    match t {
        ExprType::Other => 0,
        ExprType::Int => 1,
        ExprType::Float => 2,
        ExprType::Bool => 3,
    }
}

pub(super) fn tag_to_expr_type(tag: u8) -> ExprType {
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
