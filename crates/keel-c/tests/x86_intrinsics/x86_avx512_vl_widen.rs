#[path = "../../../keel-codegen/tests/common/avx512_vl_widen.rs"]
mod common;
use super::pclmul::parse;
#[test]
fn vl_widen_masked_and_zero_wrappers_match_scalar_and_system_headers() {
    let (_, source, _) = common::fixtures();
    let mut module = parse(&source, keel_ir::Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    common::check(&module, "vl-widen-c");
}
#[test]
fn vl_widen_wrappers_do_not_grant_permissions_to_their_callers() {
    let (_, source, _) = common::fixtures();
    let source = source.replacen("target(\"avx512vl\")", "target(\"avx512f\")", 1);
    let mut module = parse(&source, keel_ir::Target::X86_64LinuxGnu).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    assert!(keel_codegen::compile_program(&module, keel_ir::Target::X86_64LinuxGnu).is_err());
}
