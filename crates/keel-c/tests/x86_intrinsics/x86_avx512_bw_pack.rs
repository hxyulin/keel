//! Public BW packs saturate signed inputs and preserve 128-bit block ordering.
#[path = "../../../keel-codegen/tests/common/avx512_bw_pack.rs"]
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
            u32::from(case.result.lanes()).max(8)
        );
        let width = case.result.bytes() * 8;
        let bits = case.result.lane_type().bytes().unwrap() * 8;
        let vector = format!("__m{width}i");
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            512 => "_mm512",
            _ => unreachable!(),
        };
        let lane = if width == 512 {
            if bits < 32 {
                format!("__keel_bw_i{bits}_512")
            } else {
                "__keel_avx512_i32x16".into()
            }
        } else {
            format!("__keel_i{bits}_{width}")
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
            "__attribute__((target(\"{features}\"))) void {name}(const void*pm,const void*pa,const void*pb,void*out,uint64_t k){{{vector} m={},a={},b={};",
            load("pm"),
            load("pa"),
            load("pb")
        );
        let merge = if case.alias == 2 { "a" } else { "m" };
        let b = if case.alias == 1 { "a" } else { "b" };
        let api = common::api(*case);
        source += &format!("{vector} result={api}({merge},k,a,{b});");
        if width == 512 {
            let all = if case.result.lanes() == 64 {
                "~0ULL".into()
            } else {
                format!("((1ULL<<{})-1)", case.result.lanes())
            };
            source += &format!(
                "if(k=={all})result={}(a,{b});",
                api.replacen("_mask_", "_", 1)
            );
        }
        source += &format!(
            "{vector} zero={}(k,a,{b});",
            api.replacen("_mask_", "_maskz_", 1)
        );
        source += &store("out", "result");
        source += &format!(
            "for(unsigned i=0;i<{};i++)if((((k>>i)&1)?(({lane})result)[i]:0)!=(({lane})zero)[i])((unsigned char*)out)[0]^=1;",
            case.result.lanes()
        );
        source += &store("(char*)out+64", "m");
        source += &store("(char*)out+128", "a");
        source += &store("(char*)out+192", "b");
        source += "*(uint64_t*)((char*)out+256)=k;}\n";
    }
    source
}
#[test]
fn bw_pack_c_signed_saturation_and_lane_local_oracles() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-bw-pack-{}", std::process::id()));
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
fn bw_pack_c_requires_bw_and_narrow_vl() {
    for attributes in [
        "",
        "__attribute__((target(\"avx512f,avx512vl\")))",
        "__attribute__((target(\"avx512bw\")))",
    ] {
        let source = format!(
            "#include <immintrin.h>\n{attributes} __m128i f(__m128i a,__m128i b){{return _mm_mask_packus_epi32(a,-1,a,b);}}"
        );
        let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
        assert!(
            keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).is_err(),
            "{attributes}"
        );
    }
}
