//! The C-to-IR step that keel-c tests share.
//!
//! Each test names its translation unit, so diagnostics keep pointing at a
//! file name that identifies the test.
use keel_ir::{Module, Target};

/// Frontend options for `target`, with or without debug info.
pub fn options(target: Target, debug_info: bool) -> keel_c::Options {
    keel_c::Options {
        target,
        debug_info,
        ..Default::default()
    }
}

/// Compiles preprocessed `source` as the unit `path`.
pub fn frontend(
    source: &str,
    path: &str,
    target: Target,
    debug_info: bool,
) -> Result<Module, keel_c::Diagnostic> {
    keel_c::compile(source, path, options(target, debug_info))
}

/// Compiles preprocessed `source` and verifies the module, panicking on a
/// diagnostic.
pub fn module(source: &str, path: &str, target: Target, debug_info: bool) -> Module {
    let module = frontend(source, path, target, debug_info).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
