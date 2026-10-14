//! Module-level data: named, initialized memory the linker places in the
//! executable, written
//!
//! ```text
//! data @msg   = const align 1 { b"Hi!\n\0" }
//! data @table = const align 8 { ptr @add, ptr @msg + 3, i32 7, zero 4 }
//! decl data @environ
//! data @count = thread_local align 8 { i64 0 }
//! decl data thread_local @errno_value
//! ```
//!
//! A `ptr @sym` item is a relocation: its value is an address only the
//! linker (or the loader) knows, so it is kept as a symbol name, not bytes.

use crate::Type;
use std::fmt;

/// A named data object: defined here (`init` is `Some`) or elsewhere.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataObject {
    pub name: String,
    /// Each thread has its own copy, initialized from `init`;
    /// `symbol_addr` gives the current thread's.
    pub thread_local: bool,
    pub init: Option<DataInit>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataInit {
    /// Constant data may be placed in read-only memory; storing to it is
    /// not allowed.
    pub constant: bool,
    /// A power of two.
    pub align: u32,
    pub items: Vec<DataItem>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DataItem {
    Bytes(Vec<u8>),
    /// An integer of type `ty`, stored little-endian. `value` must fit in
    /// `ty` as either a signed or an unsigned number.
    Int {
        ty: Type,
        value: i64,
    },
    /// `n` zero bytes.
    Zero(u32),
    /// The address of `symbol` plus `addend` bytes: 8 bytes, filled in by
    /// the linker.
    Ptr {
        symbol: String,
        addend: u32,
    },
}

/// Pointer size on every target keel supports.
pub const PTR_BYTES: u64 = 8;

impl DataItem {
    pub fn size(&self) -> u64 {
        match self {
            DataItem::Bytes(bytes) => bytes.len() as u64,
            DataItem::Int { ty, .. } => ty.bytes().map_or(PTR_BYTES, u64::from),
            DataItem::Zero(n) => u64::from(*n),
            DataItem::Ptr { .. } => PTR_BYTES,
        }
    }
}

impl DataInit {
    pub fn size(&self) -> u64 {
        self.items.iter().map(DataItem::size).sum()
    }

    /// Whether any item needs the linker to fill in an address.
    pub fn has_relocations(&self) -> bool {
        self.items
            .iter()
            .any(|item| matches!(item, DataItem::Ptr { .. }))
    }

    /// Whether every byte is zero, so the object needs no space in the file.
    pub fn is_zero(&self) -> bool {
        self.items.iter().all(|item| match item {
            DataItem::Bytes(bytes) => bytes.iter().all(|&b| b == 0),
            DataItem::Int { value, .. } => *value == 0,
            DataItem::Zero(_) => true,
            DataItem::Ptr { .. } => false,
        })
    }
}

impl fmt::Display for DataObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let thread_local = if self.thread_local {
            "thread_local "
        } else {
            ""
        };
        let Some(init) = &self.init else {
            return write!(f, "decl data {thread_local}@{}", self.name);
        };
        write!(f, "data @{} = {thread_local}", self.name)?;
        if init.constant {
            f.write_str("const ")?;
        }
        write!(f, "align {} {{", init.align)?;
        for (i, item) in init.items.iter().enumerate() {
            f.write_str(if i == 0 { " " } else { ", " })?;
            write!(f, "{item}")?;
        }
        f.write_str(" }")
    }
}

/// Writes `bytes` as a quoted string with the escapes the parser reads.
pub(crate) fn write_quoted(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    f.write_str("\"")?;
    for &b in bytes {
        match b {
            b'\n' => f.write_str("\\n")?,
            b'\t' => f.write_str("\\t")?,
            b'\r' => f.write_str("\\r")?,
            0 => f.write_str("\\0")?,
            b'\\' => f.write_str("\\\\")?,
            b'"' => f.write_str("\\\"")?,
            b' '..=b'~' => write!(f, "{}", b as char)?,
            _ => write!(f, "\\x{b:02x}")?,
        }
    }
    f.write_str("\"")
}

impl fmt::Display for DataItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataItem::Bytes(bytes) => {
                f.write_str("b")?;
                write_quoted(f, bytes)
            }
            DataItem::Int { ty, value } => write!(f, "{ty} {value}"),
            DataItem::Zero(n) => write!(f, "zero {n}"),
            DataItem::Ptr { symbol, addend: 0 } => write!(f, "ptr @{symbol}"),
            DataItem::Ptr { symbol, addend } => write!(f, "ptr @{symbol} + {addend}"),
        }
    }
}
