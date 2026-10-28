//! Ordered function optimization requests and their supported policy resolution.
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

/// Function-local optimization preset, independent of target instruction selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FunctionOptLevel {
    /// Disable optional IR optimization.
    None,
    /// Fast local optimization (`O1`).
    Fast,
    /// Balanced release optimization (`O2`).
    Balanced,
    /// Full optimization (`O3`).
    Full,
    /// Size-oriented optimization (`Os`).
    Size,
}

/// Resolved overrides. Missing fields inherit the invocation's policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FunctionPolicy {
    /// Explicit optimization preset, with the last preset taking precedence.
    pub level: Option<FunctionOptLevel>,
    /// Optional direct-call inlining.
    pub inline: Option<bool>,
    /// Bounded loop unrolling.
    pub unroll: Option<bool>,
    /// Loop vectorization.
    pub vectorize: Option<bool>,
}
impl FunctionPolicy {
    /// Resolve supported GNU options in source order; reject unknown options.
    ///
    /// `optnone` wins over optimization presets and individual pass switches.
    /// This does not disable language-required inlining or target legalization.
    ///
    /// # Errors
    /// Returns the unsupported option rather than silently ignoring it.
    pub fn resolve(attrs: &crate::FunctionAttrs) -> Result<Self, String> {
        let mut result = Self::default();
        for argument in attrs.optimize.iter() {
            match argument {
                OptimizationOption::Level(n) => result.set_option(&format!("O{n}"))?,
                OptimizationOption::Options(options) => {
                    for option in options.split(',') {
                        result.set_option(option.trim())?;
                    }
                }
            }
        }
        if attrs.optnone {
            result.level = Some(FunctionOptLevel::None);
            result.inline = Some(false);
            result.unroll = Some(false);
            result.vectorize = Some(false);
        }
        Ok(result)
    }

    fn set_option(&mut self, option: &str) -> Result<(), String> {
        let option = option.strip_prefix('-').unwrap_or(option);
        let level = match option {
            "O0" | "0" => Some(FunctionOptLevel::None),
            "O1" | "1" => Some(FunctionOptLevel::Fast),
            "O2" | "2" => Some(FunctionOptLevel::Balanced),
            "O3" | "3" => Some(FunctionOptLevel::Full),
            "Os" | "s" => Some(FunctionOptLevel::Size),
            _ => None,
        };
        if let Some(level) = level {
            self.level = Some(level);
            return Ok(());
        }
        let option = option.strip_prefix('f').unwrap_or(option);
        let (enabled, option) = match option.strip_prefix("no-") {
            Some(option) => (false, option),
            None => (true, option),
        };
        match option {
            "inline" | "inline-functions" => self.inline = Some(enabled),
            "unroll-loops" => self.unroll = Some(enabled),
            "tree-vectorize" | "tree-loop-vectorize" => self.vectorize = Some(enabled),
            _ => {
                return Err(format!(
                    "unsupported function optimization option `{option}`"
                ));
            }
        }
        Ok(())
    }

    /// Whether optional transformations must leave the function body unchanged.
    pub fn disabled(self) -> bool {
        self.level == Some(FunctionOptLevel::None)
    }

    /// Whether optional inlining can modify this function or consume its body.
    pub fn permits_inlining(self) -> bool {
        !self.disabled()
            && self
                .inline
                .unwrap_or(!matches!(self.level, Some(FunctionOptLevel::Fast)))
    }
}
