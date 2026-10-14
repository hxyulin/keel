//! Source-level descriptions for debuggers: the types of a source
//! language, its variables and where they live. Code generation turns
//! them into DWARF. Written
//!
//! ```text
//! dtype dt0 = base "int" signed 4
//! dtype dt1 = pointer dt0
//! dtype dt2 = struct "point" 8 { "x" dt0 0, "y" dt0 4 }
//! dtype dt3 = array dt2 10
//! dtype dt4 = typedef "point_t" dt2
//! dglobal "origin" dt2 @origin
//!
//! func @f(i32) -> i32 cc(c) {
//!     ss0 = stack_slot 4, align 4
//!     dreturn dt0
//!     dparam "n" dt0 ss0
//! ```
//!
//! A variable lives in a stack slot for the whole function, so its
//! location is a fixed offset from the frame. That is what a C compiler
//! does without optimization: every variable can be read and changed from
//! the debugger anywhere. Values in registers have no description.

use crate::{EntityRef, StackSlot};
use std::fmt;

crate::entity!(
    /// A source-level type in the module's table.
    DebugType,
    "dt"
);

/// How a debugger reads a base type's bits.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Encoding {
    Signed,
    Unsigned,
    SignedChar,
    UnsignedChar,
    Bool,
    Float,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DebugTypeData {
    /// `base "unsigned int" unsigned 4`
    Base {
        name: String,
        encoding: Encoding,
        size: u32,
    },
    /// `pointer dt0`, or `pointer void`.
    Pointer(Option<DebugType>),
    /// `struct "point" 8 { "x" dt0 0, .. }`, also `union`; no name for an
    /// anonymous one; `struct "s" declared` for one never completed.
    Struct {
        name: Option<String>,
        union: bool,
        /// `None` for a declaration without a body.
        size: Option<u64>,
        members: Vec<DebugMember>,
    },
    /// `array dt0 10`, or `array dt0` with no length.
    Array { elem: DebugType, count: Option<u64> },
    /// `typedef "size_t" dt1`, or `typedef "v" void`.
    Typedef { name: String, ty: Option<DebugType> },
    /// `function`: a function type, which only appears behind a pointer.
    Function,
}

/// A struct member: `"x" dt0 4`, a byte offset; `"a" dt0 bits 35 5`, a
/// bit-field 5 bits wide starting 35 bits into the struct; `_ dt3 8`, an
/// anonymous struct or union whose members are found as if they were this
/// struct's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DebugMember {
    pub name: Option<String>,
    pub ty: DebugType,
    pub location: MemberLocation,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MemberLocation {
    Byte(u64),
    Bits { offset: u64, size: u32 },
}

/// A variable of a function, in one of its stack slots. Parameters
/// (`dparam`) are in declaration order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DebugVar {
    pub name: String,
    pub ty: DebugType,
    pub slot: StackSlot,
    pub param: bool,
}

/// A variable with static storage: `dglobal "count" dt0 @count`, at a data
/// object's address.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DebugGlobal {
    pub name: String,
    pub ty: DebugType,
    pub symbol: String,
}

impl DebugTypeData {
    /// Every type this one refers to.
    pub fn refs(&self) -> Vec<DebugType> {
        match self {
            DebugTypeData::Base { .. } | DebugTypeData::Function => Vec::new(),
            DebugTypeData::Pointer(ty) | DebugTypeData::Typedef { ty, .. } => {
                ty.iter().copied().collect()
            }
            DebugTypeData::Struct { members, .. } => members.iter().map(|m| m.ty).collect(),
            DebugTypeData::Array { elem, .. } => vec![*elem],
        }
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Encoding::Signed => "signed",
            Encoding::Unsigned => "unsigned",
            Encoding::SignedChar => "signed_char",
            Encoding::UnsignedChar => "unsigned_char",
            Encoding::Bool => "bool",
            Encoding::Float => "float",
        })
    }
}

impl Encoding {
    pub fn from_name(name: &str) -> Option<Encoding> {
        Some(match name {
            "signed" => Encoding::Signed,
            "unsigned" => Encoding::Unsigned,
            "signed_char" => Encoding::SignedChar,
            "unsigned_char" => Encoding::UnsignedChar,
            "bool" => Encoding::Bool,
            "float" => Encoding::Float,
            _ => return None,
        })
    }
}

fn quoted(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    crate::data::write_quoted(f, s.as_bytes())
}

fn opt_type(f: &mut fmt::Formatter<'_>, ty: Option<DebugType>) -> fmt::Result {
    match ty {
        Some(ty) => write!(f, "{ty}"),
        None => f.write_str("void"),
    }
}

/// `dtype dt3 = ...`, for the module's type at `index`.
pub struct DebugTypeLine<'a>(pub usize, pub &'a DebugTypeData);

impl fmt::Display for DebugTypeLine<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "dtype {} = ", DebugType::new(self.0))?;
        match self.1 {
            DebugTypeData::Base {
                name,
                encoding,
                size,
            } => {
                f.write_str("base ")?;
                quoted(f, name)?;
                write!(f, " {encoding} {size}")
            }
            DebugTypeData::Pointer(ty) => {
                f.write_str("pointer ")?;
                opt_type(f, *ty)
            }
            DebugTypeData::Struct {
                name,
                union,
                size,
                members,
            } => {
                f.write_str(if *union { "union" } else { "struct" })?;
                if let Some(name) = name {
                    f.write_str(" ")?;
                    quoted(f, name)?;
                }
                let Some(size) = size else {
                    return f.write_str(" declared");
                };
                write!(f, " {size} {{")?;
                for (i, m) in members.iter().enumerate() {
                    f.write_str(if i == 0 { " " } else { ", " })?;
                    match &m.name {
                        Some(name) => {
                            quoted(f, name)?;
                            f.write_str(" ")?;
                        }
                        None => f.write_str("_ ")?,
                    }
                    match m.location {
                        MemberLocation::Byte(offset) => write!(f, "{} {offset}", m.ty)?,
                        MemberLocation::Bits { offset, size } => {
                            write!(f, "{} bits {offset} {size}", m.ty)?
                        }
                    }
                }
                f.write_str(" }")
            }
            DebugTypeData::Array { elem, count } => {
                write!(f, "array {elem}")?;
                match count {
                    Some(n) => write!(f, " {n}"),
                    None => Ok(()),
                }
            }
            DebugTypeData::Typedef { name, ty } => {
                f.write_str("typedef ")?;
                quoted(f, name)?;
                f.write_str(" ")?;
                opt_type(f, *ty)
            }
            DebugTypeData::Function => f.write_str("function"),
        }
    }
}

impl fmt::Display for DebugGlobal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("dglobal ")?;
        quoted(f, &self.name)?;
        write!(f, " {} @{}", self.ty, self.symbol)
    }
}

impl fmt::Display for DebugVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kw = if self.param { "dparam" } else { "dvar" };
        write!(f, "{kw} ")?;
        quoted(f, &self.name)?;
        write!(f, " {} {}", self.ty, self.slot)
    }
}
