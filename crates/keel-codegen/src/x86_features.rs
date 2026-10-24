//! Configure x86 ISA legality without changing existing pipeline constructors.
use crate::{InstructionSelector, Target};
pub use keel_asm_ir::x86::{CpuLevel, Features};

pub struct FeatureSelector<'a> {
    pub inner: &'a dyn InstructionSelector,
    pub features: Features,
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
        self.features.validate()?;
        if !self.features.is_empty() && target != Target::X86_64LinuxGnu {
            return Err("x86 target features require x86_64-unknown-linux-gnu");
        }
        Ok(())
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
/// Fingerprint bit order: aes,avx,bmi1,bmi2,clflush,pclmulqdq,cx16,lahf_sahf,
/// popcnt,sse3,ssse3,sse41,sse42,avx2,f16c,fma,lzcnt,movbe,xsave,osxsave,explicit.
/// This does not inspect the host and never infers capabilities from a level name.
/// Every target prefers 128 bits; ISA support is not evidence of profitability.
pub fn vector_capabilities(
    target: Target,
    selector: &dyn InstructionSelector,
) -> Result<keel_ir::VectorCapabilities, &'static str> {
    crate::require_native_target(target)?;
    selector.validate_target(target)?;
    if target != Target::X86_64LinuxGnu {
        return Ok(keel_ir::VectorCapabilities::BASELINE);
    }
    let f = selector.x86_features();
    f.validate()?;
    let bits = [
        f.aes,
        f.avx,
        f.bmi1,
        f.bmi2,
        f.clflush,
        f.pclmulqdq,
        f.cx16,
        f.lahf_sahf,
        f.popcnt,
        f.sse3,
        f.ssse3,
        f.sse41,
        f.sse42,
        f.avx2,
        f.f16c,
        f.fma,
        f.lzcnt,
        f.movbe,
        f.xsave,
        f.osxsave,
        f.explicit,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |bits, (i, enabled)| bits | (u64::from(enabled) << i));
    Ok(keel_ir::VectorCapabilities::new(f.avx, f.avx2, false, bits))
}
