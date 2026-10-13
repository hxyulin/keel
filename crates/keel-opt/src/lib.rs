//! Optimization passes over keel IR.
//!
//! A pass is a function `fn(&mut Function) -> bool` that returns whether it
//! changed anything. Passes work on one function at a time, so functions
//! could be optimized in parallel, and compute the analyses they need
//! (CFG, dominators) themselves; there is no analysis cache until one is
//! needed.
//!
//! The pass manager runs the verifier after every pass. The IR is valid
//! between passes, always: a pass that breaks an invariant is caught at the
//! pass that broke it, not three passes later in codegen.

use keel_ir::{Function, Module, VerifierError, verify};
use std::fmt;

mod const_fold;
mod dce;
mod gcm;
mod gvn;
mod simplify_cfg;

pub struct Pass {
    pub name: &'static str,
    pub description: &'static str,
    pub run: fn(&mut Function) -> bool,
}

pub const PASSES: &[Pass] = &[
    Pass {
        name: "const-fold",
        description: "evaluate instructions whose operands are constants",
        run: const_fold::run,
    },
    Pass {
        name: "simplify-cfg",
        description: "fold constant branches, skip empty blocks, merge straight-line blocks, remove unreachable blocks",
        run: simplify_cfg::run,
    },
    Pass {
        name: "gvn",
        description: "replace pure instructions with an identical one that dominates them",
        run: gvn::run,
    },
    Pass {
        name: "dce",
        description: "remove unused pure instructions and block parameters",
        run: dce::run,
    },
    Pass {
        name: "gcm",
        description: "move pure instructions out of loops and into the branches that need them; copy constants to each block using them",
        run: gcm::run,
    },
];

/// The passes `keel compile -O` runs, in order.
pub const DEFAULT_PIPELINE: &[&str] = &[
    "simplify-cfg",
    "const-fold",
    "simplify-cfg",
    "gvn",
    "dce",
    "gcm",
];

pub fn find_pass(name: &str) -> Option<&'static Pass> {
    PASSES.iter().find(|p| p.name == name)
}

#[derive(Debug)]
pub enum OptError {
    UnknownPass(String),
    /// A pass left a function the verifier rejects: a bug in the pass.
    Verifier {
        pass: &'static str,
        func: String,
        errors: Vec<VerifierError>,
    },
}

impl fmt::Display for OptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptError::UnknownPass(name) => {
                let names: Vec<&str> = PASSES.iter().map(|p| p.name).collect();
                write!(f, "unknown pass `{name}`; passes are: {}", names.join(", "))
            }
            OptError::Verifier { pass, func, errors } => {
                write!(f, "pass `{pass}` broke @{func}:")?;
                for e in errors {
                    write!(f, "\n  {e}")?;
                }
                Ok(())
            }
        }
    }
}

/// Runs the named passes, in order, on every function in `module`.
pub fn run_passes(module: &mut Module, names: &[&str]) -> Result<(), OptError> {
    let passes = names
        .iter()
        .map(|&name| find_pass(name).ok_or_else(|| OptError::UnknownPass(name.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    for func in &mut module.functions {
        for pass in &passes {
            (pass.run)(func);
            verify(func).map_err(|errors| OptError::Verifier {
                pass: pass.name,
                func: func.name.clone(),
                errors,
            })?;
        }
    }
    Ok(())
}

/// Runs [`DEFAULT_PIPELINE`].
pub fn optimize(module: &mut Module) -> Result<(), OptError> {
    run_passes(module, DEFAULT_PIPELINE)
}
