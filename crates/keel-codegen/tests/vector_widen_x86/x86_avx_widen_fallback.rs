//! AVX transport does not make 128-bit SSE widening recipes valid for YMM results.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, path::PathBuf};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const TINY: Pools = Pools {
    caller_saved: &[7],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
const MINIMAL: &str = "func @minimal(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n v2 = load.i32x4 align 1 v0\n v3 = swiden_low.i64x4 v2\n store align 1 v3,v1\n return\n}\n";

fn profiles() -> Vec<(&'static str, Features)> {
    let explicit = |level| Features {
        avx: true,
        xsave: true,
        osxsave: true,
        ..Features::for_level(level)
    };
    vec![
        ("v1+avx", explicit(CpuLevel::V1)),
        ("v2+avx", explicit(CpuLevel::V2)),
        (
            "v3-avx2",
            Features {
                avx2: false,
                ..Features::for_level(CpuLevel::V3)
            },
        ),
        ("v3", Features::for_level(CpuLevel::V3)),
    ]
}

fn compile(
    source: &str,
    features: Features,
    s: SelectionTier,
    a: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &s,
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0);
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
        &module,
        pools,
        &pipeline,
        &mut |p| chunks.items.extend(p.items),
    )
    .unwrap();
    for p in [&program, &chunks] {
        for item in &p.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(features.contains(i.required_features()), "{i}");
            }
        }
    }
    let object = keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_program_object_with_features(&chunks, features).unwrap()
    );
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
    );
    (text, object)
}

fn export(stem: &str, ir: &str, c: &str, text: &str, object: &[u8]) {
    let dir = std::env::var_os("KEEL_AVX_FALLBACK_EVIDENCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-avx-fallback-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    for (ext, bytes) in [
        ("keel", ir.as_bytes()),
        ("c", c.as_bytes()),
        ("s", text.as_bytes()),
        ("o", object),
    ] {
        fs::write(dir.join(format!("{stem}.{ext}")), bytes).unwrap();
    }
    fs::write(
        dir.join("run_native.py"),
        include_str!("../x86_widen/run_native.py"),
    )
    .unwrap();
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    if std::is_x86_feature_detected!("avx2") {
        use std::process::Command;
        let exe = dir.join(format!("{stem}.native"));
        let build = Command::new("clang")
            .args([
                "-O2",
                "-march=x86-64",
                "-mno-avx",
                "-Wall",
                "-Wextra",
                "-Werror",
            ])
            .arg(dir.join(format!("{stem}.c")))
            .arg(dir.join(format!("{stem}.o")))
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(build.status.success(), "{build:?}");
        let run = Command::new(exe).output().unwrap();
        assert!(run.status.success(), "{stem}: {run:?}");
    }
}

#[test]
fn minimal_four_distinct_signed_lanes() {
    // A scalar C cast is the independent oracle; all four expected lanes differ.
    let c = "#include <stdint.h>\n#include <string.h>\nextern void minimal(const void*,void*);\nint main(void){int32_t x[4]={INT32_MIN,-1,17,INT32_MAX};int64_t y[4],e[4];for(unsigned i=0;i<4;i++)e[i]=(int64_t)x[i];minimal(x,y);return memcmp(y,e,sizeof e)?2:0;}\n";
    let (text, object) = compile(
        MINIMAL,
        profiles()[0].1,
        SelectionTier::Baseline,
        AllocationTier::Full,
        &keel_codegen::x86_64::REGISTERS,
    );
    // Also catch the incorrect retained SSE recipe on non-x86 test hosts.
    assert!(!text.contains("punpckldq"), "{text}");
    export("minimal", MINIMAL, c, &text, &object);
}

fn fixture() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(C_HEADER);
    let mut cases = String::from("static const struct Case cases[]={\n");
    // Every legal 128 -> 256 widening pair, including non-doubling widths.
    for (from, to) in [(8, 16), (8, 32), (8, 64), (16, 32), (16, 64), (32, 64)] {
        let src = format!("i{from}x{}", 128 / from);
        let dst = format!("i{to}x{}", 256 / to);
        for (op, signed, high) in [
            ("swiden_low", 1, 0),
            ("uwiden_low", 0, 0),
            ("swiden_high", 1, 1),
            ("uwiden_high", 0, 1),
        ] {
            let name = format!("{op}_{from}_{to}");
            ir += &format!(
                "func @{name}_abi({src})->{dst} {{\nblock0(v0:{src}):\n v1 = {op}.{dst} v0\n return v1\n}}\n"
            );
            for live in [false, true] {
                let run = format!("{name}_{}", if live { "live" } else { "direct" });
                let before = if live {
                    " v4 = load.i64x4 align 1 v1\n"
                } else {
                    ""
                };
                let convert = if live {
                    format!("call @{name}_abi(v2)")
                } else {
                    format!("{op}.{dst} v2")
                };
                let after = if live {
                    " v5 = iconst.i64 32\n v6 = ptradd v1,v5\n store align 1 v4,v6\n v7 = iconst.i64 64\n v8 = ptradd v1,v7\n store align 1 v2,v8\n"
                } else {
                    ""
                };
                ir += &format!(
                    "func @{run}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n{before} v2 = load.{src} align 1 v0\n v3 = {convert}\n store align 1 v3,v1\n{after} return\n}}\n"
                );
                c += &format!("extern void {run}(const void*,void*);\n");
                cases += &format!(
                    "{{\"{run}\",{run},{from},{to},{signed},{high},{}}},\n",
                    u8::from(live)
                );
            }
        }
    }
    cases += "};\n";
    (ir, c + &cases + C_ORACLE)
}

const C_HEADER: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
struct Case { const char*name; void(*run)(const void*,void*); unsigned from,to,sign,high,live; };
"#;
const C_ORACLE: &str = r#"
static int check(const struct Case*c,unsigned char*in,unsigned char*out) {
 unsigned char saved[16],before[32],expected[80];memcpy(saved,in,16);memcpy(before,out,32);
 unsigned lanes=256/c->to,base=c->high?128/c->from-lanes:0;
 for(unsigned i=0;i<lanes;i++) {
   uint64_t v=0;memcpy(&v,saved+(base+i)*(c->from/8),c->from/8);
   if(c->sign && (v&(UINT64_C(1)<<(c->from-1))))v|=UINT64_MAX<<c->from;
   memcpy(expected+i*(c->to/8),&v,c->to/8);
 }
 if(c->live){memcpy(expected+32,before,32);memcpy(expected+64,saved,16);}
 c->run(in,out);
 if(memcmp(out,expected,c->live?80:32)){printf("failed %s\n",c->name);return 1;}
 return 0;
}
int main(void) {
 if(!__builtin_cpu_supports("avx2"))return 77;
 for(unsigned ci=0;ci<sizeof(cases)/sizeof(cases[0]);ci++) {
  const struct Case*c=&cases[ci];unsigned size=c->live?80:32;
  for(unsigned pattern=0;pattern<260;pattern++)for(unsigned offset=0;offset<16;offset++)for(unsigned alias=0;alias<2;alias++) {
   unsigned char in[128],out[128];memset(in,0xa5,sizeof in);memset(out,0xa5,sizeof out);
   for(unsigned l=0;l<128/c->from;l++) {
    uint64_t edges[]={0,1,(UINT64_C(1)<<(c->from-1))-1,UINT64_C(1)<<(c->from-1),UINT64_MAX};
    uint64_t v=pattern<5?edges[(pattern+l)%5]:pattern+71*l+(UINT64_C(0x8123456789abcdef)>>(l%8));
    memcpy(in+16+offset+l*(c->from/8),&v,c->from/8);
   }
   unsigned char*dest=(alias?in:out)+16+offset;
   if(check(c,in+16+offset,dest))return 2;
   if(dest[-1]!=0xa5||dest[size]!=0xa5)return 3;
  }
  size_t n=(size_t)sysconf(_SC_PAGESIZE);
  unsigned char*a=mmap(0,3*n,3,0x22,-1,0),*b=mmap(0,3*n,3,0x22,-1,0);
  if(a==MAP_FAILED||b==MAP_FAILED)return 4;
  if(mprotect(a,n,0)||mprotect(a+2*n,n,0)||mprotect(b,n,0)||mprotect(b+2*n,n,0))return 5;
  for(unsigned edge=0;edge<2;edge++) {
   unsigned char*in=edge?a+2*n-16:a+n,*out=edge?b+2*n-size:b+n;
   for(unsigned i=0;i<16;i++)in[i]=(unsigned char)(128+31*i);
   memset(out,0xa5,size);if(check(c,in,out))return 6;
  }
  munmap(a,3*n);munmap(b,3*n);
 }
 puts("passed 48 widening cases: values/ABI/live-YMM/source/alias/spill/exact-guards");return 0;
}
"#;

#[test]
fn result_widths_features_abi_live_upper_halves_and_exact_footprints() {
    let (ir, c) = fixture();
    for (profile, features) in profiles() {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("tiny", &TINY),
                    ("empty", &EMPTY),
                ] {
                    let (text, object) = compile(&ir, features, s, a, pools);
                    assert!(text.contains("ymmword ptr"));
                    if features.avx2 {
                        assert!(text.contains("vpmovsxdq"), "{text}");
                    }
                    export(
                        &format!("{profile}-{s:?}-{a:?}-{pool}"),
                        &ir,
                        &c,
                        &text,
                        &object,
                    );
                }
            }
        }
    }
}

#[test]
fn strict_levels_reject_wide_values_and_bitcast_cannot_change_width() {
    for level in [CpuLevel::V1, CpuLevel::V2] {
        let module = keel_ir::parse(MINIMAL).unwrap();
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                let selector = FeatureSelector {
                    inner: &s,
                    features: Features::for_level(level),
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &a,
                };
                for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
                    assert!(
                        keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline)
                            .is_err()
                    );
                    assert!(
                        keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, pools, &pipeline
                        )
                        .is_err()
                    );
                    assert!(
                        keel_codegen::x86_64::compile_program_chunks_with_pipeline(
                            &module,
                            pools,
                            &pipeline,
                            &mut |_| {}
                        )
                        .is_err()
                    );
                }
            }
        }
    }
    let invalid = MINIMAL.replace("swiden_low", "bitcast");
    assert!(keel_ir::verify_module(&keel_ir::parse(&invalid).unwrap()).is_err());
}

#[test]
fn narrow_conversion_and_equal_width_bitcast_controls() {
    let mut ir = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut checks = String::new();
    for (from, to, lanes, source_lanes) in [
        ("i32x2", "i64x2", 2, 2),
        ("i32x4", "i64x2", 2, 4),
        ("i32x4", "i64x1", 1, 4),
    ] {
        for (op, signed, high) in [
            ("swiden_low", true, false),
            ("uwiden_low", false, false),
            ("swiden_high", true, true),
            ("uwiden_high", false, true),
        ] {
            let name = format!("control_{op}_{from}_{to}");
            ir += &format!(
                "func @{name}(ptr,ptr) {{\nblock0(v0:ptr,v1:ptr):\n v2 = load.{from} align 1 v0\n v3 = {op}.{to} v2\n store align 1 v3,v1\n return\n}}\n"
            );
            c += &format!("extern void {name}(const void*,void*);\n");
            let cast = if signed {
                "(uint64_t)(int64_t)(int32_t)"
            } else {
                "(uint64_t)"
            };
            let base = if high { source_lanes - lanes } else { 0 };
            checks += &format!(
                "{{uint32_t x[4]={{0x80000000U,17,0xffffffffU,0x7fffffffU}};uint64_t e[{lanes}];unsigned char out[34];memset(out,0xa5,sizeof out);for(unsigned i=0;i<{lanes};i++)e[i]={cast}x[i+{base}];{name}(x,out+1);if(memcmp(out+1,e,sizeof e)||out[0]!=0xa5||out[1+sizeof e]!=0xa5)return 2;}}\n"
            );
        }
    }
    ir += "func @wide_bits(ptr,ptr) {\nblock0(v0:ptr,v1:ptr):\n v2 = load.i32x8 align 1 v0\n v3 = bitcast.f64x4 v2\n store align 1 v3,v1\n return\n}\n";
    c += "extern void wide_bits(const void*,void*);\nint main(void){\n";
    c += &checks;
    c += "unsigned char in[32],out[34];for(unsigned i=0;i<32;i++)in[i]=(unsigned char)(i*71);memset(out,0xa5,sizeof out);wide_bits(in,out+1);return memcmp(in,out+1,32)||out[0]!=0xa5||out[33]!=0xa5;}\n";
    for (profile, features) in profiles() {
        for s in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for a in [AllocationTier::Fast, AllocationTier::Full] {
                for (pool, pools) in [
                    ("normal", &keel_codegen::x86_64::REGISTERS),
                    ("empty", &EMPTY),
                ] {
                    let (text, object) = compile(&ir, features, s, a, pools);
                    export(
                        &format!("controls-{profile}-{s:?}-{a:?}-{pool}"),
                        &ir,
                        &c,
                        &text,
                        &object,
                    );
                }
            }
        }
    }
}
