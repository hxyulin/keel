//! Explicit memory storage properties, independent of the host and call ABI.

/// A validated pointer storage model. This does not authorize any value type,
/// instruction, target, or calling convention. Both models use little endian
/// storage and retain the existing non-pointer scalar layout rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataLayout {
    pointer_bytes: u64,
    pointer_align: u64,
}

impl DataLayout {
    /// The existing native targets' eight-byte pointer storage and alignment.
    pub const NATIVE64: Self = Self {
        pointer_bytes: 8,
        pointer_align: 8,
    };

    /// Four-byte pointer storage and alignment for the bounded wasm32 profile.
    pub const WASM32: Self = Self {
        pointer_bytes: 4,
        pointer_align: 4,
    };

    pub const fn pointer_bytes(self) -> u64 {
        self.pointer_bytes
    }

    pub const fn pointer_align(self) -> u64 {
        self.pointer_align
    }
}
