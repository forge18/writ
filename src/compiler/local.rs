/// Information about a local variable tracked during compilation.
#[derive(Debug, Clone)]
pub struct Local {
    /// The variable name as declared in source.
    pub name: String,
    /// The slot index assigned to this local in the call frame.
    pub slot: u8,
    /// The scope depth at which this local was declared.
    pub depth: u32,
    /// Set to `true` when an inner function captures this local as an upvalue.
    /// On scope exit, captured locals emit `CloseUpvalue` instead of `Pop`.
    pub is_captured: bool,
    /// Compile-time type tag for typed instruction emission.
    /// 0 = Other/unknown, 1 = Int, 2 = Float, 3 = Bool.
    pub type_tag: u8,
}
