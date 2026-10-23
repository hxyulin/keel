//! Inspect the experimental APIs without adding unsupported production flags.
//! cargo run -p keel-toolchain --example prepare --
//!   aarch64-apple-darwin input.keel output.kir [--memory-checks]

use keel_ir::Target;
use keel_toolchain::{artifact, instrument};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(args.len() == 3 || (args.len() == 4 && args[3] == "--memory-checks")) {
        return Err("usage: prepare TARGET INPUT.keel OUTPUT.kir [--memory-checks]".into());
    }
    let target = Target::from_triple(&args[0]).ok_or("unsupported target")?;
    let mut module = keel_ir::parse(&std::fs::read_to_string(&args[1])?)
        .map_err(|e| keel_toolchain::Error(e.to_string()))?;
    if args.len() == 4 {
        let report = instrument::insert_memory_checks(&mut module)?;
        eprintln!(
            "inserted {} memory callbacks in {} functions",
            report.callbacks, report.functions
        );
    }
    let artifact = artifact::encode(&module, target)?;
    std::fs::write(&args[2], artifact)?;
    Ok(())
}
