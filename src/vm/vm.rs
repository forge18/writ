use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::compiler::opcode::{INSTR_WORDS, op};
use crate::compiler::{Chunk, ClassMeta, CompiledFunction, StructMeta, string_hash};

use super::coroutine::{Coroutine, CoroutineId, CoroutineState, WaitCondition};
#[cfg(feature = "debug-hooks")]
use super::debug::{BreakpointAction, BreakpointContext};
use super::debug::{BreakpointHandler, BreakpointKey, CallHook, LineHook, StepState};
use super::error::{RuntimeError, StackFrame, StackTrace};
use super::frame::{CallFrame, ChunkId};
use super::native::{NativeFunction, NativeMethod};
use super::object::WritObject;
use super::value::{ClosureData, Value, ValueTag};

/// Internal result type for the run loop, distinguishing normal returns
/// from coroutine yield points.
enum RunResult {
    /// Normal return (function completed or end of script).
    Return(Value),
    /// Coroutine yielded with a wait condition.
    Yield(WaitCondition),
}

/// Bundled operation function pointers for binary arithmetic helpers.
struct ArithOps {
    i32_op: fn(i32, i32) -> Option<i32>,
    i64_op: fn(i64, i64) -> Option<i64>,
    f64_op: fn(f64, f64) -> f64,
    method_name: &'static str,
}

/// Bundled comparison function pointers for comparison helpers.
struct CmpOps {
    i64_cmp: fn(&i64, &i64) -> bool,
    f64_cmp: fn(&f64, &f64) -> bool,
    method_name: &'static str,
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
    /// Pre-computed code pointer + word length for each function chunk.
    /// Index 0 = main chunk, index 1..N = function chunks.
    /// Points to the compact u32-encoded bytecode (`chunk.code()`).
    func_ip_cache: Vec<(*const u32, usize)>,
    /// Function table: compiled functions indexed by position.
    functions: Vec<CompiledFunction>,
    /// Maximum number of instructions before termination. `None` = unlimited.
    instruction_limit: Option<u64>,

    // ── Cache line 2: warm fields (closures) ─────────────────────
    /// Upvalue index arrays for each call frame, kept in sync with `frames`.
    /// `None` for non-closure frames, `Some(indices)` for closures.
    frame_upvalues: Vec<Option<Vec<u32>>>,
    /// Open upvalues: indexed by absolute stack slot. `Some(idx)` when a
    /// local has been captured; the value is an index into `upvalue_store`.
    open_upvalues: Vec<Option<u32>>,
    /// Flat upvalue value store. Closures reference slots by `u32` index.
    upvalue_store: Vec<Value>,
    /// Pre-captured upvalue index arrays for top-level functions, indexed by func_idx.
    /// Populated by `MakeClosure` while the main chunk stack is live.
    /// Enables `call_function` to provide correct upvalues after the main chunk exits.
    closure_map: Vec<Option<Vec<u32>>>,

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
    /// Indexed native function table for `CallNative` fast-path dispatch.
    native_fn_vec: Vec<NativeFunction>,
    /// Maps function name → index in `native_fn_vec` for compiler use.
    native_fn_name_to_idx: HashMap<String, u32>,
    /// Host-registered methods on value types, keyed by (value tag, method name hash).
    methods: HashMap<(ValueTag, u32), NativeMethod>,
    /// Global constants/variables, keyed by name hash → (name, value).
    globals: HashMap<u32, (String, Value)>,
    /// Struct metadata for constructing struct instances at runtime.
    struct_metas: HashMap<String, StructMeta>,
    /// Class metadata for constructing class instances at runtime.
    class_metas: HashMap<String, ClassMeta>,
    /// Pre-built shared field layouts for struct types (hash→index maps).
    struct_layouts: HashMap<String, Rc<super::field_layout::FieldLayout>>,
    /// Pre-built shared field layouts for class types (hash→index maps).
    class_layouts: HashMap<String, Rc<super::field_layout::FieldLayout>>,
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
    /// String interner for deduplicating `Rc<str>` across chunks.
    interner: super::intern::StringInterner,
}

impl VM {
    /// Creates a new VM with empty state.
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(1024),
            frames: Vec::with_capacity(64),
            main_chunk: Chunk::new(),
            functions: Vec::new(),
            function_map: HashMap::new(),
            field_names: HashMap::new(),
            native_functions: HashMap::new(),
            native_fn_vec: Vec::new(),
            native_fn_name_to_idx: HashMap::new(),
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
            frame_upvalues: Vec::with_capacity(64),
            open_upvalues: Vec::new(),
            upvalue_store: Vec::new(),
            closure_map: Vec::new(),
            has_open_upvalues: false,
            func_ip_cache: Vec::new(),
            interner: super::intern::StringInterner::new(),
        }
    }

    /// Registers a typed Rust function callable from Writ scripts.
    ///
    /// The function's parameter and return types must implement [`FromValue`]
    /// and [`IntoValue`] respectively. Arity is inferred from the function
    /// signature — no manual arity argument is needed.
    pub fn register_fn<H>(&mut self, name: &str, handler: H) -> &mut Self
    where
        H: super::binding::IntoNativeHandler,
    {
        let nf = NativeFunction::from_handler(name, None, handler);
        self.register_native_fn(name, nf);
        self
    }

    /// Registers a typed native function within a named module.
    ///
    /// Functions in a disabled module (via [`disable_module`](Self::disable_module))
    /// will produce a [`RuntimeError`] when called.
    pub fn register_fn_in_module<H>(&mut self, name: &str, module: &str, handler: H) -> &mut Self
    where
        H: super::binding::IntoNativeHandler,
    {
        let nf = NativeFunction::from_handler(name, Some(module), handler);
        self.register_native_fn(name, nf);
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
        H: super::binding::IntoNativeMethodHandler,
    {
        let hash = string_hash(name);
        self.methods.insert(
            (tag, hash),
            NativeMethod::from_handler(name, module, handler),
        );
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
        let nf = NativeFunction {
            name: name.to_string(),
            module: None,
            arity: None,
            body: Rc::new(move |args| {
                let obj = factory(args).map_err(|e| format!("{}: {}", name_owned, e))?;
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        };
        self.register_native_fn(name, nf);
        self
    }

    /// Internal helper: inserts `nf` into both `native_functions` and `native_fn_vec`.
    fn register_native_fn(&mut self, name: &str, nf: NativeFunction) {
        let idx = self.native_fn_vec.len() as u32;
        let nf2 = NativeFunction {
            name: nf.name.clone(),
            module: nf.module.clone(),
            arity: nf.arity,
            body: Rc::clone(&nf.body),
        };
        if let Some(&existing_idx) = self.native_fn_name_to_idx.get(name) {
            self.native_fn_vec[existing_idx as usize] = nf2;
        } else {
            self.native_fn_vec.push(nf2);
            self.native_fn_name_to_idx.insert(name.to_string(), idx);
        }
        self.native_functions.insert(name.to_string(), nf);
    }

    /// Returns the native function name→index map, for use by the compiler.
    pub fn native_fn_index_map(&self) -> &HashMap<String, u32> {
        &self.native_fn_name_to_idx
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
    #[cfg(feature = "debug-hooks")]
    pub fn set_breakpoint(&mut self, file: &str, line: u32) -> &mut Self {
        self.breakpoints.insert(BreakpointKey {
            file: file.to_string(),
            line,
        });
        self.update_debug_flag();
        self
    }

    /// Removes a breakpoint at the given file and line.
    #[cfg(feature = "debug-hooks")]
    pub fn remove_breakpoint(&mut self, file: &str, line: u32) -> &mut Self {
        self.breakpoints.remove(&BreakpointKey {
            file: file.to_string(),
            line,
        });
        self.update_debug_flag();
        self
    }

    /// Registers a callback invoked when a breakpoint is hit.
    #[cfg(feature = "debug-hooks")]
    pub fn on_breakpoint<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&BreakpointContext) -> BreakpointAction + 'static,
    {
        self.breakpoint_handler = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Registers a hook called before each new source line executes.
    #[cfg(feature = "debug-hooks")]
    pub fn on_line<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, u32) + 'static,
    {
        self.on_line_hook = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Registers a hook called when any function is entered.
    #[cfg(feature = "debug-hooks")]
    pub fn on_call<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, &str, u32) + 'static,
    {
        self.on_call_hook = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Registers a hook called when any function returns.
    #[cfg(feature = "debug-hooks")]
    pub fn on_return<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, &str, u32) + 'static,
    {
        self.on_return_hook = Some(Box::new(handler));
        self.update_debug_flag();
        self
    }

    /// Recomputes the fast-path debug flag.
    #[cfg(feature = "debug-hooks")]
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
    ///
    /// # Deprecation
    ///
    /// This method bypasses the type checker. Use [`Writ::reload`] instead,
    /// which runs the full lex → parse → type-check → compile pipeline before
    /// swapping bytecode. If you must call the VM directly, use
    /// [`reload_compiled`](VM::reload_compiled) with pre-compiled output.
    #[deprecated(note = "bypasses type checking; use Writ::reload or VM::reload_compiled instead")]
    pub fn reload(&mut self, file: &str, source: &str) -> Result<(), String> {
        let mut lexer = crate::lexer::Lexer::new(source);
        let tokens = lexer.tokenize().map_err(|e| format!("{e}"))?;

        let mut parser = crate::parser::Parser::new(tokens);
        let stmts = parser.parse_program().map_err(|e| format!("{e}"))?;

        let mut compiler = crate::compiler::Compiler::new();
        for stmt in &stmts {
            compiler.compile_stmt(stmt).map_err(|e| format!("{e}"))?;
        }
        let (_new_main, new_functions, new_struct_metas, new_class_metas) = compiler.into_parts();

        for meta in new_struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta);
        }

        for meta in new_class_metas {
            self.class_metas.insert(meta.name.clone(), meta);
        }

        for mut new_func in new_functions {
            new_func.chunk.set_file(file);
            new_func.chunk.intern_strings(&mut self.interner);
            if let Some(&idx) = self.function_map.get(&new_func.name) {
                self.functions[idx] = new_func;
                // Invalidate cached closure upvalues; will be re-captured on next run.
                if idx < self.closure_map.len() {
                    self.closure_map[idx] = None;
                }
            } else {
                let idx = self.functions.len();
                self.function_map.insert(new_func.name.clone(), idx);
                self.functions.push(new_func);
                self.closure_map.push(None);
            }
        }

        self.build_layouts();
        self.rebuild_ip_cache();

        Ok(())
    }

    /// Swaps function bytecode from a pre-compiled reload.
    ///
    /// This is the hot-reload counterpart to [`load_module`](VM::load_module).
    /// It replaces existing functions by name and upserts new ones, but does
    /// not execute the main chunk (hot reload preserves existing VM state).
    ///
    /// Callers are responsible for running lex → parse → type-check → compile
    /// before calling this method. Use [`Writ::reload`] rather than calling
    /// this directly when an embedder-level type check is needed.
    pub fn reload_compiled(
        &mut self,
        file: &str,
        _chunk: &Chunk,
        functions: &[CompiledFunction],
        struct_metas: &[StructMeta],
        class_metas: &[ClassMeta],
    ) -> Result<(), RuntimeError> {
        for meta in struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta.clone());
        }

        for meta in class_metas {
            self.class_metas.insert(meta.name.clone(), meta.clone());
        }

        for mut new_func in functions.iter().cloned() {
            new_func.chunk.set_file(file);
            new_func.chunk.intern_strings(&mut self.interner);
            if let Some(&idx) = self.function_map.get(&new_func.name) {
                self.functions[idx] = new_func;
                if idx < self.closure_map.len() {
                    self.closure_map[idx] = None;
                }
            } else {
                let idx = self.functions.len();
                self.function_map.insert(new_func.name.clone(), idx);
                self.functions.push(new_func);
                self.closure_map.push(None);
            }
        }

        self.build_layouts();
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
        for func in functions {
            let idx = self.functions.len();
            self.function_map.insert(func.name.clone(), idx);
            self.functions.push(func.clone());
            self.closure_map.push(None);
        }

        for meta in struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta.clone());
        }

        for meta in class_metas {
            self.class_metas.insert(meta.name.clone(), meta.clone());
        }

        for s in chunk.rc_strings() {
            self.field_names
                .insert(string_hash(s), s.to_string());
        }
        for func in functions {
            for s in func.chunk.rc_strings() {
                self.field_names
                    .insert(string_hash(s), s.to_string());
            }
        }

        // Intern strings in newly-added function chunks
        let start = self.functions.len() - functions.len();
        for func in &mut self.functions[start..] {
            func.chunk.intern_strings(&mut self.interner);
        }

        self.build_layouts();

        self.stack.clear();
        self.frames.clear();
        self.frame_upvalues.clear();
        self.instruction_count = 0;
        // Clone required: the VM quickens (mutates) this chunk in-place and
        // `rebuild_ip_cache` captures raw pointers into `main_chunk.code`.
        // Do not remove this clone. See `Chunk` doc comment for details.
        self.main_chunk = chunk.clone();
        self.main_chunk.intern_strings(&mut self.interner);

        self.rebuild_ip_cache();

        let main_max = self.main_chunk.len().min(255) as u8;
        self.ensure_registers(0, main_max.max(16));
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Main,
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: main_max.max(16),
            has_rc_values: true,
        });
        self.frame_upvalues.push(None);

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
        let upvalues = if !self.functions[func_idx].upvalues.is_empty() {
            self.closure_map.get(func_idx).and_then(|e| e.clone())
        } else {
            None
        };
        let has_upvalues = upvalues.is_some();
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base,
            result_reg: result_slot,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values || has_upvalues,
        });
        self.frame_upvalues.push(upvalues);

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

        let upvalues = if !self.functions[func_idx].upvalues.is_empty() {
            self.closure_map.get(func_idx).and_then(|e| e.clone())
        } else {
            None
        };
        let has_upvalues = upvalues.is_some();
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base,
            result_reg: base,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values || has_upvalues,
        });
        self.frame_upvalues.push(upvalues);

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
        self.closure_map.clear();
        self.field_names.clear();
        self.struct_metas.clear();
        self.class_metas.clear();
        self.struct_layouts.clear();
        self.class_layouts.clear();
        self.instruction_count = 0;
        self.coroutines.clear();
        self.next_coroutine_id = 1;
        self.active_coroutine = None;
        self.interner.clear();

        // Clone required: same quickening invariant as load_module. See `Chunk` doc.
        self.main_chunk = chunk.clone();
        self.functions = functions.to_vec();
        self.closure_map.resize(self.functions.len(), None);

        for (i, func) in self.functions.iter().enumerate() {
            self.function_map.insert(func.name.clone(), i);
        }

        for meta in struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta.clone());
        }

        for meta in class_metas {
            self.class_metas.insert(meta.name.clone(), meta.clone());
        }

        self.intern_chunk_strings();
        self.build_field_names();
        self.build_layouts();
        self.rebuild_ip_cache();

        // The compiler doesn't track max_registers for the main chunk (it's always the script body).
        let main_max = 128u8; // generous default for top-level scripts
        self.ensure_registers(0, main_max);
        self.frame_upvalues.clear();
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Main,
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: main_max,
            has_rc_values: true,
        });
        self.frame_upvalues.push(None);

        match self.run()? {
            RunResult::Return(value) => Ok(value),
            RunResult::Yield(_) => Err(self.make_error("yield outside of coroutine".to_string())),
        }
    }

    /// Builds the reverse hash → name lookup from all chunk string pools.
    /// Interns all string pool entries in main chunk and function chunks.
    fn intern_chunk_strings(&mut self) {
        self.main_chunk.intern_strings(&mut self.interner);
        for func in &mut self.functions {
            func.chunk.intern_strings(&mut self.interner);
        }
    }

    fn build_field_names(&mut self) {
        for s in self.main_chunk.rc_strings() {
            self.field_names
                .insert(string_hash(s), s.to_string());
        }
        for func in &self.functions {
            for s in func.chunk.rc_strings() {
                self.field_names
                    .insert(string_hash(s), s.to_string());
            }
        }
    }

    /// Builds shared `FieldLayout` objects from struct/class metadata.
    /// Called after loading metas so that construction and field access
    /// can use index-based `Vec<Value>` instead of `HashMap<String, Value>`.
    fn build_layouts(&mut self) {
        use super::field_layout::FieldLayout;
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
        let main_code = self.main_chunk.code();
        self.func_ip_cache
            .push((main_code.as_ptr(), main_code.len()));
        for func in &self.functions {
            let code = func.chunk.code();
            self.func_ip_cache.push((code.as_ptr(), code.len()));
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

    /// Returns cached (code_base, word_len) for the given chunk. Avoids the
    /// `chunk_for().code().as_ptr()` chain on Call/Return hot paths.
    #[inline(always)]
    fn cached_ip(&self, id: ChunkId) -> (*const u32, usize) {
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

    /// Write a value to a stack slot WITHOUT dropping the old value.
    ///
    /// # Safety
    /// Caller must ensure the old value at `slot` has no drop glue
    /// (i.e., it is I32, I64, F32, F64, Bool, Null, or CoroutineHandle).
    #[inline(always)]
    unsafe fn write_stack_nodrop(&mut self, slot: usize, value: Value) {
        unsafe { std::ptr::write(self.stack.as_mut_ptr().add(slot), value) };
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
    /// `frame.pc` is a word offset into the chunk's code vec.
    fn build_stack_trace(&self) -> StackTrace {
        let mut frames = Vec::new();
        for frame in self.frames.iter().rev() {
            let chunk = self.chunk_for(frame.chunk_id);
            let line = if frame.pc > 0 {
                chunk.line_for_word_offset(frame.pc.saturating_sub(1))
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

        // SAFETY: code vecs are not mutated during execution (only opcode bytes
        // are quickened in-place), and ip is reloaded after every Call/Return.
        let (cached_base, cached_len) = self.cached_ip(chunk_id);
        let mut ip_base: *const u32 = cached_base;
        let mut ip: *const u32 = unsafe { ip_base.add(frame.pc) };
        let mut ip_end: *const u32 = unsafe { ip_base.add(cached_len) };

        // Macro to reload ip state from a frame using the cached code pointer table.
        macro_rules! reload_ip {
            ($self:expr, $chunk_id:expr, $pc:expr) => {{
                let (base_ptr, len) = $self.cached_ip($chunk_id);
                ip_base = base_ptr;
                ip = unsafe { ip_base.add($pc) };
                ip_end = unsafe { ip_base.add(len) };
            }};
        }

        // Macro to save pc as word offset.
        macro_rules! save_pc {
            () => {
                unsafe { ip.offset_from(ip_base) as usize }
            };
        }

        loop {
            // Instruction limit enforcement (batch check every 256 instructions)
            if has_limit {
                self.instruction_count += 1;
                if self.instruction_count & 0xFF == 0
                    && self.instruction_count > self.instruction_limit.unwrap()
                {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    return Err(self.make_error("instruction limit exceeded".to_string()));
                }
            }

            // End of chunk: implicit return null
            if ip >= ip_end {
                #[cfg(feature = "debug-hooks")]
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
            #[cfg(feature = "debug-hooks")]
            if self.has_debug_hooks {
                let pc = save_pc!();
                self.frames.last_mut().unwrap().pc = pc;
                self.debug_probe(chunk_id, pc)?;
            }

            // SAFETY: ip < ip_end is guaranteed by the check above.
            let w0 = unsafe { *ip };
            ip = unsafe { ip.add(1) };
            let opc = (w0 & 0xFF) as u8;
            let a = ((w0 >> 8) & 0xFF) as u8;
            let b = ((w0 >> 16) & 0xFF) as u8;
            let c = ((w0 >> 24) & 0xFF) as u8;
            // Bx = 16-bit field in bits 16..31 (for ABx format)
            let bx = (w0 >> 16) as u16;

            match opc {
                // ── Literals ──────────────────────────────────────────
                op::LoadInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.stack[base + a as usize] = Value::I32(w1 as i32);
                }
                op::LoadConstInt => {
                    let chunk = self.chunk_for(chunk_id);
                    let v = chunk.int64_constants()[bx as usize];
                    self.stack[base + a as usize] = Value::I64(v);
                }
                op::LoadFloat => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.stack[base + a as usize] = Value::F32(f32::from_bits(w1));
                }
                op::LoadConstFloat => {
                    let chunk = self.chunk_for(chunk_id);
                    let v = chunk.float64_constants()[bx as usize];
                    self.stack[base + a as usize] = Value::F64(v);
                }
                op::LoadBool => {
                    self.stack[base + a as usize] = Value::Bool(b != 0);
                }
                op::LoadStr => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let chunk = self.chunk_for(chunk_id);
                    let s = Rc::clone(&chunk.rc_strings()[w1 as usize]);
                    self.stack[base + a as usize] = Value::Str(s);
                }
                op::LoadNull => {
                    self.stack[base + a as usize] = Value::Null;
                }
                op::Move => {
                    let val = self.stack[base + b as usize].cheap_clone();
                    let abs_dst = base + a as usize;
                    // Sync to upvalue store if destination is captured
                    if self.has_open_upvalues
                        && abs_dst < self.open_upvalues.len()
                        && let Some(uv_idx) = self.open_upvalues[abs_dst]
                    {
                        self.upvalue_store[uv_idx as usize] = val.cheap_clone();
                    }
                    self.stack[abs_dst] = val;
                }
                op::LoadGlobal => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let name_hash = w1;
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
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| format!("<unknown:{name_hash}>"));
                        Value::Str(Rc::from(name.as_str()))
                    };
                    self.stack[base + a as usize] = val;
                }

                // ── Arithmetic (generic, quickenable) ────────────────
                op::Add => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QAddInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QAddFloat;
                        },
                        _ => {}
                    }
                    self.exec_add_reg(base, a, b, c)?;
                }
                op::Sub => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QSubInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QSubFloat;
                        },
                        _ => {}
                    }
                    self.exec_binary_arith_reg(
                        base + a as usize,
                        base + b as usize,
                        base + c as usize,
                        ArithOps {
                            i32_op: i32::checked_sub,
                            i64_op: i64::checked_sub,
                            f64_op: |x, y| x - y,
                            method_name: "subtract",
                        },
                    )?;
                }
                op::Mul => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QMulInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QMulFloat;
                        },
                        _ => {}
                    }
                    self.exec_binary_arith_reg(
                        base + a as usize,
                        base + b as usize,
                        base + c as usize,
                        ArithOps {
                            i32_op: i32::checked_mul,
                            i64_op: i64::checked_mul,
                            f64_op: |x, y| x * y,
                            method_name: "multiply",
                        },
                    )?;
                }
                op::Div => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QDivInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QDivFloat;
                        },
                        _ => {}
                    }
                    self.exec_div_reg(base, a, b, c)?;
                }
                op::Mod => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_mod_reg(base, a, b, c)?;
                }

                // ── Unary ────────────────────────────────────────────
                op::Neg => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let val = &self.stack[base + b as usize];
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
                    self.stack[base + a as usize] = result;
                }
                op::Not => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let val = &self.stack[base + b as usize];
                    match val {
                        Value::Bool(v) => {
                            self.stack[base + a as usize] = Value::Bool(!v);
                        }
                        _ => {
                            return Err(
                                self.make_error(format!("cannot apply '!' to {}", val.type_name()))
                            );
                        }
                    }
                }

                // ── Type coercion ──────────────────────────────────────
                op::IntToFloat => {
                    let val = &self.stack[base + b as usize];
                    let result = match val {
                        Value::I32(v) => Value::F64(*v as f64),
                        Value::I64(v) => Value::F64(*v as f64),
                        _ => unreachable!("IntToFloat: compiler guarantees int operand"),
                    };
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }

                // ── Comparison (generic, quickenable) ────────────────
                op::Eq => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QEqInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QEqFloat;
                        },
                        _ => {}
                    }
                    let eq = self.stack[base + b as usize] == self.stack[base + c as usize];
                    self.stack[base + a as usize] = Value::Bool(eq);
                }
                op::Ne => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QNeInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QNeFloat;
                        },
                        _ => {}
                    }
                    let ne = self.stack[base + b as usize] != self.stack[base + c as usize];
                    self.stack[base + a as usize] = Value::Bool(ne);
                }
                op::Lt => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QLtInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QLtFloat;
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(
                        base,
                        a,
                        b,
                        c,
                        CmpOps {
                            i64_cmp: |x, y| x < y,
                            f64_cmp: |x, y| x < y,
                            method_name: "lt",
                        },
                    )?;
                }
                op::Le => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QLeInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QLeFloat;
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(
                        base,
                        a,
                        b,
                        c,
                        CmpOps {
                            i64_cmp: |x, y| x <= y,
                            f64_cmp: |x, y| x <= y,
                            method_name: "le",
                        },
                    )?;
                }
                op::Gt => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QGtInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QGtFloat;
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(
                        base,
                        a,
                        b,
                        c,
                        CmpOps {
                            i64_cmp: |x, y| x > y,
                            f64_cmp: |x, y| x > y,
                            method_name: "gt",
                        },
                    )?;
                }
                op::Ge => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    match (lhs, rhs) {
                        (Value::I32(_) | Value::I64(_), Value::I32(_) | Value::I64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QGeInt;
                        },
                        (Value::F32(_) | Value::F64(_), Value::F32(_) | Value::F64(_)) => unsafe {
                            *(ip.sub(1) as *mut u8) = op::QGeFloat;
                        },
                        _ => {}
                    }
                    self.exec_comparison_reg(
                        base,
                        a,
                        b,
                        c,
                        CmpOps {
                            i64_cmp: |x, y| x >= y,
                            f64_cmp: |x, y| x >= y,
                            method_name: "ge",
                        },
                    )?;
                }

                // ── Logical ──────────────────────────────────────────
                op::And => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    match (
                        &self.stack[base + b as usize],
                        &self.stack[base + c as usize],
                    ) {
                        (Value::Bool(av), Value::Bool(bv)) => {
                            self.stack[base + a as usize] = Value::Bool(*av && *bv);
                        }
                        _ => {
                            return Err(self.make_error(format!(
                                "cannot apply '&&' to {} and {}",
                                self.stack[base + b as usize].type_name(),
                                self.stack[base + c as usize].type_name()
                            )));
                        }
                    }
                }
                op::Or => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    match (
                        &self.stack[base + b as usize],
                        &self.stack[base + c as usize],
                    ) {
                        (Value::Bool(av), Value::Bool(bv)) => {
                            self.stack[base + a as usize] = Value::Bool(*av || *bv);
                        }
                        _ => {
                            return Err(self.make_error(format!(
                                "cannot apply '||' to {} and {}",
                                self.stack[base + b as usize].type_name(),
                                self.stack[base + c as usize].type_name()
                            )));
                        }
                    }
                }

                // ── Return ───────────────────────────────────────────
                op::Return => {
                    let src = a;
                    #[cfg(feature = "debug-hooks")]
                    if self.has_debug_hooks {
                        self.fire_return_hook();
                    }
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    self.frame_upvalues.pop();
                    // For scalar-only frames, use ptr::read (bitwise copy, no match dispatch).
                    let return_value = if !frame.has_rc_values {
                        unsafe { std::ptr::read(self.stack.as_ptr().add(base + src as usize)) }
                    } else {
                        self.stack[base + src as usize].cheap_clone()
                    };
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
                    // Clear Rc-bearing dead slots to prevent leaks, but
                    // keep stack.len at high-water mark to avoid resize.
                    let caller = unsafe { self.frames.last().unwrap_unchecked() };
                    if frame.has_rc_values {
                        let caller_top = caller.base + caller.max_registers as usize;
                        let frame_end = frame.base + frame.max_registers as usize;
                        if frame_end > caller_top {
                            for slot in &mut self.stack[caller_top..frame_end] {
                                *slot = Value::Null;
                            }
                        }
                    }
                    base = caller.base;
                    chunk_id = caller.chunk_id;
                    reload_ip!(self, chunk_id, caller.pc);
                }
                op::ReturnNull => {
                    #[cfg(feature = "debug-hooks")]
                    if self.has_debug_hooks {
                        self.fire_return_hook();
                    }
                    let frame = unsafe { self.frames.pop().unwrap_unchecked() };
                    self.frame_upvalues.pop();
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
                    if frame.has_rc_values {
                        let caller_top = caller.base + caller.max_registers as usize;
                        let frame_end = frame.base + frame.max_registers as usize;
                        if frame_end > caller_top {
                            for slot in &mut self.stack[caller_top..frame_end] {
                                *slot = Value::Null;
                            }
                        }
                    }
                    base = caller.base;
                    chunk_id = caller.chunk_id;
                    reload_ip!(self, chunk_id, caller.pc);
                }

                // ── Jumps ────────────────────────────────────────────
                op::Jump => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    ip = unsafe { ip.offset(w1 as i32 as isize) };
                }
                op::JumpIfFalsy => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize].is_falsy() {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::JumpIfTruthy => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if !self.stack[base + a as usize].is_falsy() {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }

                // ── Function calls ───────────────────────────────────
                op::Call => {
                    let callee_abs = base + a as usize;
                    if let Value::Closure(data) = &self.stack[callee_abs] {
                        let func_idx = data.func_idx;
                        let upvalues = data.upvalues.clone();
                        let func = &self.functions[func_idx];
                        if func.arity != b {
                            unsafe { self.frames.last_mut().unwrap_unchecked() }.pc = save_pc!();
                            return Err(self.make_error(format!(
                                "closure '{}' expects {} arguments, got {}",
                                func.name, func.arity, b
                            )));
                        }
                        let max_regs = func.max_registers;
                        let new_base = callee_abs + 1;
                        let needed = new_base + max_regs as usize;
                        if self.stack.len() < needed {
                            self.stack.resize(needed, Value::Null);
                        }
                        unsafe { self.frames.last_mut().unwrap_unchecked() }.pc = save_pc!();
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            result_reg: callee_abs,
                            max_registers: max_regs,
                            has_rc_values: true,
                        });
                        self.frame_upvalues.push(Some(upvalues));
                    } else {
                        unsafe { self.frames.last_mut().unwrap_unchecked() }.pc = save_pc!();
                        self.exec_call_reg(base, a, b)?;
                    }
                    #[cfg(feature = "debug-hooks")]
                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = f.chunk_id;
                    reload_ip!(self, chunk_id, f.pc);
                }
                op::CallDirect => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    unsafe { self.frames.last_mut().unwrap_unchecked() }.pc = save_pc!();
                    let base_reg = a;
                    let arg_count = b;
                    let func_idx = w1 as usize;
                    let n = arg_count as usize;
                    let max_regs = self.functions[func_idx].max_registers;
                    let func_has_rc = self.functions[func_idx].has_rc_values;
                    let func_is_variadic = self.functions[func_idx].is_variadic;
                    let func_arity = self.functions[func_idx].arity;
                    let result_reg = base + base_reg as usize;

                    // Only look up closure_map for functions that actually have upvalue descriptors.
                    let call_upvalues: Option<Vec<u32>> =
                        if !self.functions[func_idx].upvalues.is_empty() {
                            self.closure_map.get(func_idx).and_then(|e| e.clone())
                        } else {
                            None
                        };

                    if func_is_variadic {
                        let min_args = func_arity.saturating_sub(1);
                        if arg_count < min_args {
                            return Err(self.make_error(format!(
                                "function '{}' expects at least {} arguments, got {}",
                                self.functions[func_idx].name, min_args, arg_count
                            )));
                        }
                        let fixed_count = min_args as usize;
                        let variadic_count = n - fixed_count;
                        let arg_start = base + base_reg as usize;
                        let variadic_start = arg_start + fixed_count;
                        let variadic_args: Vec<Value> = (0..variadic_count)
                            .map(|i| self.stack[variadic_start + i].cheap_clone())
                            .collect();
                        let new_base = arg_start;
                        self.stack[new_base + fixed_count] =
                            Value::Array(Rc::new(RefCell::new(variadic_args)));
                        self.ensure_registers(new_base, max_regs);
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            result_reg,
                            max_registers: max_regs,
                            has_rc_values: true,
                        });
                        self.frame_upvalues.push(call_upvalues);
                    } else {
                        if func_arity != arg_count {
                            return Err(self.make_error(format!(
                                "function '{}' expects {} arguments, got {}",
                                self.functions[func_idx].name, func_arity, arg_count
                            )));
                        }
                        let new_base = if n == 0 {
                            result_reg + 1
                        } else {
                            base + base_reg as usize
                        };
                        let has_upvalues = call_upvalues.is_some();
                        self.ensure_registers(new_base, max_regs);
                        self.frames.push(CallFrame {
                            chunk_id: ChunkId::Function(func_idx),
                            pc: 0,
                            base: new_base,
                            result_reg,
                            max_registers: max_regs,
                            has_rc_values: func_has_rc || has_upvalues,
                        });
                        self.frame_upvalues.push(call_upvalues);
                    }

                    #[cfg(feature = "debug-hooks")]
                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    let f = unsafe { self.frames.last().unwrap_unchecked() };
                    base = f.base;
                    chunk_id = ChunkId::Function(func_idx);
                    reload_ip!(self, chunk_id, 0);
                }
                op::TailCallDirect => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let base_reg = a;
                    let arg_count = b;
                    let func_idx = w1 as usize;
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
                    // Clear upvalues for the tail-called function
                    if let Some(last) = self.frame_upvalues.last_mut() {
                        *last = None;
                    }

                    self.ensure_registers(base, max_regs);

                    #[cfg(feature = "debug-hooks")]
                    if self.has_debug_hooks {
                        self.fire_call_hook();
                    }
                    chunk_id = ChunkId::Function(func_idx);
                    reload_ip!(self, chunk_id, 0);
                }
                op::CallNative => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let native_idx = w1 as usize;
                    let arg_count = b as usize;
                    let callee_abs = base + a as usize;
                    let arg_start = callee_abs;
                    let native = unsafe { self.native_fn_vec.get_unchecked(native_idx) };
                    if let Some(ref module) = native.module.clone() {
                        if self.disabled_modules.contains(module.as_str()) {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            return Err(self.make_error(format!("module '{module}' is disabled")));
                        }
                    }
                    let body = Rc::clone(&native.body);
                    let result = {
                        let args = &self.stack[arg_start..arg_start + arg_count];
                        body(args).map_err(|msg| {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            self.make_error(msg)
                        })?
                    };
                    self.stack[callee_abs] = result;
                }

                // ── Null handling ────────────────────────────────────
                op::NullCoalesce => {
                    let val = &self.stack[base + b as usize];
                    if val.is_null() {
                        let fallback = self.stack[base + c as usize].cheap_clone();
                        self.stack[base + a as usize] = fallback;
                    } else {
                        let v = val.cheap_clone();
                        self.stack[base + a as usize] = v;
                    }
                }

                // ── String concatenation ─────────────────────────────
                op::Concat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    let result = format!("{lhs}{rhs}");
                    self.stack[base + a as usize] = Value::Str(Rc::from(result.as_str()));
                }

                // ── Collections ──────────────────────────────────────
                op::MakeArray => {
                    let n = c as usize;
                    let s = base + b as usize;
                    let elements: Vec<Value> =
                        (0..n).map(|i| self.stack[s + i].cheap_clone()).collect();
                    self.stack[base + a as usize] = Value::Array(Rc::new(RefCell::new(elements)));
                }
                op::MakeDict => {
                    let n = c as usize;
                    let s = base + b as usize;
                    let mut map = HashMap::new();
                    for i in 0..n {
                        let key = match &self.stack[s + i * 2] {
                            Value::Str(sk) => sk.to_string(),
                            other => other.to_string(),
                        };
                        let val = self.stack[s + i * 2 + 1].cheap_clone();
                        map.insert(key, val);
                    }
                    self.stack[base + a as usize] = Value::Dict(Rc::new(RefCell::new(map)));
                }

                // ── Field/Index access ───────────────────────────────
                op::GetField => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_get_field_reg(base, a, b, w1)?;
                }
                op::SetField => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_set_field_reg(base, a, w1, b)?;
                }
                op::GetIndex => {
                    // Inline Array+Int fast path; fallback to generic helper
                    // GetIndex(dst, obj, idx) = a=dst, b=obj, c=idx
                    if let Value::Array(arr) = &self.stack[base + b as usize] {
                        let arr_rc = Rc::clone(arr);
                        let idx_val = &self.stack[base + c as usize];
                        let i = match idx_val {
                            Value::I32(v) => *v as i64,
                            Value::I64(v) => *v,
                            _ => {
                                self.frames.last_mut().unwrap().pc = save_pc!();
                                self.exec_get_index_reg(base, a, b, c)?;
                                continue;
                            }
                        };
                        let arr = arr_rc.borrow();
                        if i < 0 || i as usize >= arr.len() {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            return Err(self.make_error(format!(
                                "array index {i} out of bounds (length {})",
                                arr.len()
                            )));
                        }
                        self.stack[base + a as usize] = arr[i as usize].clone();
                    } else {
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_get_index_reg(base, a, b, c)?;
                    }
                }
                op::SetIndex => {
                    // Inline Array+Int fast path; fallback to generic helper
                    // SetIndex(obj, idx, val) = a=obj, b=idx, c=val
                    if let Value::Array(arr) = &self.stack[base + a as usize] {
                        let arr_rc = Rc::clone(arr);
                        let idx_val = &self.stack[base + b as usize];
                        let i = match idx_val {
                            Value::I32(v) => *v as i64,
                            Value::I64(v) => *v,
                            _ => {
                                self.frames.last_mut().unwrap().pc = save_pc!();
                                self.exec_set_index_reg(base, a, b, c)?;
                                continue;
                            }
                        };
                        let value = self.stack[base + c as usize].clone();
                        let mut arr = arr_rc.borrow_mut();
                        if i < 0 || i as usize >= arr.len() {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            return Err(self.make_error(format!(
                                "array index {i} out of bounds (length {})",
                                arr.len()
                            )));
                        }
                        arr[i as usize] = value;
                    } else {
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_set_index_reg(base, a, b, c)?;
                    }
                }

                // ── Coroutines ───────────────────────────────────────
                op::StartCoroutine => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_start_coroutine_reg(base, a, b)?;
                }
                op::Yield => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    return Ok(RunResult::Yield(WaitCondition::OneFrame));
                }
                op::YieldSeconds => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let seconds = &self.stack[base + a as usize];
                    let secs = match seconds {
                        Value::F32(v) => *v as f64,
                        Value::F64(v) => *v,
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
                op::YieldFrames => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let frames_val = &self.stack[base + a as usize];
                    let n = match frames_val {
                        v @ (Value::I32(_) | Value::I64(_)) if v.as_i64() >= 0 => v.as_i64() as u32,
                        _ => {
                            return Err(self.make_error(format!(
                                "waitForFrames expects a non-negative int, got {}",
                                frames_val.type_name()
                            )));
                        }
                    };
                    // The current tick counts as the first frame, so subtract 1.
                    // remaining=0 means resume next tick; remaining=1 means skip one tick.
                    return Ok(RunResult::Yield(WaitCondition::Frames { remaining: n.saturating_sub(1) }));
                }
                op::YieldUntil => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let predicate = self.stack[base + a as usize].cheap_clone();
                    return Ok(RunResult::Yield(WaitCondition::Until { predicate }));
                }
                op::YieldCoroutine => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let handle = &self.stack[base + b as usize];
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
                        result_reg: a,
                    }));
                }

                // ── Spread ───────────────────────────────────────────
                op::Spread => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let val = self.stack[base + a as usize].clone();
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
                op::MakeStruct => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    // ABC+W: a=dst, b=start, c=count, w1=name_hash
                    self.exec_make_struct_reg(base, a, w1, b, c, chunk_id)?;
                }

                // ── Classes ─────────────────────────────────────────
                op::MakeClass => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_make_class_reg(base, a, w1, b, c, chunk_id)?;
                }

                // ── Method calls ─────────────────────────────────────
                op::CallMethod => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    // AB+W: a=base_reg, b=arg_count, w1=name_hash
                    self.exec_call_method_reg(base, a, w1, b)?;
                }

                // ── Closures ─────────────────────────────────────────
                op::LoadUpvalue => {
                    let uv_idx = self
                        .frame_upvalues
                        .last()
                        .and_then(|o| o.as_ref())
                        .expect("LoadUpvalue in non-closure frame")[b as usize];
                    self.stack[base + a as usize] =
                        self.upvalue_store[uv_idx as usize].cheap_clone();
                }
                op::StoreUpvalue => {
                    let val = self.stack[base + a as usize].cheap_clone();
                    let uv_idx = self
                        .frame_upvalues
                        .last()
                        .and_then(|o| o.as_ref())
                        .expect("StoreUpvalue in non-closure frame")[b as usize]
                        as usize;
                    self.upvalue_store[uv_idx] = val.cheap_clone();
                    // Sync stack slot if this upvalue is still open
                    if self.has_open_upvalues {
                        for (abs_slot, entry) in self.open_upvalues.iter().enumerate() {
                            if *entry == Some(uv_idx as u32) {
                                self.stack[abs_slot] = val;
                                break;
                            }
                        }
                    }
                }
                op::MakeClosure => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_make_closure_reg(base, a, bx)?;
                }
                op::CloseUpvalue => {
                    let abs = base + a as usize;
                    if abs < self.open_upvalues.len()
                        && let Some(uv_idx) = self.open_upvalues[abs].take()
                    {
                        self.upvalue_store[uv_idx as usize] = self.stack[abs].clone();
                        self.has_open_upvalues = self.open_upvalues.iter().any(|e| e.is_some());
                    }
                }

                // ── AoSoA conversion (mobile only) ────────────────
                #[cfg(feature = "mobile-aosoa")]
                op::ConvertToAoSoA => {
                    self.exec_convert_to_aosoa_reg(base, a)?;
                }

                // ── Typed arithmetic (compiler-guaranteed types) ────────
                op::AddInt => {
                    let abs_b = base + b as usize;
                    let abs_c = base + c as usize;
                    let result = match (&self.stack[abs_b], &self.stack[abs_c]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_add(*bv) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*av as i64 + *bv as i64),
                        },
                        _ => {
                            let r = self.stack[abs_b]
                                .as_i64()
                                .checked_add(self.stack[abs_c].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::AddFloat => {
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + b as usize],
                        &self.stack[base + c as usize],
                        |x, y| x + y,
                    );
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::SubInt => {
                    let abs_b = base + b as usize;
                    let abs_c = base + c as usize;
                    let result = match (&self.stack[abs_b], &self.stack[abs_c]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_sub(*bv) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*av as i64 - *bv as i64),
                        },
                        _ => {
                            let r = self.stack[abs_b]
                                .as_i64()
                                .checked_sub(self.stack[abs_c].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::SubFloat => {
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + b as usize],
                        &self.stack[base + c as usize],
                        |x, y| x - y,
                    );
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::MulInt => {
                    let abs_b = base + b as usize;
                    let abs_c = base + c as usize;
                    let result = match (&self.stack[abs_b], &self.stack[abs_c]) {
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
                            let r = self.stack[abs_b]
                                .as_i64()
                                .checked_mul(self.stack[abs_c].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::MulFloat => {
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + b as usize],
                        &self.stack[base + c as usize],
                        |x, y| x * y,
                    );
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::DivInt => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    let abs_b = base + b as usize;
                    let abs_c = base + c as usize;
                    if self.stack[abs_c].as_i64() == 0 {
                        return Err(self.make_error("division by zero".to_string()));
                    }
                    let result = match (&self.stack[abs_b], &self.stack[abs_c]) {
                        (Value::I32(av), Value::I32(bv)) => match av.checked_div(*bv) {
                            Some(r) => Value::I32(r),
                            None => Value::I64(*av as i64 / *bv as i64),
                        },
                        _ => {
                            let r = self.stack[abs_b]
                                .as_i64()
                                .checked_div(self.stack[abs_c].as_i64())
                                .ok_or_else(|| self.make_error("integer overflow".into()))?;
                            Value::I64(r)
                        }
                    };
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::DivFloat => {
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    if self.stack[base + c as usize].as_f64() == 0.0 {
                        return Err(self.make_error("division by zero".to_string()));
                    }
                    let result = Value::promote_float_pair_op(
                        &self.stack[base + b as usize],
                        &self.stack[base + c as usize],
                        |x, y| x / y,
                    );
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }

                // ── Immediate arithmetic ────────────────────────────────
                op::AddIntImm => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let imm = w1 as i32;
                    let abs_src = base + b as usize;
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
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }
                op::SubIntImm => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let imm = w1 as i32;
                    let abs_src = base + b as usize;
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
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }

                // ── Typed comparison ─────────────────────────────────────
                op::LtInt => {
                    let r = self.stack[base + b as usize].as_i64()
                        < self.stack[base + c as usize].as_i64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::LtFloat => {
                    let r = self.stack[base + b as usize].as_f64()
                        < self.stack[base + c as usize].as_f64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::LeInt => {
                    let r = self.stack[base + b as usize].as_i64()
                        <= self.stack[base + c as usize].as_i64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::LeFloat => {
                    let r = self.stack[base + b as usize].as_f64()
                        <= self.stack[base + c as usize].as_f64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::GtInt => {
                    let r = self.stack[base + b as usize].as_i64()
                        > self.stack[base + c as usize].as_i64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::GtFloat => {
                    let r = self.stack[base + b as usize].as_f64()
                        > self.stack[base + c as usize].as_f64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::GeInt => {
                    let r = self.stack[base + b as usize].as_i64()
                        >= self.stack[base + c as usize].as_i64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::GeFloat => {
                    let r = self.stack[base + b as usize].as_f64()
                        >= self.stack[base + c as usize].as_f64();
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::EqInt => {
                    let r = self.stack[base + b as usize] == self.stack[base + c as usize];
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::EqFloat => {
                    let r = self.stack[base + b as usize] == self.stack[base + c as usize];
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::NeInt => {
                    let r = self.stack[base + b as usize] != self.stack[base + c as usize];
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }
                op::NeFloat => {
                    let r = self.stack[base + b as usize] != self.stack[base + c as usize];
                    unsafe { self.write_stack_nodrop(base + a as usize, Value::Bool(r)) };
                }

                // ── Fused compare-and-jump (int) ────────────────────────
                op::TestLtInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize].as_i64()
                        >= self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestLeInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize].as_i64()
                        > self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestGtInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize].as_i64()
                        <= self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestGeInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize].as_i64()
                        < self.stack[base + b as usize].as_i64()
                    {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestEqInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize] != self.stack[base + b as usize] {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestNeInt => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    if self.stack[base + a as usize] == self.stack[base + b as usize] {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }

                // ── Fused compare-and-jump (int immediate, DEAD) ──────────
                op::TestLtIntImm | op::TestLeIntImm | op::TestGtIntImm | op::TestGeIntImm => unsafe {
                    std::hint::unreachable_unchecked()
                },

                // ── Fused compare-and-jump (float) ──────────────────────
                op::TestLtFloat => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(va.partial_cmp(&vb), Some(std::cmp::Ordering::Less)) {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestLeFloat => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(
                        va.partial_cmp(&vb),
                        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
                    ) {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestGtFloat => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(va.partial_cmp(&vb), Some(std::cmp::Ordering::Greater)) {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }
                op::TestGeFloat => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    let va = self.stack[base + a as usize].as_f64();
                    let vb = self.stack[base + b as usize].as_f64();
                    if !matches!(
                        va.partial_cmp(&vb),
                        Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)
                    ) {
                        ip = unsafe { ip.offset(w1 as i32 as isize) };
                    }
                }

                // ── Quickened arithmetic (runtime-specialized) ──────────
                // In quickened ops: a=dst, b=lhs, c=rhs (same ABC layout)
                op::QAddInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        let result = match (lhs, rhs) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_add(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 + *bv as i64),
                            },
                            _ => {
                                let r = lhs
                                    .as_i64()
                                    .checked_add(rhs.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Add;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_add_reg(base, a, b, c)?;
                    }
                }
                op::QAddFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        let result = Value::promote_float_pair_op(lhs, rhs, |x, y| x + y);
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Add;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_add_reg(base, a, b, c)?;
                    }
                }
                op::QSubInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        let result = match (lhs, rhs) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_sub(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 - *bv as i64),
                            },
                            _ => {
                                let r = lhs
                                    .as_i64()
                                    .checked_sub(rhs.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Sub;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_binary_arith_reg(
                            base + a as usize,
                            base + b as usize,
                            base + c as usize,
                            ArithOps {
                                i32_op: i32::checked_sub,
                                i64_op: i64::checked_sub,
                                f64_op: |x, y| x - y,
                                method_name: "subtract",
                            },
                        )?;
                    }
                }
                op::QSubFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        let result = Value::promote_float_pair_op(lhs, rhs, |x, y| x - y);
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Sub;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_binary_arith_reg(
                            base + a as usize,
                            base + b as usize,
                            base + c as usize,
                            ArithOps {
                                i32_op: i32::checked_sub,
                                i64_op: i64::checked_sub,
                                f64_op: |x, y| x - y,
                                method_name: "subtract",
                            },
                        )?;
                    }
                }
                op::QMulInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        let result = match (lhs, rhs) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_mul(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 * *bv as i64),
                            },
                            _ => {
                                let r = lhs
                                    .as_i64()
                                    .checked_mul(rhs.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Mul;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_binary_arith_reg(
                            base + a as usize,
                            base + b as usize,
                            base + c as usize,
                            ArithOps {
                                i32_op: i32::checked_mul,
                                i64_op: i64::checked_mul,
                                f64_op: |x, y| x * y,
                                method_name: "multiply",
                            },
                        )?;
                    }
                }
                op::QMulFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        let result = Value::promote_float_pair_op(lhs, rhs, |x, y| x * y);
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Mul;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_binary_arith_reg(
                            base + a as usize,
                            base + b as usize,
                            base + c as usize,
                            ArithOps {
                                i32_op: i32::checked_mul,
                                i64_op: i64::checked_mul,
                                f64_op: |x, y| x * y,
                                method_name: "multiply",
                            },
                        )?;
                    }
                }
                op::QDivInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        if rhs.as_i64() == 0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        let result = match (lhs, rhs) {
                            (Value::I32(av), Value::I32(bv)) => match av.checked_div(*bv) {
                                Some(r) => Value::I32(r),
                                None => Value::I64(*av as i64 / *bv as i64),
                            },
                            _ => {
                                let r = lhs
                                    .as_i64()
                                    .checked_div(rhs.as_i64())
                                    .ok_or_else(|| self.make_error("integer overflow".into()))?;
                                Value::I64(r)
                            }
                        };
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Div;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_div_reg(base, a, b, c)?;
                    }
                }
                op::QDivFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        if rhs.as_f64() == 0.0 {
                            return Err(self.make_error("division by zero".to_string()));
                        }
                        let result = Value::promote_float_pair_op(lhs, rhs, |x, y| x / y);
                        unsafe { self.write_stack_nodrop(base + a as usize, result) };
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Div;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_div_reg(base, a, b, c)?;
                    }
                }

                // ── Quickened comparison ─────────────────────────────────
                op::QLtInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_i64() < rhs.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Lt;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x < y,
                                f64_cmp: |x, y| x < y,
                                method_name: "lt",
                            },
                        )?;
                    }
                }
                op::QLtFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_f64() < rhs.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Lt;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x < y,
                                f64_cmp: |x, y| x < y,
                                method_name: "lt",
                            },
                        )?;
                    }
                }
                op::QLeInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_i64() <= rhs.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Le;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x <= y,
                                f64_cmp: |x, y| x <= y,
                                method_name: "le",
                            },
                        )?;
                    }
                }
                op::QLeFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_f64() <= rhs.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Le;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x <= y,
                                f64_cmp: |x, y| x <= y,
                                method_name: "le",
                            },
                        )?;
                    }
                }
                op::QGtInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_i64() > rhs.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Gt;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x > y,
                                f64_cmp: |x, y| x > y,
                                method_name: "gt",
                            },
                        )?;
                    }
                }
                op::QGtFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_f64() > rhs.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Gt;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x > y,
                                f64_cmp: |x, y| x > y,
                                method_name: "gt",
                            },
                        )?;
                    }
                }
                op::QGeInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_i64() >= rhs.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Ge;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x >= y,
                                f64_cmp: |x, y| x >= y,
                                method_name: "ge",
                            },
                        )?;
                    }
                }
                op::QGeFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_f64() >= rhs.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Ge;
                        }
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_comparison_reg(
                            base,
                            a,
                            b,
                            c,
                            CmpOps {
                                i64_cmp: |x, y| x >= y,
                                f64_cmp: |x, y| x >= y,
                                method_name: "ge",
                            },
                        )?;
                    }
                }
                op::QEqInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_i64() == rhs.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Eq;
                        }
                        let eq = self.stack[base + b as usize] == self.stack[base + c as usize];
                        self.stack[base + a as usize] = Value::Bool(eq);
                    }
                }
                op::QEqFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_f64() == rhs.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Eq;
                        }
                        let eq = self.stack[base + b as usize] == self.stack[base + c as usize];
                        self.stack[base + a as usize] = Value::Bool(eq);
                    }
                }
                op::QNeInt => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_int() && rhs.is_int() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_i64() != rhs.as_i64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Ne;
                        }
                        let ne = self.stack[base + b as usize] != self.stack[base + c as usize];
                        self.stack[base + a as usize] = Value::Bool(ne);
                    }
                }
                op::QNeFloat => {
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    if lhs.is_float() && rhs.is_float() {
                        self.stack[base + a as usize] = Value::Bool(lhs.as_f64() != rhs.as_f64());
                    } else {
                        unsafe {
                            *(ip.sub(1) as *mut u8) = op::Ne;
                        }
                        let ne = self.stack[base + b as usize] != self.stack[base + c as usize];
                        self.stack[base + a as usize] = Value::Bool(ne);
                    }
                }

                // ── Unknown opcode ─────────────────────────────────────
                _ => {
                    let words = INSTR_WORDS[opc as usize] as usize;
                    if words > 1 {
                        ip = unsafe { ip.add(words - 1) };
                    }
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    return Err(self.make_error(format!("unknown opcode {opc}")));
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
            None => i64_op(a as i64, b as i64)
                .map(Value::I64)
                .ok_or_else(|| err_msg.to_string()),
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

    /// Captures a local variable into the flat upvalue store.
    /// Returns the store index. Reuses existing capture if already open.
    fn capture_local(&mut self, abs_slot: usize) -> u32 {
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
    fn close_upvalues_above(&mut self, min_slot: usize) {
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
    fn try_operator_method(
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
    fn exec_binary_arith_reg(
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
    fn exec_comparison_reg(
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
                has_rc_values: true, // closures may capture Rc-bearing values
            });
            self.frame_upvalues.push(Some(upvalues));

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
            let expected_arity = self.functions[func_idx].arity;
            let is_variadic = self.functions[func_idx].is_variadic;
            let max_regs = self.functions[func_idx].max_registers;
            let func_has_rc = self.functions[func_idx].has_rc_values;

            // Only look up closure_map for functions that actually have upvalue descriptors.
            let call_upvalues: Option<Vec<u32>> =
                if !self.functions[func_idx].upvalues.is_empty() {
                    self.closure_map.get(func_idx).and_then(|e| e.clone())
                } else {
                    None
                };

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
                });
                self.frame_upvalues.push(call_upvalues);
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

                let has_upvalues = call_upvalues.is_some();
                self.frames.push(CallFrame {
                    chunk_id: ChunkId::Function(func_idx),
                    pc: 0,
                    base: new_base,
                    result_reg: callee_abs,
                    max_registers: max_regs,
                    has_rc_values: func_has_rc || has_upvalues,
                });
                self.frame_upvalues.push(call_upvalues);
            }

            return Ok(());
        }

        // Fall back to native functions
        let func_name = match &self.stack[callee_abs] {
            Value::Str(s) => s.to_string(),
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
            Value::Str(s) => s.to_string(),
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
            // Pass a direct slice of the stack — no Vec allocation.
            let arg_start = receiver_abs + 1;
            let result = {
                let args = &self.stack[arg_start..arg_start + n];
                (body)(&receiver, args).map_err(|msg| self.make_error(msg))?
            };
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
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
                    Value::Str(s) => s.to_string(),
                    _ => return Err(self.make_error("map expects a function".to_string())),
                };
                let items: Vec<Value> = {
                    let b = container.borrow();
                    (0..b.len())
                        .map(|i| Value::Struct(Box::new(b.get(i).unwrap())))
                        .collect()
                };
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
                    Value::Str(s) => s.to_string(),
                    _ => return Err(self.make_error("filter expects a function".to_string())),
                };
                let items: Vec<Value> = {
                    let b = container.borrow();
                    (0..b.len())
                        .map(|i| Value::Struct(Box::new(b.get(i).unwrap())))
                        .collect()
                };
                let mut result = Vec::new();
                for item in items {
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
                    Value::Str(s) => s.to_string(),
                    _ => return Err(self.make_error("for_each expects a function".to_string())),
                };
                let items: Vec<Value> = {
                    let b = container.borrow();
                    (0..b.len())
                        .map(|i| Value::Struct(Box::new(b.get(i).unwrap())))
                        .collect()
                };
                for item in &items {
                    self.call_function(&fn_name, std::slice::from_ref(item))?;
                }
                self.stack[receiver_abs] = Value::Null;
                return Ok(());
            }

            if name_hash == iter_field_hash && arg_count == 1 {
                let field_name = match &self.stack[receiver_abs + 1] {
                    Value::Str(s) => s.to_string(),
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
                            container.layout.type_name, field_name
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
    fn exec_make_struct_reg(
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

        let writ_struct = super::writ_struct::WritStruct { layout, fields };
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

        let instance = super::class_instance::WritClassInstance {
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

        let func = &self.functions[func_idx];
        let max_regs = func.max_registers as usize;
        let mut coro_stack = vec![Value::Null; max_regs];
        for (i, slot) in coro_stack.iter_mut().enumerate().take(n) {
            *slot = self.stack[callee_abs + 1 + i].clone();
        }

        let id = self.next_coroutine_id;
        self.next_coroutine_id += 1;

        let has_upvalues = closure_upvalues.is_some();
        let frame = CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: func.max_registers,
            has_rc_values: func.has_rc_values || has_upvalues,
        };

        let coro = Coroutine {
            id,
            state: CoroutineState::Running,
            stack: coro_stack,
            frames: vec![frame],
            frame_upvalues: vec![closure_upvalues],
            wait: None,
            return_value: None,
            owner_id: None,
            open_upvalues: Vec::new(),
            upvalue_store: Vec::new(),
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
        use super::aosoa::AoSoAContainer;

        let abs = base + src as usize;
        let val = &self.stack[abs];
        match val {
            Value::Array(arr) => {
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
                            *remaining <= 1e-6
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

        // Only YieldCoroutine waits on a child and receives its return value;
        // all other yields leave registers unchanged.
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
            let coro = &mut self.coroutines[idx];
            if let Some(frame) = coro.frames.last() {
                let abs = frame.base + result_reg as usize;
                coro.stack[abs] = return_value;
            }
        }

        let coro = &mut self.coroutines[idx];
        coro.state = CoroutineState::Running;
        coro.wait = None;

        std::mem::swap(&mut self.stack, &mut coro.stack);
        std::mem::swap(&mut self.frames, &mut coro.frames);
        std::mem::swap(&mut self.frame_upvalues, &mut coro.frame_upvalues);
        std::mem::swap(&mut self.open_upvalues, &mut coro.open_upvalues);
        std::mem::swap(&mut self.upvalue_store, &mut coro.upvalue_store);
        self.active_coroutine = Some(idx);

        let result = self.run();

        let coro = &mut self.coroutines[idx];
        std::mem::swap(&mut self.stack, &mut coro.stack);
        std::mem::swap(&mut self.frames, &mut coro.frames);
        std::mem::swap(&mut self.frame_upvalues, &mut coro.frame_upvalues);
        std::mem::swap(&mut self.open_upvalues, &mut coro.open_upvalues);
        std::mem::swap(&mut self.upvalue_store, &mut coro.upvalue_store);
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

        let saved_stack = std::mem::take(&mut self.stack);
        let saved_frames = std::mem::take(&mut self.frames);
        let saved_frame_upvalues = std::mem::take(&mut self.frame_upvalues);
        let saved_upvalues = std::mem::take(&mut self.open_upvalues);
        let saved_active = self.active_coroutine;
        self.active_coroutine = None;

        let max_regs = self.functions[func_idx].max_registers;
        let has_upvalues = closure_upvalues.is_some();
        self.stack.resize(max_regs as usize, Value::Null);
        self.frames.push(CallFrame {
            chunk_id: ChunkId::Function(func_idx),
            pc: 0,
            base: 0,
            result_reg: 0,
            max_registers: max_regs,
            has_rc_values: self.functions[func_idx].has_rc_values || has_upvalues,
        });
        self.frame_upvalues.push(closure_upvalues);

        let result = self.run();

        self.stack = saved_stack;
        self.frames = saved_frames;
        self.frame_upvalues = saved_frame_upvalues;
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
    #[cfg(feature = "debug-hooks")]
    #[cold]
    #[inline(never)]
    fn debug_probe(&mut self, chunk_id: ChunkId, pc: usize) -> Result<(), RuntimeError> {
        let chunk = self.chunk_for(chunk_id);
        let current_line = chunk.line_for_word_offset(pc);
        let current_file = chunk.file().unwrap_or("").to_string();

        // Only act on line changes
        let line_changed = current_line != self.last_line || current_file != self.last_file;
        if !line_changed {
            return Ok(());
        }

        self.last_line = current_line;
        self.last_file = current_file.clone();

        if let Some(ref hook) = self.on_line_hook {
            hook(&current_file, current_line);
        }

        let should_break = match &self.step_state {
            StepState::None => false,
            StepState::StepInto => true,
            StepState::StepOver { target_depth } => self.frames.len() <= *target_depth,
        };

        let at_breakpoint = !self.breakpoints.is_empty()
            && self.breakpoints.contains(&BreakpointKey {
                file: current_file.clone(),
                line: current_line,
            });

        if (should_break || at_breakpoint) && self.breakpoint_handler.is_some() {
            self.step_state = StepState::None;

            // Collect locals before borrowing the handler
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
    #[cfg(feature = "debug-hooks")]
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
    #[cfg(feature = "debug-hooks")]
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
