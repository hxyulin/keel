//! C BW convenience APIs retain scalar casts and single source evaluation.
#[path = "../../../keel-codegen/tests/common/avx512_bw_misc.rs"]
mod common;
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
#[test]
fn bw_misc_c_scalar_and_vendor_oracles_low_xmm_sources_and_spills() {
    let cases = common::cases();
    let (source, harness) = common::sources(&cases, true);
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    common::run(&module, &harness, "c");
}
#[test]
fn bw_misc_c_controls_maskz_and_function_features() {
    for width in [128, 256, 512] {
        let p = if width == 128 {
            "_mm"
        } else if width == 256 {
            "_mm256"
        } else {
            "_mm512"
        };
        for control in ["-1", "256", "count"] {
            let source = format!(
                "#include <immintrin.h>\n__attribute__((target(\"avx512bw,avx512vl\"))) __m{width}i bad(__m{width}i a,__m{width}i b,int count){{return {p}_dbsad_epu8(a,b,{control});}}"
            );
            assert!(parse(&source, Target::X86_64LinuxGnu).is_err());
        }
    }
    let cases = common::cases();
    let (source, _) = common::sources(&cases, true);
    let module = parse(
        &source
            .replace("__attribute__((target(\"avx512bw\"))) ", "")
            .replace("__attribute__((target(\"avx512bw,avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE
            .with_features("avx512f,avx512dq,avx512vl")
            .unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(
            &module,
            &keel_codegen::x86_64::REGISTERS,
            &pipeline
        )
        .is_err()
    );
}
