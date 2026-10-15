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
//! dtype dt5 = const dt4
//! dtype dt6 = enum "color" dt0 { "red" 0, "green" 1 }
//! dglobal "origin" dt5 @origin
//! dlanguage c11
//!
//! func @f(i32) -> i32 cc(c) {
//!     ss0 = stack_slot 4, align 4
//!     ss1 = stack_slot 4, align 4
//!     dreturn dt0
//!     dscope ds0
//!     dparam "n" dt0 ss0
//!     dvar "i" dt0 ss1 in ds0
//! block0(v0: i32):
//!     v1 = stack_addr ss1 !f0:3:10 ds0
//! ```
//!
//! A variable lives in a stack slot for the whole function, so its
//! location is a fixed offset from the frame. That is what a C compiler
//! does without optimization: every variable can be read and changed from
//! the debugger anywhere. Values in registers have no description.
//!
//! **Scopes.** A variable is visible to the debugger where its scope's
//! code is: a `dscope`, nested in the function (`dscope ds0`) or in
//! another (`dscope ds1 in ds0`), is the instructions whose location
//! names it, wherever they end up, and those of the scopes nested in it.
//! A variable without `in` is visible in the whole function.

use crate::{EntityRef, StackSlot};
use std::fmt;

crate::entity!(
    /// A source-level type in the module's table.
    DebugType,
    "dt"
);

crate::entity!(
    /// A lexical scope of a function, such as a C block.
    DebugScope,
    "ds"
);

/// The source language, which tells a debugger how to read expressions
/// and print values: `dlanguage c11`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Language {
    C89,
    C99,
    C11,
}

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
    /// `const dt0`, or `const void`.
    Const(Option<DebugType>),
    /// `volatile dt0`, or `volatile void`.
    Volatile(Option<DebugType>),
    /// `enum "color" dt0 { "red" 0, "green" 1 }`, with no name for an
    /// anonymous one: values of base type `ty` with names for some.
    Enum {
        name: Option<String>,
        ty: DebugType,
        values: Vec<(String, i64)>,
    },
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
    /// The slot holds a pointer to the object, rather than the object.
    pub indirect: bool,
    pub param: bool,
    /// Where it is visible; `None` for the whole function.
    pub scope: Option<DebugScope>,
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
            DebugTypeData::Pointer(ty)
            | DebugTypeData::Typedef { ty, .. }
            | DebugTypeData::Const(ty)
            | DebugTypeData::Volatile(ty) => ty.iter().copied().collect(),
            DebugTypeData::Enum { ty, .. } => vec![*ty],
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
            DebugTypeData::Const(ty) => {
                f.write_str("const ")?;
                opt_type(f, *ty)
            }
            DebugTypeData::Volatile(ty) => {
                f.write_str("volatile ")?;
                opt_type(f, *ty)
            }
            DebugTypeData::Enum { name, ty, values } => {
                f.write_str("enum ")?;
                if let Some(name) = name {
                    quoted(f, name)?;
                    f.write_str(" ")?;
                }
                write!(f, "{ty} {{")?;
                for (i, (name, value)) in values.iter().enumerate() {
                    f.write_str(if i == 0 { " " } else { ", " })?;
                    quoted(f, name)?;
                    write!(f, " {value}")?;
                }
                f.write_str(if values.is_empty() { "}" } else { " }" })
            }
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
        write!(f, " {} {}", self.ty, self.slot)?;
        if self.indirect {
            f.write_str(" indirect")?;
        }
        if let Some(scope) = self.scope {
            write!(f, " in {scope}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Language::C89 => "c89",
            Language::C99 => "c99",
            Language::C11 => "c11",
        })
    }
}

impl Language {
    pub fn from_name(name: &str) -> Option<Language> {
        Some(match name {
            "c89" => Language::C89,
            "c99" => Language::C99,
            "c11" => Language::C11,
            _ => return None,
        })
    }
}
