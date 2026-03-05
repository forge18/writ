use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use writ_compiler::{
    Chunk, ClassMeta, CmpOp, CompiledFunction, Instruction, StructMeta, string_hash,
};

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
pub struct VM {
    /// The operand stack.
    stack: Vec<Value>,
    /// The call stack.
    frames: Vec<CallFrame>,
    /// The top-level/main chunk being executed.
    main_chunk: Chunk,
    /// Function table: compiled functions indexed by position.
    functions: Vec<CompiledFunction>,
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
    /// Set of disabled module names (blocks native function calls).
    disabled_modules: HashSet<String>,
    /// Maximum number of instructions before termination. `None` = unlimited.
    instruction_limit: Option<u64>,
    /// Current instruction counter, reset per `execute_program` call.
    instruction_count: u64,
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
    /// Fast-path guard: true when any debug hook or breakpoint is active.
    has_debug_hooks: bool,
    /// Open upvalues: absolute stack slot → shared cell.
    /// While the enclosing frame is alive, reads/writes go through the cell.
    /// When the frame returns, the cell is "closed" (self-contained on heap).
    open_upvalues: HashMap<usize, Rc<RefCell<Value>>>,
    /// Cached flag: true when `open_upvalues` is non-empty. Avoids HashMap
    /// method call overhead on every LoadLocal/StoreLocal.
    has_open_upvalues: bool,
    /// Pre-computed instruction pointer + length for each function chunk.
    /// Index 0 = main chunk, index 1..N = function chunks.
    /// Avoids `chunk_for().instructions()` chain on every Call/Return.
    func_ip_cache: Vec<(*const Instruction, usize)>,
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
            open_upvalues: HashMap::new(),
            has_open_upvalues: false,
            func_ip_cache: Vec::new(),
        }
    }

    /// Registers a native Rust function callable from Writ scripts.
    ///
    /// The function receives a slice of [`Value`] arguments and returns
    /// either a [`Value`] result or a `String` error message.
    pub fn register_fn<F>(&mut self, name: &str, arity: u8, f: F) -> &mut Self
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        self.native_functions.insert(
            name.to_string(),
            NativeFunction {
                name: name.to_string(),
                module: None,
                arity: Some(arity),
                body: Rc::new(f),
            },
        );
        self
    }

    /// Registers a native function within a named module.
    ///
    /// Functions in a disabled module (via [`disable_module`](Self::disable_module))
    /// will produce a [`RuntimeError`] when called.
    pub fn register_fn_in_module<F>(
        &mut self,
        name: &str,
        module: &str,
        arity: u8,
        f: F,
    ) -> &mut Self
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        self.native_functions.insert(
            name.to_string(),
            NativeFunction {
                name: name.to_string(),
                module: Some(module.to_string()),
                arity: Some(arity),
                body: Rc::new(f),
            },
        );
        self
    }

    /// Registers a native method on a specific value type.
    ///
    /// The method body receives the receiver value and a slice of arguments.
    /// Use `module` to associate the method with a disableable module.
    pub fn register_method<F>(
        &mut self,
        tag: ValueTag,
        name: &str,
        module: Option<&str>,
        arity: Option<u8>,
        f: F,
    ) -> &mut Self
    where
        F: Fn(&Value, &[Value]) -> Result<Value, String> + 'static,
    {
        let hash = string_hash(name);
        self.methods.insert(
            (tag, hash),
            NativeMethod {
                name: name.to_string(),
                module: module.map(|m| m.to_string()),
                arity,
                body: Rc::new(f),
            },
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
        compiler
            .compile_program(&stmts)
            .map_err(|e| format!("{e}"))?;
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

        // 6. Rebuild the instruction pointer cache so it points to the new chunks
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

        // Execute the module's top-level code
        self.stack.clear();
        self.frames.clear();
        self.instruction_count = 0;
        self.main_chunk = chunk.clone();

        // Rebuild instruction pointer cache with all functions
        self.rebuild_ip_cache();

        self.frames.push(CallFrame {
            chunk_id: ChunkId::Main,
            pc: 0,
            base: 0,
            has_callee_slot: false,
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
        for arg in args {
            self.stack.push(arg.clone());
        }

        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base,
            has_callee_slot: false,
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
        for arg in args {
            self.stack.push(arg.clone());
        }

        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base,
            has_callee_slot: false,
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

        // Pre-compute instruction pointer cache for fast Call/Return.
        self.rebuild_ip_cache();

        // Push the top-level frame
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Main,
            pc: 0,
            base: 0,
            has_callee_slot: false,
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

    /// Pops a value from the operand stack.
    #[inline(always)]
    fn pop(&mut self) -> Result<Value, RuntimeError> {
        self.stack
            .pop()
            .ok_or_else(|| self.make_error("stack underflow".to_string()))
    }

    /// Peeks at the top value on the operand stack without removing it.
    #[inline(always)]
    fn peek(&self) -> Result<&Value, RuntimeError> {
        self.stack
            .last()
            .ok_or_else(|| self.make_error("stack underflow".to_string()))
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

            // End of chunk: implicit return
            if ip >= ip_end {
                if self.has_debug_hooks {
                    self.fire_return_hook();
                }
                let return_value = self.stack.pop().unwrap_or(Value::Null);
                let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                self.stack.truncate(frame.truncate_to());
                if self.frames.len() <= return_depth {
                    return Ok(RunResult::Return(return_value));
                }
                self.stack.push(return_value);
                // Reload from new top frame
                let f = unsafe { self.frames.last().unwrap_unchecked() };
                base = f.base;
                chunk_id = f.chunk_id;
                reload_ip!(self, chunk_id, f.pc);
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
                Instruction::LoadInt(v) => {
                    self.stack.push(Value::I32(v));
                }
                Instruction::LoadConstInt(idx) => {
                    let chunk = self.chunk_for(chunk_id);
                    let v = chunk.int64_constants()[idx as usize];
                    self.stack.push(Value::I64(v));
                }
                Instruction::LoadFloat(v) => {
                    self.stack.push(Value::F32(v));
                }
                Instruction::LoadConstFloat(idx) => {
                    let chunk = self.chunk_for(chunk_id);
                    let v = chunk.float64_constants()[idx as usize];
                    self.stack.push(Value::F64(v));
                }
                Instruction::LoadBool(v) => self.stack.push(Value::Bool(v)),
                Instruction::LoadStr(idx) => {
                    let chunk = self.chunk_for(chunk_id);
                    let s = Rc::clone(&chunk.rc_strings()[idx as usize]);
                    self.stack.push(Value::Str(s));
                }
                Instruction::LoadNull => self.stack.push(Value::Null),

                // ── Local variables ──────────────────────────────────
                Instruction::LoadLocal(slot) => {
                    let abs = base + slot as usize;
                    let val = if self.has_open_upvalues {
                        if let Some(cell) = self.open_upvalues.get(&abs) {
                            cell.borrow().cheap_clone()
                        } else {
                            self.stack[abs].cheap_clone()
                        }
                    } else {
                        // SAFETY: compiler guarantees slot is valid
                        unsafe { self.stack.get_unchecked(abs).cheap_clone() }
                    };
                    self.stack.push(val);
                }
                Instruction::StoreLocal(slot) => {
                    let val = self.pop()?;
                    let target = base + slot as usize;
                    // Extend the stack if the slot doesn't exist yet
                    while self.stack.len() <= target {
                        self.stack.push(Value::Null);
                    }
                    // Sync to open upvalue cell if this local is captured
                    if self.has_open_upvalues
                        && let Some(cell) = self.open_upvalues.get(&target)
                    {
                        *cell.borrow_mut() = val.cheap_clone();
                    }
                    self.stack[target] = val; // move, not clone
                }

                // ── Arithmetic ───────────────────────────────────────
                Instruction::Add => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    // Quicken based on observed operand types
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QAddInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QAddFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_add()?;
                }
                Instruction::Sub => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QSubInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QSubFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_binary_arith(
                        i32::checked_sub,
                        i64::checked_sub,
                        |a, b| a - b,
                        "subtract",
                    )?;
                }
                Instruction::Mul => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QMulInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QMulFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_binary_arith(
                        i32::checked_mul,
                        i64::checked_mul,
                        |a, b| a * b,
                        "multiply",
                    )?;
                }
                Instruction::Div => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QDivInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QDivFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_div()?;
                }
                Instruction::Mod => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_mod()?;
                }

                // ── Unary ────────────────────────────────────────────
                Instruction::Neg => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = self.pop()?;
                    let result = match val {
                        Value::I32(v) => match v.checked_neg() {
                            Some(r) => Value::I32(r),
                            None => Value::I64(-(v as i64)),
                        },
                        Value::I64(v) => {
                            Value::I64(v.checked_neg().ok_or_else(|| {
                                self.make_error("integer overflow on negation".to_string())
                            })?)
                        }
                        Value::F32(v) => Value::F32(-v),
                        Value::F64(v) => Value::F64(-v),
                        _ => {
                            return Err(
                                self.make_error(format!("cannot negate {}", val.type_name()))
                            );
                        }
                    };
                    self.stack.push(result);
                }
                Instruction::Not => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = self.pop()?;
                    match val {
                        Value::Bool(v) => self.stack.push(Value::Bool(!v)),
                        _ => {
                            return Err(
                                self.make_error(format!("cannot apply '!' to {}", val.type_name()))
                            );
                        }
                    }
                }

                // ── Comparison ───────────────────────────────────────
                Instruction::Eq => {
                    let len = self.stack.len();
                    // Quicken based on observed types
                    match (&self.stack[len - 2], &self.stack[len - 1]) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QEqInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QEqFloat;
                        },
                        _ => {}
                    }
                    let eq = self.stack[len - 2] == self.stack[len - 1];
                    self.stack[len - 2] = Value::Bool(eq);
                    self.stack.truncate(len - 1);
                }
                Instruction::Ne => {
                    let len = self.stack.len();
                    match (&self.stack[len - 2], &self.stack[len - 1]) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QNeInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::QNeFloat;
                        },
                        _ => {}
                    }
                    let ne = self.stack[len - 2] != self.stack[len - 1];
                    self.stack[len - 2] = Value::Bool(ne);
                    self.stack.truncate(len - 1);
                }
                Instruction::Lt => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QLtInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QLtFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_comparison(|a, b| a < b, |a, b| a < b)?;
                }
                Instruction::Le => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QLeInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QLeFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_comparison(|a, b| a <= b, |a, b| a <= b)?;
                }
                Instruction::Gt => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QGtInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QGtFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_comparison(|a, b| a > b, |a, b| a > b)?;
                }
                Instruction::Ge => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    if len >= 2 {
                        match (&self.stack[len - 2], &self.stack[len - 1]) {
                            (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QGeInt;
                            },
                            (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                                *(ip.sub(1) as *mut Instruction) = Instruction::QGeFloat;
                            },
                            _ => {}
                        }
                    }
                    self.exec_comparison(|a, b| a >= b, |a, b| a >= b)?;
                }

                // ── Logical ──────────────────────────────────────────
                Instruction::And => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    match (&self.stack[len - 2], &self.stack[len - 1]) {
                        (Value::Bool(a), Value::Bool(b)) => {
                            let r = *a && *b;
                            self.stack[len - 2] = Value::Bool(r);
                            self.stack.truncate(len - 1);
                        }
                        _ => {
                            return Err(self.make_error(format!(
                                "cannot apply '&&' to {} and {}",
                                self.stack[len - 2].type_name(),
                                self.stack[len - 1].type_name()
                            )));
                        }
                    }
                }
                Instruction::Or => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    match (&self.stack[len - 2], &self.stack[len - 1]) {
                        (Value::Bool(a), Value::Bool(b)) => {
                            let r = *a || *b;
                            self.stack[len - 2] = Value::Bool(r);
                            self.stack.truncate(len - 1);
                        }
                        _ => {
                            return Err(self.make_error(format!(
                                "cannot apply '||' to {} and {}",
                                self.stack[len - 2].type_name(),
                                self.stack[len - 1].type_name()
                            )));
                        }
                    }
                }

                // ── Control ──────────────────────────────────────────
                Instruction::Pop => {
                    self.pop()?;
                }
                Instruction::Return => {
                    if self.has_debug_hooks {
                        self.fire_return_hook();
                    }
                    // SAFETY: stack/frames are non-empty during execution
                    let return_value = unsafe { self.stack.pop().unwrap_unchecked() };
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    // Close any open upvalues belonging to this frame
                    if self.has_open_upvalues {
                        self.close_upvalues_above(frame.base);
                    }
                    let truncate_to = frame.truncate_to();
                    if self.stack.len() > truncate_to {
                        self.stack.truncate(truncate_to);
                    }
                    if self.frames.len() <= return_depth {
                        return Ok(RunResult::Return(return_value));
                    }
                    self.stack.push(return_value);
                    // Reload from new top frame
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = f.chunk_id;
                    reload_ip!(self, chunk_id, f.pc);
                }

                // ── Jumps ────────────────────────────────────────────
                Instruction::Jump(offset) => {
                    ip = unsafe { ip.offset(offset as isize) };
                }
                Instruction::JumpIfFalse(offset) => {
                    let val = self.peek()?;
                    if val.is_falsy() {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::JumpIfTrue(offset) => {
                    let val = self.peek()?;
                    if !val.is_falsy() {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::JumpIfFalsePop(offset) => {
                    // SAFETY: stack is non-empty when JumpIfFalsePop executes
                    let val = unsafe { self.stack.pop().unwrap_unchecked() };
                    if val.is_falsy() {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }

                // ── Function calls ───────────────────────────────────
                Instruction::Call(arg_count) => {
                    unsafe { self.frames.last_mut().unwrap_unchecked() }.pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_call(arg_count)?;
                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    // Reload from new top frame
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = f.chunk_id;
                    reload_ip!(self, chunk_id, f.pc);
                }
                Instruction::CallDirect(func_idx, arg_count) => {
                    // SAFETY: frames is non-empty during execution
                    unsafe { self.frames.last_mut().unwrap_unchecked() }.pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let func_idx = func_idx as usize;
                    let n = arg_count as usize;
                    let func = &self.functions[func_idx];

                    if func.is_variadic {
                        let min_args = func.arity.saturating_sub(1);
                        if arg_count < min_args {
                            return Err(self.make_error(format!(
                                "function '{}' expects at least {} arguments, got {}",
                                func.name, min_args, arg_count
                            )));
                        }
                        let fixed_count = min_args as usize;
                        let variadic_count = n - fixed_count;
                        let variadic_start = self.stack.len() - variadic_count;
                        let variadic_args: Vec<Value> =
                            self.stack.drain(variadic_start..).collect();
                        self.stack
                            .push(Value::Array(Rc::new(RefCell::new(variadic_args))));
                        let new_base = self.stack.len() - fixed_count - 1;
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            has_callee_slot: false,
                            upvalues: None,
                        });
                    } else {
                        debug_assert_eq!(
                            func.arity, arg_count,
                            "CallDirect arity mismatch for '{}'",
                            func.name
                        );
                        if func.arity != arg_count {
                            return Err(self.make_error(format!(
                                "function '{}' expects {} arguments, got {}",
                                func.name, func.arity, arg_count
                            )));
                        }
                        let new_base = self.stack.len() - n;
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            has_callee_slot: false,
                            upvalues: None,
                        });
                    }

                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    // Reload — new frame always starts at pc=0, so ip = ip_base
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = ChunkId::Function(func_idx);
                    reload_ip!(self, chunk_id, 0);
                }
                Instruction::CallNative(_id) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    return Err(self.make_error("native functions not yet supported".to_string()));
                }

                // ── Null handling ────────────────────────────────────
                Instruction::NullCoalesce => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let fallback = self.pop()?;
                    let value = self.pop()?;
                    if value.is_null() {
                        self.stack.push(fallback);
                    } else {
                        self.stack.push(value);
                    }
                }

                // ── String concatenation ─────────────────────────────
                Instruction::Concat => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let rhs = self.pop()?;
                    let lhs = self.pop()?;
                    let result = format!("{lhs}{rhs}");
                    self.stack.push(Value::Str(Rc::new(result)));
                }

                // ── Collections ──────────────────────────────────────
                Instruction::MakeArray(count) => {
                    let n = count as usize;
                    let start = self.stack.len() - n;
                    let elements: Vec<Value> = self.stack.drain(start..).collect();
                    self.stack
                        .push(Value::Array(Rc::new(RefCell::new(elements))));
                }
                Instruction::MakeDict(count) => {
                    let n = count as usize;
                    let start = self.stack.len() - (n * 2);
                    let pairs: Vec<Value> = self.stack.drain(start..).collect();
                    let mut map = HashMap::new();
                    for pair in pairs.chunks(2) {
                        let key = match &pair[0] {
                            Value::Str(s) => (**s).clone(),
                            other => other.to_string(),
                        };
                        map.insert(key, pair[1].clone());
                    }
                    self.stack.push(Value::Dict(Rc::new(RefCell::new(map))));
                }

                // ── Field/Index access ───────────────────────────────
                Instruction::GetField(name_hash) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_get_field(name_hash)?;
                }
                Instruction::SetField(name_hash) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_set_field(name_hash)?;
                }
                Instruction::GetIndex => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_get_index()?;
                }
                Instruction::SetIndex => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_set_index()?;
                }

                // ── Coroutines (Phase 13) ────────────────────────────
                Instruction::StartCoroutine(arg_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_start_coroutine(arg_count)?;
                }
                Instruction::Yield => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    return Ok(RunResult::Yield(WaitCondition::OneFrame));
                }
                Instruction::YieldSeconds => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let seconds = self.pop()?;
                    let secs = match &seconds {
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
                Instruction::YieldFrames => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let frames_val = self.pop()?;
                    let n = match &frames_val {
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
                Instruction::YieldUntil => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let predicate = self.pop()?;
                    return Ok(RunResult::Yield(WaitCondition::Until { predicate }));
                }
                Instruction::YieldCoroutine => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let handle = self.pop()?;
                    let child_id = match &handle {
                        Value::CoroutineHandle(id) => *id,
                        _ => {
                            return Err(self.make_error(format!(
                                "yield coroutine expects a coroutine handle, got {}",
                                handle.type_name()
                            )));
                        }
                    };
                    return Ok(RunResult::Yield(WaitCondition::Coroutine { child_id }));
                }

                // ── Spread ───────────────────────────────────────────
                Instruction::Spread => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = self.pop()?;
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

                // ── Phase 19: Structs ──────────────────────────────
                Instruction::MakeStruct(name_idx, field_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_make_struct(name_idx, field_count, chunk_id)?;
                }

                // ── Classes ─────────────────────────────────────────
                Instruction::MakeClass(name_idx, field_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_make_class(name_idx, field_count, chunk_id)?;
                }

                // ── Phase 15: Method calls ──────────────────────────
                Instruction::CallMethod(name_hash, arg_count) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_call_method(name_hash, arg_count)?;
                }

                // ── Phase 15: Global variables ──────────────────────
                Instruction::LoadGlobal(name_hash) => {
                    if let Some((_, value)) = self.globals.get(&name_hash) {
                        self.stack.push(value.clone());
                    } else {
                        // Fall back to pushing the name as a string (for function resolution)
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
                        self.stack.push(Value::Str(Rc::new(name)));
                    }
                }

                // ── Closures ─────────────────────────────────────────
                Instruction::LoadUpvalue(idx) => {
                    let val = self
                        .current_frame()
                        .upvalues
                        .as_ref()
                        .expect("LoadUpvalue in non-closure frame")[idx as usize]
                        .borrow()
                        .clone();
                    self.stack.push(val);
                }
                Instruction::StoreUpvalue(idx) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let val = self.pop()?;
                    *self
                        .current_frame()
                        .upvalues
                        .as_ref()
                        .expect("StoreUpvalue in non-closure frame")[idx as usize]
                        .borrow_mut() = val;
                }
                Instruction::MakeClosure(func_idx) => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    self.exec_make_closure(func_idx)?;
                }
                Instruction::CloseUpvalue(slot) => {
                    let abs = base + slot as usize;
                    if let Some(cell) = self.open_upvalues.remove(&abs) {
                        *cell.borrow_mut() = self.stack[abs].clone();
                        self.has_open_upvalues = !self.open_upvalues.is_empty();
                    }
                    self.pop()?;
                }

                // ── Phase 19: AoSoA conversion (mobile only) ────────────
                #[cfg(feature = "mobile-aosoa")]
                Instruction::ConvertToAoSoA => {
                    self.exec_convert_to_aosoa()?;
                }

                // ── Typed arithmetic (compiler-guaranteed types) ────────
                // Note: PC is NOT synced for typed arithmetic (AddInt, SubInt, etc.)
                // because errors are extremely rare (only i64 overflow). This keeps
                // the hot path lean. If overflow does occur, the error line may be
                // off by one instruction — acceptable tradeoff.
                Instruction::AddInt => {
                    let len = self.stack.len();
                    // SAFETY: typed instruction guarantees >= 2 Int values on stack.
                    // set_len safe: removed value is Int (no Rc to drop).
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = match (a, b) {
                            (Value::I32(a), Value::I32(b)) => match a.checked_add(*b) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*a as i64 + *b as i64),
                            },
                            _ => {
                                let r = a
                                    .as_i64()
                                    .checked_add(b.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::AddFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) =
                            Value::promote_float_pair_op(a, b, |x, y| x + y);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::SubInt => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = match (a, b) {
                            (Value::I32(a), Value::I32(b)) => match a.checked_sub(*b) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*a as i64 - *b as i64),
                            },
                            _ => {
                                let r = a
                                    .as_i64()
                                    .checked_sub(b.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::SubFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) =
                            Value::promote_float_pair_op(a, b, |x, y| x - y);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::MulInt => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = match (a, b) {
                            (Value::I32(a), Value::I32(b)) => match a.checked_mul(*b) {
                                Some(r) => Value::I32(r),
                                None => {
                                    let r64 =
                                        (*a as i64).checked_mul(*b as i64).ok_or_else(|| {
                                            self.make_error("integer overflow".into())
                                        })?;
                                    Value::I64(r64)
                                }
                            },
                            _ => {
                                let r = a
                                    .as_i64()
                                    .checked_mul(b.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::MulFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) =
                            Value::promote_float_pair_op(a, b, |x, y| x * y);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::DivInt => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        if b.as_i64() == 0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        *self.stack.get_unchecked_mut(len - 2) = match (a, b) {
                            (Value::I32(a), Value::I32(b)) => match a.checked_div(*b) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*a as i64 / *b as i64),
                            },
                            _ => {
                                let r = a
                                    .as_i64()
                                    .checked_div(b.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::DivFloat => {
                    self.frames.last_mut().unwrap().pc =
                        unsafe { ip.offset_from(ip_base) as usize };
                    let len = self.stack.len();
                    unsafe {
                        let a = self.stack.get_unchecked(len - 2);
                        let b = self.stack.get_unchecked(len - 1);
                        if b.as_f64() == 0.0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        *self.stack.get_unchecked_mut(len - 2) =
                            Value::promote_float_pair_op(a, b, |x, y| x / y);
                        self.stack.set_len(len - 1);
                    }
                }

                // ── Typed comparison ─────────────────────────────────────
                Instruction::LtInt => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_i64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_i64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a < b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::LtFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_f64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_f64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a < b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::LeInt => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_i64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_i64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a <= b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::LeFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_f64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_f64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a <= b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::GtInt => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_i64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_i64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a > b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::GtFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_f64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_f64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a > b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::GeInt => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_i64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_i64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a >= b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::GeFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let a = self
                            .stack
                            .get_unchecked(len - 2)
                            .as_f64();
                        let b = self
                            .stack
                            .get_unchecked(len - 1)
                            .as_f64();
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(a >= b);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::EqInt => {
                    let len = self.stack.len();
                    unsafe {
                        let eq = self.stack.get_unchecked(len - 2) == self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(eq);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::EqFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let eq = self.stack.get_unchecked(len - 2) == self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(eq);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::NeInt => {
                    let len = self.stack.len();
                    unsafe {
                        let ne = self.stack.get_unchecked(len - 2) != self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(ne);
                        self.stack.set_len(len - 1);
                    }
                }
                Instruction::NeFloat => {
                    let len = self.stack.len();
                    unsafe {
                        let ne = self.stack.get_unchecked(len - 2) != self.stack.get_unchecked(len - 1);
                        *self.stack.get_unchecked_mut(len - 2) = Value::Bool(ne);
                        self.stack.set_len(len - 1);
                    }
                }

                // ── Fused instructions ───────────────────────────────────
                Instruction::IncrLocalInt(slot, imm) => {
                    let abs = base + slot as usize;
                    // SAFETY: compiler guarantees slot is valid and holds Int
                    unsafe {
                        let v = self.stack.get_unchecked(abs);
                        *self.stack.get_unchecked_mut(abs) = match v {
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
                    }
                }
                Instruction::LoadLocalAddInt(slot, imm) => {
                    let abs = base + slot as usize;
                    // SAFETY: compiler guarantees slot is valid and holds Int
                    let v = unsafe { self.stack.get_unchecked(abs) };
                    self.stack.push(match v {
                        Value::I32(v) => match v.checked_add(imm) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*v as i64 + imm as i64),
                        },
                        Value::I64(v) => Value::I64(
                            v.checked_add(imm as i64)
                                .ok_or_else(|| self.make_error("integer overflow".into()))?,
                        ),
                        _ => unreachable!(),
                    });
                }
                Instruction::LoadLocalSubInt(slot, imm) => {
                    let abs = base + slot as usize;
                    // SAFETY: compiler guarantees slot is valid and holds Int
                    let v = unsafe { self.stack.get_unchecked(abs) };
                    self.stack.push(match v {
                        Value::I32(v) => match v.checked_sub(imm) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*v as i64 - imm as i64),
                        },
                        Value::I64(v) => Value::I64(
                            v.checked_sub(imm as i64)
                                .ok_or_else(|| self.make_error("integer overflow".into()))?,
                        ),
                        _ => unreachable!(),
                    });
                }
                Instruction::CmpLocalIntJump(slot, imm, cmp_op, offset) => {
                    let abs = base + slot as usize;
                    // SAFETY: compiler guarantees slot is valid and holds Int
                    let iv = unsafe { self.stack.get_unchecked(abs).as_i64() };
                    let imm_val = imm as i64;
                    let result = match CmpOp::from_u8(cmp_op) {
                        CmpOp::Lt => iv < imm_val,
                        CmpOp::Le => iv <= imm_val,
                        CmpOp::Gt => iv > imm_val,
                        CmpOp::Ge => iv >= imm_val,
                    };
                    if !result {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }
                Instruction::ReturnLocal(slot) => {
                    if self.has_debug_hooks {
                        self.fire_return_hook();
                    }
                    let abs = base + slot as usize;
                    // Must check open upvalues — the canonical value may be in
                    // the heap cell if the local was captured by a closure.
                    let return_value = if self.has_open_upvalues {
                        if let Some(cell) = self.open_upvalues.get(&abs) {
                            cell.borrow().cheap_clone()
                        } else {
                            unsafe { self.stack.get_unchecked(abs).cheap_clone() }
                        }
                    } else {
                        unsafe { self.stack.get_unchecked(abs).cheap_clone() }
                    };
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    if self.has_open_upvalues {
                        self.close_upvalues_above(frame.base);
                    }
                    let truncate_to = frame.truncate_to();
                    if self.stack.len() > truncate_to {
                        self.stack.truncate(truncate_to);
                    }
                    if self.frames.len() <= return_depth {
                        return Ok(RunResult::Return(return_value));
                    }
                    self.stack.push(return_value);
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = f.chunk_id;
                    reload_ip!(self, chunk_id, f.pc);
                }
                Instruction::AddLocals(slot_a, slot_b) => {
                    let abs_a = base + slot_a as usize;
                    let abs_b = base + slot_b as usize;
                    // SAFETY: compiler guarantees slots are valid and hold Int
                    unsafe {
                        let a = self.stack.get_unchecked(abs_a);
                        let b = self.stack.get_unchecked(abs_b);
                        self.stack.push(match (a, b) {
                            (Value::I32(a), Value::I32(b)) => match a.checked_add(*b) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*a as i64 + *b as i64),
                            },
                            _ => {
                                let r = a
                                    .as_i64()
                                    .checked_add(b.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        });
                    }
                }
                Instruction::SubLocals(slot_a, slot_b) => {
                    let abs_a = base + slot_a as usize;
                    let abs_b = base + slot_b as usize;
                    // SAFETY: compiler guarantees slots are valid and hold Int
                    unsafe {
                        let a = self.stack.get_unchecked(abs_a);
                        let b = self.stack.get_unchecked(abs_b);
                        self.stack.push(match (a, b) {
                            (Value::I32(a), Value::I32(b)) => match a.checked_sub(*b) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*a as i64 - *b as i64),
                            },
                            _ => {
                                let r = a
                                    .as_i64()
                                    .checked_sub(b.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        });
                    }
                }
                Instruction::CmpLocalsJump(slot_a, slot_b, cmp_op, offset) => {
                    let abs_a = base + slot_a as usize;
                    let abs_b = base + slot_b as usize;
                    // SAFETY: compiler guarantees slots are valid and hold Int
                    let av = unsafe { self.stack.get_unchecked(abs_a).as_i64() };
                    let bv = unsafe { self.stack.get_unchecked(abs_b).as_i64() };
                    let result = match CmpOp::from_u8(cmp_op) {
                        CmpOp::Lt => av < bv,
                        CmpOp::Le => av <= bv,
                        CmpOp::Gt => av > bv,
                        CmpOp::Ge => av >= bv,
                    };
                    if !result {
                        ip = unsafe { ip.offset(offset as isize) };
                    }
                }

                // ── Quickened arithmetic (runtime-specialized) ──────────
                // Each quickened handler tries the typed fast path. On type
                // mismatch, it deopts (rewrites back to generic) and falls
                // through to the generic handler.
                Instruction::QAddInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let (a, b) = (a_ref.cheap_clone(), b_ref.cheap_clone());
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = match (&a, &b) {
                                (Value::I32(a), Value::I32(b)) => match a.checked_add(*b) {
                                    Some(r) => Value::I32(r),
                                    None => Value::I64(*a as i64 + *b as i64),
                                },
                                _ => {
                                    let r =
                                        a.as_i64().checked_add(b.as_i64()).ok_or_else(|| {
                                            self.make_error("integer overflow".into())
                                        })?;
                                    Value::I64(r)
                                }
                            };
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        // Deopt: rewrite back to generic Add
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Add;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_add()?;
                    }
                }
                Instruction::QAddFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) =
                                Value::promote_float_pair_op(a_ref, b_ref, |x, y| x + y);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Add;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_add()?;
                    }
                }

                Instruction::QSubInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let (a, b) = (a_ref.cheap_clone(), b_ref.cheap_clone());
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = match (&a, &b) {
                                (Value::I32(a), Value::I32(b)) => match a.checked_sub(*b) {
                                    Some(r) => Value::I32(r),
                                    None => Value::I64(*a as i64 - *b as i64),
                                },
                                _ => {
                                    let r =
                                        a.as_i64().checked_sub(b.as_i64()).ok_or_else(|| {
                                            self.make_error("integer overflow".into())
                                        })?;
                                    Value::I64(r)
                                }
                            };
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Sub;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith(
                            i32::checked_sub,
                            i64::checked_sub,
                            |a, b| a - b,
                            "subtract",
                        )?;
                    }
                }
                Instruction::QSubFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) =
                                Value::promote_float_pair_op(a_ref, b_ref, |x, y| x - y);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Sub;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith(
                            i32::checked_sub,
                            i64::checked_sub,
                            |a, b| a - b,
                            "subtract",
                        )?;
                    }
                }

                Instruction::QMulInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let (a, b) = (a_ref.cheap_clone(), b_ref.cheap_clone());
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = match (&a, &b) {
                                (Value::I32(a), Value::I32(b)) => match a.checked_mul(*b) {
                                    Some(r) => Value::I32(r),
                                    None => {
                                        let r64 =
                                            (*a as i64).checked_mul(*b as i64).ok_or_else(|| {
                                                self.make_error("integer overflow".into())
                                            })?;
                                        Value::I64(r64)
                                    }
                                },
                                _ => {
                                    let r =
                                        a.as_i64().checked_mul(b.as_i64()).ok_or_else(|| {
                                            self.make_error("integer overflow".into())
                                        })?;
                                    Value::I64(r)
                                }
                            };
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Mul;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith(
                            i32::checked_mul,
                            i64::checked_mul,
                            |a, b| a * b,
                            "multiply",
                        )?;
                    }
                }
                Instruction::QMulFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) =
                                Value::promote_float_pair_op(a_ref, b_ref, |x, y| x * y);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Mul;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_binary_arith(
                            i32::checked_mul,
                            i64::checked_mul,
                            |a, b| a * b,
                            "multiply",
                        )?;
                    }
                }

                Instruction::QDivInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let (a, b) = (a_ref.cheap_clone(), b_ref.cheap_clone());
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        if b.as_i64() == 0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = match (&a, &b) {
                                (Value::I32(a), Value::I32(b)) => match a.checked_div(*b) {
                                    Some(r) => Value::I32(r),
                                    None => Value::I64(*a as i64 / *b as i64),
                                },
                                _ => {
                                    let r =
                                        a.as_i64().checked_div(b.as_i64()).ok_or_else(|| {
                                            self.make_error("integer overflow".into())
                                        })?;
                                    Value::I64(r)
                                }
                            };
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Div;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_div()?;
                    }
                }
                Instruction::QDivFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        if b_ref.as_f64() == 0.0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) =
                                Value::promote_float_pair_op(a_ref, b_ref, |x, y| x / y);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Div;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_div()?;
                    }
                }

                // ── Quickened comparison ─────────────────────────────────
                Instruction::QLtInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let r = a_ref.as_i64() < b_ref.as_i64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Lt;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a < b, |a, b| a < b)?;
                    }
                }
                Instruction::QLtFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        let r = a_ref.as_f64() < b_ref.as_f64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Lt;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a < b, |a, b| a < b)?;
                    }
                }

                Instruction::QLeInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let r = a_ref.as_i64() <= b_ref.as_i64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Le;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a <= b, |a, b| a <= b)?;
                    }
                }
                Instruction::QLeFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        let r = a_ref.as_f64() <= b_ref.as_f64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Le;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a <= b, |a, b| a <= b)?;
                    }
                }

                Instruction::QGtInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let r = a_ref.as_i64() > b_ref.as_i64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Gt;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a > b, |a, b| a > b)?;
                    }
                }
                Instruction::QGtFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        let r = a_ref.as_f64() > b_ref.as_f64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Gt;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a > b, |a, b| a > b)?;
                    }
                }

                Instruction::QGeInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let r = a_ref.as_i64() >= b_ref.as_i64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ge;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a >= b, |a, b| a >= b)?;
                    }
                }
                Instruction::QGeFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        let r = a_ref.as_f64() >= b_ref.as_f64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ge;
                        }
                        self.frames.last_mut().unwrap().pc =
                            unsafe { ip.offset_from(ip_base) as usize };
                        self.exec_comparison(|a, b| a >= b, |a, b| a >= b)?;
                    }
                }

                Instruction::QEqInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let r = a_ref.as_i64() == b_ref.as_i64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Eq;
                        }
                        let eq = self.stack[len - 2] == self.stack[len - 1];
                        self.stack[len - 2] = Value::Bool(eq);
                        self.stack.truncate(len - 1);
                    }
                }
                Instruction::QEqFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        let r = a_ref.as_f64() == b_ref.as_f64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Eq;
                        }
                        let eq = self.stack[len - 2] == self.stack[len - 1];
                        self.stack[len - 2] = Value::Bool(eq);
                        self.stack.truncate(len - 1);
                    }
                }

                Instruction::QNeInt => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_int() && b_ref.is_int() {
                        let r = a_ref.as_i64() != b_ref.as_i64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ne;
                        }
                        let ne = self.stack[len - 2] != self.stack[len - 1];
                        self.stack[len - 2] = Value::Bool(ne);
                        self.stack.truncate(len - 1);
                    }
                }
                Instruction::QNeFloat => {
                    let len = self.stack.len();
                    let a_ref = unsafe { self.stack.get_unchecked(len - 2) };
                    let b_ref = unsafe { self.stack.get_unchecked(len - 1) };
                    if a_ref.is_float() && b_ref.is_float() {
                        let r = a_ref.as_f64() != b_ref.as_f64();
                        unsafe {
                            *self.stack.get_unchecked_mut(len - 2) = Value::Bool(r);
                            self.stack.set_len(len - 1);
                        }
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut Instruction) = Instruction::Ne;
                        }
                        let ne = self.stack[len - 2] != self.stack[len - 1];
                        self.stack[len - 2] = Value::Bool(ne);
                        self.stack.truncate(len - 1);
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
        op(a, b)
            .map(Value::I64)
            .ok_or_else(|| err_msg.to_string())
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

    /// Executes the Add instruction, handling both numeric and string addition.
    fn exec_add(&mut self) -> Result<(), RuntimeError> {
        let len = self.stack.len();
        if len < 2 {
            return Err(self.make_error("stack underflow".to_string()));
        }
        let result = match (&self.stack[len - 2], &self.stack[len - 1]) {
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
                    self.stack[len - 2].type_name(),
                    self.stack[len - 1].type_name()
                )));
            }
        };
        self.stack[len - 2] = result;
        self.stack.truncate(len - 1);
        Ok(())
    }

    /// Executes a binary arithmetic instruction (Sub, Mul).
    fn exec_binary_arith(
        &mut self,
        i32_op: fn(i32, i32) -> Option<i32>,
        i64_op: fn(i64, i64) -> Option<i64>,
        f64_op: fn(f64, f64) -> f64,
        op_name: &str,
    ) -> Result<(), RuntimeError> {
        let len = self.stack.len();
        if len < 2 {
            return Err(self.make_error("stack underflow".to_string()));
        }
        let result = match (&self.stack[len - 2], &self.stack[len - 1]) {
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
                    "cannot {op_name} {} and {}",
                    self.stack[len - 2].type_name(),
                    self.stack[len - 1].type_name()
                )));
            }
        };
        self.stack[len - 2] = result;
        self.stack.truncate(len - 1);
        Ok(())
    }

    /// Executes the Div instruction with zero-check.
    fn exec_div(&mut self) -> Result<(), RuntimeError> {
        let len = self.stack.len();
        if len < 2 {
            return Err(self.make_error("stack underflow".to_string()));
        }
        let result = match (&self.stack[len - 2], &self.stack[len - 1]) {
            (Value::I32(_) | Value::I64(_) | Value::F32(_) | Value::F64(_), b @ (Value::I32(_) | Value::I64(_)))
                if b.as_i64() == 0 =>
            {
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
                    self.stack[len - 2].type_name(),
                    self.stack[len - 1].type_name()
                )));
            }
        };
        self.stack[len - 2] = result;
        self.stack.truncate(len - 1);
        Ok(())
    }

    /// Executes the Mod instruction with zero-check.
    fn exec_mod(&mut self) -> Result<(), RuntimeError> {
        let len = self.stack.len();
        if len < 2 {
            return Err(self.make_error("stack underflow".to_string()));
        }
        let result = match (&self.stack[len - 2], &self.stack[len - 1]) {
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
                    self.stack[len - 2].type_name(),
                    self.stack[len - 1].type_name()
                )));
            }
        };
        self.stack[len - 2] = result;
        self.stack.truncate(len - 1);
        Ok(())
    }

    /// Executes a comparison instruction.
    fn exec_comparison(
        &mut self,
        i64_cmp: fn(&i64, &i64) -> bool,
        f64_cmp: fn(&f64, &f64) -> bool,
    ) -> Result<(), RuntimeError> {
        let len = self.stack.len();
        if len < 2 {
            return Err(self.make_error("stack underflow".to_string()));
        }
        let result = match (&self.stack[len - 2], &self.stack[len - 1]) {
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
                    self.stack[len - 2].type_name(),
                    self.stack[len - 1].type_name()
                )));
            }
        };
        self.stack[len - 2] = Value::Bool(result);
        self.stack.truncate(len - 1);
        Ok(())
    }

    /// Executes a function call.
    ///
    /// Resolves the callee by name: first checks compiled functions,
    /// then falls back to host-registered native functions.
    fn exec_call(&mut self, arg_count: u8) -> Result<(), RuntimeError> {
        let n = arg_count as usize;
        let callee_pos = self.stack.len() - n - 1;

        // Handle closure calls — extract data by reference, then remove callee
        if let Value::Closure(data) = &self.stack[callee_pos] {
            let func_idx = data.func_idx;
            let upvalues = data.upvalues.clone();
            let expected_arity = self.functions[func_idx].arity;

            if expected_arity != arg_count {
                return Err(self.make_error(format!(
                    "closure '{}' expects {} arguments, got {}",
                    self.functions[func_idx].name, expected_arity, arg_count
                )));
            }

            // Overwrite callee slot instead of O(n) remove
            self.stack[callee_pos] = Value::Null;
            let new_base = callee_pos + 1;

            self.frames.push(CallFrame {
                chunk_id: ChunkId::Function(func_idx),
                pc: 0,
                base: new_base,
                has_callee_slot: true,
                upvalues: Some(upvalues),
            });

            return Ok(());
        }

        // Extract function name by reference — avoid cloning the string
        let func_idx = match &self.stack[callee_pos] {
            Value::Str(s) => {
                let name: &str = s;
                // Try compiled functions first
                if let Some(&idx) = self.function_map.get(name) {
                    Some(idx)
                } else {
                    None
                }
            }
            _ => {
                return Err(self.make_error(format!(
                    "callee is not a function: {}",
                    self.stack[callee_pos].type_name()
                )));
            }
        };

        if let Some(func_idx) = func_idx {
            let expected_arity = self.functions[func_idx].arity;
            let is_variadic = self.functions[func_idx].is_variadic;

            if is_variadic {
                // Variadic: need at least (arity - 1) fixed args
                let min_args = expected_arity.saturating_sub(1);
                if arg_count < min_args {
                    return Err(self.make_error(format!(
                        "function '{}' expects at least {} arguments, got {}",
                        self.functions[func_idx].name, min_args, arg_count
                    )));
                }
                // Overwrite callee slot instead of O(n) remove
                self.stack[callee_pos] = Value::Null;
                // Pack variadic args into an array
                let fixed_count = min_args as usize;
                let variadic_count = (arg_count as usize) - fixed_count;
                let variadic_start = self.stack.len() - variadic_count;
                let variadic_args: Vec<Value> = self.stack.drain(variadic_start..).collect();
                self.stack
                    .push(Value::Array(Rc::new(RefCell::new(variadic_args))));

                let new_base = callee_pos + 1;
                self.frames.push(CallFrame {
                    chunk_id: ChunkId::Function(func_idx),
                    pc: 0,
                    base: new_base,
                    has_callee_slot: true,
                    upvalues: None,
                });
            } else {
                if expected_arity != arg_count {
                    return Err(self.make_error(format!(
                        "function '{}' expects {} arguments, got {}",
                        self.functions[func_idx].name, expected_arity, arg_count
                    )));
                }

                // Overwrite callee slot instead of O(n) remove
                self.stack[callee_pos] = Value::Null;
                let new_base = callee_pos + 1;

                self.frames.push(CallFrame {
                    chunk_id: ChunkId::Function(func_idx),
                    pc: 0,
                    base: new_base,
                    has_callee_slot: true,
                    upvalues: None,
                });
            }

            return Ok(());
        }

        // Fall back: need the actual string for native/invoke lookup
        let func_name = match &self.stack[callee_pos] {
            Value::Str(s) => (**s).clone(),
            _ => unreachable!(),
        };

        // Built-in: invoke(obj, methodName, ...args)
        if func_name == "invoke" && arg_count >= 2 {
            return self.exec_invoke(arg_count, callee_pos);
        }

        // Fall back to native functions
        self.exec_native_call(&func_name, arg_count, callee_pos)
    }

    /// Dispatches a call to a host-registered native function.
    fn exec_native_call(
        &mut self,
        func_name: &str,
        arg_count: u8,
        callee_pos: usize,
    ) -> Result<(), RuntimeError> {
        let n = arg_count as usize;

        // Look up and clone the body Rc to release the borrow on self
        let native = self
            .native_functions
            .get(func_name)
            .ok_or_else(|| self.make_error(format!("undefined function '{func_name}'")))?;

        // Check if the function's module is disabled
        if let Some(ref module) = native.module
            && self.disabled_modules.contains(module)
        {
            return Err(self.make_error(format!(
                "module '{}' is disabled; cannot call '{}'",
                module, func_name
            )));
        }

        // Validate arity
        if let Some(expected) = native.arity
            && expected != arg_count
        {
            return Err(self.make_error(format!(
                "function '{}' expects {} arguments, got {}",
                func_name, expected, arg_count
            )));
        }

        let body = Rc::clone(&native.body);

        // Remove callee string, then drain arguments
        self.stack.remove(callee_pos);
        let args: Vec<Value> = self.stack.drain(callee_pos..callee_pos + n).collect();

        // Call the native function
        let result = (body)(&args).map_err(|msg| self.make_error(msg))?;
        self.stack.push(result);

        Ok(())
    }

    /// Dispatches a method call on a value.
    ///
    /// Stack layout: [receiver, arg0, ..., argN]
    /// Result: pops receiver + args, pushes method return value.
    fn exec_call_method(&mut self, name_hash: u32, arg_count: u8) -> Result<(), RuntimeError> {
        let n = arg_count as usize;

        // Peek at receiver (below the args on the stack)
        let receiver_pos = self.stack.len() - n - 1;
        let receiver = self.stack[receiver_pos].clone();

        // Look up method by (value tag, name hash)
        if let Some(tag) = receiver.tag()
            && let Some(method) = self.methods.get(&(tag, name_hash))
        {
            // Check if the method's module is disabled
            if let Some(ref module) = method.module
                && self.disabled_modules.contains(module)
            {
                return Err(self.make_error(format!(
                    "module '{}' is disabled; cannot call '{}'",
                    module, method.name
                )));
            }

            // Validate arity
            if let Some(expected) = method.arity
                && expected != arg_count
            {
                return Err(self.make_error(format!(
                    "method '{}' expects {} arguments, got {}",
                    method.name, expected, arg_count
                )));
            }

            let body = Rc::clone(&method.body);

            // Pop args, then pop receiver
            let args: Vec<Value> = self
                .stack
                .drain(receiver_pos + 1..receiver_pos + 1 + n)
                .collect();
            self.stack.remove(receiver_pos);

            // Call the method
            let result = (body)(&receiver, &args).map_err(|msg| self.make_error(msg))?;
            self.stack.push(result);

            return Ok(());
        }

        // Built-in callback methods that need VM access (map, filter, reduce)
        if let Value::Array(ref arr) = receiver {
            let map_hash = string_hash("map");
            let filter_hash = string_hash("filter");
            let reduce_hash = string_hash("reduce");

            if name_hash == map_hash && arg_count == 1 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 2)
                    .collect();
                self.stack.remove(receiver_pos);

                let fn_name = match &args[0] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("map expects a function".to_string())),
                };

                let items = arr.borrow().clone();
                let mut result = Vec::with_capacity(items.len());
                for item in &items {
                    let val = self.call_function(&fn_name, std::slice::from_ref(item))?;
                    result.push(val);
                }
                self.stack.push(Value::Array(Rc::new(RefCell::new(result))));
                return Ok(());
            }

            if name_hash == filter_hash && arg_count == 1 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 2)
                    .collect();
                self.stack.remove(receiver_pos);

                let fn_name = match &args[0] {
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
                self.stack.push(Value::Array(Rc::new(RefCell::new(result))));
                return Ok(());
            }

            if name_hash == reduce_hash && arg_count == 2 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 3)
                    .collect();
                self.stack.remove(receiver_pos);

                let fn_name = match &args[0] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("reduce expects a function".to_string())),
                };

                let items = arr.borrow().clone();
                let mut acc = args[1].clone();
                for item in &items {
                    acc = self.call_function(&fn_name, &[acc, item.clone()])?;
                }
                self.stack.push(acc);
                return Ok(());
            }
        }

        // AoSoA method dispatch: map, filter, for_each, iter_field
        #[cfg(feature = "mobile-aosoa")]
        if let Value::AoSoA(ref container) = receiver {
            let map_hash = string_hash("map");
            let filter_hash = string_hash("filter");
            let for_each_hash = string_hash("for_each");
            let iter_field_hash = string_hash("iter_field");

            if name_hash == map_hash && arg_count == 1 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 2)
                    .collect();
                self.stack.remove(receiver_pos);
                let fn_name = match &args[0] {
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
                self.stack.push(Value::Array(Rc::new(RefCell::new(result))));
                return Ok(());
            }

            if name_hash == filter_hash && arg_count == 1 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 2)
                    .collect();
                self.stack.remove(receiver_pos);
                let fn_name = match &args[0] {
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
                self.stack.push(Value::Array(Rc::new(RefCell::new(result))));
                return Ok(());
            }

            if name_hash == for_each_hash && arg_count == 1 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 2)
                    .collect();
                self.stack.remove(receiver_pos);
                let fn_name = match &args[0] {
                    Value::Str(s) => (**s).clone(),
                    _ => return Err(self.make_error("for_each expects a function".to_string())),
                };
                let len = container.borrow().len();
                for i in 0..len {
                    let elem = container.borrow().get(i).unwrap();
                    let item = Value::Struct(Box::new(elem));
                    self.call_function(&fn_name, std::slice::from_ref(&item))?;
                }
                self.stack.push(Value::Null);
                return Ok(());
            }

            if name_hash == iter_field_hash && arg_count == 1 {
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 2)
                    .collect();
                self.stack.remove(receiver_pos);
                let field_name = match &args[0] {
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
                self.stack.push(Value::Array(Rc::new(RefCell::new(values))));
                return Ok(());
            }
        }

        // Object method dispatch: try compiled class methods first, then WritObject::call_method
        if let Value::Object(ref obj) = receiver {
            let method_name = self
                .field_names
                .get(&name_hash)
                .cloned()
                .unwrap_or_else(|| format!("<unknown:{name_hash}>"));

            // Check if this is a WritClassInstance — try compiled method dispatch
            // Walk the inheritance chain: ClassName::method, then ParentName::method, etc.
            let class_name = obj.borrow().type_name().to_string();
            let mut search_class = Some(class_name.clone());
            let mut found_func = None;
            while let Some(ref cls) = search_class {
                let qualified = format!("{cls}::{method_name}");
                if let Some(&func_idx) = self.function_map.get(&qualified) {
                    found_func = Some(func_idx);
                    break;
                }
                // Walk up the inheritance chain
                search_class = self.class_metas.get(cls).and_then(|m| m.parent.clone());
            }

            if let Some(func_idx) = found_func {
                // Pop args and receiver
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 1 + n)
                    .collect();
                self.stack.remove(receiver_pos);

                // Call with self + args
                let mut all_args = Vec::with_capacity(1 + args.len());
                all_args.push(receiver);
                all_args.extend(args);

                let result = self.call_compiled_function(func_idx, &all_args)?;
                self.stack.push(result);
                return Ok(());
            }

            // Fall back to WritObject::call_method for host-registered objects
            let args: Vec<Value> = self
                .stack
                .drain(receiver_pos + 1..receiver_pos + 1 + n)
                .collect();
            self.stack.remove(receiver_pos);

            let result = obj
                .borrow_mut()
                .call_method(&method_name, &args)
                .map_err(|e| self.make_error(e))?;
            self.stack.push(result);
            return Ok(());
        }

        // Struct method dispatch: look up `TypeName::method_name` function
        if let Value::Struct(ref s) = receiver {
            let method_name = self
                .field_names
                .get(&name_hash)
                .cloned()
                .unwrap_or_else(|| format!("<unknown:{name_hash}>"));

            let qualified = format!("{}::{}", s.type_name, method_name);
            if let Some(&func_idx) = self.function_map.get(&qualified) {
                // Pop args and receiver
                let args: Vec<Value> = self
                    .stack
                    .drain(receiver_pos + 1..receiver_pos + 1 + n)
                    .collect();
                self.stack.remove(receiver_pos);

                // Call with self + args
                let mut all_args = Vec::with_capacity(1 + args.len());
                all_args.push(receiver);
                all_args.extend(args);

                let result = self.call_compiled_function(func_idx, &all_args)?;
                self.stack.push(result);
                return Ok(());
            }
        }

        let method_name = self
            .field_names
            .get(&name_hash)
            .cloned()
            .unwrap_or_else(|| format!("<unknown:{name_hash}>"));

        Err(self.make_error(format!(
            "no method '{}' on type '{}'",
            method_name,
            receiver.type_name()
        )))
    }

    /// Executes GetField on an object.
    /// Executes the `invoke(obj, methodName, ...args)` reflection function.
    fn exec_invoke(&mut self, arg_count: u8, callee_pos: usize) -> Result<(), RuntimeError> {
        let n = arg_count as usize;

        // Remove callee string ("invoke"), then collect all args
        self.stack.remove(callee_pos);
        let args: Vec<Value> = self.stack.drain(callee_pos..callee_pos + n).collect();

        if args.len() < 2 {
            return Err(self.make_error(
                "invoke requires at least 2 arguments: (obj, methodName)".to_string(),
            ));
        }

        let obj = &args[0];
        let method_name = match &args[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Err(self.make_error("invoke expects a string method name".to_string())),
        };
        let method_args = &args[2..];

        // Struct method dispatch
        if let Value::Struct(s) = obj {
            let qualified = format!("{}::{}", s.type_name, method_name);
            if let Some(&func_idx) = self.function_map.get(&qualified) {
                let mut all_args = Vec::with_capacity(1 + method_args.len());
                all_args.push(obj.clone());
                all_args.extend_from_slice(method_args);
                let result = self.call_compiled_function(func_idx, &all_args)?;
                self.stack.push(result);
                return Ok(());
            }
            return Err(self.make_error(format!(
                "no method '{}' on type '{}'",
                method_name, s.type_name
            )));
        }

        // Object method dispatch
        if let Value::Object(o) = obj {
            let result = o
                .borrow_mut()
                .call_method(&method_name, method_args)
                .map_err(|e| self.make_error(e))?;
            self.stack.push(result);
            return Ok(());
        }

        Err(self.make_error(format!(
            "invoke not supported on type '{}'",
            obj.type_name()
        )))
    }

    /// Constructs a struct instance from values on the stack.
    fn exec_make_struct(
        &mut self,
        name_idx: u32,
        field_count: u16,
        chunk_id: ChunkId,
    ) -> Result<(), RuntimeError> {
        // Resolve struct name from the chunk's string pool
        let chunk = self.chunk_for(chunk_id);
        let struct_name = chunk
            .rc_strings()
            .get(name_idx as usize)
            .map(|s| s.as_str().to_string())
            .ok_or_else(|| self.make_error(format!("invalid struct name index {name_idx}")))?;

        // Get struct metadata
        let meta = self
            .struct_metas
            .get(&struct_name)
            .cloned()
            .ok_or_else(|| self.make_error(format!("unknown struct type '{struct_name}'")))?;

        // Pop field values from stack (in declaration order)
        let n = field_count as usize;
        if self.stack.len() < n {
            return Err(self.make_error("stack underflow in MakeStruct".to_string()));
        }
        let values: Vec<Value> = self.stack.drain(self.stack.len() - n..).collect();

        // Build field map
        let mut fields = HashMap::new();
        for (i, name) in meta.field_names.iter().enumerate() {
            if i < values.len() {
                fields.insert(name.clone(), values[i].clone());
            }
        }

        let writ_struct = crate::writ_struct::WritStruct {
            type_name: struct_name,
            fields,
            field_order: meta.field_names.clone(),
            public_fields: meta.public_fields.clone(),
            public_methods: meta.public_methods.clone(),
        };

        self.stack.push(Value::Struct(Box::new(writ_struct)));
        Ok(())
    }

    fn exec_make_class(
        &mut self,
        name_idx: u32,
        field_count: u16,
        chunk_id: ChunkId,
    ) -> Result<(), RuntimeError> {
        // Resolve class name from the chunk's string pool
        let chunk = self.chunk_for(chunk_id);
        let class_name = chunk
            .rc_strings()
            .get(name_idx as usize)
            .map(|s| s.as_str().to_string())
            .ok_or_else(|| self.make_error(format!("invalid class name index {name_idx}")))?;

        // Get class metadata
        let meta = self
            .class_metas
            .get(&class_name)
            .cloned()
            .ok_or_else(|| self.make_error(format!("unknown class type '{class_name}'")))?;

        // Pop field values from stack (in declaration order, parent fields first)
        let n = field_count as usize;
        if self.stack.len() < n {
            return Err(self.make_error("stack underflow in MakeClass".to_string()));
        }
        let values: Vec<Value> = self.stack.drain(self.stack.len() - n..).collect();

        // Build field map
        let mut fields = HashMap::new();
        for (i, name) in meta.field_names.iter().enumerate() {
            if i < values.len() {
                fields.insert(name.clone(), values[i].clone());
            }
        }

        let instance = crate::class_instance::WritClassInstance {
            class_name,
            fields,
            field_order: meta.field_names.clone(),
            parent_class: meta.parent.clone(),
        };

        self.stack
            .push(Value::Object(Rc::new(RefCell::new(instance))));
        Ok(())
    }

    fn exec_get_field(&mut self, name_hash: u32) -> Result<(), RuntimeError> {
        let object = self.pop()?;
        match &object {
            Value::Array(arr) => {
                let length_hash = string_hash("length");
                if name_hash == length_hash {
                    let len = arr.borrow().len() as i32;
                    self.stack.push(Value::I32(len));
                } else {
                    return Err(self.make_error(format!("unknown array field (hash {name_hash})")));
                }
            }
            Value::Dict(dict) => {
                let dict = dict.borrow();
                let val = dict
                    .iter()
                    .find(|(key, _)| string_hash(key) == name_hash)
                    .map(|(_, v)| v.clone());
                self.stack.push(val.unwrap_or(Value::Null));
            }
            Value::Object(obj) => {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?
                    .clone();
                let val = obj
                    .borrow()
                    .get_field(&field_name)
                    .map_err(|e| self.make_error(e))?;
                self.stack.push(val);
            }
            Value::Struct(s) => {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?
                    .clone();
                let val = s.get_field(&field_name).cloned().ok_or_else(|| {
                    self.make_error(format!("'{}' has no field '{}'", s.type_name, field_name))
                })?;
                self.stack.push(val);
            }
            #[cfg(feature = "mobile-aosoa")]
            Value::AoSoA(container) => {
                let length_hash = string_hash("length");
                if name_hash == length_hash {
                    let len = container.borrow().len() as i32;
                    self.stack.push(Value::I32(len));
                } else {
                    return Err(self.make_error(format!("unknown AoSoA field (hash {name_hash})")));
                }
            }
            _ => {
                return Err(self.make_error(format!("field access on {}", object.type_name())));
            }
        }
        Ok(())
    }

    /// Executes SetField on an object.
    fn exec_set_field(&mut self, name_hash: u32) -> Result<(), RuntimeError> {
        let value = self.pop()?;
        let object = self.pop()?;
        match object {
            Value::Dict(ref dict) => {
                let mut dict = dict.borrow_mut();
                // Find existing key by matching hash
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
            Value::Object(ref obj) => {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?
                    .clone();
                obj.borrow_mut()
                    .set_field(&field_name, value)
                    .map_err(|e| self.make_error(e))?;
            }
            Value::Struct(mut s) => {
                let field_name = self
                    .field_names
                    .get(&name_hash)
                    .ok_or_else(|| self.make_error(format!("unknown field (hash {name_hash})")))?
                    .clone();
                s.set_field(&field_name, value)
                    .map_err(|e| self.make_error(e))?;
                // Push modified struct back (caller emits StoreLocal to save)
                self.stack.push(Value::Struct(s));
            }
            _ => {
                return Err(self.make_error(format!("field assignment on {}", object.type_name())));
            }
        }
        Ok(())
    }

    /// Executes GetIndex on a collection.
    fn exec_get_index(&mut self) -> Result<(), RuntimeError> {
        let index = self.pop()?;
        let collection = self.pop()?;
        match (&collection, &index) {
            (Value::Array(arr), idx_val @ (Value::I32(_) | Value::I64(_))) => {
                let arr = arr.borrow();
                let i = idx_val.as_i64();
                let idx = if i < 0 {
                    return Err(self.make_error(format!(
                        "array index {i} out of bounds (length {})",
                        arr.len()
                    )));
                } else {
                    i as usize
                };
                if idx >= arr.len() {
                    return Err(self.make_error(format!(
                        "array index {i} out of bounds (length {})",
                        arr.len()
                    )));
                }
                self.stack.push(arr[idx].clone());
            }
            (Value::Dict(dict), Value::Str(key)) => {
                let dict = dict.borrow();
                let val = dict.get(key.as_str()).cloned().unwrap_or(Value::Null);
                self.stack.push(val);
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
                self.stack.push(Value::Struct(Box::new(writ_struct)));
            }
            _ => {
                return Err(self.make_error(format!(
                    "cannot index {} with {}",
                    collection.type_name(),
                    index.type_name()
                )));
            }
        }
        Ok(())
    }

    /// Executes SetIndex on a collection.
    fn exec_set_index(&mut self) -> Result<(), RuntimeError> {
        let value = self.pop()?;
        let index = self.pop()?;
        let collection = self.pop()?;
        match (&collection, &index) {
            (Value::Array(arr), idx_val @ (Value::I32(_) | Value::I64(_))) => {
                let mut arr = arr.borrow_mut();
                let i = idx_val.as_i64();
                let idx = if i < 0 {
                    return Err(self.make_error(format!(
                        "array index {i} out of bounds (length {})",
                        arr.len()
                    )));
                } else {
                    i as usize
                };
                if idx >= arr.len() {
                    return Err(self.make_error(format!(
                        "array index {i} out of bounds (length {})",
                        arr.len()
                    )));
                }
                arr[idx] = value;
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

    // ── AoSoA helpers (mobile only) ─────────────────────────────────

    /// Converts the top-of-stack Array into an AoSoA container if all elements
    /// are structs of the same type. Falls back to keeping the regular Array
    /// if the elements are not homogeneous structs.
    #[cfg(feature = "mobile-aosoa")]
    fn exec_convert_to_aosoa(&mut self) -> Result<(), RuntimeError> {
        use crate::aosoa::AoSoAContainer;

        let top = self.pop()?;
        match top {
            Value::Array(ref arr) => {
                let elements = arr.borrow();

                // Check if all elements are structs of the same type.
                let first_type = match elements.first() {
                    Some(Value::Struct(s)) => Some(s.type_name.clone()),
                    _ => None,
                };

                let type_name = match first_type {
                    Some(name)
                        if elements
                            .iter()
                            .all(|v| matches!(v, Value::Struct(s) if s.type_name == name)) =>
                    {
                        name
                    }
                    _ => {
                        // Not homogeneous structs — keep as regular array.
                        drop(elements);
                        self.stack.push(top);
                        return Ok(());
                    }
                };

                // Look up struct metadata.
                let meta = match self.struct_metas.get(&type_name) {
                    Some(meta) => meta.clone(),
                    None => {
                        // No metadata — keep as regular array.
                        drop(elements);
                        self.stack.push(top);
                        return Ok(());
                    }
                };

                // Build the AoSoA container.
                let mut container = AoSoAContainer::new(
                    type_name,
                    meta.field_names.clone(),
                    meta.public_fields.clone(),
                    meta.public_methods.clone(),
                    elements.len(),
                );
                for elem in elements.iter() {
                    if let Value::Struct(s) = elem {
                        container.push(s).map_err(|e| self.make_error(e))?;
                    }
                }
                drop(elements);
                self.stack
                    .push(Value::AoSoA(Rc::new(RefCell::new(container))));
            }
            other => {
                // Not an array — push back unchanged.
                self.stack.push(other);
            }
        }
        Ok(())
    }

    // ── Closure helpers ──────────────────────────────────────────

    /// Executes a `MakeClosure` instruction: creates a `Value::Closure`
    /// from the function at `func_idx` and captures upvalues per the
    /// function's upvalue descriptors.
    fn exec_make_closure(&mut self, func_idx: u16) -> Result<(), RuntimeError> {
        let func_idx = func_idx as usize;
        let descriptors = self.functions[func_idx].upvalues.clone();
        let mut upvalues = Vec::with_capacity(descriptors.len());

        for desc in &descriptors {
            if desc.is_local {
                // Capture a local from the current frame
                let abs_slot = self.current_frame().base + desc.index as usize;
                let cell = self.capture_local(abs_slot);
                upvalues.push(cell);
            } else {
                // Transitive capture: clone Rc from parent frame's upvalue array
                let parent_uv = self
                    .current_frame()
                    .upvalues
                    .as_ref()
                    .expect("transitive capture requires parent closure");
                upvalues.push(Rc::clone(&parent_uv[desc.index as usize]));
            }
        }

        self.stack
            .push(Value::Closure(Box::new(ClosureData { func_idx, upvalues })));
        Ok(())
    }

    /// Returns a shared cell for the local at the given absolute stack slot.
    /// If an open upvalue already exists for this slot, reuses it. Otherwise
    /// creates a new one initialized with the current stack value.
    fn capture_local(&mut self, abs_slot: usize) -> Rc<RefCell<Value>> {
        if let Some(existing) = self.open_upvalues.get(&abs_slot) {
            return Rc::clone(existing);
        }
        let cell = Rc::new(RefCell::new(self.stack[abs_slot].clone()));
        self.open_upvalues.insert(abs_slot, Rc::clone(&cell));
        self.has_open_upvalues = true;
        cell
    }

    /// Closes all open upvalues at or above `min_slot` by syncing the
    /// current stack value into their heap cells, then removing them
    /// from the open set.
    fn close_upvalues_above(&mut self, min_slot: usize) {
        self.open_upvalues.retain(|&slot, cell| {
            if slot >= min_slot {
                if slot < self.stack.len() {
                    *cell.borrow_mut() = self.stack[slot].clone();
                }
                false
            } else {
                true
            }
        });
        self.has_open_upvalues = !self.open_upvalues.is_empty();
    }

    // ── Coroutine helpers ─────────────────────────────────────────

    /// Creates a new coroutine from a function call on the stack.
    fn exec_start_coroutine(&mut self, arg_count: u8) -> Result<(), RuntimeError> {
        let n = arg_count as usize;
        let callee_pos = self.stack.len() - n - 1;
        let callee = self.stack[callee_pos].clone();

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

        // Remove callee, drain args
        self.stack.remove(callee_pos);
        let args: Vec<Value> = self.stack.drain(callee_pos..callee_pos + n).collect();

        // Create the coroutine
        let id = self.next_coroutine_id;
        self.next_coroutine_id += 1;

        let frame = CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            has_callee_slot: false,
            upvalues: closure_upvalues,
        };

        let coro = Coroutine {
            id,
            state: CoroutineState::Running,
            stack: args,
            frames: vec![frame],
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: HashMap::new(),
            children: Vec::new(),
        };

        self.coroutines.push(coro);

        // Register as child of parent
        if let Some(parent_idx) = self.active_coroutine {
            self.coroutines[parent_idx].children.push(id);
        }

        // Push coroutine handle onto current stack
        self.stack.push(Value::CoroutineHandle(id));
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
                        Some(WaitCondition::Coroutine { child_id }) => {
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
                Some(WaitCondition::Coroutine { child_id }) => Some(*child_id),
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

        // Push the resume value onto the coroutine's stack.
        // Yield is an expression, so when the coroutine resumes, the yield
        // evaluates to a value (Null for most yields, child's return value
        // for YieldCoroutine). First-run coroutines (wait == None) skip this.
        match &self.coroutines[idx].wait {
            Some(WaitCondition::Coroutine { child_id }) => {
                let child_id = *child_id;
                let return_value = self
                    .coroutines
                    .iter()
                    .find(|c| c.id == child_id)
                    .and_then(|c| c.return_value.clone())
                    .unwrap_or(Value::Null);
                self.coroutines[idx].stack.push(return_value);
            }
            Some(_) => {
                self.coroutines[idx].stack.push(Value::Null);
            }
            None => {} // First run, no resume value needed
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
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            has_callee_slot: false,
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
