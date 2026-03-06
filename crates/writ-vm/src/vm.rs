use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use writ_compiler::{Chunk, ClassMeta, CompiledFunction, Instruction, StructMeta, string_hash};

use crate::coroutine::{Coroutine, CoroutineId, CoroutineState, WaitCondition};
use crate::debug::{
    BreakpointAction, BreakpointContext, BreakpointHandler, BreakpointKey, CallHook, LineHook,
    StepState,
};
use crate::error::{RuntimeError, StackFrame, StackTrace};
use crate::frame::{CallFrame, ChunkId};
use crate::native::{NativeFunction, NativeMethod};
use crate::object::WritObject;
use crate::value::{ClosureData, Value, ValueTag};

/// Internal result type for the run loop, distinguishing normal returns
/// from coroutine yield points.
enum RunResult {
    /// Normal return (function completed or end of script).
    Return(Value),
    /// Coroutine yielded with a wait condition.
    Yield(WaitCondition),
}

/// The Writ bytecode virtual machine.
///
/// Executes compiled bytecode chunks produced by the Writ compiler.
/// Supports function calls, local variables, control flow, and collections.
// Hot-field layout: `#[repr(C)]` pins field order so the dispatch loop's
// most-touched fields land on the first two 64-byte cache lines.
//
// Cache line 0 (0–63):   stack(24) + frames(24) + instruction_count(8)
//                         + has_debug_hooks(1) + has_open_upvalues(1) + pad(6) = 64
// Cache line 1 (64–127): func_ip_cache(24) + functions(24) + instruction_limit(16) = 64
// Cache line 2 (128–175): open_upvalues(24) + main_chunk (starts here, cold)
// Cache lines 3+:        cold fields (maps, debug state, coroutines…)
#[repr(C)]
pub struct VM {
    // ── Cache line 0: every-instruction hot fields ───────────────
    /// The operand stack.
    stack: Vec<Value>,
    /// The call stack.
    frames: Vec<CallFrame>,
    /// Current instruction counter, reset per `execute_program` call.
    instruction_count: u64,
    /// Fast-path guard: true when any debug hook or breakpoint is active.
    has_debug_hooks: bool,
    /// Cached flag: true when any open upvalue exists. Avoids scanning the
    /// vec on every LoadLocal/StoreLocal.
    has_open_upvalues: bool,

    // ── Cache line 1: call/return hot fields ─────────────────────
    /// Pre-computed instruction pointer + length for each function chunk.
    /// Index 0 = main chunk, index 1..N = function chunks.
    /// Avoids `chunk_for().instructions()` chain on every Call/Return.
    func_ip_cache: Vec<(*const Instruction, usize)>,
    /// Function table: compiled functions indexed by position.
    functions: Vec<CompiledFunction>,
    /// Maximum number of instructions before termination. `None` = unlimited.
    instruction_limit: Option<u64>,

    // ── Cache line 2: warm fields (closures) ─────────────────────
    /// Open upvalues: indexed by absolute stack slot. `Some(cell)` when a
    /// local has been captured. Direct indexing eliminates HashMap hashing.
    open_upvalues: Vec<Option<Rc<RefCell<Value>>>>,

    // ── Cold fields: setup, metadata, debug ──────────────────────
    /// The top-level/main chunk being executed.
    main_chunk: Chunk,
    /// Fast lookup: function name → index in `functions`.
    function_map: HashMap<String, usize>,
    /// Reverse lookup: field name hash → original string name.
    /// Built from chunk string pools at load time.
    field_names: HashMap<u32, String>,
    /// Host-registered native functions, keyed by name.
    native_functions: HashMap<String, NativeFunction>,
    /// Host-registered methods on value types, keyed by (value tag, method name hash).
    methods: HashMap<(ValueTag, u32), NativeMethod>,
    /// Global constants/variables, keyed by name hash → (name, value).
    globals: HashMap<u32, (String, Value)>,
    /// Struct metadata for constructing struct instances at runtime.
    struct_metas: HashMap<String, StructMeta>,
    /// Class metadata for constructing class instances at runtime.
    class_metas: HashMap<String, ClassMeta>,
    /// Pre-built shared field layouts for struct types (hash→index maps).
    struct_layouts: HashMap<String, Rc<crate::field_layout::FieldLayout>>,
    /// Pre-built shared field layouts for class types (hash→index maps).
    class_layouts: HashMap<String, Rc<crate::field_layout::FieldLayout>>,
    /// Set of disabled module names (blocks native function calls).
    disabled_modules: HashSet<String>,
    /// All active coroutines managed by the scheduler.
    coroutines: Vec<Coroutine>,
    /// Next coroutine ID to assign.
    next_coroutine_id: CoroutineId,
    /// Index of the coroutine currently being executed (None = main script).
    active_coroutine: Option<usize>,
    /// Active breakpoint locations.
    breakpoints: HashSet<BreakpointKey>,
    /// Callback invoked when a breakpoint is hit.
    breakpoint_handler: Option<BreakpointHandler>,
    /// Debug hook: called before each new source line executes.
    on_line_hook: Option<LineHook>,
    /// Debug hook: called when a function is entered.
    on_call_hook: Option<CallHook>,
    /// Debug hook: called when a function returns.
    on_return_hook: Option<CallHook>,
    /// Current stepping state for the debugger.
    step_state: StepState,
    /// Last line number seen (for detecting line changes).
    last_line: u32,
    /// Last file path seen (for detecting line changes).
    last_file: String,
}

impl VM {
    /// Creates a new VM with empty state.
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(256),
            frames: Vec::with_capacity(64),
            main_chunk: Chunk::new(),
            functions: Vec::new(),
            function_map: HashMap::new(),
            field_names: HashMap::new(),
            native_functions: HashMap::new(),
            methods: HashMap::new(),
            globals: HashMap::new(),
            struct_metas: HashMap::new(),
            class_metas: HashMap::new(),
            struct_layouts: HashMap::new(),
            class_layouts: HashMap::new(),
            disabled_modules: HashSet::new(),
            instruction_limit: None,
            instruction_count: 0,
            coroutines: Vec::new(),
            next_coroutine_id: 1,
            active_coroutine: None,
            breakpoints: HashSet::new(),
            breakpoint_handler: None,
            on_line_hook: None,
            on_call_hook: None,
            on_return_hook: None,
            step_state: StepState::None,
            last_line: 0,
            last_file: String::new(),
            has_debug_hooks: false,
            open_upvalues: Vec::new(),
            has_open_upvalues: false,
            func_ip_cache: Vec::new(),
        }
    }

    /// Registers a typed Rust function callable from Writ scripts.
    ///
    /// The function's parameter and return types must implement [`FromValue`]
    /// and [`IntoValue`] respectively. Arity is inferred from the function
    /// signature — no manual arity argument is needed.
    pub fn register_fn<H>(&mut self, name: &str, handler: H) -> &mut Self
    where
        H: crate::binding::IntoNativeHandler,
    {
        self.native_functions.insert(
            name.to_string(),
            NativeFunction::from_handler(name, None, handler),
        );
        self
    }

    /// Registers a typed native function within a named module.
    ///
    /// Functions in a disabled module (via [`disable_module`](Self::disable_module))
    /// will produce a [`RuntimeError`] when called.
    pub fn register_fn_in_module<H>(&mut self, name: &str, module: &str, handler: H) -> &mut Self
    where
        H: crate::binding::IntoNativeHandler,
    {
        self.native_functions.insert(
            name.to_string(),
            NativeFunction::from_handler(name, Some(module), handler),
        );
        self
    }

    /// Registers a typed native method on a specific value type.
    ///
    /// The handler receives the receiver as its first typed argument, followed
    /// by the method's arguments. Arity is inferred from the handler's type.
    /// Use `module` to associate the method with a disableable module.
    pub fn register_method<H>(
        &mut self,
        tag: ValueTag,
        name: &str,
        module: Option<&str>,
        handler: H,
    ) -> &mut Self
    where
        H: crate::binding::IntoNativeMethodHandler,
    {
        let hash = string_hash(name);
        self.methods.insert(
            (tag, hash),
            NativeMethod::from_handler(name, module, handler),
        );
        // Store the method name for reverse lookup
        self.field_names.insert(hash, name.to_string());
        self
    }

    /// Registers a global constant or variable accessible by name from scripts.
    pub fn register_global(&mut self, name: &str, value: Value) -> &mut Self {
        let hash = string_hash(name);
        self.globals.insert(hash, (name.to_string(), value));
        self
    }

    /// Registers a host type with a factory function that creates instances.
    ///
    /// The factory receives the call arguments and returns a `WritObject`.
    /// Scripts can then call `TypeName(args...)` to create instances.
    pub fn register_type<F>(&mut self, name: &str, factory: F) -> &mut Self
    where
        F: Fn(&[Value]) -> Result<Box<dyn WritObject>, String> + 'static,
    {
        let name_owned = name.to_string();
        self.native_functions.insert(
            name.to_string(),
            NativeFunction {
                name: name.to_string(),
                module: None,
                arity: None,
                body: Rc::new(move |args| {
                    let obj = factory(args).map_err(|e| format!("{}: {}", name_owned, e))?;
                    Ok(Value::Object(Rc::new(RefCell::new(obj))))
                }),
            },
        );
        self
    }

    /// Disables a standard library module, blocking all native function
    /// calls registered under that module name.
    pub fn disable_module(&mut self, module: &str) -> &mut Self {
        self.disabled_modules.insert(module.to_string());
        self
    }

    /// Sets the maximum number of instructions the VM will execute
    /// before returning an error. Used to prevent infinite loops in
    /// untrusted scripts.
    pub fn set_instruction_limit(&mut self, limit: u64) -> &mut Self {
        self.instruction_limit = Some(limit);
        self
    }

    // ── Debug API ───────────────────────────────────────────────────

    /// Registers a breakpoint at the given file and line.
    pub fn set_breakpoint(&mut self, file: &str, line: u32) -> &mut Self {
        self.breakpoints.insert(BreakpointKey {
            file: file.to_string(),
            line,
        });
        self.update_debug_flag();
        self
    }

    /// Removes a breakpoint at the given file and line.
    pub fn remove_breakpoint(&mut self, file: &str, line: u32) -> &mut Self {
        self.breakpoints.remove(&BreakpointKey {
            file: file.to_string(),
            line,
        });
        self.update_debug_flag();
        self
    }

    /// Registers a callback invoked when a breakpoint is hit.
    pub fn on_breakpoint<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&BreakpointContext) -> BreakpointAction + 'static,
    {
        self.breakpoint_handler = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Registers a hook called before each new source line executes.
    pub fn on_line<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, u32) + 'static,
    {
        self.on_line_hook = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Registers a hook called when any function is entered.
    pub fn on_call<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, &str, u32) + 'static,
    {
        self.on_call_hook = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Registers a hook called when any function returns.
    pub fn on_return<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, &str, u32) + 'static,
    {
        self.on_return_hook = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Recomputes the fast-path debug flag.
    fn update_debug_flag(&mut self) {
        self.has_debug_hooks = !self.breakpoints.is_empty()
            || self.breakpoint_handler.is_some()
            || self.on_line_hook.is_some()
            || self.on_call_hook.is_some()
            || self.on_return_hook.is_some();
    }

    /// Recompiles a source string and swaps function bytecode in-place.
    ///
    /// Only function bodies are swapped; the main chunk is not replaced
    /// (it has already executed). If compilation fails, the existing
    /// bytecode is preserved and the error is returned.
    pub fn reload(&mut self, file: &str, source: &str) -> Result<(), String> {
        // 1. Lex
        let mut lexer = writ_lexer::Lexer::new(source);
        let tokens = lexer.tokenize().map_err(|e| format!("{e}"))?;

        // 2. Parse
        let mut parser = writ_parser::Parser::new(tokens);
        let stmts = parser.parse_program().map_err(|e| format!("{e}"))?;

        // 3. Compile
        let mut compiler = writ_compiler::Compiler::new();
        for stmt in &stmts {
            compiler.compile_stmt(stmt).map_err(|e| format!("{e}"))?;
        }
        let (_new_main, new_functions, new_struct_metas, new_class_metas) = compiler.into_parts();

        // 4. Load struct metadata
        for meta in new_struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta);
        }

        // 4b. Load class metadata
        for meta in new_class_metas {
            self.class_metas.insert(meta.name.clone(), meta);
        }

        // 5. Swap matching functions, add new ones
        for mut new_func in new_functions {
            new_func.chunk.set_file(file);
            if let Some(&idx) = self.function_map.get(&new_func.name) {
                self.functions[idx] = new_func;
            } else {
                let idx = self.functions.len();
                self.function_map.insert(new_func.name.clone(), idx);
                self.functions.push(new_func);
            }
        }

        // 6. Rebuild field layouts for reloaded struct/class types
        self.build_layouts();

        // 7. Rebuild the instruction pointer cache so it points to the new chunks
        self.rebuild_ip_cache();

        Ok(())
    }

    /// Loads a module's compiled functions into the VM without clearing
    /// existing state.
    ///
    /// Unlike [`execute_program`](VM::execute_program), this does not reset
    /// the function table or other VM state. New functions are appended to the
    /// existing table, and the module's top-level chunk is executed for side
    /// effects (e.g. global variable initialization).
    pub fn load_module(
        &mut self,
        chunk: &Chunk,
        functions: &[CompiledFunction],
        struct_metas: &[StructMeta],
        class_metas: &[ClassMeta],
    ) -> Result<Value, RuntimeError> {
        // Append new functions (preserving existing ones)
        for func in functions {
            let idx = self.functions.len();
            self.function_map.insert(func.name.clone(), idx);
            self.functions.push(func.clone());
        }

        // Load struct metadata
        for meta in struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta.clone());
        }

        // Load class metadata
        for meta in class_metas {
            self.class_metas.insert(meta.name.clone(), meta.clone());
        }

        // Build field name reverse lookup for new strings
        for s in chunk.rc_strings() {
            self.field_names
                .insert(string_hash(s), s.as_str().to_string());
        }
        for func in functions {
            for s in func.chunk.rc_strings() {
                self.field_names
                    .insert(string_hash(s), s.as_str().to_string());
            }
        }

        // Build shared field layouts for new struct/class types
        self.build_layouts();

        // Execute the module's top-level code
        self.stack.clear();
        self.frames.clear();
        self.instruction_count = 0;
        self.main_chunk = chunk.clone();

        // Rebuild instruction pointer cache with all functions
        self.rebuild_ip_cache();

        let main_max = self.main_chunk.instructions().len().min(255) as u8;
        self.ensure_registers(0, main_max.max(16));
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Main,
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: main_max.max(16),
            has_rc_values: true,
            upvalues: None,
        });

        match self.run()? {
            RunResult::Return(value) => Ok(value),
            RunResult::Yield(_) => {
                Err(self.make_error("yield outside of coroutine in module".to_string()))
            }
        }
    }

    /// Calls a named function without resetting VM state.
    ///
    /// Used for hot reload testing, host-to-script function calls, and
    /// callback methods like `map`/`filter`/`reduce`.
    pub fn call_function(&mut self, name: &str, args: &[Value]) -> Result<Value, RuntimeError> {
        let func_idx = *self
            .function_map
            .get(name)
            .ok_or_else(|| self.make_error(format!("function '{name}' not found")))?;

        let expected_arity = self.functions[func_idx].arity;
        if args.len() != expected_arity as usize {
            return Err(self.make_error(format!(
                "function '{name}' expects {expected_arity} arguments, got {}",
                args.len()
            )));
        }

        let saved_instruction_count = self.instruction_count;
        let return_depth = self.frames.len();
        let base = self.stack.len();
        let max_regs = self.functions[func_idx].max_registers;
        for arg in args {
            self.stack.push(arg.clone());
        }
        self.ensure_registers(base, max_regs);

        // Result goes into a temporary slot after the frame
        let result_slot = base; // We'll read it from R(0) after return
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base,
            result_reg: result_slot,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values,
            upvalues: None,
        });

        match self.run_until(return_depth)? {
            RunResult::Return(value) => {
                self.instruction_count = saved_instruction_count;
                Ok(value)
            }
            RunResult::Yield(_) => {
                Err(self.make_error("unexpected yield in call_function".to_string()))
            }
        }
    }

    /// Calls a compiled function by index with the given arguments.
    fn call_compiled_function(
        &mut self,
        func_idx: usize,
        args: &[Value],
    ) -> Result<Value, RuntimeError> {
        let saved_instruction_count = self.instruction_count;
        let return_depth = self.frames.len();
        let base = self.stack.len();
        let max_regs = self.functions[func_idx].max_registers;
        for arg in args {
            self.stack.push(arg.clone());
        }
        self.ensure_registers(base, max_regs);

        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base,
            result_reg: base,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values,
            upvalues: None,
        });

        match self.run_until(return_depth)? {
            RunResult::Return(value) => {
                self.instruction_count = saved_instruction_count;
                Ok(value)
            }
            RunResult::Yield(_) => {
                Err(self.make_error("unexpected yield in method call".to_string()))
            }
        }
    }

    /// Executes a chunk as the top-level script (no functions).
    pub fn execute(&mut self, chunk: &Chunk) -> Result<Value, RuntimeError> {
        self.execute_program(chunk, &[], &[], &[])
    }

    /// Executes a compiled program (main chunk + function table).
    pub fn execute_program(
        &mut self,
        chunk: &Chunk,
        functions: &[CompiledFunction],
        struct_metas: &[StructMeta],
        class_metas: &[ClassMeta],
    ) -> Result<Value, RuntimeError> {
        // Reset per-execution state (preserves registrations and coroutines)
        self.stack.clear();
        self.frames.clear();
        self.function_map.clear();
        self.field_names.clear();
        self.struct_metas.clear();
        self.class_metas.clear();
        self.struct_layouts.clear();
        self.class_layouts.clear();
        self.instruction_count = 0;
        self.coroutines.clear();
        self.next_coroutine_id = 1;
        self.active_coroutine = None;

        // Load the main chunk and functions
        self.main_chunk = chunk.clone();
        self.functions = functions.to_vec();

        // Build function name → index map
        for (i, func) in self.functions.iter().enumerate() {
            self.function_map.insert(func.name.clone(), i);
        }

        // Load struct metadata
        for meta in struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta.clone());
        }

        // Load class metadata
        for meta in class_metas {
            self.class_metas.insert(meta.name.clone(), meta.clone());
        }

        // Build field name reverse lookup from all string pools
        self.build_field_names();

        // Build shared field layouts for struct/class types
        self.build_layouts();

        // Pre-compute instruction pointer cache for fast Call/Return.
        self.rebuild_ip_cache();

        // Push the top-level frame
        // For the main chunk, we use a generous register count since the compiler
        // doesn't track max_registers for the main chunk (it's always the script body).
        let main_max = 128u8; // generous default for top-level scripts
        self.ensure_registers(0, main_max);
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Main,
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: main_max,
            has_rc_values: true,
            upvalues: None,
        });

        match self.run()? {
            RunResult::Return(value) => Ok(value),
            RunResult::Yield(_) => Err(self.make_error("yield outside of coroutine".to_string())),
        }
    }

    /// Builds the reverse hash → name lookup from all chunk string pools.
    fn build_field_names(&mut self) {
        for s in self.main_chunk.rc_strings() {
            self.field_names
                .insert(string_hash(s), s.as_str().to_string());
        }
        for func in &self.functions {
            for s in func.chunk.rc_strings() {
                self.field_names
                    .insert(string_hash(s), s.as_str().to_string());
            }
        }
    }

    /// Builds shared `FieldLayout` objects from struct/class metadata.
    /// Called after loading metas so that construction and field access
    /// can use index-based `Vec<Value>` instead of `HashMap<String, Value>`.
    fn build_layouts(&mut self) {
        use crate::field_layout::FieldLayout;
        for (name, meta) in &self.struct_metas {
            let layout = Rc::new(FieldLayout::new(
                name.clone(),
                meta.field_names.clone(),
                meta.public_fields.clone(),
                meta.public_methods.clone(),
            ));
            self.struct_layouts.insert(name.clone(), layout);
        }
        for (name, meta) in &self.class_metas {
            let layout = Rc::new(FieldLayout::new(
                name.clone(),
                meta.field_names.clone(),
                meta.public_fields.clone(),
                meta.public_methods.clone(),
            ));
            self.class_layouts.insert(name.clone(), layout);
        }
    }

    /// Rebuilds the instruction pointer cache for all chunks.
    /// Must be called after main_chunk or functions are modified.
    fn rebuild_ip_cache(&mut self) {
        self.func_ip_cache.clear();
        let main_instrs = self.main_chunk.instructions();
        self.func_ip_cache
            .push((main_instrs.as_ptr(), main_instrs.len()));
        for func in &self.functions {
            let instrs = func.chunk.instructions();
            self.func_ip_cache.push((instrs.as_ptr(), instrs.len()));
        }
    }

    /// Returns a reference to the chunk identified by `id`.
    #[inline(always)]
    fn chunk_for(&self, id: ChunkId) -> &Chunk {
        match id {
            ChunkId::Main => &self.main_chunk,
            ChunkId::Function(idx) => &self.functions[idx].chunk,
        }
    }

    /// Returns cached (ip_base, len) for the given chunk. Avoids the
    /// `chunk_for().instructions().as_ptr()` chain on Call/Return hot paths.
    #[inline(always)]
    fn cached_ip(&self, id: ChunkId) -> (*const Instruction, usize) {
        let idx = match id {
            ChunkId::Main => 0,
            ChunkId::Function(i) => i + 1,
        };
        // SAFETY: func_ip_cache is populated for all chunks at load time
        unsafe { *self.func_ip_cache.get_unchecked(idx) }
    }

    /// Returns the current (topmost) call frame.
    #[inline(always)]
    fn current_frame(&self) -> &CallFrame {
        self.frames.last().expect("call stack is empty")
    }

    /// Ensures the stack is large enough for the given frame.
    #[inline(always)]
    fn ensure_registers(&mut self, base: usize, max_regs: u8) {
        let needed = base + max_regs as usize;
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::Null);
        }
    }

    /// Constructs a RuntimeError with a stack trace from the current call stack.
    #[cold]
    #[inline(never)]
    fn make_error(&self, message: String) -> RuntimeError {
        RuntimeError {
            message,
            trace: self.build_stack_trace(),
        }
    }

    /// Builds a stack trace from the current call stack.
    fn build_stack_trace(&self) -> StackTrace {
        let mut frames = Vec::new();
        for frame in self.frames.iter().rev() {
            let chunk = self.chunk_for(frame.chunk_id);
            let line = if frame.pc > 0 && frame.pc - 1 < chunk.len() {
                chunk.line(frame.pc - 1)
            } else {
                0
            };
            frames.push(StackFrame {
                function: display_function_name(frame.func_index(), &self.functions),
                file: chunk.file().unwrap_or("").to_string(),
                line,
                is_native: false,
            });
        }
        StackTrace { frames }
    }

    /// The main execution loop. Returns when the frame stack is empty.
    fn run(&mut self) -> Result<RunResult, RuntimeError> {
        self.run_until(0)
    }

    /// Execution loop that returns when the frame stack depth drops to `return_depth`.
    fn run_until(&mut self, return_depth: usize) -> Result<RunResult, RuntimeError> {
        // Cache frame data in local variables to avoid repeated frames.last() calls.
        // Must sync ip before Call/Return/yield and reload after frame changes.
        let frame = self.frames.last().unwrap();
        let mut base = frame.base;
        let mut chunk_id = frame.chunk_id;
        let has_limit = self.instruction_limit.is_some();

        // Auto-advancing instruction pointer. SAFETY: instruction slices are not
        // mutated during execution, and ip is reloaded after every Call/Return.
        // ip_base is kept for deriving integer pc (error paths, frame save).
        let (cached_base, cached_len) = self.cached_ip(chunk_id);
        let mut ip_base: *const Instruction = cached_base;
        let mut ip: *const Instruction = unsafe { ip_base.add(frame.pc) };
        let mut ip_end: *const Instruction = unsafe { ip_base.add(cached_len) };

        // Macro to reload ip state from a frame using the cached instruction
        // pointer table. Avoids `chunk_for().instructions()` on every Call/Return.
        macro_rules! reload_ip {
            ($self:expr, $chunk_id:expr, $pc:expr) => {{
                let (base_ptr, len) = $self.cached_ip($chunk_id);
                ip_base = base_ptr;
                ip = unsafe { ip_base.add($pc) };
                ip_end = unsafe { ip_base.add(len) };
            }};
        }

        loop {
            // Instruction limit enforcement (batch check every 256 instructions)
            if has_limit {
                self.instruction_count += 1;
                if self.instruction_count & 0xFF == 0
                    && self.instruction_count > self.instruction_limit.unwrap()
                {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    return Err(self.make_error("instruction limit exceeded".to_string()));
                }
            }

            // End of chunk: implicit return null
            if ip >= ip_end {
                if self.has_debug_hooks {
                    self.fire_return_hook();
                }
                let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                if self.has_open_upvalues {
                    self.close_upvalues_above(frame.base);
                }
                let result_reg = frame.result_reg;
                if self.frames.len() <= return_depth {
                    // Top-level frame: return value in register 0
                    let result = if frame.base < self.stack.len() {
                        std::mem::replace(&mut self.stack[frame.base], Value::Null)
                    } else {
                        Value::Null
                    };
                    if frame.has_rc_values {
                        self.stack.truncate(frame.base);
                    } else {
                        unsafe { self.stack.set_len(frame.base) };
                    }
                    return Ok(RunResult::Return(result));
                }
                // Write null to caller's result register, truncate frame
                if result_reg < self.stack.len() {
                    self.stack[result_reg] = Value::Null;
                }
                let caller = unsafe { self.frames.last().unwrap_unchecked() };
                let caller_top = caller.base + caller.max_registers as usize;
                if frame.has_rc_values {
                    self.stack.truncate(caller_top);
                } else {
                    unsafe { self.stack.set_len(caller_top) };
                }
                base = caller.base;
                chunk_id = caller.chunk_id;
                reload_ip!(self, chunk_id, caller.pc);
                continue;
            }

            // Debug probe: check for line changes, breakpoints, and step state
            if self.has_debug_hooks {
                let pc = unsafe { ip.offset_from(ip_base) as usize };
                self.frames.last_mut().unwrap().pc = pc;
                self.debug_probe(chunk_id, pc)?;
            }

            // SAFETY: ip < ip_end is guaranteed by the check above.
            let instruction = unsafe { *ip };
            ip = unsafe { ip.add(1) };

            match instruction {
                // ── Literals ──────────────────────────────────────────
                Instruction::LoadInt(dst, v) => {
                    self.stack[base + dst as usize] = Value::I32(v);
                }
                Instruction::LoadConstInt(dst, idx) => {
                    let chunk = self.chunk_for(chunk_id);
                    let v = chunk.int64_constants()[idx as usize];
                    self.stack[base + dst as usize] = Value::I64(v);
                }
                Instruction::LoadFloat(dst, v) => {
                    self.stack[base + dst as usize] = Value::F32(v);
                }
                Instruction::LoadConstFloat(dst, idx) => {
                    let chunk = self.chunk_for(chunk_id);
                    let v = chunk.float64_constants()[idx as usize];
                    self.stack[base + dst as usize] = Value::F64(v);
                }
                Instruction::LoadBool(dst, v) => {
                    self.stack[base + dst as usize] = Value::Bool(v);
                }
                Instruction::LoadStr(dst, idx) => {
                    let chunk = self.chunk_for(chunk_id);
                    let s = Rc::clone(&chunk.rc_strings()[idx as usize]);
                    self.stack[base + dst as usize] = Value::Str(s);
                }
                Instruction::LoadNull(dst) => {
                    self.stack[base + dst as usize] = Value::Null;
                }
                Instruction::Move(dst, src) => {
                    let val = self.stack[base + src as usize].cheap_clone();
                    let abs_dst = base + dst as usize;
                    // Sync to open upvalue cell if destination is captured
                    if self.has_open_upvalues
                        && abs_dst < self.open_upvalues.len()
                        && let Some(cell) = &self.open_upvalues[abs_dst]
                    {
                        *cell.borrow_mut() = val.cheap_clone();
                    }
                    self.stack[abs_dst] = val;
                }
                Instruction::LoadGlobal(dst, name_hash) => {
                    let val = if let Some((_, value)) = self.globals.get(&name_hash) {
                        value.clone()
                    } else {
                        let name = self
                            .field_names
                            .get(&name_hash)
                            .cloned()
                            .or_else(|| {
                                let chunk = self.chunk_for(chunk_id);
                                chunk
                                    .rc_strings()
                                    .iter()
                                    .find(|s| string_hash(s) == name_hash)
                                    .map(|s| s.as_str().to_string())
                            })
                            .unwrap_or_else(|| format!("<unknown:{name_hash}>"));
                        Value::Str(Rc::new(name))
                    };
                    self.stack[base + dst as usize] = val;
                }

                // ── Arithmetic (generic, quickenable) ────────────────
                Instruction::Add(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    // Quicken based on observed operand types
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QAddInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QAddFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_add_reg(base, dst, a, b)?;
                }
                Instruction::Sub(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QSubInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QSubFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_binary_arith_reg(
                        base + dst as usize,
                        base + a as usize,
                        base + b as usize,
                        i32::checked_sub,
                        i64::checked_sub,
                        |x, y| x - y,
                    )?;
                }
                Instruction::Mul(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QMulInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QMulFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_binary_arith_reg(
                        base + dst as usize,
                        base + a as usize,
                        base + b as usize,
                        i32::checked_mul,
                        i64::checked_mul,
                        |x, y| x * y,
                    )?;
                }
                Instruction::Div(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QDivInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QDivFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_div_reg(base, dst, a, b)?;
                }
                Instruction::Mod(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_mod_reg(base, dst, a, b)?;
                }

                // ── Unary ────────────────────────────────────────────
                Instruction::Neg(dst, src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = &self.stack[base + src as usize];
                    let result = match val {
                        Value::I32(v) => match v.checked_neg() {
                            Some(r) => Value::I32(r),
                            None => Value::I64(-(*v as i64)),
                        },
                        Value::I64(v) => Value::I64(v.checked_neg().ok_or_else(|| {
                            self.make_error("integer overflow on negation".to_string())
                        })?),
                        Value::F32(v) => Value::F32(-v),
                        Value::F64(v) => Value::F64(-v),
                        _ => {
                            return Err(
                                self.make_error(format!("cannot negate {}", val.type_name()))
                            );
                        }
                    };
                    self.stack[base + dst as usize] = result;
                }
                Instruction::Not(dst, src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = &self.stack[base + src as usize];
                    match val {
                        Value::Bool(v) => {
                            self.stack[base + dst as usize] = Value::Bool(!v);
                        }
                        _ => {
                            return Err(
                                self.make_error(format!("cannot apply '!' to {}", val.type_name()))
                            );
                        }
                    }
                }

                // ── Type coercion ──────────────────────────────────────
                Instruction::IntToFloat(dst, src) => {
                    let val = &self.stack[base + src as usize];
                    let result = match val {
                        Value::I32(v) => Value::F64(*v as f64),
                        Value::I64(v) => Value::F64(*v as f64),
                        _ => unreachable!("IntToFloat: compiler guarantees int operand"),
                    };
                    self.stack[base + dst as usize] = result;
                }

                // ── Comparison (generic, quickenable) ────────────────
                Instruction::Eq(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QEqInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QEqFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    let eq = self.stack[base + a as usize] == self.stack[base + b as usize];
                    self.stack[base + dst as usize] = Value::Bool(eq);
                }
                Instruction::Ne(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QNeInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QNeFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    let ne = self.stack[base + a as usize] != self.stack[base + b as usize];
                    self.stack[base + dst as usize] = Value::Bool(ne);
                }
                Instruction::Lt(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QLtInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QLtFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(base, dst, a, b, |x, y| x < y, |x, y| x < y)?;
                }
                Instruction::Le(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QLeInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QLeFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(base, dst, a, b, |x, y| x <= y, |x, y| x <= y)?;
                }
                Instruction::Gt(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QGtInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QGtFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(base, dst, a, b, |x, y| x > y, |x, y| x > y)?;
                }
                Instruction::Ge(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    match (a_ref, b_ref) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QGeInt(dst, a, b);
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QGeFloat(dst, a, b);
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(base, dst, a, b, |x, y| x >= y, |x, y| x >= y)?;
                }

                // ── Logical ──────────────────────────────────────────
                Instruction::And(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    match (
                        &self.stack[base + a as usize],
                        &self.stack[base + b as usize],
                    ) {
                        (Value::Bool(av), Value::Bool(bv)) => {
                            self.stack[base + dst as usize] = Value::Bool(*av && *bv);
                        }
                        _ => {
                            return Err(self.make_error(format!(
                                "cannot apply '&&' to {} and {}",
                                self.stack[base + a as usize].type_name(),
                                self.stack[base + b as usize].type_name()
                            )));
                        }
                    }
                }
                Instruction::Or(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    match (
                        &self.stack[base + a as usize],
                        &self.stack[base + b as usize],
                    ) {
                        (Value::Bool(av), Value::Bool(bv)) => {
                            self.stack[base + dst as usize] = Value::Bool(*av || *bv);
                        }
                        _ => {
                            return Err(self.make_error(format!(
                                "cannot apply '||' to {} and {}",
                                self.stack[base + a as usize].type_name(),
                                self.stack[base + b as usize].type_name()
                            )));
                        }
                    }
                }

                // ── Return ───────────────────────────────────────────
                Instruction::Return(src) => {
                    if self.has_debug_hooks {
                        self.fire_return_hook();
                    }
                    let return_value = self.stack[base + src as usize].cheap_clone();
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    if self.has_open_upvalues {
                        self.close_upvalues_above(frame.base);
                    }
                    let result_reg = frame.result_reg;
                    if self.frames.len() <= return_depth {
                        if frame.has_rc_values {
                            self.stack.truncate(frame.base);
                        } else {
                            unsafe { self.stack.set_len(frame.base) };
                        }
                        return Ok(RunResult::Return(return_value));
                    }
                    // Write return value to caller's result register
                    self.stack[result_reg] = return_value;
                    // Restore caller's stack
                    let caller = unsafe { self.frames.last().unwrap_unchecked() };
                    let caller_top = caller.base + caller.max_registers as usize;
                    if frame.has_rc_values {
                        self.stack.truncate(caller_top);
                    } else {
                        unsafe { self.stack.set_len(caller_top) };
                    }
                    base = caller.base;
                    chunk_id = caller.chunk_id;
                    reload_ip!(self, chunk_id, caller.pc);
                }
                Instruction::ReturnNull => {
                    if self.has_debug_hooks {
                        self.fire_return_hook();
                    }
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    if self.has_open_upvalues {
                        self.close_upvalues_above(frame.base);
                    }
                    let result_reg = frame.result_reg;
                    if self.frames.len() <= return_depth {
                        if frame.has_rc_values {
                            self.stack.truncate(frame.base);
                        } else {
                            unsafe { self.stack.set_len(frame.base) };
                        }
                        return Ok(RunResult::Return(Value::Null));
                    }
                    self.stack[result_reg] = Value::Null;
                    let caller = unsafe { self.frames.last().unwrap_unchecked() };
                    let caller_top = caller.base + caller.max_registers as usize;
                    if frame.has_rc_values {
                        self.stack.truncate(caller_top);
                    } else {
                        unsafe { self.stack.set_len(caller_top) };
                    }
                    base = caller.base;
                    chunk_id = caller.chunk_id;
                    reload_ip!(self, chunk_id, caller.pc);
                }

                // ── Jumps ────────────────────────────────────────────
                Instruction::Jump(offset) => {
                    ip = unsafe { ip.offset(offset as isize) };
                }
                Instruction::JumpIfFalsy(src, offset) => {
                    if self.stack[base + src as usize].is_falsy() {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::JumpIfTruthy(src, offset) => {
                    if !self.stack[base + src as usize].is_falsy() {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }

                // ── Function calls ───────────────────────────────────
                Instruction::Call(base_reg, arg_count) => {
                    unsafe { self.frames.last_mut().unwrap_unchecked() }.pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_call_reg(base, base_reg, arg_count)?;
                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = f.chunk_id;
                    reload_ip!(self, chunk_id, f.pc);
                }
                Instruction::CallDirect(base_reg, func_idx_u16, arg_count) => {
                    unsafe { self.frames.last_mut().unwrap_unchecked() }.pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let func_idx = func_idx_u16 as usize;
                    let n = arg_count as usize;
                    let func = &self.functions[func_idx];
                    let max_regs = func.max_registers;
                    let func_has_rc = func.has_rc_values;
                    let result_reg = base + base_reg as usize; // caller's result register

                    if func.is_variadic {
                        let min_args = func.arity.saturating_sub(1);
                        if arg_count < min_args {
                            return Err(self.make_error(format!(
                                "function '{}' expects at least {} arguments, got {}",
                                func.name, min_args, arg_count
                            )));
                        }
                        // Pack variadic args into array
                        let fixed_count = min_args as usize;
                        let variadic_count = n - fixed_count;
                        let arg_start = base + base_reg as usize;
                        let variadic_start = arg_start + fixed_count;
                        let variadic_args: Vec<Value> = (0..variadic_count)
                            .map(|i| self.stack[variadic_start + i].cheap_clone())
                            .collect();
                        // New frame base = arg_start (args already in place)
                        let new_base = arg_start;
                        // Put variadic array after fixed args
                        self.stack[new_base + fixed_count] =
                            Value::Array(Rc::new(RefCell::new(variadic_args)));
                        self.ensure_registers(new_base, max_regs);
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            result_reg,
                            max_registers: max_regs,
                            has_rc_values: true, // variadic creates Array
                            upvalues: None,
                        });
                    } else {
                        if func.arity != arg_count {
                            return Err(self.make_error(format!(
                                "function '{}' expects {} arguments, got {}",
                                func.name, func.arity, arg_count
                            )));
                        }
                        // new_base must be > result_reg so truncate(frame.base) won't
                        // destroy the result. With args, base_reg+0..arity-1 hold args,
                        // but with 0 args new_base == result_reg — so bump by 1.
                        let new_base = if n == 0 {
                            result_reg + 1
                        } else {
                            base + base_reg as usize
                        };
                        self.ensure_registers(new_base, max_regs);
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            result_reg,
                            max_registers: max_regs,
                            has_rc_values: func_has_rc,
                            upvalues: None,
                        });
                    }

                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = ChunkId::Function(func_idx);
                    reload_ip!(self, chunk_id, 0);
                }
                Instruction::TailCallDirect(base_reg, func_idx_u16, arg_count) => {
                    let func_idx = func_idx_u16 as usize;
                    let n = arg_count as usize;
                    let func = &self.functions[func_idx];

                    if func.arity != arg_count {
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        return Err(self.make_error(format!(
                            "function '{}' expects {} arguments, got {}",
                            func.name, func.arity, arg_count
                        )));
                    }

                    let max_regs = func.max_registers;
                    let func_has_rc = func.has_rc_values;

                    // Close upvalues in the current frame before reusing it
                    if self.has_open_upvalues {
                        self.close_upvalues_above(base);
                    }

                    // Shift arguments down to current frame's base
                    let arg_src = base + base_reg as usize;
                    for i in 0..n {
                        self.stack[base + i] = self.stack[arg_src + i].cheap_clone();
                    }

                    // Update current frame in-place
                    let frame = unsafe { self.frames.last_mut().unwrap_unchecked() };
                    frame.chunk_id = ChunkId::Function(func_idx);
                    frame.pc = 0;
                    frame.max_registers = max_regs;
                    frame.has_rc_values = func_has_rc;
                    // frame.base and frame.result_reg stay the same

                    self.ensure_registers(base, max_regs);

                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    chunk_id = ChunkId::Function(func_idx);
                    reload_ip!(self, chunk_id, 0);
                }
                Instruction::CallNative(_base_reg, _id, _arg_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    return Err(self.make_error("native functions not yet supported".to_string()));
                }

                // ── Null handling ────────────────────────────────────
                Instruction::NullCoalesce(dst, a, b) => {
                    let val = &self.stack[base + a as usize];
                    if val.is_null() {
                        let fallback = self.stack[base + b as usize].cheap_clone();
                        self.stack[base + dst as usize] = fallback;
                    } else {
                        let v = val.cheap_clone();
                        self.stack[base + dst as usize] = v;
                    }
                }

                // ── String concatenation ─────────────────────────────
                Instruction::Concat(dst, a, b) => {
                    let lhs = &self.stack[base + a as usize];
                    let rhs = &self.stack[base + b as usize];
                    let result = format!("{lhs}{rhs}");
                    self.stack[base + dst as usize] = Value::Str(Rc::new(result));
                }

                // ── Collections ──────────────────────────────────────
                Instruction::MakeArray(dst, start, count) => {
                    let n = count as usize;
                    let s = base + start as usize;
                    let elements: Vec<Value> =
                        (0..n).map(|i| self.stack[s + i].cheap_clone()).collect();
                    self.stack[base + dst as usize] = Value::Array(Rc::new(RefCell::new(elements)));
                }
                Instruction::MakeDict(dst, start, count) => {
                    let n = count as usize;
                    let s = base + start as usize;
                    let mut map = HashMap::new();
                    for i in 0..n {
                        let key = match &self.stack[s + i * 2] {
                            Value::Str(sk) => (**sk).clone(),
                            other => other.to_string(),
                        };
                        let val = self.stack[s + i * 2 + 1].cheap_clone();
                        map.insert(key, val);
                    }
                    self.stack[base + dst as usize] = Value::Dict(Rc::new(RefCell::new(map)));
                }

                // ── Field/Index access ───────────────────────────────
                Instruction::GetField(dst, obj_reg, name_hash) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_get_field_reg(base, dst, obj_reg, name_hash)?;
                }
                Instruction::SetField(obj_reg, name_hash, val_reg) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_set_field_reg(base, obj_reg, name_hash, val_reg)?;
                }
                Instruction::GetIndex(dst, obj_reg, idx_reg) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_get_index_reg(base, dst, obj_reg, idx_reg)?;
                }
                Instruction::SetIndex(obj_reg, idx_reg, val_reg) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_set_index_reg(base, obj_reg, idx_reg, val_reg)?;
                }

                // ── Coroutines ───────────────────────────────────────
                Instruction::StartCoroutine(base_reg, arg_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_start_coroutine_reg(base, base_reg, arg_count)?;
                }
                Instruction::Yield => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    return Ok(RunResult::Yield(WaitCondition::OneFrame));
                }
                Instruction::YieldSeconds(src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let seconds = &self.stack[base + src as usize];
                    let secs = match seconds {
                        v @ (Value::F32(_) | Value::F64(_)) => v.as_f64(),
                        v @ (Value::I32(_) | Value::I64(_)) => v.as_i64() as f64,
                        _ => {
                            return Err(self.make_error(format!(
                                "waitForSeconds expects a number, got {}",
                                seconds.type_name()
                            )));
                        }
                    };
                    return Ok(RunResult::Yield(WaitCondition::Seconds { remaining: secs }));
                }
                Instruction::YieldFrames(src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let frames_val = &self.stack[base + src as usize];
                    let n = match frames_val {
                        v @ (Value::I32(_) | Value::I64(_)) if v.as_i64() >= 0 => v.as_i64() as u32,
                        _ => {
                            return Err(self.make_error(format!(
                                "waitForFrames expects a non-negative int, got {}",
                                frames_val.type_name()
                            )));
                        }
                    };
                    return Ok(RunResult::Yield(WaitCondition::Frames { remaining: n }));
                }
                Instruction::YieldUntil(src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let predicate = self.stack[base + src as usize].cheap_clone();
                    return Ok(RunResult::Yield(WaitCondition::Until { predicate }));
                }
                Instruction::YieldCoroutine(dst, src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let handle = &self.stack[base + src as usize];
                    let child_id = match handle {
                        Value::CoroutineHandle(id) => *id,
                        _ => {
                            return Err(self.make_error(format!(
                                "yield coroutine expects a coroutine handle, got {}",
                                handle.type_name()
                            )));
                        }
                    };
                    return Ok(RunResult::Yield(WaitCondition::Coroutine {
                        child_id,
                        result_reg: dst,
                    }));
                }

                // ── Spread ───────────────────────────────────────────
                Instruction::Spread(src) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = self.stack[base + src as usize].clone();
                    match val {
                        Value::Array(arr) => {
                            let items = arr.borrow();
                            for item in items.iter() {
                                self.stack.push(item.clone());
                            }
                        }
                        _ => {
                            return Err(
                                self.make_error("spread operator requires an array".to_string())
                            );
                        }
                    }
                }

                // ── Structs ──────────────────────────────────────────
                Instruction::MakeStruct(dst, name_idx, start, field_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_make_struct_reg(base, dst, name_idx, start, field_count, chunk_id)?;
                }

                // ── Classes ─────────────────────────────────────────
                Instruction::MakeClass(dst, name_idx, start, field_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_make_class_reg(base, dst, name_idx, start, field_count, chunk_id)?;
                }

                // ── Method calls ─────────────────────────────────────
                Instruction::CallMethod(base_reg, name_hash, arg_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_call_method_reg(base, base_reg, name_hash, arg_count)?;
                }

                // ── Closures ─────────────────────────────────────────
                Instruction::LoadUpvalue(dst, idx) => {
                    let val = self
                        .current_frame()
                        .upvalues
                        .as_ref()
                        .expect("LoadUpvalue in non-closure frame")[idx as usize]
                        .borrow()
                        .clone();
                    self.stack[base + dst as usize] = val;
                }
                Instruction::StoreUpvalue(src, idx) => {
                    let val = self.stack[base + src as usize].cheap_clone();
                    let cell = Rc::clone(
                        &self
                            .current_frame()
                            .upvalues
                            .as_ref()
                            .expect("StoreUpvalue in non-closure frame")[idx as usize],
                    );
                    *cell.borrow_mut() = val;
                    // Write-through: if this upvalue is still open (pointing to a
                    // parent stack slot), sync the value back to the stack so the
                    // parent function sees the update when reading the register.
                    if self.has_open_upvalues {
                        for (abs_slot, entry) in self.open_upvalues.iter().enumerate() {
                            if let Some(open_cell) = entry
                                && Rc::ptr_eq(&cell, open_cell)
                            {
                                self.stack[abs_slot] = cell.borrow().cheap_clone();
                                break;
                            }
                        }
                    }
                }
                Instruction::MakeClosure(dst, func_idx) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_make_closure_reg(base, dst, func_idx)?;
                }
                Instruction::CloseUpvalue(slot) => {
                    let abs = base + slot as usize;
                    if abs < self.open_upvalues.len()
                        && let Some(cell) = self.open_upvalues[abs].take()
                    {
                        *cell.borrow_mut() = self.stack[abs].clone();
                        self.has_open_upvalues = self.open_upvalues.iter().any(|e| e.is_some());
                    }
                }

                // ── AoSoA conversion (mobile only) ────────────────
                #[cfg(feature = "mobile-aosoa")]
                Instruction::ConvertToAoSoA(src) => {
                    self.exec_convert_to_aosoa_reg(base, src)?;
                }

                // ── Typed arithmetic (compiler-guaranteed types) ────────
                Instruction::AddInt(dst, a, b) => {
                    let abs_a = base + a as usize;
                    let abs_b = base + b as usize;
                    let result = match (&self.stack[abs_a], &self.stack[abs_b]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_add(*bv) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*av as i64 + *bv as i64),
                        },
                        _ => {
                            let r = self.stack[abs_a]
                                .as_i64()
                                .checked_add(self.stack[abs_b].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    self.stack[base + dst as usize] = result;
                }
                Instruction::AddFloat(dst, a, b) => {
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + a as usize],
                        &self.stack[base + b as usize],
                        |x, y| x + y,
                    );
                    self.stack[base + dst as usize] = result;
                }
                Instruction::SubInt(dst, a, b) => {
                    let abs_a = base + a as usize;
                    let abs_b = base + b as usize;
                    let result = match (&self.stack[abs_a], &self.stack[abs_b]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_sub(*bv) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*av as i64 - *bv as i64),
                        },
                        _ => {
                            let r = self.stack[abs_a]
                                .as_i64()
                                .checked_sub(self.stack[abs_b].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    self.stack[base + dst as usize] = result;
                }
                Instruction::SubFloat(dst, a, b) => {
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + a as usize],
                        &self.stack[base + b as usize],
                        |x, y| x - y,
                    );
                    self.stack[base + dst as usize] = result;
                }
                Instruction::MulInt(dst, a, b) => {
                    let abs_a = base + a as usize;
                    let abs_b = base + b as usize;
                    let result = match (&self.stack[abs_a], &self.stack[abs_b]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_mul(*bv) {
                            Some(r) => Value::I32(r),
                            None => {
                                let r64 = (*av as i64)
                                    .checked_mul(*bv as i64)
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r64)
                            }
                        },
                        _ => {
                            let r = self.stack[abs_a]
                                .as_i64()
                                .checked_mul(self.stack[abs_b].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    self.stack[base + dst as usize] = result;
                }
                Instruction::MulFloat(dst, a, b) => {
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + a as usize],
                        &self.stack[base + b as usize],
                        |x, y| x * y,
                    );
                    self.stack[base + dst as usize] = result;
                }
                Instruction::DivInt(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let abs_a = base + a as usize;
                    let abs_b = base + b as usize;
                    if self.stack[abs_b].as_i64() == 0 {
                        return Err(self.make_error("division by zero".to_string()));
                    }
                    let result = match (&self.stack[abs_a], &self.stack[abs_b]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_div(*bv) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*av as i64 / *bv as i64),
                        },
                        _ => {
                            let r = self.stack[abs_a]
                                .as_i64()
                                .checked_div(self.stack[abs_b].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    self.stack[base + dst as usize] = result;
                }
                Instruction::DivFloat(dst, a, b) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    if self.stack[base + b as usize].as_f64() == 0.0 {
                        return Err(self.make_error("division by zero".to_string()));
                    }
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + a as usize],
                        &self.stack[base + b as usize],
                        |x, y| x / y,
                    );
                    self.stack[base + dst as usize] = result;
                }

                // ── Immediate arithmetic ────────────────────────────────
                Instruction::AddIntImm(dst, src, imm) => {
                    let abs_src = base + src as usize;
                    let result = match &self.stack[abs_src] {
                        Value::I32(v) => match v.checked_add(imm) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*v as i64 + imm as i64),
                        },
                        Value::I64(v) => Value::I64(
                            v.checked_add(imm as i64)
                                .ok_or_else(|| self.make_error("integer overflow".into()))?,
                        ),
                        _ => unreachable!(),
                    };
                    self.stack[base + dst as usize] = result;
                }
                Instruction::SubIntImm(dst, src, imm) => {
                    let abs_src = base + src as usize;
                    let result = match &self.stack[abs_src] {
                        Value::I32(v) => match v.checked_sub(imm) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*v as i64 - imm as i64),
                        },
                        Value::I64(v) => Value::I64(
                            v.checked_sub(imm as i64)
                                .ok_or_else(|| self.make_error("integer overflow".into()))?,
                        ),
                        _ => unreachable!(),
                    };
                    self.stack[base + dst as usize] = result;
                }

                // ── Typed comparison ─────────────────────────────────────
                Instruction::LtInt(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_i64()
                        < self.stack[base + b as usize].as_i64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::LtFloat(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_f64()
                        < self.stack[base + b as usize].as_f64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::LeInt(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_i64()
                        <= self.stack[base + b as usize].as_i64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::LeFloat(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_f64()
                        <= self.stack[base + b as usize].as_f64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::GtInt(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_i64()
                        > self.stack[base + b as usize].as_i64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::GtFloat(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_f64()
                        > self.stack[base + b as usize].as_f64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::GeInt(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_i64()
                        >= self.stack[base + b as usize].as_i64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::GeFloat(dst, a, b) => {
                    let r = self.stack[base + a as usize].as_f64()
                        >= self.stack[base + b as usize].as_f64();
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::EqInt(dst, a, b) => {
                    let r = self.stack[base + a as usize] == self.stack[base + b as usize];
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::EqFloat(dst, a, b) => {
                    let r = self.stack[base + a as usize] == self.stack[base + b as usize];
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::NeInt(dst, a, b) => {
                    let r = self.stack[base + a as usize] != self.stack[base + b as usize];
                    self.stack[base + dst as usize] = Value::Bool(r);
                }
                Instruction::NeFloat(dst, a, b) => {
                    let r = self.stack[base + a as usize] != self.stack[base + b as usize];
                    self.stack[base + dst as usize] = Value::Bool(r);
                }

                // ── Fused compare-and-jump (int) ────────────────────────
                Instruction::TestLtInt(a, b, offset) => {
                    if self.stack[base + a as usize].as_i64()
                        >= self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestLeInt(a, b, offset) => {
                    if self.stack[base + a as usize].as_i64()
                        > self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestGtInt(a, b, offset) => {
                    if self.stack[base + a as usize].as_i64()
                        <= self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestGeInt(a, b, offset) => {
                    if self.stack[base + a as usize].as_i64()
                        < self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestEqInt(a, b, offset) => {
                    if self.stack[base + a as usize] != self.stack[base + b as usize] {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestNeInt(a, b, offset) => {
                    if self.stack[base + a as usize] == self.stack[base + b as usize] {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }

                // ── Fused compare-and-jump (int immediate) ──────────────
                Instruction::TestLtIntImm(a, imm, offset) => {
                    if self.stack[base + a as usize].as_i64() >= imm as i64 {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestLeIntImm(a, imm, offset) => {
                    if self.stack[base + a as usize].as_i64() > imm as i64 {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestGtIntImm(a, imm, offset) => {
                    if self.stack[base + a as usize].as_i64() <= imm as i64 {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestGeIntImm(a, imm, offset) => {
                    if self.stack[base + a as usize].as_i64() < imm as i64 {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }

                // ── Fused compare-and-jump (float) ──────────────────────
                Instruction::TestLtFloat(a, b, offset) => {
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(va.partial_cmp(&vb), Some(std::cmp::Ordering::Less)) {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestLeFloat(a, b, offset) => {
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(
                        va.partial_cmp(&vb),
                        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                    ) {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestGtFloat(a, b, offset) => {
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(va.partial_cmp(&vb), Some(std::cmp::Ordering::Greater)) {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::TestGeFloat(a, b, offset) => {
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(
                        va.partial_cmp(&vb),
                        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                    ) {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }

                // ── Quickened arithmetic (runtime-specialized) ──────────
                Instruction::QAddInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        let result = match (a_ref, b_ref) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_add(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 + *bv as i64),
                            },
                            _ => {
                                let r = a_ref
                                    .as_i64()
                                    .checked_add(b_ref.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack[base + dst as usize] = result;
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Add(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_add_reg(base, dst, a, b)?;
                    }
                }
                Instruction::QAddFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::promote_float_pair_op(a_ref, b_ref, |x, y| x + y);
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Add(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_add_reg(base, dst, a, b)?;
                    }
                }
                Instruction::QSubInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        let result = match (a_ref, b_ref) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_sub(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 - *bv as i64),
                            },
                            _ => {
                                let r = a_ref
                                    .as_i64()
                                    .checked_sub(b_ref.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack[base + dst as usize] = result;
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Sub(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith_reg(
                            base + dst as usize,
                            base + a as usize,
                            base + b as usize,
                            i32::checked_sub,
                            i64::checked_sub,
                            |x, y| x - y,
                        )?;
                    }
                }
                Instruction::QSubFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::promote_float_pair_op(a_ref, b_ref, |x, y| x - y);
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Sub(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith_reg(
                            base + dst as usize,
                            base + a as usize,
                            base + b as usize,
                            i32::checked_sub,
                            i64::checked_sub,
                            |x, y| x - y,
                        )?;
                    }
                }

                Instruction::QMulInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        let result = match (a_ref, b_ref) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_mul(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 * *bv as i64),
                            },
                            _ => {
                                let r = a_ref
                                    .as_i64()
                                    .checked_mul(b_ref.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack[base + dst as usize] = result;
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Mul(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith_reg(
                            base + dst as usize,
                            base + a as usize,
                            base + b as usize,
                            i32::checked_mul,
                            i64::checked_mul,
                            |x, y| x * y,
                        )?;
                    }
                }
                Instruction::QMulFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::promote_float_pair_op(a_ref, b_ref, |x, y| x * y);
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Mul(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith_reg(
                            base + dst as usize,
                            base + a as usize,
                            base + b as usize,
                            i32::checked_mul,
                            i64::checked_mul,
                            |x, y| x * y,
                        )?;
                    }
                }

                Instruction::QDivInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        if b_ref.as_i64() == 0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        let result = match (a_ref, b_ref) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_div(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 / *bv as i64),
                            },
                            _ => {
                                let r = a_ref
                                    .as_i64()
                                    .checked_div(b_ref.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack[base + dst as usize] = result;
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Div(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_div_reg(base, dst, a, b)?;
                    }
                }
                Instruction::QDivFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        if b_ref.as_f64() == 0.0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        self.stack[base + dst as usize] =
                            Value::promote_float_pair_op(a_ref, b_ref, |x, y| x / y);
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Div(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_div_reg(base, dst, a, b)?;
                    }
                }

                // ── Quickened comparison ─────────────────────────────────
                Instruction::QLtInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_i64() < b_ref.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Lt(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x < y, |x, y| x < y)?;
                    }
                }
                Instruction::QLtFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_f64() < b_ref.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Lt(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x < y, |x, y| x < y)?;
                    }
                }

                Instruction::QLeInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_i64() <= b_ref.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Le(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x <= y, |x, y| x <= y)?;
                    }
                }
                Instruction::QLeFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_f64() <= b_ref.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Le(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x <= y, |x, y| x <= y)?;
                    }
                }

                Instruction::QGtInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_i64() > b_ref.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Gt(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x > y, |x, y| x > y)?;
                    }
                }
                Instruction::QGtFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_f64() > b_ref.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Gt(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x > y, |x, y| x > y)?;
                    }
                }

                Instruction::QGeInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_i64() >= b_ref.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ge(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x >= y, |x, y| x >= y)?;
                    }
                }
                Instruction::QGeFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_f64() >= b_ref.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ge(dst, a, b);
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison_reg(base, dst, a, b, |x, y| x >= y, |x, y| x >= y)?;
                    }
                }

                Instruction::QEqInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_i64() == b_ref.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Eq(dst, a, b);
                        }
                        let eq = self.stack[base + a as usize] == self.stack[base + b as usize];
                        self.stack[base + dst as usize] = Value::Bool(eq);
                    }
                }
                Instruction::QEqFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_f64() == b_ref.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Eq(dst, a, b);
                        }
                        let eq = self.stack[base + a as usize] == self.stack[base + b as usize];
                        self.stack[base + dst as usize] = Value::Bool(eq);
                    }
                }

                Instruction::QNeInt(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_int() && b_ref.is_int() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_i64() != b_ref.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ne(dst, a, b);
                        }
                        let ne = self.stack[base + a as usize] != self.stack[base + b as usize];
                        self.stack[base + dst as usize] = Value::Bool(ne);
                    }
                }
                Instruction::QNeFloat(dst, a, b) => {
                    let a_ref = &self.stack[base + a as usize];
                    let b_ref = &self.stack[base + b as usize];
                    if a_ref.is_float() && b_ref.is_float() {
                        self.stack[base + dst as usize] =
                            Value::Bool(a_ref.as_f64() != b_ref.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ne(dst, a, b);
                        }
                        let ne = self.stack[base + a as usize] != self.stack[base + b as usize];
                        self.stack[base + dst as usize] = Value::Bool(ne);
                    }
                }
            }
        }
    }

    // ── Instruction helpers ──────────────────────────────────────────

    /// Performs a checked i32 operation, promoting to i64 on overflow.
    #[inline(always)]
    fn int_arith_i32(
        a: i32,
        b: i32,
        i32_op: fn(i32, i32) -> Option<i32>,
        i64_op: fn(i64, i64) -> Option<i64>,
        err_msg: &str,
    ) -> Result<Value, String> {
        match i32_op(a, b) {
            Some(r) => Ok(Value::I32(r)),
            None => {
                // Promote to i64 and retry
                i64_op(a as i64, b as i64)
                    .map(Value::I64)
                    .ok_or_else(|| err_msg.to_string())
            }
        }
    }

    /// Performs a checked i64 operation.
    #[inline(always)]
    fn int_arith_i64(
        a: i64,
        b: i64,
        op: fn(i64, i64) -> Option<i64>,
        err_msg: &str,
    ) -> Result<Value, String> {
        op(a, b).map(Value::I64).ok_or_else(|| err_msg.to_string())
    }

    /// Executes integer arithmetic with automatic width promotion.
    #[inline(always)]
    fn exec_int_arith(
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

    fn capture_local(&mut self, abs_slot: usize) -> Rc<RefCell<Value>> {
        if abs_slot < self.open_upvalues.len()
            && let Some(existing) = &self.open_upvalues[abs_slot]
        {
            return Rc::clone(existing);
        }
        let cell = Rc::new(RefCell::new(self.stack[abs_slot].clone()));
        if abs_slot >= self.open_upvalues.len() {
            self.open_upvalues.resize(abs_slot + 1, None);
        }
        self.open_upvalues[abs_slot] = Some(Rc::clone(&cell));
        self.has_open_upvalues = true;
        cell
    }

    /// Closes all open upvalues at or above `min_slot` by syncing the
    /// current stack value into their heap cells, then removing them
    /// from the open set.
    fn close_upvalues_above(&mut self, min_slot: usize) {
        let end = self.open_upvalues.len();
        if min_slot >= end {
            return;
        }
        let mut any_remaining = false;
        for slot in min_slot..end {
            if let Some(cell) = self.open_upvalues[slot].take()
                && slot < self.stack.len()
            {
                *cell.borrow_mut() = self.stack[slot].clone();
            }
        }
        // Check if any open upvalues remain below min_slot
        for slot in 0..min_slot.min(end) {
            if self.open_upvalues[slot].is_some() {
                any_remaining = true;
                break;
            }
        }
        self.has_open_upvalues = any_remaining;
    }

    // ── Register-based instruction helpers ───────────────────────

    /// Register-based Add: handles int, float, mixed, and string concat.
    fn exec_add_reg(&mut self, base: usize, dst: u8, a: u8, b: u8) -> Result<(), RuntimeError> {
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
            (Value::Str(a), Value::Str(b)) => Value::Str(Rc::new(format!("{a}{b}"))),
            _ => {
                return Err(self.make_error(format!(
                    "cannot add {} and {}",
                    self.stack[base + a as usize].type_name(),
                    self.stack[base + b as usize].type_name()
                )));
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based binary arithmetic (Sub, Mul).
    /// `dst_abs`, `a_abs`, `b_abs` are absolute stack indices (base + reg).
    fn exec_binary_arith_reg(
        &mut self,
        dst_abs: usize,
        a_abs: usize,
        b_abs: usize,
        i32_op: fn(i32, i32) -> Option<i32>,
        i64_op: fn(i64, i64) -> Option<i64>,
        f64_op: fn(f64, f64) -> f64,
    ) -> Result<(), RuntimeError> {
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
                return Err(self.make_error(format!(
                    "cannot perform arithmetic on {} and {}",
                    self.stack[a_abs].type_name(),
                    self.stack[b_abs].type_name()
                )));
            }
        };
        self.stack[dst_abs] = result;
        Ok(())
    }

    /// Register-based Div with zero-check.
    fn exec_div_reg(&mut self, base: usize, dst: u8, a: u8, b: u8) -> Result<(), RuntimeError> {
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
                return Err(self.make_error(format!(
                    "cannot divide {} by {}",
                    self.stack[base + a as usize].type_name(),
                    self.stack[base + b as usize].type_name()
                )));
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based Mod with zero-check.
    fn exec_mod_reg(&mut self, base: usize, dst: u8, a: u8, b: u8) -> Result<(), RuntimeError> {
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
                return Err(self.make_error(format!(
                    "cannot modulo {} by {}",
                    self.stack[base + a as usize].type_name(),
                    self.stack[base + b as usize].type_name()
                )));
            }
        };
        self.stack[base + dst as usize] = result;
        Ok(())
    }

    /// Register-based comparison.
    fn exec_comparison_reg(
        &mut self,
        base: usize,
        dst: u8,
        a: u8,
        b: u8,
        i64_cmp: fn(&i64, &i64) -> bool,
        f64_cmp: fn(&f64, &f64) -> bool,
    ) -> Result<(), RuntimeError> {
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
                return Err(self.make_error(format!(
                    "cannot compare {} and {}",
                    self.stack[base + a as usize].type_name(),
                    self.stack[base + b as usize].type_name()
                )));
            }
        };
        self.stack[base + dst as usize] = Value::Bool(result);
        Ok(())
    }

    /// Register-based Call instruction.
    ///
    /// In the register model: `Call(base_reg, arg_count)` means the callee
    /// value is in `stack[base + base_reg]`, args are in consecutive registers
    /// starting at `base + base_reg + 1`. The result will be written to
    /// `stack[base + base_reg]` (the caller's result register).
    fn exec_call_reg(
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
            // Ensure stack has room for the callee frame's registers
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
                has_rc_values: true, // closure has upvalue Rc's
                upvalues: Some(upvalues),
            });

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
            let func = &self.functions[func_idx];
            let expected_arity = func.arity;
            let is_variadic = func.is_variadic;
            let max_regs = func.max_registers;
            let func_has_rc = func.has_rc_values;

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
                    upvalues: None,
                });
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

                self.frames.push(CallFrame {
                    chunk_id: ChunkId::Function(func_idx),
                    pc: 0,
                    base: new_base,
                    result_reg: callee_abs,
                    max_registers: max_regs,
                    has_rc_values: func_has_rc,
                    upvalues: None,
                });
            }

            return Ok(());
        }

        // Fall back to native functions
        let func_name = match &self.stack[callee_abs] {
            Value::Str(s) => (**s).clone(),
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
    fn exec_native_call_reg(
        &mut self,
        func_name: &str,
        base: usize,
        base_reg: u8,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let callee_abs = base + base_reg as usize;
        let n = arg_count as usize;

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

        // Pass a direct slice of the stack — no Vec allocation.
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
    fn exec_invoke_reg(
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
            Value::Str(s) => (**s).clone(),
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
    fn exec_call_method_reg(
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

            let body = Rc::clone(&method.body);
            let args: Vec<Value> = (0..n)
                .map(|i| self.stack[receiver_abs + 1 + i].clone())
                .collect();

            let result = (body)(&receiver, &args).map_err(|msg| self.make_error(msg))?;
            self.stack[receiver_abs] = result;
            return Ok(());
        }

        // Built-in callback methods (map, filter, reduce) on arrays
        if let Value::Array(ref arr) = receiver {
            let map_hash = string_hash("map");
            let filter_hash = string_hash("filter");
            let reduce_hash = string_hash("reduce");

            if name_hash == map_hash && arg_count == 1 {
                let fn_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("map expects a function".to_string())),
                };
                let items = arr.borrow().clone();
                let mut result = Vec::with_capacity(items.len());
                for item in &items {
                    let val = self.call_function(&fn_name, std::slice::from_ref(item))?;
                    result.push(val);
                }
                self.stack[receiver_abs] = Value::Array(Rc::new(RefCell::new(result)));
                return Ok(());
            }

            if name_hash == filter_hash && arg_count == 1 {
                let fn_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("filter expects a function".to_string())),
                };
                let items = arr.borrow().clone();
                let mut result = Vec::new();
                for item in &items {
                    let keep = self.call_function(&fn_name, std::slice::from_ref(item))?;
                    if !keep.is_falsy() {
                        result.push(item.clone());
                    }
                }
                self.stack[receiver_abs] = Value::Array(Rc::new(RefCell::new(result)));
                return Ok(());
            }

            if name_hash == reduce_hash && arg_count == 2 {
                let fn_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("reduce expects a function".to_string())),
                };
                let items = arr.borrow().clone();
                let mut acc = self.stack[receiver_abs + 2].clone();
                for item in &items {
                    acc = self.call_function(&fn_name, &[acc, item.clone()])?;
                }
                self.stack[receiver_abs] = acc;
                return Ok(());
            }
        }

        // AoSoA method dispatch
        #[cfg(feature = "mobile-aosoa")]
        if let Value::AoSoA(ref container) = receiver {
            let map_hash = string_hash("map");
            let filter_hash = string_hash("filter");
            let for_each_hash = string_hash("for_each");
            let iter_field_hash = string_hash("iter_field");

            if name_hash == map_hash && arg_count == 1 {
                let fn_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("map expects a function".to_string())),
                };
                let len = container.borrow().len();
                let mut result = Vec::with_capacity(len);
                for i in 0..len {
                    let elem = container.borrow().get(i).unwrap();
                    let item = Value::Struct(Box::new(elem));
                    let val = self.call_function(&fn_name, std::slice::from_ref(&item))?;
                    result.push(val);
                }
                self.stack[receiver_abs] = Value::Array(Rc::new(RefCell::new(result)));
                return Ok(());
            }

            if name_hash == filter_hash && arg_count == 1 {
                let fn_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("filter expects a function".to_string())),
                };
                let len = container.borrow().len();
                let mut result = Vec::new();
                for i in 0..len {
                    let elem = container.borrow().get(i).unwrap();
                    let item = Value::Struct(Box::new(elem));
                    let keep = self.call_function(&fn_name, std::slice::from_ref(&item))?;
                    if !keep.is_falsy() {
                        result.push(item);
                    }
                }
                self.stack[receiver_abs] = Value::Array(Rc::new(RefCell::new(result)));
                return Ok(());
            }

            if name_hash == for_each_hash && arg_count == 1 {
                let fn_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("for_each expects a function".to_string())),
                };
                let len = container.borrow().len();
                for i in 0..len {
                    let elem = container.borrow().get(i).unwrap();
                    let item = Value::Struct(Box::new(elem));
                    self.call_function(&fn_name, std::slice::from_ref(&item))?;
                }
                self.stack[receiver_abs] = Value::Null;
                return Ok(());
            }

            if name_hash == iter_field_hash && arg_count == 1 {
                let field_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => (**s).clone(),
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
                            container.type_name, field_name
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

    /// Register-based GetField.
    fn exec_get_field_reg(
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
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?
                    .clone();
                obj.borrow()
                    .get_field(&field_name)
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
    fn exec_set_field_reg(
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
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?
                    .clone();
                obj.borrow_mut()
                    .set_field(&field_name, value)
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
    fn exec_get_index_reg(
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
                dict.get(key.as_str()).cloned().unwrap_or(Value::Null)
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
    fn exec_set_index_reg(
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
                dict.insert((**key).clone(), value);
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
    fn exec_make_struct_reg(
        &mut self,
        base: usize,
        dst: u8,
        name_idx: u32,
        start: u8,
        field_count: u16,
        chunk_id: ChunkId,
    ) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let struct_name = chunk
            .rc_strings()
            .get(name_idx as usize)
            .map(|s| s.as_str().to_string())
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

        let writ_struct = crate::writ_struct::WritStruct { layout, fields };
        self.stack[base + dst as usize] = Value::Struct(Box::new(writ_struct));
        Ok(())
    }

    /// Register-based MakeClass.
    fn exec_make_class_reg(
        &mut self,
        base: usize,
        dst: u8,
        name_idx: u32,
        start: u8,
        field_count: u16,
        chunk_id: ChunkId,
    ) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let class_name = chunk
            .rc_strings()
            .get(name_idx as usize)
            .map(|s| s.as_str().to_string())
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

        let instance = crate::class_instance::WritClassInstance {
            layout,
            fields,
            parent_class,
        };

        self.stack[base + dst as usize] = Value::Object(Rc::new(RefCell::new(instance)));
        Ok(())
    }

    /// Register-based MakeClosure.
    fn exec_make_closure_reg(
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
                let cell = self.capture_local(abs_slot);
                upvalues.push(cell);
            } else {
                let parent_uv = self
                    .current_frame()
                    .upvalues
                    .as_ref()
                    .expect("transitive capture requires parent closure");
                upvalues.push(Rc::clone(&parent_uv[desc.index as usize]));
            }
        }

        let abs_dst = base + dst as usize;
        self.stack[abs_dst] = Value::Closure(Box::new(ClosureData { func_idx, upvalues }));
        // If this closure captured its own slot (self-recursion), the upvalue
        // cell was created before the closure was stored. Sync it now.
        if self.has_open_upvalues
            && abs_dst < self.open_upvalues.len()
            && let Some(cell) = &self.open_upvalues[abs_dst]
        {
            *cell.borrow_mut() = self.stack[abs_dst].cheap_clone();
        }
        Ok(())
    }

    /// Register-based StartCoroutine.
    fn exec_start_coroutine_reg(
        &mut self,
        base: usize,
        base_reg: u8,
        arg_count: u8,
    ) -> Result<(), RuntimeError> {
        let callee_abs = base + base_reg as usize;
        let n = arg_count as usize;
        let callee = self.stack[callee_abs].clone();

        let (func_idx, closure_upvalues) = match &callee {
            Value::Str(s) => {
                let name: &str = s;
                let idx = self
                    .function_map
                    .get(name)
                    .copied()
                    .ok_or_else(|| self.make_error(format!("undefined function '{name}'")))?;
                (idx, None)
            }
            Value::Closure(data) => (data.func_idx, Some(data.upvalues.clone())),
            _ => {
                return Err(self.make_error(format!(
                    "start requires a function, got {}",
                    callee.type_name()
                )));
            }
        };

        let expected_arity = self.functions[func_idx].arity;
        if expected_arity != arg_count {
            return Err(self.make_error(format!(
                "function '{}' expects {} arguments, got {}",
                self.functions[func_idx].name, expected_arity, arg_count
            )));
        }

        // Build the coroutine's register window
        let func = &self.functions[func_idx];
        let max_regs = func.max_registers as usize;
        let mut coro_stack = vec![Value::Null; max_regs];
        // Copy args into the first registers
        for (i, slot) in coro_stack.iter_mut().enumerate().take(n) {
            *slot = self.stack[callee_abs + 1 + i].clone();
        }

        let id = self.next_coroutine_id;
        self.next_coroutine_id += 1;

        let frame = CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: func.max_registers,
            has_rc_values: func.has_rc_values || closure_upvalues.is_some(),
            upvalues: closure_upvalues,
        };

        let coro = Coroutine {
            id,
            state: CoroutineState::Running,
            stack: coro_stack,
            frames: vec![frame],
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            children: Vec::new(),
        };

        self.coroutines.push(coro);

        if let Some(parent_idx) = self.active_coroutine {
            self.coroutines[parent_idx].children.push(id);
        }

        self.stack[callee_abs] = Value::CoroutineHandle(id);
        Ok(())
    }

    /// Register-based ConvertToAoSoA.
    #[cfg(feature = "mobile-aosoa")]
    fn exec_convert_to_aosoa_reg(&mut self, base: usize, src: u8) -> Result<(), RuntimeError> {
        use crate::aosoa::AoSoAContainer;

        let abs = base + src as usize;
        let val = &self.stack[abs];
        match val {
            Value::Array(ref arr) => {
                let elements = arr.borrow();

                let first_type = match elements.first() {
                    Some(Value::Struct(s)) => Some(s.layout.type_name.clone()),
                    _ => None,
                };

                let type_name = match first_type {
                    Some(name)
                        if elements.iter().all(
                            |v| matches!(v, Value::Struct(s) if s.layout.type_name == name),
                        ) =>
                    {
                        name
                    }
                    _ => {
                        return Ok(());
                    }
                };

                let layout = match self.struct_layouts.get(&type_name) {
                    Some(layout) => Rc::clone(layout),
                    None => {
                        return Ok(());
                    }
                };

                let mut container = AoSoAContainer::new(layout, elements.len());
                for elem in elements.iter() {
                    if let Value::Struct(s) = elem {
                        container.push(s).map_err(|e| self.make_error(e))?;
                    }
                }
                drop(elements);
                self.stack[abs] = Value::AoSoA(Rc::new(RefCell::new(container)));
            }
            _ => {}
        }
        Ok(())
    }

    // ── Coroutine scheduler (public) ─────────────────────────────

    /// Advances the coroutine scheduler by one frame.
    ///
    /// Cancels all coroutines owned by the given object ID.
    ///
    /// This implements structured concurrency: when a host-side object is
    /// destroyed, all coroutines it owns are automatically cancelled,
    /// including their children.
    pub fn cancel_coroutines_for_owner(&mut self, owner_id: u64) {
        let mut to_cancel: Vec<CoroutineId> = Vec::new();

        // Find all coroutines owned by this owner
        for coro in &self.coroutines {
            if coro.owner_id == Some(owner_id) {
                to_cancel.push(coro.id);
            }
        }

        // Cancel them and their children recursively
        while let Some(id) = to_cancel.pop() {
            if let Some(coro) = self.coroutines.iter_mut().find(|c| c.id == id)
                && coro.state != CoroutineState::Cancelled
                && coro.state != CoroutineState::Complete
            {
                coro.state = CoroutineState::Cancelled;
                to_cancel.extend(coro.children.iter().copied());
            }
        }
    }

    /// Checks wait conditions for all suspended coroutines and resumes
    /// those that are ready. Called once per frame by the host game loop.
    pub fn tick(&mut self, delta: f64) -> Result<(), RuntimeError> {
        // Phase 1: Determine which coroutines are ready to resume.
        // We use index-based iteration to avoid borrowing issues.
        let mut ready_ids: Vec<CoroutineId> = Vec::new();
        let count = self.coroutines.len();

        for i in 0..count {
            match self.coroutines[i].state {
                CoroutineState::Cancelled | CoroutineState::Complete => continue,
                CoroutineState::Running => {
                    ready_ids.push(self.coroutines[i].id);
                }
                CoroutineState::Suspended => {
                    let should_resume = match &mut self.coroutines[i].wait {
                        None => true,
                        Some(WaitCondition::OneFrame) => true,
                        Some(WaitCondition::Seconds { remaining }) => {
                            *remaining -= delta;
                            *remaining <= 0.0
                        }
                        Some(WaitCondition::Frames { remaining }) => {
                            if *remaining > 0 {
                                *remaining -= 1;
                            }
                            *remaining == 0
                        }
                        Some(WaitCondition::Until { .. }) => {
                            // Will be evaluated during resume
                            true
                        }
                        Some(WaitCondition::Coroutine { child_id, .. }) => {
                            let child_id = *child_id;
                            // Inline check: is the child done?
                            self.coroutines
                                .iter()
                                .find(|c| c.id == child_id)
                                .map(|c| {
                                    matches!(
                                        c.state,
                                        CoroutineState::Complete | CoroutineState::Cancelled
                                    )
                                })
                                .unwrap_or(true)
                        }
                    };
                    if should_resume {
                        ready_ids.push(self.coroutines[i].id);
                    }
                }
            }
        }

        // Phase 2: Resume each ready coroutine
        for id in ready_ids {
            self.resume_coroutine_by_id(id)?;
        }

        // Phase 3: Remove completed and cancelled coroutines.
        // Keep completed coroutines if another coroutine is still waiting on them
        // (WaitCondition::Coroutine), so the parent can read the return value.
        let waited_on: HashSet<CoroutineId> = self
            .coroutines
            .iter()
            .filter_map(|c| match &c.wait {
                Some(WaitCondition::Coroutine { child_id, .. }) => Some(*child_id),
                _ => None,
            })
            .collect();
        self.coroutines.retain(|c| {
            if matches!(c.state, CoroutineState::Complete) && waited_on.contains(&c.id) {
                return true; // keep for parent to read return value
            }
            !matches!(
                c.state,
                CoroutineState::Complete | CoroutineState::Cancelled
            )
        });

        Ok(())
    }

    /// Resumes a coroutine by its ID.
    fn resume_coroutine_by_id(&mut self, id: CoroutineId) -> Result<(), RuntimeError> {
        let idx = match self.coroutines.iter().position(|c| c.id == id) {
            Some(i) => i,
            None => return Ok(()), // already removed
        };

        // Handle WaitUntil — evaluate the predicate first
        if let Some(WaitCondition::Until { .. }) = &self.coroutines[idx].wait {
            let predicate =
                if let Some(WaitCondition::Until { predicate }) = &self.coroutines[idx].wait {
                    predicate.clone()
                } else {
                    unreachable!()
                };
            let result = self.eval_predicate(&predicate)?;
            if !result {
                // Condition not yet met, stay suspended
                return Ok(());
            }
        }

        // Write the resume value into the correct register.
        // Only YieldCoroutine produces a resume value (the child's return value).
        // Other yields don't produce values in registers. First-run coroutines skip this.
        if let Some(WaitCondition::Coroutine {
            child_id,
            result_reg,
        }) = &self.coroutines[idx].wait
        {
            let child_id = *child_id;
            let result_reg = *result_reg;
            let return_value = self
                .coroutines
                .iter()
                .find(|c| c.id == child_id)
                .and_then(|c| c.return_value.clone())
                .unwrap_or(Value::Null);
            // Write to the destination register in the coroutine's current frame
            let coro = &mut self.coroutines[idx];
            if let Some(frame) = coro.frames.last() {
                let abs = frame.base + result_reg as usize;
                coro.stack[abs] = return_value;
            }
        }

        // Swap coroutine state into VM
        let coro = &mut self.coroutines[idx];
        coro.state = CoroutineState::Running;
        coro.wait = None;

        std::mem::swap(&mut self.stack, &mut coro.stack);
        std::mem::swap(&mut self.frames, &mut coro.frames);
        std::mem::swap(&mut self.open_upvalues, &mut coro.open_upvalues);
        self.active_coroutine = Some(idx);

        // Run until yield or return
        let result = self.run();

        // Swap back
        let coro = &mut self.coroutines[idx];
        std::mem::swap(&mut self.stack, &mut coro.stack);
        std::mem::swap(&mut self.frames, &mut coro.frames);
        std::mem::swap(&mut self.open_upvalues, &mut coro.open_upvalues);
        self.active_coroutine = None;

        match result {
            Ok(RunResult::Yield(wait)) => {
                coro.state = CoroutineState::Suspended;
                coro.wait = Some(wait);
            }
            Ok(RunResult::Return(value)) => {
                coro.state = CoroutineState::Complete;
                coro.return_value = Some(value);
            }
            Err(e) => {
                coro.state = CoroutineState::Complete;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Evaluates a predicate value (function name string or lambda reference).
    /// Returns true if the predicate is satisfied.
    fn eval_predicate(&mut self, predicate: &Value) -> Result<bool, RuntimeError> {
        let (func_idx, closure_upvalues) = match predicate {
            Value::Str(s) => {
                let name: &str = s;
                // Try native function first
                if let Some(native) = self.native_functions.get(name) {
                    let body = Rc::clone(&native.body);
                    let result = body(&[]).map_err(|e| self.make_error(e))?;
                    return Ok(!result.is_falsy());
                }
                let idx = self.function_map.get(name).copied().ok_or_else(|| {
                    self.make_error(format!("undefined predicate function '{name}'"))
                })?;
                (idx, None)
            }
            Value::Closure(data) => (data.func_idx, Some(data.upvalues.clone())),
            _ => {
                return Err(self.make_error(format!(
                    "waitUntil expects a function reference, got {}",
                    predicate.type_name()
                )));
            }
        };

        let expected_arity = self.functions[func_idx].arity;
        if expected_arity != 0 {
            return Err(self.make_error(format!(
                "waitUntil predicate must take 0 arguments, '{}' takes {}",
                self.functions[func_idx].name, expected_arity
            )));
        }

        // Save VM state
        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_upvalues = std::mem::take(&mut self.open_upvalues);
        let saved_active = self.active_coroutine;
        self.active_coroutine = None;

        // Set up a call frame for the predicate
        let max_regs = self.functions[func_idx].max_registers;
        self.stack.resize(max_regs as usize, Value::Null);
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values || closure_upvalues.is_some(),
            upvalues: closure_upvalues,
        });

        let result = self.run();

        // Restore VM state
        self.stack = saved_stack;
        self.frames = saved_frames;
        self.open_upvalues = saved_upvalues;
        self.active_coroutine = saved_active;

        match result {
            Ok(RunResult::Return(value)) => Ok(!value.is_falsy()),
            Ok(RunResult::Yield(_)) => {
                Err(self.make_error("waitUntil predicate must not yield".to_string()))
            }
            Err(e) => Err(e),
        }
    }

    /// Cancels all coroutines owned by the given object ID.
    pub fn cancel_coroutines_for(&mut self, object_id: u64) {
        let ids_to_cancel: Vec<CoroutineId> = self
            .coroutines
            .iter()
            .filter(|c| c.owner_id == Some(object_id))
            .map(|c| c.id)
            .collect();
        for id in ids_to_cancel {
            self.cancel_coroutine(id);
        }
    }

    /// Cancels a single coroutine and propagates to its children.
    fn cancel_coroutine(&mut self, id: CoroutineId) {
        let children: Vec<CoroutineId> = self
            .coroutines
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.children.clone())
            .unwrap_or_default();

        if let Some(coro) = self.coroutines.iter_mut().find(|c| c.id == id) {
            coro.state = CoroutineState::Cancelled;
        }

        for child_id in children {
            self.cancel_coroutine(child_id);
        }
    }

    /// Assigns an owner object to a coroutine (for structured concurrency).
    pub fn set_coroutine_owner(&mut self, coroutine_id: CoroutineId, owner_id: u64) {
        if let Some(coro) = self.coroutines.iter_mut().find(|c| c.id == coroutine_id) {
            coro.owner_id = Some(owner_id);
        }
    }

    /// Returns the ID of the most recently created coroutine.
    pub fn last_coroutine_id(&self) -> Option<CoroutineId> {
        self.coroutines.last().map(|c| c.id)
    }

    /// Returns the number of active coroutines (not completed or cancelled).
    pub fn active_coroutine_count(&self) -> usize {
        self.coroutines
            .iter()
            .filter(|c| {
                !matches!(
                    c.state,
                    CoroutineState::Complete | CoroutineState::Cancelled
                )
            })
            .count()
    }

    // ── Debug internals ─────────────────────────────────────────────

    /// Checks for line changes, fires debug hooks, and handles breakpoints.
    /// Called once per instruction when `has_debug_hooks` is true.
    #[cold]
    #[inline(never)]
    fn debug_probe(&mut self, chunk_id: ChunkId, pc: usize) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let current_line = chunk.line(pc);
        let current_file = chunk.file().unwrap_or("").to_string();

        // Only act on line changes
        let line_changed = current_line != self.last_line || current_file != self.last_file;
        if !line_changed {
            return Ok(());
        }

        self.last_line = current_line;
        self.last_file = current_file.clone();

        // Fire on_line hook
        if let Some(ref hook) = self.on_line_hook {
            hook(&current_file, current_line);
        }

        // Check stepping state
        let should_break = match &self.step_state {
            StepState::None => false,
            StepState::StepInto => true,
            StepState::StepOver { target_depth } => self.frames.len() <= *target_depth,
        };

        // Check breakpoints
        let at_breakpoint = !self.breakpoints.is_empty()
            && self.breakpoints.contains(&BreakpointKey {
                file: current_file.clone(),
                line: current_line,
            });

        if (should_break || at_breakpoint) && self.breakpoint_handler.is_some() {
            self.step_state = StepState::None;

            // Build context from locals before borrowing the handler
            let trace = self.build_stack_trace();
            let fn_name = display_function_name(self.current_frame().func_index(), &self.functions);

            let ctx = BreakpointContext {
                file: &current_file,
                line: current_line,
                function: &fn_name,
                stack_trace: &trace,
            };

            let action = (self.breakpoint_handler.as_ref().unwrap())(&ctx);

            match action {
                BreakpointAction::Continue => {}
                BreakpointAction::StepOver => {
                    self.step_state = StepState::StepOver {
                        target_depth: self.frames.len(),
                    };
                }
                BreakpointAction::StepInto => {
                    self.step_state = StepState::StepInto;
                }
                BreakpointAction::Abort => {
                    return Err(self.make_error("execution aborted by debugger".to_string()));
                }
            }
        }

        Ok(())
    }

    /// Fires the on_call debug hook for the current (just-pushed) frame.
    #[cold]
    #[inline(never)]
    fn fire_call_hook(&self) {
        if let Some(ref hook) = self.on_call_hook {
            let frame = self.current_frame();
            let chunk = self.chunk_for(frame.chunk_id);
            let file = chunk.file().unwrap_or("");
            let line = if frame.pc > 0 {
                chunk.line(frame.pc - 1)
            } else {
                chunk.line(0)
            };
            let name = display_function_name(frame.func_index(), &self.functions);
            hook(&name, file, line);
        }
    }

    /// Fires the on_return debug hook for the current (about-to-pop) frame.
    #[cold]
    #[inline(never)]
    fn fire_return_hook(&self) {
        if let Some(ref hook) = self.on_return_hook {
            let frame = self.current_frame();
            let chunk = self.chunk_for(frame.chunk_id);
            let file = chunk.file().unwrap_or("");
            let line = if frame.pc > 0 && frame.pc - 1 < chunk.len() {
                chunk.line(frame.pc - 1)
            } else {
                0
            };
            let name = display_function_name(frame.func_index(), &self.functions);
            hook(&name, file, line);
        }
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns a display-friendly function name from an optional function index.
/// Translates internal names: `None` → `<script>`, `__lambda_N` → `<lambda>`.
fn display_function_name(func_index: Option<usize>, functions: &[CompiledFunction]) -> String {
    match func_index {
        None => "<script>".to_string(),
        Some(idx) => {
            let name = &functions[idx].name;
            if name.starts_with("__lambda_") {
                "<lambda>".to_string()
            } else {
                name.clone()
            }
        }
    }
}
