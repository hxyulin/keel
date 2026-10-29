//! Frontend provenance for supported standard-library memory operations.
use crate::types::{IntKind, Ty, ULONG};
use keel_ir::Intrinsic;

/// A standard-library operation that ordinary direct calls may use as a builtin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryBuiltin {
    /// Copy bytes between nonoverlapping regions.
    Memcpy,
    /// Copy bytes while allowing overlap.
    Memmove,
    /// Fill bytes with a converted unsigned character.
    Memset,
}

impl LibraryBuiltin {
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "memcpy" => Some(Self::Memcpy),
            "memmove" => Some(Self::Memmove),
            "memset" => Some(Self::Memset),
            _ => None,
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Memcpy => 1,
            Self::Memmove => 2,
            Self::Memset => 4,
        }
    }

    pub(crate) const fn intrinsic(self) -> Intrinsic {
        match self {
            Self::Memcpy => Intrinsic::Memcpy,
            Self::Memmove => Intrinsic::Memmove,
            Self::Memset => Intrinsic::Memset,
        }
    }

    pub(crate) fn prototype(self, ty: &Ty<'_>) -> bool {
        let Ty::Func(f) = ty else {
            return false;
        };
        const COPY_QUALIFIERS: &[&[u8]] = &[&[0], &[1], &[]];
        const FILL_QUALIFIERS: &[&[u8]] = &[&[0], &[], &[]];
        let void_ptr = |ty: &Ty<'_>| matches!(ty, Ty::Ptr(p) if **p == Ty::Void);
        f.has_prototype()
            && !f.variadic
            && !f.microsoft_x64
            && void_ptr(&f.ret)
            && f.params.len() == 3
            && void_ptr(&f.params[0])
            && if self == Self::Memset {
                f.params[1] == Ty::Int(IntKind::Int)
            } else {
                void_ptr(&f.params[1])
            }
            && f.params[2] == ULONG
            && f.param_quals
                == if self == Self::Memset {
                    FILL_QUALIFIERS
                } else {
                    COPY_QUALIFIERS
                }
    }
}

/// Recognition policy for ordinary C library calls. Explicit `__builtin_` calls
/// retain their semantics regardless of this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LibraryBuiltins {
    enabled: u8,
}

impl Default for LibraryBuiltins {
    fn default() -> Self {
        Self::hosted()
    }
}

impl LibraryBuiltins {
    /// Recognize supported canonical memory-library declarations in hosted C.
    pub const fn hosted() -> Self {
        Self { enabled: 7 }
    }

    /// Preserve ordinary calls, as required by the freestanding default.
    pub const fn disabled() -> Self {
        Self { enabled: 0 }
    }

    /// Enable one operation without changing the other operation policies.
    #[must_use]
    pub const fn enable(mut self, operation: LibraryBuiltin) -> Self {
        self.enabled |= operation.bit();
        self
    }

    /// Disable one operation without changing explicit builtin calls.
    #[must_use]
    pub const fn disable(mut self, operation: LibraryBuiltin) -> Self {
        self.enabled &= !operation.bit();
        self
    }

    pub(crate) const fn allows(self, operation: LibraryBuiltin) -> bool {
        self.enabled & operation.bit() != 0
    }
}
