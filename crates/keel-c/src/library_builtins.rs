//! Frontend provenance for supported standard-library memory and math operations.
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
    /// Square root of a `double`.
    Sqrt,
    /// Square root of a `float`.
    Sqrtf,
    /// Square root of a `long double`, where it has the `double` format.
    Sqrtl,
}

impl LibraryBuiltin {
    /// Every operation, in the order of their `-fno-builtin-` names.
    pub const ALL: [Self; 6] = [
        Self::Memcpy,
        Self::Memmove,
        Self::Memset,
        Self::Sqrt,
        Self::Sqrtf,
        Self::Sqrtl,
    ];

    /// The library function's name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Memcpy => "memcpy",
            Self::Memmove => "memmove",
            Self::Memset => "memset",
            Self::Sqrt => "sqrt",
            Self::Sqrtf => "sqrtf",
            Self::Sqrtl => "sqrtl",
        }
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.name() == name)
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }

    /// The memory intrinsic, for the memory operations.
    pub(crate) const fn intrinsic(self) -> Option<Intrinsic> {
        match self {
            Self::Memcpy => Some(Intrinsic::Memcpy),
            Self::Memmove => Some(Intrinsic::Memmove),
            Self::Memset => Some(Intrinsic::Memset),
            Self::Sqrt | Self::Sqrtf | Self::Sqrtl => None,
        }
    }

    pub(crate) fn prototype(self, ty: &Ty<'_>) -> bool {
        let Ty::Func(f) = ty else {
            return false;
        };
        if self.intrinsic().is_none() {
            // `long double` has the `double` format exactly where it is `Ty::Double`.
            let float = if self == Self::Sqrtf {
                Ty::Float
            } else {
                Ty::Double
            };
            return f.has_prototype()
                && !f.variadic
                && !f.microsoft_x64
                && f.ret == float
                && f.params == [float]
                && f.param_quals == [&[] as &[u8]];
        }
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
    /// Recognize supported canonical library declarations in hosted C.
    pub const fn hosted() -> Self {
        Self { enabled: 0x3f }
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
