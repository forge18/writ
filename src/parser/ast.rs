use crate::lexer::Span;

// --- Type Expressions ---

/// A type annotation: `float`, `Result<float>`, `Array<Weapon>`, `(float, float)`.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    /// Simple type name: `float`, `string`, `Player`.
    Simple(String),

    /// Generic type: `Result<float>`, `Array<Weapon>`, `Dictionary<string, int>`.
    Generic { name: String, args: Vec<TypeExpr> },

    /// Tuple type: `(float, float)`.
    Tuple(Vec<TypeExpr>),
}

// --- Shared Types ---

/// Visibility modifier for class fields and methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Default,
}

/// A function parameter: `name: Type` or `...name: Type`.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncParam {
    pub name: String,
    pub type_annotation: TypeExpr,
    pub is_variadic: bool,
}

/// An argument in a function call -- positional or named.
#[derive(Debug, Clone, PartialEq)]
pub enum CallArg {
    Positional(Expr),
    Named { name: String, value: Expr },
}

// --- Expression AST ---

/// A single expression in the Writ language.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// All expression variants for Phase 2.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Integer, float, string, or boolean literal.
    Literal(Literal),

    /// Variable or unresolved name reference.
    Identifier(String),

    /// Binary operation: `lhs op rhs`.
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    /// Unary operation: `op operand`.
    Unary { op: UnaryOp, operand: Box<Expr> },

    /// Parenthesized expression: `(expr)`.
    Grouped(Box<Expr>),

    /// Ternary: `condition ? then_expr : else_expr`.
    Ternary {
        condition: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },

    /// Range: `start..end` or `start..=end`.
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
    },

    /// Null coalescing: `lhs ?? rhs`.
    NullCoalesce { lhs: Box<Expr>, rhs: Box<Expr> },

    /// Safe member access: `object?.member`.
    SafeAccess { object: Box<Expr>, member: String },

    /// Member access: `object.member`.
    MemberAccess { object: Box<Expr>, member: String },

    /// Index access: `collection[index]`.
    Index { object: Box<Expr>, index: Box<Expr> },

    /// Type cast: `expr as Type`.
    Cast {
        expr: Box<Expr>,
        target_type: TypeExpr,
    },

    /// String interpolation: sequence of literal and expression segments.
    StringInterpolation(Vec<InterpolationSegment>),

    /// Function call: `callee(arg1, name: arg2, ...)`.
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },

    /// Lambda: `(x: float) => x * 2` or `(x: float) => { ... }`.
    Lambda {
        params: Vec<FuncParam>,
        body: LambdaBody,
    },

    /// Tuple literal: `(10.0, 20.0)`.
    Tuple(Vec<Expr>),

    /// Error propagation: `expr?` -- unwrap `Result<T>` or propagate error.
    ErrorPropagate(Box<Expr>),

    /// Array literal: `[1, 2, 3]` or `[...a, 4]`.
    ArrayLiteral(Vec<ArrayElement>),

    /// Dictionary literal: `{"key": value}` or `{...other, "key": value}`.
    DictLiteral(Vec<DictElement>),

    /// Namespace access: `alias::Member` for wildcard imports.
    NamespaceAccess { namespace: String, member: String },

    /// Yield expression: `yield`, `yield expr`, or `yield waitForSeconds(2.0)`.
    /// `None` = bare yield (suspend one frame). `Some(expr)` = yield with argument.
    Yield(Option<Box<Expr>>),

    /// Super method call: `super.methodName(args)`.
    /// Only valid inside a class method that has a parent class.
    Super { method: String, args: Vec<CallArg> },

    /// `when` used as an expression -- each arm yields a value.
    When {
        subject: Option<Box<Expr>>,
        arms: Vec<WhenArm>,
    },
}

/// An element inside an array literal -- either a plain expression or a spread.
#[derive(Debug, Clone, PartialEq)]
pub enum ArrayElement {
    /// A regular expression element: `42`.
    Expr(Expr),
    /// A spread element: `...arr`.
    Spread(Expr),
}

/// An entry in a dictionary literal -- either a key-value pair or a spread.
#[derive(Debug, Clone, PartialEq)]
pub enum DictElement {
    /// A `key: value` pair.
    KeyValue { key: Expr, value: Expr },
    /// A spread: `...other_dict`.
    Spread(Expr),
}

/// Body of a lambda -- single expression or block.
#[derive(Debug, Clone, PartialEq)]
pub enum LambdaBody {
    Expr(Box<Expr>),
    Block(Vec<Stmt>),
}

/// Literal values.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

/// Binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}

/// A segment of a string interpolation.
#[derive(Debug, Clone, PartialEq)]
pub enum InterpolationSegment {
    Literal(String),
    Expression(Expr),
}

// --- Statement AST (Phase 3) ---

/// A single statement in the Writ language.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

/// All statement variants.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// `let name: type = value` -- runtime immutable binding.
    Let {
        name: String,
        type_annotation: Option<TypeExpr>,
        initializer: Expr,
    },

    /// `var name: type = value` -- mutable binding.
    Var {
        name: String,
        type_annotation: Option<TypeExpr>,
        initializer: Expr,
    },

    /// `const name = value` -- compile-time constant.
    Const { name: String, initializer: Expr },

    /// `target = value` or `target += value`, etc.
    Assignment {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },

    /// Expression used as a statement (e.g., function call).
    ExprStmt(Expr),

    /// `return expr` or `return`.
    Return(Option<Expr>),

    /// `break`
    Break,

    /// `continue`
    Continue,

    /// `{ stmt* }` -- block of statements.
    Block(Vec<Stmt>),

    /// `if condition { ... } else if ... else { ... }`
    If {
        condition: Expr,
        then_block: Vec<Stmt>,
        else_branch: Option<ElseBranch>,
    },

    /// `while condition { body }`
    While { condition: Expr, body: Vec<Stmt> },

    /// `for variable in iterable { body }`
    For {
        variable: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },

    /// `when subject? { arms }`
    When {
        subject: Option<Expr>,
        arms: Vec<WhenArm>,
    },

    /// `let (x, y) = expr` -- tuple destructuring.
    LetDestructure {
        names: Vec<String>,
        initializer: Expr,
    },

    /// Function declaration.
    Func(FuncDecl),

    /// Class declaration.
    Class(ClassDecl),

    /// Trait declaration.
    Trait(TraitDecl),

    /// Enum declaration.
    Enum(EnumDecl),

    /// Struct declaration.
    Struct(StructDecl),

    /// Named import: `import { A, B } from "path"`.
    Import(ImportDecl),

    /// Wildcard import: `import * as alias from "path"`.
    WildcardImport(WildcardImportDecl),

    /// Export wrapper: `export class Foo { ... }`.
    Export(Box<Stmt>),

    /// `start expr` -- launches a coroutine, returns immediately.
    Start(Expr),
}

/// The else branch of an if statement -- either another if or a block.
#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch {
    ElseIf(Box<Stmt>),
    ElseBlock(Vec<Stmt>),
}

/// Assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
}

/// A single arm in a `when` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct WhenArm {
    pub pattern: WhenPattern,
    pub body: WhenBody,
}

/// Pattern variants for `when` arms.
#[derive(Debug, Clone, PartialEq)]
pub enum WhenPattern {
    /// Single value: `0 => ...`
    Value(Expr),

    /// Multiple values: `0, 1, 2 => ...`
    MultipleValues(Vec<Expr>),

    /// Range: `0..25 => ...` or `0..=25 => ...`
    Range {
        start: Expr,
        end: Expr,
        inclusive: bool,
    },

    /// Type match with optional binding: `is Success(value) => ...`
    TypeMatch {
        type_name: String,
        binding: Option<String>,
    },

    /// Guard: `x if x < 0 => ...`
    Guard { binding: String, condition: Expr },

    /// `else => ...` (default arm)
    Else,
}

/// Body of a `when` arm -- either a single expression or a block.
#[derive(Debug, Clone, PartialEq)]
pub enum WhenBody {
    Expr(Expr),
    Block(Vec<Stmt>),
}

// --- Declaration AST (Phase 4) ---

/// A top-level declaration in a Writ file.
#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    pub kind: DeclKind,
    pub span: Span,
}

/// All top-level declaration variants.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclKind {
    Func(FuncDecl),
    Class(ClassDecl),
    Trait(TraitDecl),
    Enum(EnumDecl),
    Struct(StructDecl),
    Import(ImportDecl),
    WildcardImport(WildcardImportDecl),
    Export(Box<Decl>),
    Stmt(Stmt),
}

/// A constraint on a type parameter: `T : TraitName`.
/// Used in `where` clauses on generic functions and classes.
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// The type parameter being constrained (e.g., `"T"`).
    pub type_param: String,
    /// The trait that the type parameter must implement (e.g., `"Updatable"`).
    pub trait_name: String,
}

/// Function declaration: `func name[<T>](params) -> Type [where T : Trait] { body }`.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub params: Vec<FuncParam>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    pub is_static: bool,
    pub visibility: Visibility,
    /// Generic constraints: `where T : Trait, U : OtherTrait`.
    pub where_clauses: Vec<WhereClause>,
}

/// Class declaration: `class Name<T, U> [where T : Trait] extends Parent with Trait1, Trait2 { ... }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub extends: Option<String>,
    pub traits: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FuncDecl>,
    /// Generic constraints on the class's type parameters.
    pub where_clauses: Vec<WhereClause>,
}

/// A field in a class or enum: `[visibility] name: Type [= default] [set(param) { ... }]`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub type_annotation: TypeExpr,
    pub default: Option<Expr>,
    pub visibility: Visibility,
    pub setter: Option<Setter>,
}

/// Custom setter for a field: `set(param) { body }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Setter {
    pub param_name: String,
    pub body: Vec<Stmt>,
}

/// Trait declaration: `trait Name { methods }`.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitDecl {
    pub name: String,
    pub methods: Vec<TraitMethod>,
}

/// A method signature in a trait, optionally with a default body.
#[derive(Debug, Clone, PartialEq)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<FuncParam>,
    pub return_type: Option<TypeExpr>,
    pub default_body: Option<Vec<Stmt>>,
}

/// Enum declaration: `enum Name { variants; fields; methods }`.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FuncDecl>,
}

/// A single enum variant, optionally with a value.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<Expr>,
}

/// Struct declaration: `struct Name<T> { fields; methods }`.
/// Value type -- no inheritance, no traits.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<FuncDecl>,
}

/// Named import: `import { A, B } from "path"`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub names: Vec<String>,
    pub from: String,
}

/// Wildcard import: `import * as alias from "path"`.
#[derive(Debug, Clone, PartialEq)]
pub struct WildcardImportDecl {
    pub alias: String,
    pub from: String,
}
