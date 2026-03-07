use std::collections::HashMap;
use std::rc::Rc;

/// A string interner that deduplicates `Rc<str>` allocations across chunks.
///
/// Identical string literals in different function chunks share a single
/// `Rc<str>`, enabling O(1) pointer-based equality via `Rc::ptr_eq`.
pub struct StringInterner {
    map: HashMap<String, Rc<str>>,
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            map: HashMap::with_capacity(256),
        }
    }

    /// Interns a string slice, returning the canonical `Rc<str>`.
    ///
    /// If the string was previously interned, returns a clone of the
    /// existing `Rc` (refcount bump only). Otherwise allocates a new
    /// `Rc<str>` and stores it.
    #[inline]
    pub fn intern(&mut self, s: &str) -> Rc<str> {
        if let Some(existing) = self.map.get(s) {
            return Rc::clone(existing);
        }
        let rc: Rc<str> = Rc::from(s);
        self.map.insert(s.to_string(), Rc::clone(&rc));
        rc
    }

    /// Interns an already-allocated `Rc<str>`, reusing its allocation if
    /// the string content is new to the interner.
    #[inline]
    pub fn intern_rc(&mut self, rc: &Rc<str>) -> Rc<str> {
        let s: &str = rc.as_ref();
        if let Some(existing) = self.map.get(s) {
            return Rc::clone(existing);
        }
        self.map.insert(s.to_string(), Rc::clone(rc));
        Rc::clone(rc)
    }

    /// Clears all interned strings. Called on VM reset between executions.
    pub fn clear(&mut self) {
        self.map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_deduplicates() {
        let mut interner = StringInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("hello");
        assert!(Rc::ptr_eq(&a, &b));
    }

    #[test]
    fn intern_rc_deduplicates() {
        let mut interner = StringInterner::new();
        let rc1: Rc<str> = Rc::from("world");
        let rc2: Rc<str> = Rc::from("world");
        assert!(!Rc::ptr_eq(&rc1, &rc2));

        let a = interner.intern_rc(&rc1);
        let b = interner.intern_rc(&rc2);
        assert!(Rc::ptr_eq(&a, &b));
    }

    #[test]
    fn intern_and_intern_rc_share() {
        let mut interner = StringInterner::new();
        let a = interner.intern("shared");
        let rc: Rc<str> = Rc::from("shared");
        let b = interner.intern_rc(&rc);
        assert!(Rc::ptr_eq(&a, &b));
    }

    #[test]
    fn clear_resets() {
        let mut interner = StringInterner::new();
        let a = interner.intern("temp");
        interner.clear();
        let b = interner.intern("temp");
        assert!(!Rc::ptr_eq(&a, &b));
    }
}
