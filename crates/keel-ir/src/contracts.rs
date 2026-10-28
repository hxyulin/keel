//! Scoped producer contracts, independent of ABI and instruction scheduling.
use crate::{Purpose, Signature, Type};
use std::fmt;

/// Which aspects of an address may escape a passed pointer copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AddressCapture {
    None,
    Nullness,
    #[default]
    Address,
}

/// Access capability that may survive the invocation through a passed copy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ProvenanceCapture {
    None,
    Read,
    #[default]
    ReadWrite,
}

/// Allowed capture components at one escape location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CaptureComponents {
    pub address: AddressCapture,
    pub provenance: ProvenanceCapture,
}
impl CaptureComponents {
    pub const NONE: Self = Self {
        address: AddressCapture::None,
        provenance: ProvenanceCapture::None,
    };
    fn intersect(self, other: Self) -> Self {
        use AddressCapture as A;
        use ProvenanceCapture as P;
        Self {
            address: match (self.address, other.address) {
                (A::None, _) | (_, A::None) => A::None,
                (A::Nullness, _) | (_, A::Nullness) => A::Nullness,
                _ => A::Address,
            },
            provenance: match (self.provenance, other.provenance) {
                (P::None, _) | (_, P::None) => P::None,
                (P::Read, _) | (_, P::Read) => P::Read,
                _ => P::ReadWrite,
            },
        }
    }
}
impl fmt::Display for CaptureComponents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let address = match self.address {
            AddressCapture::None => "",
            AddressCapture::Nullness => "address_is_null",
            AddressCapture::Address => "address",
        };
        let provenance = match self.provenance {
            ProvenanceCapture::None => "",
            ProvenanceCapture::Read => "read_provenance",
            ProvenanceCapture::ReadWrite => "provenance",
        };
        match (address, provenance) {
            ("", "") => f.write_str("none"),
            ("", p) => f.write_str(p),
            (a, "") => f.write_str(a),
            (a, p) => write!(f, "{a}, {p}"),
        }
    }
}

/// Parameter-copy capture permissions, separated by return and other escapes.
/// These do not restrict other copies of the pointer or prove fault freedom.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CaptureContract {
    pub other: CaptureComponents,
    pub returned: CaptureComponents,
}
impl CaptureContract {
    pub const NONE: Self = Self {
        other: CaptureComponents::NONE,
        returned: CaptureComponents::NONE,
    };
    pub fn intersect(self, other: Self) -> Self {
        Self {
            other: self.other.intersect(other.other),
            returned: self.returned.intersect(other.returned),
        }
    }
}

/// A nonempty modular half-open integer interval, with width supplied by its ABI
/// position. Lower greater than upper denotes a range that wraps through zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntegerRange {
    pub lower: u64,
    pub upper: u64,
}

/// Externally relevant reads and writes permitted in a memory region.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MemoryAccess {
    None,
    Read,
    Write,
    #[default]
    ReadWrite,
}
impl MemoryAccess {
    /// Combine two promises that both apply to an execution.
    pub fn intersect(self, other: Self) -> Self {
        match (
            self.may_read() && other.may_read(),
            self.may_write() && other.may_write(),
        ) {
            (false, false) => Self::None,
            (true, false) => Self::Read,
            (false, true) => Self::Write,
            (true, true) => Self::ReadWrite,
        }
    }
    pub fn may_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }
    pub fn may_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}
impl fmt::Display for MemoryAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "none",
            Self::Read => "read",
            Self::Write => "write",
            Self::ReadWrite => "readwrite",
        })
    }
}

/// Allowed effects by pointer accessibility. Unknown summaries allow every access.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MemorySummary {
    pub argmem: MemoryAccess,
    pub other: MemoryAccess,
    pub inaccessible: MemoryAccess,
}
impl MemorySummary {
    pub fn intersect(self, other: Self) -> Self {
        Self {
            argmem: self.argmem.intersect(other.argmem),
            other: self.other.intersect(other.other),
            inaccessible: self.inaccessible.intersect(other.inaccessible),
        }
    }
    /// Whether a call may change bytes reachable by ordinary program pointers.
    pub fn may_write_accessible(self) -> bool {
        self.argmem.may_write() || self.other.may_write()
    }
}

/// Facts about one pointer at its parameter or return boundary.
/// Readability is a byte extent at that boundary, not permanent object validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerContract {
    pub align: u32,
    pub nonnull: bool,
    pub readable_bytes: u64,
    /// Readable extent only if the pointer is non-null, at this boundary.
    pub readable_or_null_bytes: u64,
    /// Parameter-only permissions on capture of the passed pointer copy.
    pub capture: CaptureContract,
    /// Return-only allocation-like relation: returned storage is disjoint from
    /// other objects accessible to the caller. This implies neither non-null,
    /// an allocation size, nor permission to access any bytes.
    pub return_noalias: bool,
}
impl Default for PointerContract {
    fn default() -> Self {
        Self {
            align: 1,
            nonnull: false,
            readable_bytes: 0,
            readable_or_null_bytes: 0,
            capture: CaptureContract::default(),
            return_noalias: false,
        }
    }
}
impl PointerContract {
    pub fn intersect(self, other: Self) -> Self {
        Self {
            align: self.align.max(other.align),
            nonnull: self.nonnull || other.nonnull,
            readable_bytes: self.readable_bytes.max(other.readable_bytes),
            readable_or_null_bytes: self
                .readable_or_null_bytes
                .max(other.readable_or_null_bytes),
            capture: self.capture.intersect(other.capture),
            return_noalias: self.return_noalias || other.return_noalias,
        }
    }
}

/// Producer promises for one function invocation, separate from its machine ABI.
/// A missing property is unknown. Contracts do not authorize motion or elimination.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CallContract {
    pub memory: MemorySummary,
    pub nofree: bool,
    pub nosync: bool,
    /// No callback into the caller's module during this invocation, following
    /// LLVM's contextual contract. This does not prohibit every unrelated callback.
    pub nocallback: bool,
    /// On normal return, existing live objects retain their readable/writable
    /// accessibility. This excludes permission/mapping changes, not byte writes.
    /// LLVM nofree alone does not establish Keel's observable-fault contract.
    pub preserves_access: bool,
    /// Sparse pointer facts indexed by ABI parameter position.
    pub parameters: Vec<(u32, PointerContract)>,
    /// Sparse pointer facts indexed by return position.
    pub returns: Vec<(u32, PointerContract)>,
    /// Scoped conjunctions of integer facts. Several intervals at one position
    /// all apply; they are not a union or facts about unrelated SSA values.
    pub parameter_ranges: Vec<(u32, IntegerRange)>,
    pub return_ranges: Vec<(u32, IntegerRange)>,
}
impl CallContract {
    pub fn is_unknown(&self) -> bool {
        self == &Self::default()
    }
    /// Intersect allowed effects and combine facts promised at the same boundary.
    pub fn intersect(&self, other: &Self) -> Self {
        fn pointers(
            a: &[(u32, PointerContract)],
            b: &[(u32, PointerContract)],
        ) -> Vec<(u32, PointerContract)> {
            let mut out = a.to_vec();
            for &(index, facts) in b {
                if let Some((_, old)) = out.iter_mut().find(|(i, _)| *i == index) {
                    *old = old.intersect(facts);
                } else {
                    out.push((index, facts));
                }
            }
            out.sort_by_key(|p| p.0);
            out
        }
        Self {
            memory: self.memory.intersect(other.memory),
            nofree: self.nofree || other.nofree,
            nosync: self.nosync || other.nosync,
            nocallback: self.nocallback || other.nocallback,
            preserves_access: self.preserves_access || other.preserves_access,
            parameters: pointers(&self.parameters, &other.parameters),
            returns: pointers(&self.returns, &other.returns),
            parameter_ranges: ranges(&self.parameter_ranges, &other.parameter_ranges),
            return_ranges: ranges(&self.return_ranges, &other.return_ranges),
        }
    }
    /// Validate structure and ABI positions; the producer must prove the promises.
    pub fn validate(&self, sig: &Signature) -> Result<(), String> {
        for (facts, params, is_return) in [
            (&self.parameters, &sig.params, false),
            (&self.returns, &sig.returns, true),
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for &(index, p) in facts {
                if !seen.insert(index) {
                    return Err("duplicate pointer contract position".into());
                }
                let Some(arg) = params.get(index as usize) else {
                    return Err("pointer contract position is outside the signature".into());
                };
                if arg.ty != Type::Ptr || arg.purpose != Purpose::Normal {
                    return Err("pointer contracts require normal-purpose pointer positions".into());
                }
                if !p.align.is_power_of_two() {
                    return Err("pointer contract alignment must be a nonzero power of two".into());
                }
                if p.readable_bytes > i64::MAX as u64 || p.readable_or_null_bytes > i64::MAX as u64
                {
                    return Err("readable byte extent must fit i64".into());
                }
                if is_return && p.capture != CaptureContract::default() {
                    return Err("capture permissions are parameter-only contracts".into());
                }
                if !is_return && p.return_noalias {
                    return Err("allocation-like noalias is a return-only contract".into());
                }
            }
        }
        for (facts, params) in [
            (&self.parameter_ranges, &sig.params),
            (&self.return_ranges, &sig.returns),
        ] {
            for &(index, range) in facts {
                let Some(arg) = params.get(index as usize) else {
                    return Err("integer range position is outside the signature".into());
                };
                let width = match arg.ty {
                    Type::I8 => 8,
                    Type::I16 => 16,
                    Type::I32 => 32,
                    Type::I64 => 64,
                    _ => return Err("integer ranges require i8/i16/i32/i64 positions".into()),
                };
                if arg.purpose != Purpose::Normal || range.lower == range.upper {
                    return Err(
                        "integer range must be nonempty at a normal-purpose position".into(),
                    );
                }
                if width < 64 && (range.lower >> width != 0 || range.upper >> width != 0) {
                    return Err("integer range endpoint exceeds the position's bit width".into());
                }
            }
        }
        Ok(())
    }
}
impl fmt::Display for CallContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unknown() {
            return Ok(());
        }
        f.write_str(" contract(")?;
        let mut separator = "";
        if self.memory != MemorySummary::default() {
            write!(
                f,
                "memory(argmem: {}, other: {}, inaccessible: {})",
                self.memory.argmem, self.memory.other, self.memory.inaccessible
            )?;
            separator = ", ";
        }
        for (set, name) in [
            (self.nofree, "nofree"),
            (self.nosync, "nosync"),
            (self.nocallback, "nocallback"),
            (self.preserves_access, "preserves_access"),
        ] {
            if set {
                write!(f, "{separator}{name}")?;
                separator = ", ";
            }
        }
        for (name, positions) in [("param", &self.parameters), ("result", &self.returns)] {
            for (index, p) in positions {
                write!(f, "{separator}{name}({index}")?;
                if p.align != 1 {
                    write!(f, ", align {}", p.align)?;
                }
                if p.nonnull {
                    f.write_str(", nonnull")?;
                }
                if p.readable_bytes != 0 {
                    write!(f, ", readable {}", p.readable_bytes)?;
                }
                if p.readable_or_null_bytes != 0 {
                    write!(f, ", readable_or_null {}", p.readable_or_null_bytes)?;
                }
                if p.capture == CaptureContract::NONE {
                    f.write_str(", nocapture")?;
                } else if p.capture != CaptureContract::default() {
                    write!(
                        f,
                        ", captures(other({}), ret({}))",
                        p.capture.other, p.capture.returned
                    )?;
                }
                if p.return_noalias {
                    f.write_str(", noalias")?;
                }
                f.write_str(")")?;
                separator = ", ";
            }
        }
        for (name, positions) in [
            ("range_param", &self.parameter_ranges),
            ("range_result", &self.return_ranges),
        ] {
            for (index, range) in positions {
                write!(
                    f,
                    "{separator}{name}({index}, {:#x}, {:#x})",
                    range.lower, range.upper
                )?;
                separator = ", ";
            }
        }
        f.write_str(")")
    }
}

fn ranges(a: &[(u32, IntegerRange)], b: &[(u32, IntegerRange)]) -> Vec<(u32, IntegerRange)> {
    let mut out = a.to_vec();
    for &range in b {
        if !out.contains(&range) {
            out.push(range);
        }
    }
    out.sort_by_key(|(index, range)| (*index, range.lower, range.upper));
    out
}
