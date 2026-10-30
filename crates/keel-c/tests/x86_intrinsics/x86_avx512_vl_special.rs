//! All packed VL special wrappers include mask-zero FIXUP's independent input a.
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
#[test]
fn vl_special_wrappers_match_bits_flags_and_feature_contract() {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\ntypedef void(*function)(void*,void*,void*,void*,void*,uint8_t);\nstruct testcase{function run,reference;unsigned bytes,bits;const char*name;};\n",
    );
    let mut cases = String::new();
    for (width, prefix) in [(128, "_mm"), (256, "_mm256")] {
        for op in [
            "getexp",
            "getmant",
            "scalef",
            "rcp14",
            "rsqrt14",
            "roundscale",
            "fixupimm",
        ] {
            for suffix in ["ps", "pd"] {
                for kind in ["plain", "mask", "maskz"] {
                    let bits = if suffix == "ps" { 32 } else { 64 };
                    let vector = if bits == 32 {
                        format!("__m{width}")
                    } else {
                        format!("__m{width}d")
                    };
                    let immediates: &[u32] = match op {
                        "getmant" => &[0, 5, 10, 15],
                        "roundscale" => &[0, 3, 0x48, 255],
                        "fixupimm" => &[0, 255],
                        _ => &[0],
                    };
                    for immediate in immediates {
                        let name = format!("consumer_{op}{suffix}{width}_{kind}_{immediate}");
                        let extra = match op {
                            "getmant" => format!(",{},{}", immediate & 3, immediate >> 2),
                            "roundscale" => format!(",{immediate}"),
                            "fixupimm" => format!(",b,t,{immediate}"),
                            "scalef" => ",b".to_string(),
                            _ => String::new(),
                        };
                        let expression = if op == "fixupimm" {
                            match kind {
                                "plain" => format!("{prefix}_{op}_{suffix}(a{extra})"),
                                "mask" => format!("{prefix}_mask_{op}_{suffix}(a,k{extra})"),
                                _ => format!("{prefix}_maskz_{op}_{suffix}(k,a{extra})"),
                            }
                        } else {
                            match kind {
                                "plain" => format!("{prefix}_{op}_{suffix}(a{extra})"),
                                "mask" => format!("{prefix}_mask_{op}_{suffix}(s,k,a{extra})"),
                                _ => format!("{prefix}_maskz_{op}_{suffix}(k,a{extra})"),
                            }
                        };
                        let body = format!(
                            "{vector} s=*(const {vector}*)sp,a=*(const {vector}*)ap,b=*(const {vector}*)bp;__m{width}i t=*(const __m{width}i*)tp;{prefix}_storeu_{suffix}(out,{expression});"
                        );
                        source += &format!(
                            "__attribute__((target(\"avx512vl\"))) void {name}(void*sp,void*ap,void*bp,void*tp,void*out,__mmask8 k){{{body}}}\n"
                        );
                        oracle += &format!(
                            "extern void {name}(void*,void*,void*,void*,void*,uint8_t);\nstatic void reference_{name}(void*sp,void*ap,void*bp,void*tp,void*out,__mmask8 k){{{body}}}\n"
                        );
                        cases += &format!(
                            "{{{name},reference_{name},{},{bits},\"{name}\"}},\n",
                            width / 8
                        );
                    }
                }
            }
        }
    }
    oracle += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    oracle += r#"
static const uint32_t edges32[]={0,0x80000000u,0x3f800000u,0xc0000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1,0x80000001u,0x00800000u,0x3f000000u,0xbf000000u,0x3fc00000u,0x40200000u,0xc0200000u};
static const uint64_t edges64[]={0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xc000000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0123456789abcULL,1,0x8000000000000001ULL,0x0010000000000000ULL,0x3fe0000000000000ULL,0xbfe0000000000000ULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x4004000000000000ULL,0xc004000000000000ULL};
int main(void){unsigned saved=_mm_getcsr();for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase tc=cases[ci];uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),table[8] __attribute__((aligned(64)));unsigned char out[64],want[64];unsigned lane_bytes=tc.bits/8,lanes=tc.bytes/lane_bytes;uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<lanes)-1)};for(unsigned sample=0;sample<16;sample++){for(unsigned lane=0;lane<64/lane_bytes;lane++){if(tc.bits==32){((uint32_t*)s)[lane]=edges32[(lane+sample+9)%16];((uint32_t*)a)[lane]=edges32[(lane+sample)%16];((uint32_t*)b)[lane]=edges32[(lane+sample+3)%16];((uint32_t*)table)[lane]=sample*0x11111111u;}else{s[lane]=edges64[(lane+sample+9)%16];a[lane]=edges64[(lane+sample)%16];b[lane]=edges64[(lane+sample+3)%16];table[lane]=(uint64_t)(sample*0x11111111u)|UINT64_C(0xabcdef0000000000);}}for(unsigned rounding=0;rounding<4;rounding++)for(unsigned denormal=0;denormal<4;denormal++)for(unsigned mi=0;mi<7;mi++){unsigned csr=0x1f80|(rounding<<13)|((denormal&1)<<6)|((denormal>>1)<<15);memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);_mm_setcsr(csr);tc.reference(s,a,b,table,want,masks[mi]);unsigned expected=_mm_getcsr();_mm_setcsr(csr);tc.run(s,a,b,table,out,masks[mi]);unsigned actual=_mm_getcsr();if(actual!=expected||memcmp(out,want,sizeof out)){fprintf(stderr,"%s sample=%u rounding=%u denormal=%u mask=%x csr=%x expected=%x\n",tc.name,sample,rounding,denormal,masks[mi],actual,expected);return 1;}}}}_mm_setcsr(saved);return 0;}
"#;
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.fma && !features.avx512dq);
    let missing = parse(
        &source.replace("__attribute__((target(\"avx512vl\"))) ", ""),
        Target::X86_64LinuxGnu,
    )
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: base,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&missing, &EMPTY, &pipeline).is_err()
    );
    let directory = std::env::temp_dir().join(format!("keel-c-vl-special-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("special.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512vl") {
                eprintln!("SKIP native C VL special: unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-mavx512f", "-mavx512vl", "-frounding-math"])
                .arg(directory.join("oracle.c"))
                .arg(directory.join("special.o"))
                .arg("-o")
                .arg(directory.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(
                Command::new(directory.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}
