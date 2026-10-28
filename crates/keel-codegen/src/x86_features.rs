//! Configure x86 ISA legality without changing existing pipeline constructors.
use crate::{InstructionSelector, Target};
pub use keel_asm_ir::x86::{CpuLevel, Features};

pub(crate) fn validate_target_features(
    target: Target,
    features: Features,
) -> Result<(), &'static str> {
    features.validate()?;
    if !features.is_empty() && target != Target::X86_64LinuxGnu {
        return Err("x86 target features require x86_64-unknown-linux-gnu");
    }
    Ok(())
}

pub struct FeatureSelector<'a> {
    pub inner: &'a dyn InstructionSelector,
    pub features: Features,
}
impl std::fmt::Debug for FeatureSelector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeatureSelector")
            .field("inner", &self.inner.name())
            .field("features", &self.features)
            .finish()
    }
}
impl InstructionSelector for FeatureSelector<'_> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn x86_features(&self) -> Features {
        self.features
    }
    fn validate_target(&self, target: Target) -> Result<(), &'static str> {
        crate::require_native_target(target)?;
        self.inner.validate_target(target)?;
        validate_target_features(target, self.features)
    }

    fn select(
        &self,
        func: &keel_ir::Function,
        target: crate::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
    ) -> crate::select::Selection {
        self.inner.select(func, target, folds)
    }
    fn select_with_uses(
        &self,
        func: &keel_ir::Function,
        target: crate::select::Target,
        folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        uses: &[u8],
    ) -> crate::select::Selection {
        self.inner.select_with_uses(func, target, folds, uses)
    }
}

/// Project the validated selector's effective contract into optimizer capabilities.
/// Fingerprint bit IDs come from the shared feature schema.
/// This does not inspect the host and never infers capabilities from a level name.
/// Every target prefers 128 bits; ISA support is not evidence of profitability.
pub fn vector_capabilities(
    target: Target,
    selector: &dyn InstructionSelector,
) -> Result<keel_ir::VectorCapabilities, &'static str> {
    crate::require_native_target(target)?;
    selector.validate_target(target)?;
    validate_target_features(target, selector.x86_features())?;
    if target != Target::X86_64LinuxGnu {
        return Ok(keel_ir::VectorCapabilities::BASELINE);
    }
    let f = selector.x86_features();
    f.validate()?;
    let bits = f.fingerprint();
    Ok(keel_ir::VectorCapabilities::new(f.avx, f.avx2, false, bits))
}
