//! Intrinsics: operations with known meaning that behave like calls, written
//! `intrinsic memcpy(v0, v1, v2)`.
//!
//! Pure arithmetic belongs in the instruction set, where the optimizer sees
//! it directly. Intrinsics are for call-like operations (memory functions)
//! and target-specific ones. Everything a pass needs to know about one lives
//! in its [`IntrinsicInfo`], so adding an intrinsic touches only this file
//! and the backends.

use crate::Type;
use std::fmt;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Intrinsic {
    /// `memcpy(dst, src, len)`: the regions must not overlap.
    Memcpy,
    /// `memmove(dst, src, len)`: the regions may overlap.
    Memmove,
    /// `memset(dst, byte, len)`.
    Memset,
    /// Stops in the debugger, which can resume past it. Without a debugger
    /// the process gets SIGTRAP.
    Debugtrap,
}

pub struct IntrinsicInfo {
    pub name: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
    /// Pure intrinsics float like pure instructions; the rest are pinned.
    pub pure: bool,
}

impl Intrinsic {
    pub const ALL: [Intrinsic; 4] = [
        Intrinsic::Memcpy,
        Intrinsic::Memmove,
        Intrinsic::Memset,
        Intrinsic::Debugtrap,
    ];

    pub fn info(self) -> IntrinsicInfo {
        const MEM: &[Type] = &[Type::Ptr, Type::Ptr, Type::I64];
        let (name, params, returns, pure): (_, &'static [Type], &'static [Type], _) = match self {
            Intrinsic::Memcpy => ("memcpy", MEM, &[], false),
            Intrinsic::Memmove => ("memmove", MEM, &[], false),
            Intrinsic::Memset => ("memset", &[Type::Ptr, Type::I8, Type::I64], &[], false),
            Intrinsic::Debugtrap => ("debugtrap", &[], &[], false),
        };
        IntrinsicInfo {
            name,
            params,
            returns,
            pure,
        }
    }

    pub fn from_name(name: &str) -> Option<Intrinsic> {
        Intrinsic::ALL.into_iter().find(|i| i.info().name == name)
    }
}

impl fmt::Display for Intrinsic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.info().name)
    }
}
