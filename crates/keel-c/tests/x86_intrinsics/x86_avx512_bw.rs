//! Standard BW wrappers share the independent scalar and protected-memory oracle.
#[path = "../../../keel-codegen/tests/common/avx512_bw.rs"]
mod common;
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features, module_features},
};
use keel_ir::{Target, intrinsic::X86Avx512BwKind};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source(cases: &[common::Case]) -> String {
    let mut source = String::from("#include <immintrin.h>\n");
    for case in cases {
        let d = case.id.x86_avx512_bw_core().unwrap();
        let width = d.vector.bytes() * 8;
        let bits = d.vector.lane_type().bytes().unwrap() * 8;
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            512 => "_mm512",
            _ => unreachable!(),
        };
        let vector = format!("__m{width}i");
        let lane_alias = if width == 512 {
            format!("__keel_bw_i{bits}_512")
        } else {
            format!("__keel_i{bits}_{width}")
        };
        let operation = match case.operation() {
            "add" => format!("add_epi{bits}"),
            "sub" => format!("sub_epi{bits}"),
            "adds" => format!("adds_epi{bits}"),
            "subs" => format!("subs_epi{bits}"),
            "addus" => format!("adds_epu{bits}"),
            "subus" => format!("subs_epu{bits}"),
            "mins" => format!("min_epi{bits}"),
            "minu" => format!("min_epu{bits}"),
            "maxs" => format!("max_epi{bits}"),
            "maxu" => format!("max_epu{bits}"),
            "move" => format!("mov_epi{bits}"),
            "load" => format!("loadu_epi{bits}"),
            "store" => format!("storeu_epi{bits}"),
            _ => unreachable!(),
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
            "__attribute__((target(\"{features}\"))) void {name}(const void*pm,const void*pa,const void*pb,void*out,unsigned long long k){{ {vector} m={};",
            load("pm")
        );
        let (merge, a, b) = if case.alias {
            ("a", "m", "a")
        } else {
            ("m", "a", "b")
        };
        if matches!(d.kind, X86Avx512BwKind::Binary | X86Avx512BwKind::Move) {
            source += &format!("{vector} a={},b={};", load("pa"), load("pb"));
        }
        let masked = format!("{prefix}_mask_{operation}");
        let zero = format!("{prefix}_maskz_{operation}");
        let all = if d.vector.lanes() == 64 {
            "~0ULL".into()
        } else {
            format!("((1ULL<<{})-1)", d.vector.lanes())
        };
        let arguments = match d.kind {
            X86Avx512BwKind::Binary => format!("{merge},k,{a},{b}"),
            X86Avx512BwKind::Move => "m,k,a".into(),
            X86Avx512BwKind::Load => "m,k,pa".into(),
            X86Avx512BwKind::Store => "pa,k,m".into(),
        };
        if d.kind == X86Avx512BwKind::Store {
            if width == 512 {
                source += &format!("if(k=={all}){prefix}_{operation}(pa,m);else ");
            }
            source += &format!("{masked}({arguments});");
        } else {
            source += &format!("{vector} r={masked}({arguments});");
            if width == 512 && d.kind != X86Avx512BwKind::Move {
                let args = if d.kind == X86Avx512BwKind::Binary {
                    format!("{a},{b}")
                } else {
                    "pa".into()
                };
                source += &format!("if(k=={all})r={prefix}_{operation}({args});");
            }
            let args = match d.kind {
                X86Avx512BwKind::Binary => format!("k,{a},{b}"),
                X86Avx512BwKind::Move => "k,a".into(),
                _ => "k,pa".into(),
            };
            source += &format!("{vector} z={zero}({args});");
            source += &store("out", "r");
            source += &format!(
                "for(unsigned i=0;i<{};i++)if((((k>>i)&1)?(({lane_alias})r)[i]:0)!=(({lane_alias})z)[i])((unsigned char*)out)[0]^=1;",
                d.vector.lanes()
            );
        }
        source += &store("(char*)out+64", "m");
        if matches!(d.kind, X86Avx512BwKind::Binary | X86Avx512BwKind::Move) {
            source += &store("(char*)out+128", "a");
            source += &store("(char*)out+192", "b");
        }
        source += "*(unsigned long long*)((char*)out+256)=k;}\n";
    }
    source
}
#[test]
fn bw_c_wrappers_mask_width_scalar_and_memory_oracles() {
    let cases = common::cases();
    let raw = parse(&source(&cases), Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-bw-core-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("runner.c"), common::runner(&cases)).unwrap();
    for optimize in [false, true] {
        let mut module = raw.clone();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
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
                #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
                if std::is_x86_feature_detected!("avx512bw")
                    && std::is_x86_feature_detected!("avx512vl")
                {
                    let exe = dir.join(name);
                    let linked = Command::new("cc")
                        .arg(dir.join("runner.c"))
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
    let disabled=parse("#include <immintrin.h>\nvoid f(__m512i*p,__m512i*a,__m512i*b){*p=_mm512_mask_add_epi8(*p,~0ULL,*a,*b);}", Target::X86_64LinuxGnu).unwrap();
    assert!(keel_codegen::compile_program(&disabled, Target::X86_64LinuxGnu).is_err());
}
