use std::collections::HashMap;

use super::types::Type;

/// Mutability of a variable binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mutability {
    /// `let` -- immutable after initialization.
    Immutable,
    /// `var` -- can be reassigned.
    Mutable,
    /// `const` -- compile-time constant, immutable.
    Constant,
}

/// Information stored for each variable in the type environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarInfo {
    pub ty: Type,
    pub mutability: Mutability,
}

/// A scoped symbol table for tracking variable types and mutability.
///
/// The environment is a stack of scopes. Each scope is a `HashMap` from
/// variable name to [`VarInfo`]. [`push_scope`](TypeEnv::push_scope) creates
/// a new scope; [`pop_scope`](TypeEnv::pop_scope) discards it.
/// [`lookup`](TypeEnv::lookup) searches from innermost to outermost scope.
pub struct TypeEnv {
    scopes: Vec<HashMap<String, VarInfo>>,
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeEnv {
    /// Creates a new type environment with a single global scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Pushes a new scope onto the stack.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pops the innermost scope. Panics if only the global scope remains.
    pub fn pop_scope(&mut self) {
        assert!(self.scopes.len() > 1, "cannot pop the global scope");
        self.scopes.pop();
    }

    /// Defines a variable in the current (innermost) scope.
    ///
    /// If the variable already exists in the current scope, it is overwritten
    /// (shadowing within the same scope).
    pub fn define(&mut self, name: &str, info: VarInfo) {
        self.scopes
            .last_mut()
            .expect("at least one scope must exist")
            .insert(name.to_string(), info);
    }

    /// Looks up a variable by name, searching from innermost to outermost scope.
    ///
    /// Returns `None` if the variable is not defined in any visible scope.
    pub fn lookup(&self, name: &str) -> Option<&VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    /// Returns all variable names visible in the current scope chain.
    ///
    /// Inner scopes shadow outer scopes -- each name appears only once with
    /// the innermost binding. Used by the LSP for completion suggestions.
    pub fn all_visible(&self) -> Vec<(&str, &VarInfo)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for scope in self.scopes.iter().rev() {
            for (name, info) in scope {
                if seen.insert(name.as_str()) {
                    result.push((name.as_str(), info));
                }
            }
        }
        result
    }

    /// Returns all variable names visible in the current scope chain, with
    /// scope depth information.
    ///
    /// Each entry is `(name, info, depth)` where `depth` is 0 for the
    /// innermost scope, 1 for the next, etc. Used for weighted suggestion
    /// scoring that prefers same-scope matches.
    pub fn all_visible_with_depth(&self) -> Vec<(&str, &VarInfo, usize)> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for (depth, scope) in self.scopes.iter().rev().enumerate() {
            for (name, info) in scope {
                if seen.insert(name.as_str()) {
                    result.push((name.as_str(), info, depth));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn define_and_lookup_global() {
        let mut env = TypeEnv::new();
        env.define(
            "x",
            VarInfo {
                ty: Type::Int,
                mutability: Mutability::Immutable,
            },
        );
        let info = env.lookup("x").unwrap();
        assert_eq!(info.ty, Type::Int);
        assert_eq!(info.mutability, Mutability::Immutable);
    }

    #[test]
    fn lookup_inner_scope() {
        let mut env = TypeEnv::new();
        env.push_scope();
        env.define(
            "y",
            VarInfo {
                ty: Type::Str,
                mutability: Mutability::Mutable,
            },
        );
        assert_eq!(env.lookup("y").unwrap().ty, Type::Str);
    }

    #[test]
    fn pop_scope_removes_inner_variables() {
        let mut env = TypeEnv::new();
        env.push_scope();
        env.define(
            "y",
            VarInfo {
                ty: Type::Int,
                mutability: Mutability::Immutable,
            },
        );
        env.pop_scope();
        assert!(env.lookup("y").is_none());
    }

    #[test]
    fn shadowing_inner_scope() {
        let mut env = TypeEnv::new();
        env.define(
            "x",
            VarInfo {
                ty: Type::Int,
                mutability: Mutability::Immutable,
            },
        );
        env.push_scope();
        env.define(
            "x",
            VarInfo {
                ty: Type::Str,
                mutability: Mutability::Mutable,
            },
        );
        assert_eq!(env.lookup("x").unwrap().ty, Type::Str);
        env.pop_scope();
        assert_eq!(env.lookup("x").unwrap().ty, Type::Int);
    }

    #[test]
    fn lookup_undefined_returns_none() {
        let env = TypeEnv::new();
        assert!(env.lookup("missing").is_none());
    }

    #[test]
    #[should_panic(expected = "cannot pop the global scope")]
    fn pop_global_scope_panics() {
        let mut env = TypeEnv::new();
        env.pop_scope();
    }
}
