//! BW public compare/test/conversion names retain every scalar mask bit.
#[path = "../../../keel-codegen/tests/common/avx512_bw_mask.rs"]
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
        let bits = case.vector.lane_type().bytes().unwrap() * 8;
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            512 => "_mm512",
            _ => unreachable!(),
        };
        let vector = format!("__m{width}i");
        let name = case.name();
        let features = if width == 512 {
            "avx512bw"
        } else {
            "avx512bw,avx512vl"
        };
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
            "__attribute__((target(\"{features}\"))) void {name}(const void*pa,const void*pb,void*out,uint64_t k){{{vector} a={},b={};",
            load("pa"),
            load("pb")
        );
        let api = common::api(*case);
        let b = if case.alias { "a" } else { "b" };
        if case.operation == 5 {
            source += &format!("{vector} result={api}(k);");
            source += &store("out", "result");
            source += &format!("*(uint64_t*)((char*)out+256)={prefix}_movepi{bits}_mask(result);");
        } else {
            let args = match case.operation {
                0 | 1 => format!("k,a,{b},{}", case.predicate),
                2 | 3 => format!("k,a,{b}"),
                4 => "a".into(),
                _ => unreachable!(),
            };
            let mut invocation = format!("{api}({args})");
            // Named predicates are convenience aliases of the generic controls.
            if case.operation < 2 {
                let suffix = match case.predicate {
                    0 => Some("eq"),
                    1 => Some("lt"),
                    2 => Some("le"),
                    4 => Some("neq"),
                    5 => Some("ge"),
                    6 => Some("gt"),
                    _ => None,
                };
                if let Some(suffix) = suffix {
                    invocation = format!(
                        "{}(k,a,{b})",
                        api.replace("_mask_cmp_", &format!("_mask_cmp{suffix}_"))
                    );
                }
            }
            source += &format!("uint64_t answer={invocation};");
            if case.operation < 4 {
                let unmasked = api.replacen("_mask_", "_", 1);
                let args = if case.operation < 2 {
                    format!("a,{b},{}", case.predicate)
                } else {
                    format!("a,{b}")
                };
                let all = if case.vector.lanes() == 64 {
                    "~0ULL".into()
                } else {
                    format!("((1ULL<<{})-1)", case.vector.lanes())
                };
                source += &format!("if(k=={all})answer={unmasked}({args});");
            }
            source += "*(uint64_t*)out=answer;";
            source += &format!("{vector} selected={prefix}_mask_mov_epi{bits}(a,answer,b);");
            source += &store("(char*)out+256", "selected");
        }
        source += &store("(char*)out+64", "a");
        source += &store("(char*)out+128", "b");
        source += "*(uint64_t*)((char*)out+192)=k;}\n";
    }
    source
}
#[test]
fn bw_mask_c_all_predicates_and_scalar_mask_chains() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-bw-mask-{}", std::process::id()));
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
fn bw_mask_c_predicate_controls_are_bounded() {
    for predicate in ["-1", "8", "255", "p"] {
        let source = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512bw\"))) unsigned long long f(__m512i a,__m512i b,int p){{return _mm512_mask_cmp_epi8_mask(~0ULL,a,b,{predicate});}}"
        );
        assert!(
            parse(&source, Target::X86_64LinuxGnu).is_err(),
            "{predicate}"
        );
    }
    let disabled=parse("#include <immintrin.h>\nunsigned long long f(__m512i a,__m512i b){return _mm512_mask_test_epi8_mask(~0ULL,a,b);}",Target::X86_64LinuxGnu).unwrap();
    assert!(keel_codegen::compile_program(&disabled, Target::X86_64LinuxGnu).is_err());
}
