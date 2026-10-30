//! Standard BW mask APIs preserve scalar masks, packed flags and exact accesses.
#[path = "../../../keel-codegen/tests/common/avx512_bw_mask_alu.rs"]
mod common;
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::AllocationTier,
    x86_features::{FeatureSelector, Features},
};
use keel_ir::Target;
#[test]
fn bw_mask_alu_c_vendor_scalar_single_evaluation_and_memory_canaries() {
    let (source, harness) = common::fixtures(true);
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    common::run(&module, &harness, "c");
}
#[test]
fn bw_mask_alu_c_feature_and_unsigned_immediate_boundaries() {
    let (source, _) = common::fixtures(true);
    let source = source.replace("__attribute__((target(\"avx512bw\"))) ", "");
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
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
    for bits in [32, 64] {
        for control in ["-1", "256", "count"] {
            let source = format!(
                "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) __mmask{bits} bad(__mmask{bits} a,int count){{return _kshiftli_mask{bits}(a,{control});}}"
            );
            assert!(
                parse(&source, Target::X86_64LinuxGnu).is_err(),
                "{bits} {control}"
            );
        }
        let source = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) __mmask{bits} wrapped(__mmask{bits} a){{return _kshiftli_mask{bits}(a,0x100000000ULL);}}"
        );
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512f").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        keel_codegen::x86_64::compile_program_with_pipeline(
            &module,
            &keel_codegen::x86_64::REGISTERS,
            &pipeline,
        )
        .unwrap();
    }
    // Scalar conveniences intentionally remain usable without any AVX-512 permission.
    let source = "#include <immintrin.h>\nunsigned long long scalar(__mmask64*p,unsigned long long a){_store_mask64(p,_cvtu64_mask64(a));return _cvtmask64_u64(_load_mask64(p));}\nunsigned int scalar32(__mmask32*p,unsigned int a){_store_mask32(p,_cvtu32_mask32(a));return _cvtmask32_u32(_load_mask32(p));}";
    let module = parse(source, Target::X86_64LinuxGnu).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    keel_codegen::x86_64::compile_program_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
}
