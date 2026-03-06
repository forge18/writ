//! Writ — an embeddable scripting language for Rust.
//!
//! The [`Writ`] struct is the primary entry point. It composes the full
//! pipeline (lexer → parser → type checker → compiler → VM) behind a
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

use writ_compiler::Compiler;
use writ_lexer::Lexer;
use writ_parser::Parser;
use writ_types::TypeChecker;
use writ_vm::VM;

// Re-export key types that embedders need.
pub use writ_compiler::CompileError;
pub use writ_lexer::{LexError, SourceLine, format_error_context};
pub use writ_parser::ParseError;
pub use writ_types::{Type, TypeError};
// FloatValue and IntValue have been flattened into Value directly (I32/I64/F32/F64 variants).
pub use writ_vm::{Fn0, Fn1, Fn2, Fn3, MFn0, MFn1, MFn2, MFn3};
pub use writ_vm::{RuntimeError, StackFrame, StackTrace, Value, ValueTag, WritObject};
pub use writ_vm::{fn0, fn1, fn2, fn3, mfn0, mfn1, mfn2, mfn3};

// Re-export codegen for embedders that want Rust source output.
pub use writ_codegen::RustCodegen;

/// Unified error type for the Writ pipeline.
///
/// Each variant wraps the error type from the corresponding pipeline stage.
#[derive(Debug)]
pub enum WritError {
    /// Error during lexical analysis.
    Lex(LexError),
    /// Error during parsing.
    Parse(ParseError),
    /// Error during type checking.
    Type(TypeError),
    /// Error during bytecode compilation.
    Compile(CompileError),
    /// Error during VM execution.
    Runtime(RuntimeError),
    /// I/O error (e.g. file not found in [`Writ::load`]).
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
/// Composes the full pipeline (lexer → parser → type checker → compiler → VM)
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
        writ_stdlib::register_all(&mut vm);
        Self {
            vm,
            type_checker: TypeChecker::new(),
            type_check_enabled: true,
        }
    }

    /// Runs a source string through the full pipeline and returns the result.
    ///
    /// Pipeline: lex → parse → type check → compile → execute.
    pub fn run(&mut self, source: &str) -> Result<Value, WritError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize()?;

        let mut parser = Parser::new(tokens);
        let stmts = parser.parse_program()?;

        if self.type_check_enabled {
            self.type_checker.check_program(&stmts)?;
        }

        let mut compiler = Compiler::new();
        for stmt in &stmts {
            compiler.compile_stmt(stmt)?;
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

        if self.type_check_enabled {
            self.type_checker.check_program(&stmts)?;
        }

        let mut compiler = Compiler::new();
        for stmt in &stmts {
            compiler.compile_stmt(stmt)?;
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
    pub fn tick(&mut self, delta: f64) -> Result<(), WritError> {
        Ok(self.vm.tick(delta)?)
    }

    /// Registers a host function that scripts can call.
    pub fn register_fn<H>(&mut self, name: &str, handler: H) -> &mut Self
    where
        H: writ_vm::binding::IntoNativeHandler,
    {
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
        H: writ_vm::binding::IntoNativeMethodHandler,
    {
        self.vm.register_method(tag, name, module, handler);
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

    /// Reloads a script file, recompiling it and swapping the bytecode
    /// in place. Existing object state is preserved where possible.
    pub fn reload(&mut self, file: &str) -> Result<(), WritError> {
        let source = fs::read_to_string(file)?;
        self.vm.reload(file, &source).map_err(|msg| {
            WritError::Runtime(RuntimeError {
                message: msg,
                trace: StackTrace { frames: vec![] },
            })
        })
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

    /// Returns a mutable reference to the underlying VM for advanced
    /// configuration (breakpoints, debug hooks, etc.).
    pub fn vm_mut(&mut self) -> &mut VM {
        &mut self.vm
    }

    /// Returns a mutable reference to the type checker for registering
    /// host type signatures.
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

        self.type_checker.check_program(&stmts)?;

        let codegen = writ_codegen::RustCodegen::new();
        Ok(codegen.generate(self.type_checker.registry()))
    }
}
