//! The registry of machine-level covers: hand-written fused patterns that
//! claim one root instruction and consume adjacent ones. Each cover has a
//! stable name for `--isel` toggles and `--codegen-report` firing counts.
//! [`crate::select::Selection`] records which cover owns each root.
use crate::select::Target;

macro_rules! covers {
    ($($(#[$doc:meta])* $variant:ident = $name:literal, $target:expr;)*) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        #[repr(u8)]
        pub enum Cover {
            $($(#[$doc])* $variant,)*
        }
        impl Cover {
            /// Every cover, in registry order.
            pub const ALL: [Self; [$(stringify!($variant)),*].len()] = [$(Self::$variant),*];
            /// The stable kebab-case name.
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
            /// The one target that runs this cover, or `None` for both.
            pub fn target(self) -> Option<Target> {
                match self {
                    $(Self::$variant => $target,)*
                }
            }
        }
    };
}

const X86: Option<Target> = Some(Target::X86);
const AARCH64: Option<Target> = Some(Target::Aarch64);

covers! {
    // Selection stage. These run only with tree selection.
    /// A single-use `ptradd` of a constant offset, encoded in its memory access.
    OffsetAddress = "offset-address", None;
    /// A single-use `ptradd` of an optionally shifted index, encoded in its
    /// memory access.
    IndexedAddress = "indexed-address", None;
    /// An equality test against zero of an extended compare, which reuses
    /// the original condition.
    ZeroTest = "zero-test", None;
    /// A remainder computed as `x - (x / y) * y` that reads the division's
    /// native remainder.
    X86DivisionRemainder = "x86-division-remainder", X86;
    /// `x * 3 + c` as one LEA.
    X86LeaMul3 = "x86-lea-mul3", X86;
    /// An adjacent load encoded as an integer binary operation's memory RHS.
    X86BinaryMemory = "x86-binary-memory", X86;
    /// An adjacent load encoded as an integer compare's memory RHS.
    X86CompareMemory = "x86-compare-memory", X86;
    /// A shifted low-bit mask as one UBFX.
    Aarch64Ubfx = "aarch64-ubfx", AARCH64;
    /// A multiply by a tuned constant as shift and add.
    Aarch64MultiplyShift = "aarch64-multiply-shift", AARCH64;

    // Backend stage.
    /// A single-use compare re-emitted at its conditional branch.
    CompareBranch = "compare-branch", None;
    /// Packed wrapping absolute value from compare, negate and select.
    PackedAbs = "packed-abs", None;
    /// An i32 `and` with a literal mask tested against zero, as one TEST.
    X86MaskTest = "x86-mask-test", X86;
    /// A single-use scalar compare re-emitted at its select.
    X86SelectCompare = "x86-select-compare", X86;
    /// An i64 load, splat and strictly positive filter, widened.
    X86PackedPositive = "x86-packed-positive", X86;
    /// An i64x2 compare consumed by a clamp or sign select.
    X86PackedSign = "x86-packed-sign", X86;
    /// A load whose only use is a low-half widening, as one 8-byte load.
    X86PackedLowLoad = "x86-packed-low-load", X86;
    /// An m128 load widened to YMM, as one VPMOVSXDQ.
    X86PackedWideLoad = "x86-packed-wide-load", X86;
    /// An i8 load sign-extended to i32, as one MOVSX.
    X86SignedByteLoad = "x86-signed-byte-load", X86;
    /// Scalar absolute value as CMP and CSNEG.
    Aarch64Abs = "aarch64-abs", AARCH64;
    /// A widened absolute difference accumulated as SABAL.
    Aarch64AbsDiff = "aarch64-abs-diff", AARCH64;
    /// A packed compare and select as one min or max.
    Aarch64PackedMinmax = "aarch64-packed-minmax", AARCH64;
    /// A pointer plus an extended, shifted i32 index as one ADD.
    Aarch64ExtendedAdd = "aarch64-extended-add", AARCH64;
    /// A single-use integer compare delayed to its select.
    Aarch64SelectCompare = "aarch64-select-compare", AARCH64;
    /// A load whose only use is a low-half widening, as one 8-byte load.
    Aarch64PackedLowLoad = "aarch64-packed-low-load", AARCH64;
    /// A load splatted to every lane, as one LD1R.
    Aarch64ReplicateLoad = "aarch64-replicate-load", AARCH64;
    /// Zero splats and shifts by a splatted constant, as immediates.
    Aarch64VectorImmediate = "aarch64-vector-immediate", AARCH64;
    /// A low-half widening added to an accumulator, as SADDW or UADDW.
    Aarch64WideningAdd = "aarch64-widening-add", AARCH64;
}

impl Cover {
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// A set of covers.
#[derive(Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CoverSet(u64);

const _: () = assert!(Cover::ALL.len() <= 64);

impl CoverSet {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(u64::MAX >> (64 - Cover::ALL.len()));

    pub const fn contains(self, cover: Cover) -> bool {
        self.0 & (1 << cover as u8) != 0
    }
    pub fn insert(&mut self, cover: Cover) {
        self.0 |= 1 << cover as u8;
    }
    pub fn remove(&mut self, cover: Cover) {
        self.0 &= !(1 << cover as u8);
    }
    /// The covers in `self` and not in `other`.
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
    pub fn iter(self) -> impl Iterator<Item = Cover> {
        Cover::ALL.into_iter().filter(move |&c| self.contains(c))
    }
}

impl std::fmt::Debug for CoverSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.iter().map(Cover::name)).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_kebab_case_and_prefixed_by_target() {
        for (i, cover) in Cover::ALL.into_iter().enumerate() {
            assert_eq!(cover as usize, i);
            let name = cover.name();
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                "{name}"
            );
            assert_eq!(Cover::from_name(name), Some(cover));
            let prefix = match cover.target() {
                Some(Target::X86) => Some("x86-"),
                Some(Target::Aarch64) => Some("aarch64-"),
                None => None,
            };
            assert_eq!(
                prefix,
                ["x86-", "aarch64-"]
                    .into_iter()
                    .find(|p| name.starts_with(p)),
                "{name}"
            );
        }
    }

    #[test]
    fn sets_hold_exactly_their_members() {
        let mut set = CoverSet::NONE;
        set.insert(Cover::X86MaskTest);
        set.insert(Cover::Aarch64WideningAdd);
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [Cover::X86MaskTest, Cover::Aarch64WideningAdd]
        );
        set.remove(Cover::X86MaskTest);
        assert!(!set.contains(Cover::X86MaskTest));
        assert_eq!(CoverSet::ALL.iter().count(), Cover::ALL.len());
        assert_eq!(CoverSet::default(), CoverSet::NONE);
    }
}
