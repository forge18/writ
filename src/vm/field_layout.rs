use std::collections::{HashMap, HashSet};

use crate::compiler::string_hash;

/// Shared field layout for all instances of a given struct or class type.
///
/// Built once at load time from `StructMeta`/`ClassMeta`. Shared via `Rc`
/// across all instances of the same type to avoid per-instance string
/// cloning and per-access HashMap lookups.
#[derive(Debug, Clone)]
pub struct FieldLayout {
    /// The type name (e.g. "Point", "Player").
    pub type_name: String,
    /// Number of fields (length of the instance's `Vec<Value>`).
    pub field_count: usize,
    /// Maps field name hash (FNV-1a u32) to index in the `Vec<Value>`.
    pub hash_to_index: HashMap<u32, usize>,
    /// Field names in declaration order (index i = field_names\[i\]).
    /// Used for Display, Hash, reflection, and AoSoA interop.
    pub field_names: Vec<String>,
    /// Set of public field names (for hasField/fields reflection).
    pub public_fields: HashSet<String>,
    /// Set of public method names (for hasMethod/methods reflection).
    pub public_methods: HashSet<String>,
}

impl FieldLayout {
    /// Builds a `FieldLayout` from field names and visibility sets.
    pub fn new(
        type_name: String,
        field_names: Vec<String>,
        public_fields: HashSet<String>,
        public_methods: HashSet<String>,
    ) -> Self {
        let field_count = field_names.len();
        let mut hash_to_index = HashMap::with_capacity(field_count);
        for (i, name) in field_names.iter().enumerate() {
            let prev = hash_to_index.insert(string_hash(name), i);
            debug_assert!(
                prev.is_none(),
                "FNV-1a hash collision for fields in type '{type_name}': '{}' collides at index {i}",
                name
            );
        }
        Self {
            type_name,
            field_count,
            hash_to_index,
            field_names,
            public_fields,
            public_methods,
        }
    }
}
