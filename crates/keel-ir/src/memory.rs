//! Explicit contracts on non-atomic memory accesses.

use std::fmt;

/// Access properties, preserved independently of instruction scheduling.
/// Defaults make no alignment or fault-freedom promise. Volatile accesses
/// must execute as written; they are not atomic and are not thread fences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MemoryFlags {
    pub volatile: bool,
    /// Minimum byte alignment of the address; a nonzero power of two.
    pub align: u32,
    /// The producer guarantees this particular access cannot fault. This
    /// does not establish alias freedom, immutable memory or safe hoisting.
    pub notrap: bool,
}

impl Default for MemoryFlags {
    fn default() -> Self {
        Self {
            volatile: false,
            align: 1,
            notrap: false,
        }
    }
}

impl fmt::Display for MemoryFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.volatile {
            write!(f, "volatile ")?;
        }
        if self.align != 1 {
            write!(f, "align {} ", self.align)?;
        }
        if self.notrap {
            write!(f, "notrap ")?;
        }
        Ok(())
    }
}

/// Conservative memory interaction. A barrier invalidates all memory facts,
/// including object lifetime: calls, atomics, memory-clobber asm and stack restore
/// cannot be crossed merely because an address is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryEffects {
    None,
    Read,
    Write,
    ReadWrite,
    Barrier,
}

impl MemoryEffects {
    #[inline]
    pub fn may_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite | Self::Barrier)
    }
    #[inline]
    pub fn may_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite | Self::Barrier)
    }
}
