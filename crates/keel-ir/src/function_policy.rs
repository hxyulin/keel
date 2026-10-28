//! Source function optimization requests retained for later policy consumers.
use std::{fmt, sync::Arc};

/// A nonstandard function entry/return contract awaiting target lowering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FunctionEntryContract {
    /// Entry and return are owned by the function body.
    Naked,
    /// Entry preserves interrupt state and returns through an interrupt frame.
    Interrupt,
}
impl fmt::Display for FunctionEntryContract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Naked => "naked",
            Self::Interrupt => "interrupt",
        })
    }
}

/// One ordered argument of a GNU function `optimize` attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptimizationOption {
    /// Numeric optimization level, without interpreting its target policy.
    Level(u32),
    /// Original string, including comma-separated options inside that string.
    Options(String),
}

/// Ordered function-local optimization requests. Clones share immutable storage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionOptimization(Option<Arc<[OptimizationOption]>>);
impl FunctionOptimization {
    /// Preserve the producer's argument order without applying the requests.
    pub fn new(options: impl IntoIterator<Item = OptimizationOption>) -> Self {
        let options: Vec<_> = options.into_iter().collect();
        if options.is_empty() {
            Self::default()
        } else {
            Self(Some(options.into()))
        }
    }
    /// Requests in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &OptimizationOption> {
        self.0.iter().flat_map(|options| options.iter())
    }
    /// Whether there is no per-function optimization request.
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
    /// Append a later declaration's requests, retaining repeated options.
    pub fn append(&mut self, other: &Self) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = other.clone();
            return;
        }
        *self = Self::new(self.iter().chain(other.iter()).cloned());
    }
    pub(crate) fn is_valid(&self) -> bool {
        self.iter().all(|option| match option {
            OptimizationOption::Level(_) => true,
            OptimizationOption::Options(s) => s.bytes().all(|b| b.is_ascii_graphic() || b == b' '),
        })
    }
}
impl fmt::Display for FunctionOptimization {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("optimize(")?;
        for (i, option) in self.iter().enumerate() {
            if i != 0 {
                f.write_str(", ")?;
            }
            match option {
                OptimizationOption::Level(n) => write!(f, "{n}")?,
                OptimizationOption::Options(s) => write!(f, "{s:?}")?,
            }
        }
        f.write_str(")")
    }
}
