use std::collections::HashMap;

use super::types::Type;

/// Maps module paths to their exported names and types.
///
/// The type checker does not load files — the caller populates this
/// registry before type checking begins.
pub struct ModuleRegistry {
    modules: HashMap<String, HashMap<String, Type>>,
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    /// Register all exports from a module.
    pub fn register_module(&mut self, path: &str, exports: HashMap<String, Type>) {
        self.modules.insert(path.to_string(), exports);
    }

    /// Look up a named export from a module.
    pub fn get_export(&self, path: &str, name: &str) -> Option<&Type> {
        self.modules.get(path)?.get(name)
    }

    /// Get all exports from a module (for wildcard imports).
    pub fn get_module(&self, path: &str) -> Option<&HashMap<String, Type>> {
        self.modules.get(path)
    }

    /// Returns an iterator over all registered module paths.
    pub fn all_paths(&self) -> impl Iterator<Item = &str> {
        self.modules.keys().map(|s| s.as_str())
    }

    /// Returns all export names for a module.
    pub fn export_names(&self, path: &str) -> Vec<&str> {
        self.modules
            .get(path)
            .map(|m| m.keys().map(|s| s.as_str()).collect())
            .unwrap_or_default()
    }
}
