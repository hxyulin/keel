//! Standard narrowing interfaces preserve zero tails and exact masked stores.
#[path = "../../../keel-codegen/tests/common/avx512_vl_narrow.rs"]
mod common;
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::Target;
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source(cases: &[common::Case]) -> String {
    let mut source = String::from("#include <immintrin.h>\n#include <stdint.h>\n");
    for case in cases {
        let name = case.name();
        let api = case.api();
        let width = case.source().bytes() * 8;
        assert_eq!(case.result().bytes(), 16);
        source += &format!(
            "__attribute__((target(\"avx512f,avx512vl\"))) void {name}(const void*pm,void*pa,void*out,uint64_t k){{"
        );
        let prefix = if width == 128 { "_mm" } else { "_mm256" };
        let pointer = if case.store || case.mode == 1 {
            "pm"
        } else {
            "pa"
        };
        source +=
            &format!("__m{width}i a={prefix}_loadu_si{width}((const __m{width}i*){pointer});");
        if case.store {
            source += &format!(
                "{api}(pa,k,a);{prefix}_storeu_si{width}((__m{width}i*)((char*)out+64),a);"
            );
        } else {
            source += "__m128i m=_mm_loadu_si128((const __m128i*)pm);";
            if case.mode == 1 {
                source += "__m512i live=_mm512_loadu_si512(pm);";
            }
            source += &match case.mode {
                0 | 1 => format!("__m128i r={api}(m,k,a);"),
                2 => format!("__m128i r={api}(k,a);"),
                3 => format!("__m128i r={api}(a);"),
                _ => unreachable!(),
            };
            source +=
                "_mm_storeu_si128((__m128i*)out,r);_mm_storeu_si128((__m128i*)((char*)out+64),m);";
            source += &format!("{prefix}_storeu_si{width}((__m{width}i*)((char*)out+128),a);");
            if case.mode == 1 {
                source += "_mm512_storeu_si512((char*)out+256,live);";
            }
        }
        source += "*(uint64_t*)((char*)out+192)=k;}\n";
    }
    source
}
#[test]
fn vl_narrow_c_all_interfaces_saturation_tails_and_guard_stores() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let expected: std::collections::HashSet<_> = cases.iter().map(|case| case.id()).collect();
    let mut actual = std::collections::HashSet::new();
    for function in &raw.functions {
        for block in function.blocks() {
            for &inst in function.block_insts(block) {
                if let keel_ir::InstData::Intrinsic { id, .. } = function.inst_data(inst)
                    && id.x86_avx512_vl_narrow().is_some()
                {
                    actual.insert(*id);
                }
            }
        }
    }
    assert_eq!(actual, expected);
    let dir = common::directory("keel-vl-narrow-c");
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native =
        std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        let result = Command::new("clang")
            .args(["-O2", "-c"])
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    for optimized in [false, true] {
        let mut module = raw.clone();
        if optimized {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let encoding = module_features(&module, Features::NONE).unwrap();
        assert!(
            encoding.avx512f
                && encoding.avx512vl
                && !encoding.avx512bw
                && !encoding.avx512dq
                && !encoding.avx2
        );
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (label, pools) in [
                ("normal", &keel_codegen::x86_64::REGISTERS),
                ("spill", &EMPTY),
            ] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocation,
                };
                let text =
                    keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
                let program =
                    keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                        .unwrap();
                let object =
                    keel_asm::x86::assemble_program_object_with_features(&program, encoding)
                        .unwrap();
                assert_eq!(
                    object,
                    keel_asm::x86::assemble_object_with_features(&text, encoding).unwrap()
                );
                let name = format!("{optimized}-{allocation:?}-{label}");
                let path = dir.join(format!("{name}.o"));
                fs::write(&path, object).unwrap();
                if native {
                    let exe = dir.join(name);
                    let result = Command::new("clang")
                        .arg(dir.join("runner.o"))
                        .arg(path)
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    let result = Command::new(exe).output().unwrap();
                    assert!(
                        result.status.success(),
                        "{optimized} {allocation:?} {label}: {result:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn vl_narrow_c_requires_f_and_vl_not_bw_or_dq() {
    let body = "void f(__m128i*s,__m256i*a,void*p){*s=_mm256_mask_cvtusepi64_epi8(*s,0xff,*a);_mm_mask_cvtsepi32_storeu_epi16(p,0xff,*s);}";
    for features in ["", "avx512f", "avx512bw", "avx512dq"] {
        let attributes = if features.is_empty() {
            String::new()
        } else {
            format!("__attribute__((target(\"{features}\")))")
        };
        let source = format!("#include <immintrin.h>\n{attributes} {body}");
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        assert!(
            keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).is_err(),
            "{features}"
        );
    }
    let source =
        format!("#include <immintrin.h>\n__attribute__((target(\"avx512f,avx512vl\"))) {body}");
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(!encoding.avx512bw && !encoding.avx512dq);
    let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, encoding).unwrap();
}
