//! Public BW conversions preserve short tails, exact memory widths and source state.
#[path = "../../../keel-codegen/tests/common/avx512_bw_convert.rs"]
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
        assert_eq!(
            case.mask.bytes().unwrap() * 8,
            u32::from(case.encoded / 16).max(8)
        );
        let vector = |bytes: u32| format!("__m{}i", bytes * 8);
        let load = |p: &str, bytes: u32| {
            if bytes == 64 {
                format!("_mm512_loadu_si512({p})")
            } else {
                let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
                format!(
                    "{prefix}_loadu_si{}((const {}*){p})",
                    bytes * 8,
                    vector(bytes)
                )
            }
        };
        let store = |p: &str, v: &str, bytes: u32| {
            if bytes == 64 {
                format!("_mm512_storeu_si512({p},{v});")
            } else {
                let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
                format!(
                    "{prefix}_storeu_si{}(({}*)({p}),{v});",
                    bytes * 8,
                    vector(bytes)
                )
            }
        };
        let name = case.name();
        let api = common::api(*case);
        let input = case.input().bytes();
        let output = case.output().bytes();
        let features = if case.encoded == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        source += &format!(
            "__attribute__((target(\"{features}\"))) void {name}(const void*pm,void*pa,void*out,uint64_t k){{"
        );
        if case.kind == 3 {
            source += &format!("{} a={};{api}(pa,k,a);", vector(input), load("pm", input));
            source += &store("(char*)out+64", "a", input);
        } else {
            source += &format!("{} m={};", vector(output), load("pm", output));
            let arg = if case.kind == 1 {
                "pa"
            } else {
                let p = if case.alias { "pm" } else { "pa" };
                source += &format!("{} a={};", vector(input), load(p, input));
                "a"
            };
            source += &format!("{} r={api}(m,k,{arg});", vector(output));
            if case.kind != 1 && (case.operation >= 2 || case.encoded == 512) {
                let all = format!("((1ULL<<{})-1)", case.encoded / 16);
                source += &format!("if(k=={all})r={}(a);", api.replacen("_mask_", "_", 1));
            }
            source += &format!(
                "{} z={}(k,{arg});",
                vector(output),
                api.replacen("_mask_", "_maskz_", 1)
            );
            source += &store("out", "r", output);
            let lane = if case.operation < 2 {
                if case.encoded == 512 {
                    "__keel_bw_i16_512".into()
                } else {
                    format!("__keel_i16_{}", case.encoded)
                }
            } else {
                if output == 16 {
                    "__keel_i8_128".into()
                } else {
                    "__keel_i8_256".into()
                }
            };
            let capacity = if case.operation < 2 {
                output / 2
            } else {
                output
            };
            source += &format!(
                "for(unsigned i=0;i<{capacity};i++)if(((i<{}&&((k>>i)&1))?(({lane})r)[i]:0)!=(({lane})z)[i])((unsigned char*)out)[0]^=1;",
                case.encoded / 16
            );
            source += &store("(char*)out+64", "m", output);
            if case.kind != 1 {
                source += &store("(char*)out+128", "a", input);
            }
        }
        source += "*(uint64_t*)((char*)out+192)=k;}\n";
    }
    source
}
#[test]
fn bw_convert_c_short_results_masked_memory_and_saturation_oracles() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = common::directory("keel-c-bw-convert");
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    let native =
        std::is_x86_feature_detected!("avx512bw") && std::is_x86_feature_detected!("avx512vl");
    #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
    let native = false;
    if native {
        let output = Command::new("clang")
            .arg("-O2")
            .arg("-c")
            .arg(dir.join("runner.c"))
            .arg("-o")
            .arg(dir.join("runner.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for optimize in [false, true] {
        let mut module = raw.clone();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        keel_ir::verify_module(&module).unwrap();
        let encoding = module_features(&module, Features::NONE).unwrap();
        assert!(encoding.avx512bw && !encoding.avx2 && !encoding.avx512dq);
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE,
        };
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for (spilled, pools) in [(false, &keel_codegen::x86_64::REGISTERS), (true, &EMPTY)] {
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
                let name = format!("{allocation:?}-{spilled}-{optimize}");
                let path = dir.join(format!("{name}.o"));
                fs::write(&path, object).unwrap();
                if native {
                    let exe = dir.join(name);
                    let linked = Command::new("clang")
                        .arg(dir.join("runner.o"))
                        .arg(path)
                        .arg("-o")
                        .arg(&exe)
                        .output()
                        .unwrap();
                    assert!(
                        linked.status.success(),
                        "{}",
                        String::from_utf8_lossy(&linked.stderr)
                    );
                    let output = Command::new(exe).output().unwrap();
                    assert!(
                        output.status.success(),
                        "{allocation:?} spill={spilled} optimized={optimize}: {output:?}"
                    );
                }
            }
        }
    }
}
#[test]
fn bw_convert_c_512_short_operands_need_no_vl() {
    let source = "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) void f(__m256i*s,__m512i*w,__m256i*b,void*p){*w=_mm512_mask_cvtepi8_epi16(*w,1,*b);*s=_mm512_mask_cvtusepi16_epi8(*s,1,*w);_mm512_mask_cvtepi16_storeu_epi8(p,1,*w);}";
    let module = parse(source, Target::X86_64LinuxGnu).unwrap();
    let encoding = module_features(&module, Features::NONE).unwrap();
    assert!(encoding.avx512bw && !encoding.avx512vl && !encoding.avx2);
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::x86_64::compile_program_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &pipeline,
    )
    .unwrap();
    keel_asm::x86::assemble_program_object_with_features(&program, encoding).unwrap();
    for attributes in [
        "",
        "__attribute__((target(\"avx512f,avx512vl\")))",
        "__attribute__((target(\"avx512bw\")))",
    ] {
        let source = format!(
            "#include <immintrin.h>\n{attributes} __m128i f(__m128i a){{return _mm_mask_cvtsepi16_epi8(a,-1,a);}}"
        );
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        assert!(
            keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).is_err(),
            "{attributes}"
        );
    }
}
