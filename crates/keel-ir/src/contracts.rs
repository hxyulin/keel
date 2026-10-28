//! Scoped producer contracts, independent of ABI and instruction scheduling.
use crate::{Purpose, Signature, Type};
use std::fmt;

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
    /// The passed pointer's address and access capability are not captured.
    /// This parameter-only property is invalid on return values.
    pub no_capture: bool,
}
impl Default for PointerContract {
    fn default() -> Self {
        Self {
            align: 1,
            nonnull: false,
            readable_bytes: 0,
            no_capture: false,
        }
    }
}
impl PointerContract {
    pub fn intersect(self, other: Self) -> Self {
        Self {
            align: self.align.max(other.align),
            nonnull: self.nonnull || other.nonnull,
            readable_bytes: self.readable_bytes.max(other.readable_bytes),
            no_capture: self.no_capture || other.no_capture,
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
                if p.readable_bytes > i64::MAX as u64 {
                    return Err("readable byte extent must fit i64".into());
                }
                if is_return && p.no_capture {
                    return Err("nocapture is a parameter-only contract".into());
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
                if p.no_capture {
                    f.write_str(", nocapture")?;
                }
                f.write_str(")")?;
                separator = ", ";
            }
        }
        f.write_str(")")
    }
}
