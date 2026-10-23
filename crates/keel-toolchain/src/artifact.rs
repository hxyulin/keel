//! Versioned transport for a single target-specific Keel module.
//!
//! This is intentionally separate from native objects. Consumers must still
//! resolve symbols, rename private definitions, and remap debug IDs before
//! combining units. LLVM bitcode and native object readers cannot read it.

use crate::{Error, validate};
use keel_ir::{Module, Target};

const MAGIC: &str = "KEEL-IR-UNIT 4";

/// Serialize verified IR with an explicit target and version envelope.
/// The version covers the envelope and IR dialect; bump it on incompatible
/// changes. No host target is inferred when reading an artifact.
pub fn encode(module: &Module, target: Target) -> Result<Vec<u8>, Error> {
    validate(module)?;
    keel_ir::section::validate_module(module, target).map_err(Error)?;
    let ir = module.to_string();
    // Refuse artifacts that the current parser cannot recover. Parsing also
    // canonicalizes value numbering: inserted instructions may have newer IDs
    // than the original instructions that now follow them in layout order.
    let recovered = keel_ir::parse(&ir).map_err(|e| Error(e.to_string()))?;
    validate(&recovered)?;
    let ir = recovered.to_string();
    Ok(format!(
        "{MAGIC}\ntarget {}\nbytes {}\n\n{ir}",
        target.triple(),
        ir.len()
    )
    .into_bytes())
}

/// Read one complete artifact, rejecting other versions, targets, truncated
/// payloads, trailing bytes, malformed UTF-8 and invalid modules.
pub fn decode(bytes: &[u8], expected_target: Target) -> Result<Module, Error> {
    let text = std::str::from_utf8(bytes).map_err(|e| Error(e.to_string()))?;
    let (header, ir) = text
        .split_once("\n\n")
        .ok_or_else(|| Error("missing IR unit header".into()))?;
    let mut lines = header.lines();
    if lines.next() != Some(MAGIC) {
        return Err(Error("unsupported IR unit format/version".into()));
    }
    let triple = lines
        .next()
        .and_then(|s| s.strip_prefix("target "))
        .ok_or_else(|| Error("missing IR unit target".into()))?;
    if Target::from_triple(triple) != Some(expected_target) {
        return Err(Error(format!(
            "IR unit target {triple} does not match {}",
            expected_target.triple()
        )));
    }
    let size: usize = lines
        .next()
        .and_then(|s| s.strip_prefix("bytes "))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error("invalid IR unit payload size".into()))?;
    if lines.next().is_some() || size != ir.len() {
        return Err(Error(
            "IR unit size mismatch or unexpected header fields".into(),
        ));
    }
    let module = keel_ir::parse(ir).map_err(|e| Error(e.to_string()))?;
    validate(&module)?;
    keel_ir::section::validate_module(&module, expected_target).map_err(Error)?;
    Ok(module)
}
