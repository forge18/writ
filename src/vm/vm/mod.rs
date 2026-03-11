use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::compiler::opcode::{INSTR_WORDS, op};
use crate::compiler::{Chunk, ClassMeta, CompiledFunction, EnumMeta, StructMeta, string_hash};

use super::coroutine::{Coroutine, CoroutineId, WaitCondition};
#[cfg(feature = "debug-hooks")]
use super::debug::{BreakpointAction, BreakpointContext};
use super::debug::{BreakpointHandler, BreakpointKey, CallHook, LineHook, StepState};
use super::error::{RuntimeError, StackFrame, StackTrace};
use super::frame::{CallFrame, ChunkId};
use super::native::{NativeFunction, NativeMethod, SeqNativeFunction, SeqNativeMethod};
use super::object::WritObject;
use super::sequence::SequenceAction;
use super::value::{Value, ValueTag};

enum RunResult {
    Return(Value),
    Yield(WaitCondition),
}

struct ArithOps {
    i32_op: fn(i32, i32) -> Option<i32>,
    i64_op: fn(i64, i64) -> Option<i64>,
    f64_op: fn(f64, f64) -> f64,
    method_name: &'static str,
}

struct CmpOps {
    i64_cmp: fn(&i64, &i64) -> bool,
    f64_cmp: fn(&f64, &f64) -> bool,
    method_name: &'static str,
}

/// The Writ bytecode virtual machine.
// Hot-field layout: `#[repr(C)]` pins field order so the dispatch loop's
// most-touched fields land on the first two 64-byte cache lines.
//
// Cache line 0 (0-63):   stack(24) + frames(24) + instruction_count(8)
//                         + has_debug_hooks(1) + has_open_upvalues(1) + pad(6) = 64
// Cache line 1 (64-127): func_ip_cache(24) + functions(24) + instruction_limit(16) = 64
// Cache line 2 (128-175): open_upvalues(24) + main_chunk (starts here, cold)
// Cache lines 3+:        cold fields (maps, debug state, coroutines...)
#[repr(C)]
pub struct VM {
    // --- Cache line 0: every-instruction hot fields ---
    stack: Vec<Value>,
    frames: Vec<CallFrame>,
    /// Reset per `execute_program` call.
    instruction_count: u64,
    /// True when any debug hook or breakpoint is active.
    has_debug_hooks: bool,
    /// Avoids scanning open_upvalues on every LoadLocal/StoreLocal.
    has_open_upvalues: bool,

    // --- Cache line 1: call/return hot fields ---
    /// Code pointer + word length per chunk. Index 0 = main, 1..N = functions.
    func_ip_cache: Vec<(*const u32, usize)>,
    functions: Vec<CompiledFunction>,
    /// `None` = unlimited.
    instruction_limit: Option<u64>,

    // --- Cache line 2: warm fields (closures) ---
    /// Parallel to `frames`. `None` for non-closure frames.
    frame_upvalues: Vec<Option<Vec<u32>>>,
    /// Indexed by absolute stack slot. `Some(idx)` -> index into `upvalue_store`.
    open_upvalues: Vec<Option<u32>>,
    upvalue_store: Vec<Value>,
    /// Populated by `MakeClosure` while the main chunk stack is live.
    /// Lets `call_function` provide correct upvalues after main chunk exits.
    closure_map: Vec<Option<Vec<u32>>>,

    // --- Cold fields: setup, metadata, debug ---
    main_chunk: Chunk,
    function_map: HashMap<String, usize>,
    /// Field name hash -> original string. Built from chunk string pools at load time.
    field_names: HashMap<u32, String>,
    native_functions: HashMap<String, NativeFunction>,
    /// For `CallNative` fast-path dispatch.
    native_fn_vec: Vec<NativeFunction>,
    native_fn_name_to_idx: HashMap<String, u32>,
    /// Keyed by (value tag, method name hash).
    methods: HashMap<(ValueTag, u32), NativeMethod>,
    /// Keyed by name hash -> (name, value).
    globals: HashMap<u32, (String, Value)>,
    struct_metas: HashMap<String, StructMeta>,
    class_metas: HashMap<String, ClassMeta>,
    struct_layouts: HashMap<String, Rc<super::field_layout::FieldLayout>>,
    class_layouts: HashMap<String, Rc<super::field_layout::FieldLayout>>,
    disabled_modules: HashSet<String>,
    /// Sequence-capable native functions (may invoke script callbacks).
    seq_native_functions: HashMap<String, SeqNativeFunction>,
    /// Sequence-capable native methods, keyed by (value tag, method name hash).
    seq_methods: HashMap<(ValueTag, u32), SeqNativeMethod>,
    coroutines: Vec<Coroutine>,
    next_coroutine_id: CoroutineId,
    /// `None` = main script.
    active_coroutine: Option<usize>,
    breakpoints: HashSet<BreakpointKey>,
    breakpoint_handler: Option<BreakpointHandler>,
    on_line_hook: Option<LineHook>,
    on_call_hook: Option<CallHook>,
    on_return_hook: Option<CallHook>,
    step_state: StepState,
    last_line: u32,
    /// Last file path seen (for detecting line changes).
    last_file: String,
    /// String interner for deduplicating `Rc<str>` across chunks.
    interner: super::intern::StringInterner,
    /// Reusable scratch buffer for string concatenation (avoids malloc/free per Concat).
    concat_buf: String,
    /// When set, coroutines auto-tick at the start of `call_function` and `load_module`.
    tick_source: Option<Box<dyn Fn() -> f64>>,
}

mod arithmetic;
mod calls;
mod coroutines;
mod debug_hooks;
mod objects;

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
            seq_native_functions: HashMap::new(),
            seq_methods: HashMap::new(),
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
            concat_buf: String::with_capacity(256),
            tick_source: None,
        }
    }

    /// Registers a typed Rust function callable from Writ scripts.
    ///
    /// The function's parameter and return types must implement [`FromValue`]
    /// and [`IntoValue`] respectively. Arity is inferred from the function
    /// signature -- no manual arity argument is needed.
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

    /// Registers a sequence-capable native function (can invoke script callbacks).
    ///
    /// The handler returns [`NativeResult`] which may be an immediate value or
    /// a [`Sequence`](super::sequence::Sequence) that the VM polls to drive
    /// deferred callback invocations.
    pub fn register_seq_fn(&mut self, name: &str, body: super::native::NativeSeqFn) -> &mut Self {
        self.seq_native_functions.insert(
            name.to_string(),
            SeqNativeFunction {
                module: None,
                arity: None,
                body,
            },
        );
        self
    }

    /// Registers a sequence-capable native method on a specific value type.
    pub fn register_seq_method(
        &mut self,
        tag: ValueTag,
        name: &str,
        module: Option<&str>,
        body: super::native::NativeSeqMethodFn,
    ) -> &mut Self {
        let hash = string_hash(name);
        self.seq_methods.insert(
            (tag, hash),
            SeqNativeMethod {
                name: name.to_string(),
                module: module.map(|m| m.to_string()),
                arity: None,
                body,
            },
        );
        self.field_names.insert(hash, name.to_string());
        self
    }

    /// Drives a sequence to completion, executing script callbacks as requested.
    ///
    /// The VM polls the sequence repeatedly. When it yields a `Call` request,
    /// the VM invokes the callee through normal frame dispatch, then feeds
    /// the result back on the next poll.
    fn drive_sequence(
        &mut self,
        mut seq: Box<dyn super::sequence::Sequence>,
        result_slot: usize,
    ) -> Result<(), RuntimeError> {
        let mut last_result: Option<Value> = None;

        loop {
            match seq.poll(last_result.take()) {
                SequenceAction::Done(value) => {
                    self.stack[result_slot] = value;
                    return Ok(());
                }
                SequenceAction::Error(msg) => {
                    return Err(self.make_error(msg));
                }
                SequenceAction::Call { callee, args } => {
                    let result = match &callee {
                        Value::Closure(data) => {
                            self.call_compiled_function(data.func_idx, &args)?
                        }
                        Value::Str(name) => self.call_function(name, &args)?,
                        other => {
                            return Err(self.make_error(format!(
                                "sequence attempted to call non-function: {}",
                                other.type_name()
                            )));
                        }
                    };
                    last_result = Some(result);
                }
            }
        }
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

    /// Returns the native function name->index map, for use by the compiler.
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

    // --- Tick source API ---

    /// Registers a tick source for automatic coroutine advancement.
    ///
    /// When set, coroutines advance automatically at the start of each
    /// `call_function` or `load_module` invocation. The callback should
    /// return the elapsed time in seconds since the last call.
    pub fn set_tick_source<F: Fn() -> f64 + 'static>(&mut self, source: F) {
        self.tick_source = Some(Box::new(source));
    }

    /// Removes the tick source, reverting to manual `tick()` mode.
    pub fn clear_tick_source(&mut self) {
        self.tick_source = None;
    }

    /// Auto-tick if a tick source is registered and coroutines exist.
    fn auto_tick(&mut self) -> Result<(), RuntimeError> {
        if !self.coroutines.is_empty()
            && let Some(ref source) = self.tick_source
        {
            let delta = source();
            self.tick(delta)?;
        }
        Ok(())
    }

    // --- Debug API ---

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
    /// which runs the full lex -> parse -> type-check -> compile pipeline before
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
        let (_new_main, new_functions, new_struct_metas, new_class_metas, new_enum_metas) =
            compiler.into_parts();

        for meta in new_struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta);
        }

        for meta in new_class_metas {
            self.class_metas.insert(meta.name.clone(), meta);
        }

        self.build_enum_globals(&new_enum_metas);

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
    /// Callers are responsible for running lex -> parse -> type-check -> compile
    /// before calling this method. Use [`Writ::reload`] rather than calling
    /// this directly when an embedder-level type check is needed.
    pub fn reload_compiled(
        &mut self,
        file: &str,
        _chunk: &Chunk,
        functions: &[CompiledFunction],
        struct_metas: &[StructMeta],
        class_metas: &[ClassMeta],
        enum_metas: &[EnumMeta],
    ) -> Result<(), RuntimeError> {
        for meta in struct_metas {
            self.struct_metas.insert(meta.name.clone(), meta.clone());
        }

        for meta in class_metas {
            self.class_metas.insert(meta.name.clone(), meta.clone());
        }

        self.build_enum_globals(enum_metas);

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
        enum_metas: &[EnumMeta],
    ) -> Result<Value, RuntimeError> {
        self.auto_tick()?;

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
            self.field_names.insert(string_hash(s), s.to_string());
        }
        for func in functions {
            for s in func.chunk.rc_strings() {
                self.field_names.insert(string_hash(s), s.to_string());
            }
        }

        // Intern strings in newly-added function chunks
        let start = self.functions.len() - functions.len();
        for func in &mut self.functions[start..] {
            func.chunk.intern_strings(&mut self.interner);
        }

        self.build_layouts();
        self.build_enum_globals(enum_metas);

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
        self.auto_tick()?;

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
        self.execute_program(chunk, &[], &[], &[], &[])
    }

    /// Executes a compiled program (main chunk + function table).
    pub fn execute_program(
        &mut self,
        chunk: &Chunk,
        functions: &[CompiledFunction],
        struct_metas: &[StructMeta],
        class_metas: &[ClassMeta],
        enum_metas: &[EnumMeta],
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
        self.build_enum_globals(enum_metas);
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

    /// Builds the reverse hash -> name lookup from all chunk string pools.
    /// Interns all string pool entries in main chunk and function chunks.
    fn intern_chunk_strings(&mut self) {
        self.main_chunk.intern_strings(&mut self.interner);
        for func in &mut self.functions {
            func.chunk.intern_strings(&mut self.interner);
        }
    }

    fn build_field_names(&mut self) {
        for s in self.main_chunk.rc_strings() {
            self.field_names.insert(string_hash(s), s.to_string());
        }
        for func in &self.functions {
            for s in func.chunk.rc_strings() {
                self.field_names.insert(string_hash(s), s.to_string());
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

    /// Builds enum globals from enum metadata.
    ///
    /// Each enum becomes a global `WritStruct` namespace whose fields are the
    /// variant instances. Each variant is itself a `WritStruct` with `__variant`
    /// (name), `__ordinal` (index), and any declared enum fields.
    fn build_enum_globals(&mut self, enum_metas: &[EnumMeta]) {
        use super::field_layout::FieldLayout;
        use super::writ_struct::WritStruct;

        for meta in enum_metas {
            // Build the variant layout: [__variant, __ordinal, ...field_names]
            let mut variant_field_names = vec!["__variant".to_string(), "__ordinal".to_string()];
            variant_field_names.extend(meta.field_names.clone());
            let variant_public: HashSet<String> = variant_field_names.iter().cloned().collect();
            let variant_layout = Rc::new(FieldLayout::new(
                meta.name.clone(), // type_name = enum name (so typeof() works)
                variant_field_names,
                variant_public,
                HashSet::new(), // methods resolved via function_map lookup
            ));

            // Build each variant instance
            let mut namespace_variant_values: Vec<Value> = Vec::new();
            for (i, variant_name) in meta.variant_names.iter().enumerate() {
                let mut fields: Vec<Value> = vec![
                    Value::Str(Rc::from(variant_name.as_str())), // __variant
                    Value::I32(i as i32),                        // __ordinal
                ];
                // If enum has fields, the variant value initializes the first field
                if !meta.field_names.is_empty() {
                    let v = meta.variant_values[i];
                    let val = if v >= i32::MIN as i64 && v <= i32::MAX as i64 {
                        Value::I32(v as i32)
                    } else {
                        Value::I64(v)
                    };
                    fields.push(val);
                    // Remaining fields default to Null
                    for _ in 1..meta.field_names.len() {
                        fields.push(Value::Null);
                    }
                }
                let variant_struct = WritStruct {
                    layout: variant_layout.clone(),
                    fields,
                };
                namespace_variant_values.push(Value::Struct(Box::new(variant_struct)));
            }

            // Build the namespace struct: Direction.North, Direction.South, etc.
            let ns_field_names = meta.variant_names.clone();
            let ns_public: HashSet<String> = ns_field_names.iter().cloned().collect();
            let ns_layout = Rc::new(FieldLayout::new(
                meta.name.clone(),
                ns_field_names,
                ns_public,
                HashSet::new(),
            ));
            let ns_struct = WritStruct {
                layout: ns_layout,
                fields: namespace_variant_values,
            };
            self.register_global(&meta.name, Value::Struct(Box::new(ns_struct)));
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
                // --- Literals ---
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

                // --- Arithmetic (generic, quickenable) ---
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

                // --- Unary ---
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

                // --- Type coercion ---
                op::IntToFloat => {
                    let val = &self.stack[base + b as usize];
                    let result = match val {
                        Value::I32(v) => Value::F64(*v as f64),
                        Value::I64(v) => Value::F64(*v as f64),
                        _ => unreachable!("IntToFloat: compiler guarantees int operand"),
                    };
                    unsafe { self.write_stack_nodrop(base + a as usize, result) };
                }

                // --- Comparison (generic, quickenable) ---
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

                // --- Logical ---
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

                // --- Return ---
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

                // --- Jumps ---
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

                // --- Function calls ---
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
                    if let Some(module) = native.module.as_deref()
                        && self.disabled_modules.contains(module)
                    {
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        return Err(self.make_error(format!("module '{module}' is disabled")));
                    }
                    // Use raw pointer to body to avoid Rc::clone refcount bump.
                    // SAFETY: native_fn_vec is not modified during run_until.
                    let body: *const dyn Fn(&[Value]) -> Result<Value, String> = &*native.body;
                    let result = {
                        let args = &self.stack[arg_start..arg_start + arg_count];
                        (unsafe { &*body })(args).map_err(|msg| {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            self.make_error(msg)
                        })?
                    };
                    self.stack[callee_abs] = result;
                }

                // --- Null handling ---
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

                // --- String concatenation ---
                op::Concat => {
                    // Fast path: Str + Str uses a reusable scratch buffer,
                    // avoiding a String allocation per concat.
                    let lhs = &self.stack[base + b as usize];
                    let rhs = &self.stack[base + c as usize];
                    let result: Rc<str> = match (lhs, rhs) {
                        (Value::Str(a_str), Value::Str(b_str)) => {
                            self.concat_buf.clear();
                            self.concat_buf.push_str(a_str);
                            self.concat_buf.push_str(b_str);
                            Rc::from(self.concat_buf.as_str())
                        }
                        _ => {
                            use std::fmt::Write;
                            self.concat_buf.clear();
                            let _ = write!(self.concat_buf, "{lhs}{rhs}");
                            Rc::from(self.concat_buf.as_str())
                        }
                    };
                    self.stack[base + a as usize] = Value::Str(result);
                }

                // --- Collections ---
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

                // --- Field/Index access ---
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
                    // Read index as scalar first, then extract Rc ptr without
                    // Rc::clone (avoids refcount bump on the fast path).
                    let idx_slot = base + c as usize;
                    let obj_slot = base + b as usize;
                    let i: i64 = match &self.stack[idx_slot] {
                        Value::I32(v) => *v as i64,
                        Value::I64(v) => *v,
                        _ => {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            self.exec_get_index_reg(base, a, b, c)?;
                            continue;
                        }
                    };
                    if let Value::Array(arr) = &self.stack[obj_slot] {
                        let arr_ptr: *const RefCell<Vec<Value>> = &**arr;
                        // SAFETY: arr_ptr is valid for the duration of this block.
                        // We only write to self.stack[dst] after releasing the borrow.
                        let val = {
                            let arr = unsafe { &*arr_ptr }.borrow();
                            if i < 0 || i as usize >= arr.len() {
                                self.frames.last_mut().unwrap().pc = save_pc!();
                                return Err(self.make_error(format!(
                                    "array index {i} out of bounds (length {})",
                                    arr.len()
                                )));
                            }
                            arr[i as usize].cheap_clone()
                        };
                        self.stack[base + a as usize] = val;
                    } else {
                        self.frames.last_mut().unwrap().pc = save_pc!();
                        self.exec_get_index_reg(base, a, b, c)?;
                    }
                }
                op::SetIndex => {
                    // Inline Array+Int fast path; fallback to generic helper
                    // SetIndex(obj, idx, val) = a=obj, b=idx, c=val
                    // Read index scalar and value before borrowing array mutably.
                    let idx_slot = base + b as usize;
                    let i: i64 = match &self.stack[idx_slot] {
                        Value::I32(v) => *v as i64,
                        Value::I64(v) => *v,
                        _ => {
                            self.frames.last_mut().unwrap().pc = save_pc!();
                            self.exec_set_index_reg(base, a, b, c)?;
                            continue;
                        }
                    };
                    let obj_slot = base + a as usize;
                    if let Value::Array(arr) = &self.stack[obj_slot] {
                        let arr_ptr: *const RefCell<Vec<Value>> = &**arr;
                        let value = self.stack[base + c as usize].cheap_clone();
                        // SAFETY: arr_ptr is valid; we only read from it.
                        let mut arr = unsafe { &*arr_ptr }.borrow_mut();
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

                // --- Coroutines ---
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
                    return Ok(RunResult::Yield(WaitCondition::Frames {
                        remaining: n.saturating_sub(1),
                    }));
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

                // --- Spread ---
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

                // --- Structs ---
                op::MakeStruct => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    // ABC+W: a=dst, b=start, c=count, w1=name_hash
                    self.exec_make_struct_reg(base, a, w1, b, c, chunk_id)?;
                }

                // --- Classes ---
                op::MakeClass => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    self.exec_make_class_reg(base, a, w1, b, c, chunk_id)?;
                }

                // --- Method calls ---
                op::CallMethod => {
                    let w1 = unsafe { *ip };
                    ip = unsafe { ip.add(1) };
                    self.frames.last_mut().unwrap().pc = save_pc!();
                    // AB+W: a=base_reg, b=arg_count, w1=name_hash
                    self.exec_call_method_reg(base, a, w1, b)?;
                }

                // --- Closures ---
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

                // --- AoSoA conversion (mobile only) ---
                #[cfg(feature = "mobile-aosoa")]
                op::ConvertToAoSoA => {
                    self.exec_convert_to_aosoa_reg(base, a)?;
                }

                // --- Typed arithmetic (compiler-guaranteed types) ---
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

                // --- Immediate arithmetic ---
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

                // --- Typed comparison ---
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

                // --- Fused compare-and-jump (int) ---
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

                // --- Fused compare-and-jump (int immediate, DEAD) ---
                op::TestLtIntImm | op::TestLeIntImm | op::TestGtIntImm | op::TestGeIntImm => unsafe {
                    std::hint::unreachable_unchecked()
                },

                // --- Fused compare-and-jump (float) ---
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

                // --- Quickened arithmetic (runtime-specialized) ---
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

                // --- Quickened comparison ---
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

                // --- Unknown opcode ---
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
}
impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns a display-friendly function name from an optional function index.
/// Translates internal names: `None` -> `<script>`, `__lambda_N` -> `<lambda>`.
pub(super) fn display_function_name(
    func_index: Option<usize>,
    functions: &[CompiledFunction],
) -> String {
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
