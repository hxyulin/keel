//! Public BW word-shift wrappers preserve raw saturating count semantics.
#[path = "../../../keel-codegen/tests/common/avx512_bw_shift.rs"]
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
            u32::from(case.vector.lanes()).max(8)
        );
        let width = case.vector.bytes() * 8;
        let vector = format!("__m{width}i");
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            512 => "_mm512",
            _ => unreachable!(),
        };
        let lane = if width == 512 {
            "__keel_bw_i16_512".into()
        } else {
            format!("__keel_i16_{width}")
        };
        let features = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
        let name = case.name();
        let load = |p: &str| {
            if width == 512 {
                format!("_mm512_loadu_si512({p})")
            } else {
                format!("{prefix}_loadu_si{width}((const {vector}*){p})")
            }
        };
        let store = |p: &str, value: &str| {
            if width == 512 {
                format!("_mm512_storeu_si512({p},{value});")
            } else {
                format!("{prefix}_storeu_si{width}(({vector}*)({p}),{value});")
            }
        };
        source += &format!(
            "__attribute__((target(\"{features}\"))) void {name}(const void*pm,const void*pa,const void*pc,void*out,uint64_t k){{{vector} m={},a={};",
            load("pm"),
            load("pa")
        );
        if case.kind == 1 {
            source += "__m128i count=_mm_loadu_si128((const __m128i*)pc);";
        } else {
            source += &format!("{vector} count={};", load("pc"));
        }
        let merge = if case.alias { "a" } else { "m" };
        let amount = match case.kind {
            0 => case.immediate.to_string(),
            1 => "count".into(),
            2 => {
                if case.alias {
                    "a".into()
                } else {
                    "count".into()
                }
            }
            _ => unreachable!(),
        };
        let api = common::api(*case);
        source += &format!("{vector} result={api}({merge},k,a,{amount});");
        if width == 512 || case.kind == 2 {
            let all = format!("((1ULL<<{})-1)", case.vector.lanes());
            source += &format!(
                "if(k=={all})result={}(a,{amount});",
                api.replacen("_mask_", "_", 1)
            );
        }
        source += &format!(
            "{vector} zero={}(k,a,{amount});",
            api.replacen("_mask_", "_maskz_", 1)
        );
        source += &store("out", "result");
        source += &format!(
            "for(unsigned i=0;i<{};i++)if((((k>>i)&1)?(({lane})result)[i]:0)!=(({lane})zero)[i])((unsigned char*)out)[0]^=1;",
            case.vector.lanes()
        );
        source += &store("(char*)out+64", "m");
        source += &store("(char*)out+128", "a");
        if case.kind == 1 {
            source += "_mm_storeu_si128((__m128i*)((char*)out+192),count);";
        } else {
            source += &store("(char*)out+192", "count");
        }
        source += "*(uint64_t*)((char*)out+256)=k;}\n";
    }
    source
}
#[test]
fn bw_shift_c_counts_masks_aliases_and_live_payloads() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-bw-shift-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
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
fn bw_shift_c_immediate_and_permission_boundaries() {
    for count in ["-1", "256", "p"] {
        let source = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) __m512i f(__m512i a,int p){{return _mm512_mask_slli_epi16(a,-1,a,{count});}}"
        );
        assert!(parse(&source, Target::X86_64LinuxGnu).is_err(), "{count}");
    }
    for features in [
        "",
        "__attribute__((target(\"avx512f,avx512vl\")))",
        "__attribute__((target(\"avx512bw\")))",
    ] {
        let source = format!(
            "#include <immintrin.h>\n{features} __m128i f(__m128i a){{return _mm_mask_slli_epi16(a,-1,a,1);}}"
        );
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        assert!(
            keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).is_err(),
            "{features}"
        );
    }
}
