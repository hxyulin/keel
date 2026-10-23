//! Experimental building blocks for future sanitizers and LTO.
//!
//! These APIs do not implement ASan or an LTO link. They provide explicit
//! memory-access callbacks and a versioned, target-checked IR transport.

pub mod artifact;
pub mod instrument;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn validate(module: &keel_ir::Module) -> Result<(), Error> {
    keel_ir::verify_module(module).map_err(|errors| {
        Error(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })
}
