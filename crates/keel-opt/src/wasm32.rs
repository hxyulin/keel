//! Deliberately separate from the native and Portable pass managers.
use crate::OptError;
use keel_ir::{DataLayout, Module, verify_module_with_layout};

/// Optimize a backend-preflighted wasm32 module using the fixed basic profile.
///
/// The caller must first validate the COMPLETE ORIGINAL module against the
/// Wasm runtime subset, including unused definitions and instructions. Layout
/// verification here cannot establish backend support or input provenance.
/// In particular, this helper does not perform runtime capability admission;
/// callers must also check the optimized module with their normal final emitter.
/// The input must already use WASM32 storage layout; this is not a layout
/// conversion API. An input-verification error leaves the module unchanged.
/// A later verification error may leave earlier transformations applied, so
/// callers needing transactional behavior must optimize a clone.
/// This entry point never canonicalizes, tunes for the host, or selects a native
/// tier. Verification after every pass is mandatory in release builds too.
pub fn optimize_wasm32_basic(module: &mut Module) -> Result<(), OptError> {
    fn verify(module: &Module, pass: &'static str) -> Result<(), OptError> {
        verify_module_with_layout(module, DataLayout::WASM32).map_err(|errors| OptError::Verifier {
            pass,
            func: "<wasm32-module>".into(),
            errors,
        })
    }
    // Clear thread-local analysis even when verification fails mid-pipeline.
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            crate::analysis::clear();
        }
    }
    let _clear = Clear;
    verify(module, "wasm32-input")?;
    // No caller-selected passes, native restoration, or fixed-point expansion.
    for (name, run) in BASIC {
        for function in &mut module.functions {
            run(function);
        }
        verify(module, name)?;
    }
    Ok(())
}

// Private immutable allowlist; additions require a Wasm semantics audit.
const BASIC: &[(&str, fn(&mut keel_ir::Function) -> bool)] = &[
    ("const-fold", crate::const_fold::run),
    ("simplify-cfg", crate::simplify_cfg::run),
    ("phi-simplify", crate::phi_simplify::run),
    ("dce", crate::dce::run),
];
