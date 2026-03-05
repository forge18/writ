/// Describes where an upvalue comes from during compilation.
///
/// Each compiled function stores a list of these descriptors. At runtime,
/// the VM uses them to build the closure's upvalue array when executing
/// a `MakeClosure` instruction.
#[derive(Debug, Clone)]
pub struct UpvalueDescriptor {
    /// If `true`, this upvalue captures a local variable from the
    /// immediately enclosing function (identified by `index` as the
    /// local's slot number).
    ///
    /// If `false`, this upvalue captures an upvalue from the enclosing
    /// function's own upvalue array (identified by `index` as the
    /// parent's upvalue index). This enables transitive capture through
    /// multiple nesting levels.
    pub is_local: bool,
    /// Local slot index (if `is_local`) or parent upvalue index (if not).
    pub index: u8,
}
