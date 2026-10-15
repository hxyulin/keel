//! Configure x86 ISA legality without changing existing pipeline constructors.
use crate::{InstructionSelector, Target};
use keel_asm_ir::x86::Features;

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
        self.inner.validate_target(target)?;
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
}
