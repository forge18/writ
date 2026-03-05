//! AoSoA (Array of Structs of Arrays) memory layout for cache-friendly
//! batch operations on homogeneous struct collections.
//!
//! This module is only compiled when the `mobile-aosoa` feature is enabled.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::value::Value;
use crate::writ_struct::WritStruct;

/// Default elements per chunk, chosen for cache-line alignment.
pub const DEFAULT_CHUNK_SIZE: usize = 64;

/// One chunk holding up to `chunk_size` elements in columnar layout.
#[derive(Debug, Clone)]
struct AoSoAChunk {
    /// One column per field. `columns[i]` holds values for `field_names[i]`.
    columns: Vec<Vec<Value>>,
    /// Number of elements in this chunk.
    len: usize,
}

impl AoSoAChunk {
    fn new(num_fields: usize, chunk_size: usize) -> Self {
        Self {
            columns: (0..num_fields)
                .map(|_| Vec::with_capacity(chunk_size))
                .collect(),
            len: 0,
        }
    }

    fn is_full(&self, chunk_size: usize) -> bool {
        self.len >= chunk_size
    }
}

/// AoSoA container for homogeneous struct arrays.
///
/// Stores struct fields in contiguous columnar chunks for improved cache
/// coherence when iterating over a single field across many elements.
#[derive(Debug, Clone)]
pub struct AoSoAContainer {
    /// The struct type name all elements share.
    pub type_name: String,
    /// Field names in declaration order.
    pub field_names: Vec<String>,
    /// Public field names.
    pub public_fields: HashSet<String>,
    /// Public method names.
    pub public_methods: HashSet<String>,
    /// Chunks of interleaved SoA data.
    chunks: Vec<AoSoAChunk>,
    /// Number of elements per chunk.
    chunk_size: usize,
    /// Total element count across all chunks.
    total_len: usize,
}

impl AoSoAContainer {
    /// Creates a new AoSoA container with the given field layout.
    pub fn new(
        type_name: String,
        field_names: Vec<String>,
        public_fields: HashSet<String>,
        public_methods: HashSet<String>,
        capacity: usize,
    ) -> Self {
        let chunk_size = DEFAULT_CHUNK_SIZE;
        let num_fields = field_names.len();
        let _ = capacity; // Capacity hint not used for pre-allocation
        let chunks = vec![AoSoAChunk::new(num_fields, chunk_size)];
        Self {
            type_name,
            field_names,
            public_fields,
            public_methods,
            chunks,
            chunk_size,
            total_len: 0,
        }
    }

    /// Pushes a struct value, decomposing it into columnar storage.
    pub fn push(&mut self, writ_struct: &WritStruct) -> Result<(), String> {
        if writ_struct.type_name != self.type_name {
            return Err(format!(
                "AoSoA type mismatch: expected '{}', got '{}'",
                self.type_name, writ_struct.type_name
            ));
        }

        // Find or create a chunk with space.
        if self.chunks.is_empty()
            || self
                .chunks
                .last()
                .is_some_and(|c| c.is_full(self.chunk_size))
        {
            self.chunks
                .push(AoSoAChunk::new(self.field_names.len(), self.chunk_size));
        }
        let chunk = self.chunks.last_mut().unwrap();

        // Decompose struct into columns.
        for (i, field_name) in self.field_names.iter().enumerate() {
            let field_val = writ_struct
                .fields
                .get(field_name)
                .cloned()
                .unwrap_or(Value::Null);
            chunk.columns[i].push(field_val);
        }
        chunk.len += 1;
        self.total_len += 1;
        Ok(())
    }

    /// Gets the element at the given index, reconstructing the struct.
    pub fn get(&self, index: usize) -> Option<WritStruct> {
        if index >= self.total_len {
            return None;
        }
        let chunk_idx = index / self.chunk_size;
        let inner_idx = index % self.chunk_size;
        let chunk = &self.chunks[chunk_idx];

        let mut fields = HashMap::new();
        for (i, name) in self.field_names.iter().enumerate() {
            fields.insert(name.clone(), chunk.columns[i][inner_idx].clone());
        }

        Some(WritStruct {
            type_name: self.type_name.clone(),
            fields,
            field_order: self.field_names.clone(),
            public_fields: self.public_fields.clone(),
            public_methods: self.public_methods.clone(),
        })
    }

    /// Sets the element at the given index by decomposing the struct into columns.
    pub fn set(&mut self, index: usize, writ_struct: &WritStruct) -> Result<(), String> {
        if index >= self.total_len {
            return Err(format!(
                "AoSoA index {} out of bounds (length {})",
                index, self.total_len
            ));
        }
        if writ_struct.type_name != self.type_name {
            return Err(format!(
                "AoSoA type mismatch: expected '{}', got '{}'",
                self.type_name, writ_struct.type_name
            ));
        }
        let chunk_idx = index / self.chunk_size;
        let inner_idx = index % self.chunk_size;
        let chunk = &mut self.chunks[chunk_idx];

        for (i, field_name) in self.field_names.iter().enumerate() {
            chunk.columns[i][inner_idx] = writ_struct
                .fields
                .get(field_name)
                .cloned()
                .unwrap_or(Value::Null);
        }
        Ok(())
    }

    /// Returns the total number of elements.
    pub fn len(&self) -> usize {
        self.total_len
    }

    /// Returns `true` if the container has no elements.
    pub fn is_empty(&self) -> bool {
        self.total_len == 0
    }

    /// Iterates a single field's values across all elements.
    ///
    /// This is the key AoSoA benefit: field data is contiguous within each
    /// chunk, providing cache-friendly iteration.
    pub fn iter_field(&self, field_name: &str) -> Option<impl Iterator<Item = &Value>> {
        let col_idx = self.field_names.iter().position(|n| n == field_name)?;
        Some(
            self.chunks
                .iter()
                .flat_map(move |chunk| chunk.columns[col_idx].iter()),
        )
    }
}

impl fmt::Display for AoSoAContainer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@packed [")?;
        for i in 0..self.total_len {
            if i > 0 {
                write!(f, ", ")?;
            }
            if let Some(s) = self.get(i) {
                write!(f, "{}(", s.type_name)?;
                for (j, field_name) in s.field_order.iter().enumerate() {
                    if j > 0 {
                        write!(f, ", ")?;
                    }
                    if let Some(val) = s.fields.get(field_name) {
                        write!(f, "{field_name}: {val}")?;
                    }
                }
                write!(f, ")")?;
            }
        }
        write!(f, "]")
    }
}

impl PartialEq for AoSoAContainer {
    fn eq(&self, other: &Self) -> bool {
        if self.type_name != other.type_name || self.total_len != other.total_len {
            return false;
        }
        // Compare element-by-element.
        for i in 0..self.total_len {
            if self.get(i) != other.get(i) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_point_struct(x: i32, y: i32) -> WritStruct {
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Value::I32(x));
        fields.insert("y".to_string(), Value::I32(y));
        let mut public_fields = HashSet::new();
        public_fields.insert("x".to_string());
        public_fields.insert("y".to_string());
        WritStruct {
            type_name: "Point".to_string(),
            fields,
            field_order: vec!["x".to_string(), "y".to_string()],
            public_fields,
            public_methods: HashSet::new(),
        }
    }

    fn make_container(capacity: usize) -> AoSoAContainer {
        let mut public_fields = HashSet::new();
        public_fields.insert("x".to_string());
        public_fields.insert("y".to_string());
        AoSoAContainer::new(
            "Point".to_string(),
            vec!["x".to_string(), "y".to_string()],
            public_fields,
            HashSet::new(),
            capacity,
        )
    }

    #[test]
    fn push_and_get() {
        let mut c = make_container(4);
        c.push(&make_point_struct(1, 2)).unwrap();
        c.push(&make_point_struct(3, 4)).unwrap();
        assert_eq!(c.len(), 2);

        let p0 = c.get(0).unwrap();
        assert_eq!(p0.get_field("x"), Some(&Value::I32(1)));
        assert_eq!(p0.get_field("y"), Some(&Value::I32(2)));

        let p1 = c.get(1).unwrap();
        assert_eq!(p1.get_field("x"), Some(&Value::I32(3)));
        assert_eq!(p1.get_field("y"), Some(&Value::I32(4)));
    }

    #[test]
    fn set_element() {
        let mut c = make_container(4);
        c.push(&make_point_struct(1, 2)).unwrap();
        c.set(0, &make_point_struct(10, 20)).unwrap();

        let p = c.get(0).unwrap();
        assert_eq!(p.get_field("x"), Some(&Value::I32(10)));
        assert_eq!(p.get_field("y"), Some(&Value::I32(20)));
    }

    #[test]
    fn type_mismatch_rejected() {
        let mut c = make_container(4);
        let mut wrong = make_point_struct(1, 2);
        wrong.type_name = "Enemy".to_string();
        assert!(c.push(&wrong).is_err());
    }

    #[test]
    fn iter_field_contiguous() {
        let mut c = make_container(4);
        c.push(&make_point_struct(10, 20)).unwrap();
        c.push(&make_point_struct(30, 40)).unwrap();
        c.push(&make_point_struct(50, 60)).unwrap();

        let xs: Vec<_> = c.iter_field("x").unwrap().cloned().collect();
        assert_eq!(xs, vec![Value::I32(10), Value::I32(30), Value::I32(50),]);

        let ys: Vec<_> = c.iter_field("y").unwrap().cloned().collect();
        assert_eq!(ys, vec![Value::I32(20), Value::I32(40), Value::I32(60),]);

        assert!(c.iter_field("z").is_none());
    }

    #[test]
    fn chunk_boundary() {
        let mut c = make_container(DEFAULT_CHUNK_SIZE + 10);
        for i in 0..(DEFAULT_CHUNK_SIZE + 10) {
            c.push(&make_point_struct(i as i32, (i * 2) as i32))
                .unwrap();
        }
        assert_eq!(c.len(), DEFAULT_CHUNK_SIZE + 10);

        // Verify elements across chunk boundary.
        let last = c.get(DEFAULT_CHUNK_SIZE + 9).unwrap();
        assert_eq!(
            last.get_field("x"),
            Some(&Value::I32((DEFAULT_CHUNK_SIZE + 9) as i32))
        );

        // Verify iter_field spans both chunks.
        let xs: Vec<_> = c.iter_field("x").unwrap().cloned().collect();
        assert_eq!(xs.len(), DEFAULT_CHUNK_SIZE + 10);
    }

    #[test]
    fn empty_container() {
        let c = make_container(0);
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.get(0).is_none());
    }

    #[test]
    fn equality() {
        let mut a = make_container(4);
        let mut b = make_container(4);
        a.push(&make_point_struct(1, 2)).unwrap();
        b.push(&make_point_struct(1, 2)).unwrap();
        assert_eq!(a, b);

        b.push(&make_point_struct(3, 4)).unwrap();
        assert_ne!(a, b);
    }
}
