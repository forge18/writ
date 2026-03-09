//! Writ -- an embeddable scripting language for Rust.
//!
//! The [`Writ`] struct is the primary entry point. It composes the full
//! pipeline (lexer -> parser -> type checker -> compiler -> VM) behind a
//! simple API suitable for embedding in game engines and other host
//! applications.
//!
//! # Example
//!
//! ```no_run
//! use writ::Writ;
//!
//! let mut writ = Writ::new();
//! let result = writ.run(r#"return 1 + 2"#).unwrap();
//! ```

use std::fs;

pub mod codegen;
pub mod compiler;
pub mod lexer;
pub mod parser;
pub mod stdlib;
pub mod types;
pub mod vm;

use compiler::Compiler;
use lexer::Lexer;
use parser::Parser;
use types::TypeChecker;
use vm::VM;

// Re-export key types that embedders need.
pub use compiler::CompileError;
pub use lexer::{LexError, SourceLine, format_error_context};
pub use parser::ParseError;
pub use types::{Type, TypeError};
// FloatValue and IntValue have been flattened into Value directly (I32/I64/F32/F64 variants).
#[cfg(feature = "debug-hooks")]
pub use vm::{BreakpointAction, BreakpointContext};
pub use vm::{Fn0, Fn1, Fn2, Fn3, MFn0, MFn1, MFn2, MFn3};
pub use vm::{NativeResult, Sequence, SequenceAction, WritFn};
pub use vm::{RuntimeError, StackFrame, StackTrace, Value, ValueTag, WritObject};
pub use vm::{SeqFn1, SeqFn2, SeqFn3, SeqMFn0, SeqMFn1, SeqMFn2, SeqMFn3};
pub use vm::{fn0, fn1, fn2, fn3, mfn0, mfn1, mfn2, mfn3};
pub use vm::{seq_fn1, seq_fn2, seq_fn3, seq_mfn0, seq_mfn1, seq_mfn2, seq_mfn3};

// Re-export codegen for embedders that want Rust source output.
pub use codegen::RustCodegen;

#[derive(Debug)]
pub enum WritError {
    Lex(LexError),
    Parse(ParseError),
    Type(TypeError),
    Compile(CompileError),
    Runtime(RuntimeError),
    Io(std::io::Error),
}

impl std::fmt::Display for WritError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WritError::Lex(e) => write!(f, "{e}"),
            WritError::Parse(e) => write!(f, "{e}"),
            WritError::Type(e) => write!(f, "{e}"),
            WritError::Compile(e) => write!(f, "{e}"),
            WritError::Runtime(e) => write!(f, "{e}"),
            WritError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for WritError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WritError::Lex(e) => Some(e),
            WritError::Parse(e) => Some(e),
            WritError::Type(e) => Some(e),
            WritError::Compile(e) => Some(e),
            WritError::Runtime(e) => Some(e),
            WritError::Io(e) => Some(e),
        }
    }
}

impl From<LexError> for WritError {
    fn from(e: LexError) -> Self {
        WritError::Lex(e)
    }
}

impl From<ParseError> for WritError {
    fn from(e: ParseError) -> Self {
        WritError::Parse(e)
    }
}

impl From<TypeError> for WritError {
    fn from(e: TypeError) -> Self {
        WritError::Type(e)
    }
}

impl From<CompileError> for WritError {
    fn from(e: CompileError) -> Self {
        WritError::Compile(e)
    }
}

impl From<RuntimeError> for WritError {
    fn from(e: RuntimeError) -> Self {
        WritError::Runtime(e)
    }
}

impl From<std::io::Error> for WritError {
    fn from(e: std::io::Error) -> Self {
        WritError::Io(e)
    }
}

/// The Writ scripting engine.
///
/// Composes the full pipeline (lexer -> parser -> type checker -> compiler -> VM)
/// behind a single struct. Create one instance per scripting context.
pub struct Writ {
    vm: VM,
    type_checker: TypeChecker,
    type_check_enabled: bool,
}

impl Default for Writ {
    fn default() -> Self {
        Self::new()
    }
}

impl Writ {
    /// Creates a new Writ instance with the standard library registered.
    pub fn new() -> Self {
        let mut vm = VM::new();
        stdlib::register_all(&mut vm);
        Self {
            vm,
            type_checker: TypeChecker::new(),
            type_check_enabled: true,
        }
    }

    /// Runs a source string through the full pipeline and returns the result.
    ///
    /// Pipeline: lex -> parse -> type check -> compile -> execute.
    pub fn run(&mut self, source: &str) -> Result<Value, WritError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;

        let mut compiler = Compiler::new();
        compiler.set_native_index(self.vm.native_fn_index_map().clone());
        compiler.pre_register_classes(&stmts);
        if self.type_check_enabled {
            let typed_stmts = self.type_checker.check_program_typed(&stmts)?;
            for typed in &typed_stmts {
                compiler.compile_typed_stmt(typed)?;
            }
        } else {
            for stmt in &stmts {
                compiler.compile_stmt(stmt)?;
            }
        }
        let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();

        Ok(self
            .vm
            .execute_program(&chunk, &functions, &struct_metas, &class_metas)?)
    }

    /// Compiles and loads a file's functions into the VM without clearing
    /// existing state.
    ///
    /// The file's top-level code is executed for side effects (e.g. global
    /// initialization). Functions defined in the file become callable via
    /// [`call`](Writ::call) or from scripts run with [`run`](Writ::run).
    pub fn load(&mut self, path: &str) -> Result<(), WritError> {
        let source = fs::read_to_string(path)?;

        let mut lexer = Lexer::with_file(&source, path);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;

        let mut compiler = Compiler::new();
        compiler.set_native_index(self.vm.native_fn_index_map().clone());
        compiler.pre_register_classes(&stmts);
        if self.type_check_enabled {
            let typed_stmts = self.type_checker.check_program_typed(&stmts)?;
            for typed in &typed_stmts {
                compiler.compile_typed_stmt(typed)?;
            }
        } else {
            for stmt in &stmts {
                compiler.compile_stmt(stmt)?;
            }
        }
        let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();

        self.vm
            .load_module(&chunk, &functions, &struct_metas, &class_metas)?;
        Ok(())
    }

    /// Calls a named function that was previously compiled via [`run`](Writ::run)
    /// or [`load`](Writ::load).
    pub fn call(&mut self, name: &str, args: &[Value]) -> Result<Value, WritError> {
        Ok(self.vm.call_function(name, args)?)
    }

    /// Advances the coroutine scheduler by `delta` seconds.
    ///
    /// When a tick source is registered via [`set_tick_source`](Self::set_tick_source),
    /// coroutines advance automatically and this method is not needed.
    pub fn tick(&mut self, delta: f64) -> Result<(), WritError> {
        Ok(self.vm.tick(delta)?)
    }

    /// Registers a tick source for automatic coroutine advancement.
    ///
    /// When set, coroutines advance automatically at the start of each
    /// [`call`](Self::call) or [`load`](Self::load) invocation. The callback
    /// should return the elapsed time in seconds since the last call.
    ///
    /// For manual control, leave unset and call [`tick`](Self::tick) directly.
    pub fn set_tick_source<F: Fn() -> f64 + 'static>(&mut self, source: F) -> &mut Self {
        self.vm.set_tick_source(source);
        self
    }

    /// Convenience: use wall-clock time as the tick source.
    ///
    /// Equivalent to registering a tick source that tracks elapsed time
    /// via `std::time::Instant`. Suitable for non-game applications or
    /// prototyping where real-time progression is acceptable.
    pub fn use_wall_clock(&mut self) -> &mut Self {
        let last = std::cell::Cell::new(std::time::Instant::now());
        self.set_tick_source(move || {
            let now = std::time::Instant::now();
            let delta = now.duration_since(last.get()).as_secs_f64();
            last.set(now);
            delta
        })
    }

    /// Removes the tick source, reverting to manual [`tick`](Self::tick) mode.
    pub fn clear_tick_source(&mut self) -> &mut Self {
        self.vm.clear_tick_source();
        self
    }

    /// Registers a host function that scripts can call.
    pub fn register_fn<H>(&mut self, name: &str, handler: H) -> &mut Self
    where
        H: vm::binding::IntoNativeHandler,
    {
        self.vm.register_fn(name, handler);
        self
    }

    /// Registers a host function in both the VM and the type checker atomically.
    ///
    /// This is the preferred way to expose host functions to scripts. Calling
    /// [`register_fn`](Self::register_fn) alone leaves the type checker unaware
    /// of the function, causing type errors in type-checked scripts. Calling
    /// [`type_checker_mut`](Self::type_checker_mut) alone registers the
    /// signature without the runtime handler.
    ///
    /// `params` and `return_type` are [`Type`] values from `writ_types`. Use
    /// `Type::Any` for parameters that accept any value.
    pub fn register_host_fn<H>(
        &mut self,
        name: &str,
        params: Vec<Type>,
        return_type: Type,
        handler: H,
    ) -> &mut Self
    where
        H: vm::binding::IntoNativeHandler,
    {
        self.type_checker
            .register_host_function(name, params, return_type);
        self.vm.register_fn(name, handler);
        self
    }

    /// Registers a host function without type information.
    ///
    /// The type checker treats calls to this function as valid regardless of
    /// argument count or types. All other type checking continues normally.
    /// Argument expressions are still checked for undefined variable references.
    ///
    /// Prefer [`register_host_fn`](Self::register_host_fn) when type information
    /// is available. Use this only for dynamic dispatch functions or FFI wrappers
    /// whose signatures cannot be expressed in the Writ type system.
    pub fn register_host_fn_untyped<H>(&mut self, name: &str, handler: H) -> &mut Self
    where
        H: vm::binding::IntoNativeHandler,
    {
        self.type_checker
            .register_host_function(name, vec![Type::Unknown], Type::Unknown);
        self.vm.register_fn(name, handler);
        self
    }

    /// Registers a method on a value type that scripts can call.
    pub fn register_method<H>(
        &mut self,
        tag: ValueTag,
        name: &str,
        module: Option<&str>,
        handler: H,
    ) -> &mut Self
    where
        H: vm::binding::IntoNativeMethodHandler,
    {
        self.vm.register_method(tag, name, module, handler);
        self
    }

    /// Registers a sequence-capable host function that can invoke script callbacks.
    ///
    /// The handler returns [`NativeResult`] — either an immediate value or a
    /// [`Sequence`] that the VM polls to drive deferred callback invocations.
    pub fn register_seq_fn<H>(&mut self, name: &str, handler: H) -> &mut Self
    where
        H: vm::binding::IntoNativeSeqHandler,
    {
        self.vm.register_seq_fn(name, handler.into_seq_handler());
        self
    }

    /// Registers a sequence-capable method that can invoke script callbacks.
    pub fn register_seq_method<H>(
        &mut self,
        tag: ValueTag,
        name: &str,
        module: Option<&str>,
        handler: H,
    ) -> &mut Self
    where
        H: vm::binding::IntoNativeSeqMethodHandler,
    {
        self.vm
            .register_seq_method(tag, name, module, handler.into_seq_method_handler());
        self
    }

    /// Registers a global constant or variable accessible from scripts.
    pub fn register_global(&mut self, name: &str, value: Value) -> &mut Self {
        self.vm.register_global(name, value);
        self
    }

    /// Registers a host type with a factory function.
    ///
    /// Scripts call `TypeName(args...)` to create instances of the type.
    pub fn register_type<F>(&mut self, name: &str, factory: F) -> &mut Self
    where
        F: Fn(&[Value]) -> Result<Box<dyn WritObject>, String> + 'static,
    {
        self.vm.register_type(name, factory);
        self
    }

    /// Reloads a script file through the full pipeline (lex -> parse ->
    /// type-check -> compile) and swaps function bytecode in place.
    ///
    /// Existing VM state (globals, live coroutines) is preserved. Only
    /// function bodies are updated; the main chunk is discarded because it
    /// has already executed.
    ///
    /// Type checking honours [`disable_type_checking`](Self::disable_type_checking).
    pub fn reload(&mut self, file: &str) -> Result<(), WritError> {
        let source = fs::read_to_string(file)?;

        let mut lexer = Lexer::with_file(&source, file);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;

        let mut compiler = Compiler::new();
        compiler.set_native_index(self.vm.native_fn_index_map().clone());
        compiler.pre_register_classes(&stmts);
        if self.type_check_enabled {
            let typed_stmts = self.type_checker.check_program_typed(&stmts)?;
            for typed in &typed_stmts {
                compiler.compile_typed_stmt(typed)?;
            }
        } else {
            for stmt in &stmts {
                compiler.compile_stmt(stmt)?;
            }
        }
        let (chunk, functions, struct_metas, class_metas) = compiler.into_parts();

        self.vm
            .reload_compiled(file, &chunk, &functions, &struct_metas, &class_metas)
            .map_err(WritError::from)
    }

    /// Cancels all coroutines owned by the given object ID.
    ///
    /// Call this when destroying a game object to ensure its coroutines
    /// are cleaned up (structured concurrency).
    pub fn cancel_coroutines_for_owner(&mut self, owner_id: u64) {
        self.vm.cancel_coroutines_for_owner(owner_id);
    }

    /// Sets the maximum number of instructions before the VM terminates
    /// execution with an error.
    pub fn set_instruction_limit(&mut self, limit: u64) -> &mut Self {
        self.vm.set_instruction_limit(limit);
        self
    }

    /// Disables a standard library module, blocking all its functions.
    pub fn disable_module(&mut self, module: &str) -> &mut Self {
        self.vm.disable_module(module);
        self
    }

    /// Disables type checking for subsequent [`run`](Writ::run) and
    /// [`load`](Writ::load) calls.
    ///
    /// Useful when scripts call host-registered functions whose type
    /// signatures have not been registered with the type checker.
    pub fn disable_type_checking(&mut self) -> &mut Self {
        self.type_check_enabled = false;
        self
    }

    /// Enables type checking for subsequent [`run`](Writ::run) and
    /// [`load`](Writ::load) calls.
    pub fn enable_type_checking(&mut self) -> &mut Self {
        self.type_check_enabled = true;
        self
    }

    /// Resets the type checker to an empty state, discarding all
    /// script-defined types and functions accumulated across prior
    /// [`run`](Self::run) and [`load`](Self::load) calls.
    ///
    /// Host-registered functions and types (added via
    /// [`register_host_fn`](Self::register_host_fn) and
    /// [`register_type`](Self::register_type)) are also cleared; re-register
    /// them after calling this method if type checking is enabled.
    ///
    /// Use this when you need strict per-invocation isolation -- for example,
    /// when executing untrusted or user-supplied scripts where previously
    /// accumulated definitions should not be visible.
    pub fn reset_script_types(&mut self) -> &mut Self {
        self.type_checker = TypeChecker::new();
        self
    }

    /// Registers a breakpoint at the given source file and line number.
    #[cfg(feature = "debug-hooks")]
    pub fn set_breakpoint(&mut self, file: &str, line: u32) -> &mut Self {
        self.vm.set_breakpoint(file, line);
        self
    }

    /// Removes a previously registered breakpoint.
    #[cfg(feature = "debug-hooks")]
    pub fn remove_breakpoint(&mut self, file: &str, line: u32) -> &mut Self {
        self.vm.remove_breakpoint(file, line);
        self
    }

    /// Registers a callback invoked when a breakpoint is hit.
    #[cfg(feature = "debug-hooks")]
    pub fn on_breakpoint<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&vm::BreakpointContext) -> vm::BreakpointAction + 'static,
    {
        self.vm.on_breakpoint(handler);
        self
    }

    /// Registers a hook called before each new source line executes.
    #[cfg(feature = "debug-hooks")]
    pub fn on_line<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, u32) + 'static,
    {
        self.vm.on_line(handler);
        self
    }

    /// Registers a hook called when any function is entered.
    #[cfg(feature = "debug-hooks")]
    pub fn on_call<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, &str, u32) + 'static,
    {
        self.vm.on_call(handler);
        self
    }

    /// Registers a hook called when any function returns.
    #[cfg(feature = "debug-hooks")]
    pub fn on_return<F>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(&str, &str, u32) + 'static,
    {
        self.vm.on_return(handler);
        self
    }

    /// Returns a mutable reference to the underlying VM.
    ///
    /// Prefer the dedicated methods on [`Writ`] for common operations
    /// (breakpoints, debug hooks, function registration). Use this only when
    /// you need VM capabilities not yet exposed on `Writ`.
    #[doc(hidden)]
    pub fn vm_mut(&mut self) -> &mut VM {
        &mut self.vm
    }

    /// Returns a mutable reference to the type checker.
    ///
    /// Prefer [`register_host_fn`](Self::register_host_fn) and
    /// [`register_host_type`](Self::register_type) for host type registration.
    /// Use this only for advanced type checker access not yet exposed on `Writ`.
    #[doc(hidden)]
    pub fn type_checker_mut(&mut self) -> &mut TypeChecker {
        &mut self.type_checker
    }

    /// Generates Rust source code for all class and trait declarations in
    /// the given Writ source.
    ///
    /// The output includes struct definitions with `Deref`-based inheritance,
    /// trait definitions, constructors, and `WritObject` implementations.
    pub fn codegen_rust(&mut self, source: &str) -> Result<String, WritError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;

        self.type_checker.check_program_typed(&stmts)?;

        let codegen = codegen::RustCodegen::new();
        Ok(codegen.generate(self.type_checker.registry()))
    }
}
