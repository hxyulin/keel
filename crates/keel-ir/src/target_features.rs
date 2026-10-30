//! Function-local x86 permissions, separate from optimization and tuning.
use keel_asm_ir::x86::Features;
use std::{fmt, sync::Arc};

#[derive(Debug, PartialEq, Eq)]
struct Edits {
    sources: Box<[String]>,
    enabled: u64,
    disabled: u64,
}

/// Ordered x86 feature edits applied to the consumer's module baseline.
/// Clones share validated immutable requests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct X86TargetFeatures(Option<Arc<Edits>>);

impl X86TargetFeatures {
    /// Validate feature names and retain source order. Dependency validation
    /// occurs in [`Self::resolve`] once the module baseline is known.
    ///
    /// # Errors
    /// Returns an error for empty lists or unknown feature names.
    pub fn new(sources: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let sources: Box<[String]> = sources.into_iter().collect();
        if sources.is_empty() {
            return Ok(Self::default());
        }
        let mut enabled = 0;
        let mut disabled = 0;
        for source in &sources {
            if source == "none" {
                enabled = 0;
                disabled = Features::all_known().fingerprint();
                continue;
            }
            for token in source.split(',') {
                let (positive, name) = token.strip_prefix('-').map_or_else(
                    || (true, token.strip_prefix('+').unwrap_or(token)),
                    |name| (false, name),
                );
                if name.is_empty() || name == "none" {
                    return Err(format!("unknown x86 target feature `{token}`"));
                }
                let feature = Features::parse(name)?;
                let mask = if positive {
                    feature.fingerprint()
                } else {
                    // Enabling one feature provides its prerequisite closure.
                    // Removing that feature alone leaves its prerequisites valid
                    // and identifies the edited bit, including accepted aliases.
                    feature.fingerprint()
                        ^ feature.with_features(&format!("-{name}"))?.fingerprint()
                };
                if positive {
                    enabled |= mask;
                    disabled &= !mask;
                } else {
                    disabled |= mask;
                    enabled &= !mask;
                }
            }
        }
        Ok(Self(Some(Arc::new(Edits {
            sources,
            enabled,
            disabled,
        }))))
    }

    /// Requests in producer order.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0
            .iter()
            .flat_map(|edits| edits.sources.iter().map(String::as_str))
    }

    /// Whether the function inherits the module baseline unchanged.
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// Append a later declaration's validated requests.
    pub fn append(&mut self, other: &Self) {
        if other.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = other.clone();
            return;
        }
        let sources = self.iter().chain(other.iter()).map(str::to_owned);
        *self = Self::new(sources).expect("appending validated target feature names");
    }

    /// Apply ordered edits, then validate their complete prerequisite contract.
    ///
    /// # Errors
    /// Returns an error if the resulting permissions lack a prerequisite.
    pub fn resolve(&self, baseline: Features) -> Result<Features, String> {
        baseline.with_feature_lists(self.iter())
    }

    /// Apply local permissions to optimizer widths while retaining its explicit
    /// profitability preference. An inherited context is returned unchanged.
    ///
    /// # Errors
    /// Returns an error for invalid baseline fingerprints or prerequisites.
    pub fn vector_capabilities(
        &self,
        baseline: crate::VectorCapabilities,
    ) -> Result<crate::VectorCapabilities, String> {
        if self.is_empty() {
            return Ok(baseline);
        }
        let features =
            Features::from_fingerprint(baseline.effective_x86_features()).map_err(str::to_owned)?;
        let features = self.resolve(features)?;
        Ok(crate::VectorCapabilities::new(
            features.avx,
            features.avx2,
            baseline.preferred_bits() == 256,
            features.fingerprint(),
        )
        .with_512_bit_values(features.avx512f))
    }

    /// Whether copying `callee` into this function is safe for every module
    /// baseline. This conservative check also works during frontend-required
    /// inlining, before a backend baseline has been supplied.
    pub fn permits_inlining(&self, callee: &Self) -> bool {
        let masks = |value: &Self| {
            value
                .0
                .as_ref()
                .map_or((0, 0), |edits| (edits.enabled, edits.disabled))
        };
        let (caller_on, caller_off) = masks(self);
        let (callee_on, callee_off) = masks(callee);
        callee_on & !caller_on == 0 && caller_off & !callee_off == 0
    }
}

impl fmt::Display for X86TargetFeatures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("target_features(")?;
        for (index, source) in self.iter().enumerate() {
            if index != 0 {
                f.write_str(", ")?;
            }
            write!(f, "{source:?}")?;
        }
        f.write_str(")")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edits(source: &str) -> X86TargetFeatures {
        X86TargetFeatures::new([source.to_owned()]).unwrap()
    }

    #[test]
    fn ordered_edits_preserve_dependencies_and_inline_boundaries() {
        let avx = edits("avx");
        let avx2 = edits("avx2");
        let baseline = X86TargetFeatures::default();
        assert!(avx2.permits_inlining(&avx));
        assert!(avx2.permits_inlining(&baseline));
        assert!(!avx.permits_inlining(&avx2));
        assert!(!baseline.permits_inlining(&avx));
        assert!(!edits("-avx").permits_inlining(&baseline));
        assert!(edits("-avx").permits_inlining(&edits("-avx")));
        assert!(edits("avx2,-avx").resolve(Features::NONE).is_err());
        assert!(
            edits("avx2,-avx,+avx")
                .resolve(Features::NONE)
                .unwrap()
                .avx2
        );
        assert!(X86TargetFeatures::new(["unknown".to_owned()]).is_err());
        assert!(X86TargetFeatures::new(["".to_owned()]).is_err());
    }
}
