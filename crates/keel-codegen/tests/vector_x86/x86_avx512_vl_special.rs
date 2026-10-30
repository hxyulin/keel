//! Every legal immediate is compared with system intrinsics and raw MXCSR effects.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[0],
    float_callee_saved: &[],
};
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <math.h>\n#include <float.h>\ntypedef void(*function)(void*,void*,void*,void*,void*,uint8_t);\nstruct testcase{function run,reference;unsigned bytes,bits,operation,immediate,alias;const char*name;};\n",
    );
    let mut cases = String::new();
    for width in [128, 256] {
        for (operation, op) in [
            "getexp",
            "getmant",
            "scalef",
            "rcp14",
            "rsqrt14",
            "roundscale",
            "fixupimm",
        ]
        .into_iter()
        .enumerate()
        {
            for suffix in ["ps", "pd"] {
                let bits = if suffix == "ps" { 32 } else { 64 };
                let lanes = width / bits;
                let shape = format!("f{bits}x{lanes}");
                let table = format!("i{bits}x{lanes}");
                let vector = if bits == 32 {
                    format!("__m{width}")
                } else {
                    format!("__m{width}d")
                };
                let prefix = if width == 128 { "_mm" } else { "_mm256" };
                let bytes = width / 8;
                let immediates: Vec<u32> = match op {
                    "getmant" => (0..16).collect(),
                    "roundscale" | "fixupimm" => (0..256).collect(),
                    _ => (0..2).collect(),
                };
                for immediate in immediates {
                    let alias = immediate & 1;
                    let name = format!("{op}{suffix}{width}_{immediate}");
                    let a = if alias == 1 { "v6" } else { "v7" };
                    let mut arguments = format!("v6,v5,{a}");
                    if op == "fixupimm" {
                        arguments += ",v8,v9,v10";
                    } else if op == "scalef" {
                        arguments += ",v8";
                    } else if matches!(op, "getmant" | "roundscale") {
                        arguments += ",v10";
                    }
                    ir += &format!(
                        "func @{name}(ptr,ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i8):\n v6=load.{shape} align 1 v0\n v7=load.{shape} align 1 v1\n v8=load.{shape} align 1 v2\n v9=load.{table} align 1 v3\n v10=iconst.i32 {immediate}\n v11=intrinsic x86_avx512vl_{op}{suffix}{width}({arguments})\n store align 1 v11,v4\n v12=iconst.i64 64\n v13=ptradd v4,v12\n store align 1 v6,v13\n v14=ptradd v13,v12\n store align 1 v7,v14\n v15=ptradd v14,v12\n store align 1 v8,v15\n v16=ptradd v15,v12\n store align 1 v9,v16\n v17=ptradd v16,v12\n store align 1 v5,v17\n return\n}}\n"
                    );
                    let a = if alias == 1 { "s" } else { "a" };
                    let expression = match op {
                        "getmant" => format!(
                            "{prefix}_mask_getmant_{suffix}(s,k,{a},{},{})",
                            immediate & 3,
                            immediate >> 2
                        ),
                        "roundscale" => {
                            format!("{prefix}_mask_roundscale_{suffix}(s,k,{a},{immediate})")
                        }
                        "fixupimm" => format!(
                            "{prefix}_mask_mov_{suffix}(s,k,{prefix}_mask_fixupimm_{suffix}({a},k,b,t,{immediate}))"
                        ),
                        "scalef" => format!("{prefix}_mask_scalef_{suffix}(s,k,{a},b)"),
                        _ => format!("{prefix}_mask_{op}_{suffix}(s,k,{a})"),
                    };
                    c += &format!(
                        "extern void {name}(void*,void*,void*,void*,void*,uint8_t);\nstatic void reference_{name}(void*sp,void*ap,void*bp,void*tp,void*out,uint8_t k){{{vector} s,a,b;__m{width}i t;memcpy(&s,sp,{bytes});memcpy(&a,ap,{bytes});memcpy(&b,bp,{bytes});memcpy(&t,tp,{bytes});{vector} r={expression};memcpy(out,&r,{bytes});}}\n"
                    );
                    cases += &format!(
                        "{{{name},reference_{name},{bytes},{bits},{operation},{immediate},{alias},\"{name}\"}},\n"
                    );
                }
            }
        }
    }
    c += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    c += r#"
static const uint32_t edges32[]={0,0x80000000u,0x3f800000u,0xc0000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1,0x80000001u,0x00800000u,0x3f000000u,0xbf000000u,0x3fc00000u,0x40200000u,0xc0200000u};
static const uint64_t edges64[]={0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xc000000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0123456789abcULL,1,0x8000000000000001ULL,0x0010000000000000ULL,0x3fe0000000000000ULL,0xbfe0000000000000ULL,0x3ff8000000000000ULL,0x4004000000000000ULL,0xc004000000000000ULL};
int main(void){unsigned saved=_mm_getcsr();for(unsigned ci=0;ci<sizeof(cases)/sizeof(*cases);ci++){struct testcase tc=cases[ci];unsigned lane_bytes=tc.bits/8,lanes=tc.bytes/lane_bytes;uint64_t s[8],a[8],b[8],table[8];unsigned char out[336],want[64];uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<lanes)-1)};
for(unsigned sample=0;sample<16;sample++){for(unsigned i=0;i<64/lane_bytes;i++){if(tc.bits==32){((uint32_t*)s)[i]=edges32[(i+sample+9)%16];((uint32_t*)a)[i]=edges32[(i+sample)%16];((uint32_t*)b)[i]=edges32[(i+sample+3)%16];((uint32_t*)table)[i]=0x11111111u*sample;}else{s[i]=edges64[(i+sample+9)%16];a[i]=edges64[(i+sample)%16];b[i]=edges64[(i+sample+3)%16];table[i]=(uint64_t)(0x11111111u*sample)|UINT64_C(0xabcdef0000000000);}}
for(unsigned rounding=0;rounding<4;rounding++)for(unsigned denormal=0;denormal<4;denormal++)for(unsigned mi=0;mi<7;mi++){unsigned csr=0x1f80|(rounding<<13)|((denormal&1)<<6)|((denormal>>1)<<15);memset(out,0xa5,sizeof out);_mm_setcsr(csr);tc.reference(s,a,b,table,want,masks[mi]);unsigned expected=_mm_getcsr();_mm_setcsr(csr);tc.run(s,a,b,table,out,masks[mi]);unsigned actual=_mm_getcsr();if(memcmp(out,want,tc.bytes)||actual!=expected){fprintf(stderr,"%s sample=%u rounding=%u denormal=%u mask=%x csr=%x expected=%x\n",tc.name,sample,rounding,denormal,masks[mi],actual,expected);return 1;}for(unsigned j=tc.bytes;j<64;j++)if(out[j]!=0xa5)return 2;if(memcmp(out+64,s,tc.bytes)||memcmp(out+128,a,tc.bytes)||memcmp(out+192,b,tc.bytes)||memcmp(out+256,table,tc.bytes)||out[320]!=masks[mi])return 3;
if(tc.operation==6&&!sample){void*input=tc.alias?(void*)s:(void*)a;for(unsigned lane=0;lane<lanes;lane++)if(masks[mi]&(1u<<lane))if(memcmp(out+lane*lane_bytes,(char*)input+lane*lane_bytes,lane_bytes))return 4;}
if((tc.operation==3||tc.operation==4)&&!rounding&&!denormal){for(unsigned lane=0;lane<lanes;lane++){if(!(masks[mi]&(1u<<lane)))continue;long double x,r;void*input=tc.alias?(void*)s:(void*)a;if(tc.bits==32){float fx,fr;memcpy(&fx,(char*)input+4*lane,4);memcpy(&fr,out+4*lane,4);x=fx;r=fr;}else{double fx,fr;memcpy(&fx,(char*)input+8*lane,8);memcpy(&fr,out+8*lane,8);x=fx;r=fr;}long double minimum=tc.bits==32?FLT_MIN:DBL_MIN;if(isfinite(x)&&x>0&&isfinite(r)&&r>=minimum){long double exact=tc.operation==3?1.0L/x:1.0L/sqrtl(x);if(fabsl((r-exact)/exact)>1.0L/16384.0L)return 5;}}}
}
_mm_setcsr(0x1f00);tc.run(s,a,b,table,out,0);if(_mm_getcsr()!=0x1f00||memcmp(out,s,tc.bytes))return 6;
}}
_mm_setcsr(saved);return 0;}
"#;
    (ir, c)
}
#[test]
fn vl_special_every_immediate_matches_bits_mxcsr_and_approximation_contracts() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512dq && !features.fma);
    let directory = std::env::temp_dir().join(format!("keel-vl-special-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux")) && {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512vl")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };
    if native {
        let output = Command::new("cc")
            .args(["-O1", "-mavx512f", "-mavx512vl", "-frounding-math", "-c"])
            .arg(directory.join("oracle.c"))
            .arg("-o")
            .arg(directory.join("oracle.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY, &ONE] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
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
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            fs::write(directory.join("special.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("special.o"))
                    .args(["-lm", "-o"])
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
                        .success(),
                    "{allocation:?}"
                );
            }
        }
    }
}
#[test]
fn vl_special_control_bounds_raw_effects_and_per_function_features() {
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_vl_special().is_some())
    {
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(inst.is_pinned() && inst.may_trap() && !id.info().pure);
        assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
        if let Some((name, _, Some(index))) = id.x86_avx512_vl_special() {
            let maximum = if name.starts_with("vgetmant") {
                15
            } else {
                255
            };
            assert!(id.valid_x86_avx512_special_control(index, maximum));
            assert!(!id.valid_x86_avx512_special_control(index, -1));
            assert!(!id.valid_x86_avx512_special_control(index, maximum + 1));
        }
    }
    for (operation, maximum) in [("getmant", 15), ("roundscale", 255), ("fixupimm", 255)] {
        for immediate in [-1, maximum + 1] {
            let args = if operation == "fixupimm" {
                "v0,v1,v2,v3,v4,v5"
            } else {
                "v0,v1,v2,v5"
            };
            let invalid = format!(
                "func @invalid(f32x4,i8,f32x4,f32x4,i32x4)->f32x4 {{\nblock0(v0:f32x4,v1:i8,v2:f32x4,v3:f32x4,v4:i32x4):\n v5=iconst.i32 {immediate}\n v6=intrinsic x86_avx512vl_{operation}ps128({args})\n return v6\n}}\n"
            );
            assert!(keel_ir::verify_module(&keel_ir::parse(&invalid).unwrap()).is_err());
        }
    }
    let source = "func @operation(f32x4,i8,f32x4)->f32x4 {\nblock0(v0:f32x4,v1:i8,v2:f32x4):\n v3=intrinsic x86_avx512vl_getexpps128(v0,v1,v2)\n return v3\n}\n";
    let compile = |source: &str, features| {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
    };
    for features in [
        Features::NONE,
        Features::NONE.with_features("avx512f").unwrap(),
        Features::NONE.with_features("avx2,fma").unwrap(),
    ] {
        assert!(compile(source, features).is_err());
    }
    compile(
        &source.replace("->f32x4 {", "->f32x4 target_features(\"avx512vl\") {"),
        Features::NONE,
    )
    .unwrap();
    assert!(
        compile(
            &source.replace("->f32x4 {", "->f32x4 target_features(\"-avx512vl\") {"),
            Features::NONE.with_features("avx512vl").unwrap()
        )
        .is_err()
    );
}
