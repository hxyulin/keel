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
    /// `va_start(list)`: initializes the `va_list` at `list` to the first
    /// variadic argument of the current (variadic) function. A `va_list` is
    /// target-defined memory: 8 bytes, align 8, on Apple arm64. Read it with
    /// `va_arg`; copying its bytes copies the position.
    VaStart,
    /// `stack_alloc(size) -> ptr`: `size` bytes of stack, 16-byte aligned,
    /// live until the function returns or a `stack_restore` to a point
    /// before it. For `alloca` and variable-length arrays.
    StackAlloc,
    /// `stack_save() -> ptr`: marks the current extent of the stack, for
    /// `stack_restore`.
    StackSave,
    /// `stack_restore(mark)`: frees every `stack_alloc` since the
    /// `stack_save` that returned `mark`, as a VLA's scope ends.
    StackRestore,
}

pub struct IntrinsicInfo {
    pub name: &'static str,
    pub params: &'static [Type],
    pub returns: &'static [Type],
    /// Pure intrinsics float like pure instructions; the rest are pinned.
    pub pure: bool,
}

impl Intrinsic {
    pub const ALL: [Intrinsic; 8] = [
        Intrinsic::Memcpy,
        Intrinsic::Memmove,
        Intrinsic::Memset,
        Intrinsic::Debugtrap,
        Intrinsic::VaStart,
        Intrinsic::StackAlloc,
        Intrinsic::StackSave,
        Intrinsic::StackRestore,
    ];

    pub fn info(self) -> IntrinsicInfo {
        const MEM: &[Type] = &[Type::Ptr, Type::Ptr, Type::I64];
        let (name, params, returns, pure): (_, &'static [Type], &'static [Type], _) = match self {
            Intrinsic::Memcpy => ("memcpy", MEM, &[], false),
            Intrinsic::Memmove => ("memmove", MEM, &[], false),
            Intrinsic::Memset => ("memset", &[Type::Ptr, Type::I8, Type::I64], &[], false),
            Intrinsic::Debugtrap => ("debugtrap", &[], &[], false),
            Intrinsic::VaStart => ("va_start", &[Type::Ptr], &[], false),
            Intrinsic::StackAlloc => ("stack_alloc", &[Type::I64], &[Type::Ptr], false),
            Intrinsic::StackSave => ("stack_save", &[], &[Type::Ptr], false),
            Intrinsic::StackRestore => ("stack_restore", &[Type::Ptr], &[], false),
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
