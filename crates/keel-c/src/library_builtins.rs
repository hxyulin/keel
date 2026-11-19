//! Frontend provenance for supported standard-library memory and math operations.
use crate::types::{IntKind, Ty, ULONG};
use keel_ir::Intrinsic;

/// A standard-library operation that ordinary direct calls may use as a builtin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryBuiltin {
    /// Absolute value of an `int`.
    Abs,
    /// Absolute value of a `long`.
    Labs,
    /// Absolute value of a `long long`.
    Llabs,
    /// Absolute value of a `double`.
    Fabs,
    /// Absolute value of a `float`.
    Fabsf,
    /// Absolute value of a `long double` with the `double` format.
    Fabsl,
    /// Copy the sign of a `double`.
    Copysign,
    /// Copy the sign of a `float`.
    Copysignf,
    /// Copy the sign of a `long double` with the `double` format.
    Copysignl,
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
    /// Compare the object representations of two memory regions.
    Memcmp,
}

impl LibraryBuiltin {
    /// Every operation, in the order of their `-fno-builtin-` names.
    pub const ALL: [Self; 16] = [
        Self::Abs,
        Self::Labs,
        Self::Llabs,
        Self::Fabs,
        Self::Fabsf,
        Self::Fabsl,
        Self::Copysign,
        Self::Copysignf,
        Self::Copysignl,
        Self::Memcpy,
        Self::Memmove,
        Self::Memset,
        Self::Sqrt,
        Self::Sqrtf,
        Self::Sqrtl,
        Self::Memcmp,
    ];

    /// The library function's name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Abs => "abs",
            Self::Labs => "labs",
            Self::Llabs => "llabs",
            Self::Fabs => "fabs",
            Self::Fabsf => "fabsf",
            Self::Fabsl => "fabsl",
            Self::Copysign => "copysign",
            Self::Copysignf => "copysignf",
            Self::Copysignl => "copysignl",
            Self::Memcpy => "memcpy",
            Self::Memmove => "memmove",
            Self::Memset => "memset",
            Self::Sqrt => "sqrt",
            Self::Sqrtf => "sqrtf",
            Self::Sqrtl => "sqrtl",
            Self::Memcmp => "memcmp",
        }
    }

    pub(crate) fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.name() == name)
    }

    const fn bit(self) -> u32 {
        1 << self as u8
    }

    /// The memory intrinsic, for the memory operations.
    pub(crate) const fn intrinsic(self) -> Option<Intrinsic> {
        match self {
            Self::Memcpy => Some(Intrinsic::Memcpy),
            Self::Memmove => Some(Intrinsic::Memmove),
            Self::Memset => Some(Intrinsic::Memset),
            Self::Memcmp => Some(Intrinsic::Memcmp),
            _ => None,
        }
    }

    pub(crate) fn prototype(self, ty: &Ty<'_>) -> bool {
        let Ty::Func(f) = ty else {
            return false;
        };
        if self.intrinsic().is_none() {
            // `long double` has the `double` format exactly where it is `Ty::Double`.
            let scalar = match self {
                Self::Abs => Ty::Int(IntKind::Int),
                Self::Labs => Ty::Int(IntKind::Long),
                Self::Llabs => Ty::Int(IntKind::LLong),
                Self::Fabsf | Self::Sqrtf | Self::Copysignf => Ty::Float,
                _ => Ty::Double,
            };
            let arity = if matches!(self, Self::Copysign | Self::Copysignf | Self::Copysignl) {
                2
            } else {
                1
            };
            return f.has_prototype()
                && !f.variadic
                && !f.microsoft_x64
                && f.ret == scalar
                && f.params.len() == arity
                && f.params.iter().all(|ty| *ty == scalar)
                && f.param_quals.len() == arity
                && f.param_quals.iter().all(|q| q.is_empty());
        }
        const COPY_QUALIFIERS: &[&[u8]] = &[&[0], &[1], &[]];
        const FILL_QUALIFIERS: &[&[u8]] = &[&[0], &[], &[]];
        let void_ptr = |ty: &Ty<'_>| matches!(ty, Ty::Ptr(p) if **p == Ty::Void);
        if self == Self::Memcmp {
            const COMPARE_QUALIFIERS: &[&[u8]] = &[&[1], &[1], &[]];
            return f.has_prototype()
                && !f.variadic
                && !f.microsoft_x64
                && f.ret == Ty::Int(IntKind::Int)
                && f.params.len() == 3
                && void_ptr(&f.params[0])
                && void_ptr(&f.params[1])
                && f.params[2] == ULONG
                && f.param_quals == COMPARE_QUALIFIERS;
        }
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
    enabled: u32,
    math_errno: bool,
}

impl Default for LibraryBuiltins {
    fn default() -> Self {
        Self::hosted()
    }
}

impl LibraryBuiltins {
    const OPERATION_COUNT: usize = LibraryBuiltin::ALL.len();
    /// Recognize supported canonical library declarations in hosted C.
    pub const fn hosted() -> Self {
        Self {
            enabled: (1 << Self::OPERATION_COUNT) - 1,
            math_errno: true,
        }
    }

    /// Preserve ordinary calls, as required by the freestanding default.
    pub const fn disabled() -> Self {
        Self {
            enabled: 0,
            math_errno: true,
        }
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

    /// Permit recognized square roots to omit the library's errno side effect.
    /// This leaves disabled operations as ordinary calls and does not grant
    /// floating-point reassociation or approximation permissions.
    #[must_use]
    pub const fn without_math_errno(mut self) -> Self {
        self.math_errno = false;
        self
    }

    pub(crate) const fn preserves_math_errno(self) -> bool {
        self.math_errno
    }

    pub(crate) const fn allows(self, operation: LibraryBuiltin) -> bool {
        self.enabled & operation.bit() != 0
    }
}
