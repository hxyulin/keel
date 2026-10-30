//! Scalar unsigned count rules are independent of descriptor and instruction names.
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
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\ntypedef void(*function)(void*,void*,void*,void*,uint8_t);\nstruct testcase{function run,reference;unsigned width,bits,operation,kind,immediate,alias;const char*name;};\n",
    );
    let mut cases = String::new();
    for width in [128, 256] {
        for (stem, operation, kind) in [
            ("slli", 0, 0),
            ("srli", 1, 0),
            ("srai", 2, 0),
            ("sll", 0, 1),
            ("srl", 1, 1),
            ("sra", 2, 1),
            ("sllv", 0, 2),
            ("srlv", 1, 2),
            ("srav", 2, 2),
            ("rol", 3, 0),
            ("ror", 4, 0),
            ("rolv", 3, 2),
            ("rorv", 4, 2),
        ] {
            for (bits, lane) in [(32, "d"), (64, "q")] {
                let shape = format!("i{bits}x{}", width / bits);
                let count_shape = if kind == 1 {
                    "i64x2".to_string()
                } else {
                    shape.clone()
                };
                let prefix = if width == 128 { "_mm" } else { "_mm256" };
                let immediates = if kind == 0 {
                    vec![0, 1, bits - 1, bits, bits + 1, 127, 255]
                } else {
                    vec![0]
                };
                for immediate in immediates {
                    for alias in [0, 1] {
                        let name = format!("{stem}{lane}{width}_{immediate}_{alias}");
                        let merge = if alias == 1 { "v6" } else { "v5" };
                        let count = if kind == 0 {
                            "v8"
                        } else if kind == 2 && alias == 1 {
                            "v6"
                        } else {
                            "v7"
                        };
                        let wide = if alias == 1 {
                            " v9=load.i64x8 align 1 v0\n"
                        } else {
                            ""
                        };
                        ir += &format!(
                            "func @{name}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{shape} align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{count_shape} align 1 v2\n v8=iconst.i32 {immediate}\n{wide} v10=intrinsic x86_avx512vl_{stem}{lane}{width}({merge},v4,v6,{count})\n store align 1 v10,v3\n v11=iconst.i64 64\n v12=ptradd v3,v11\n store align 1 v5,v12\n v13=ptradd v12,v11\n store align 1 v6,v13\n v14=ptradd v13,v11\n store align 1 v7,v14\n v15=ptradd v14,v11\n store align 1 v4,v15\n"
                        );
                        if alias == 1 {
                            ir += " v16=ptradd v15,v11\n store align 1 v9,v16\n";
                        }
                        ir += " return\n}\n";
                        let c_arg = if kind == 0 {
                            immediate.to_string()
                        } else if kind == 2 && alias == 1 {
                            "a".into()
                        } else {
                            "count".into()
                        };
                        let cm = if alias == 1 { "a" } else { "s" };
                        let bytes = width / 8;
                        let count_bytes = if kind == 1 { 16 } else { bytes };
                        c += &format!(
                            "extern void {name}(void*,void*,void*,void*,uint8_t);\nstatic void reference_{name}(void*sp,void*ap,void*cp,void*out,uint8_t k){{__m{width}i s,a;__m{}i count;memcpy(&s,sp,{bytes});memcpy(&a,ap,{bytes});memcpy(&count,cp,{count_bytes});__m{width}i r={prefix}_mask_{stem}_epi{bits}({cm},k,a,{c_arg});memcpy(out,&r,{bytes});}}\n",
                            if kind == 1 { 128 } else { width }
                        );
                        cases += &format!(
                            "{{{name},reference_{name},{width},{bits},{operation},{kind},{immediate},{alias},\"{name}\"}},\n"
                        );
                    }
                }
            }
        }
    }
    c += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    c += r#"
static uint64_t scalar(uint64_t x,uint64_t n,unsigned bits,unsigned operation){uint64_t mask=bits==64?UINT64_MAX:UINT32_MAX;x&=mask;if(operation>=3){n&=bits-1;if(!n)return x;return(operation==3?(x<<n)|(x>>(bits-n)):(x>>n)|(x<<(bits-n)))&mask;}if(n>=bits)return operation==2&&(x>>(bits-1))?mask:0;if(!n)return x;if(operation==0)return(x<<n)&mask;if(operation==1)return x>>n;return((x>>n)|((x>>(bits-1))?(mask^(mask>>n)):0))&mask;}
static uint64_t read_lane(void*p,unsigned bits,unsigned lane){uint64_t x=0;memcpy(&x,(char*)p+lane*(bits/8),bits/8);return x;}
int main(void){unsigned saved=_mm_getcsr();const uint64_t counts[]={0,1,31,32,33,63,64,65,127,128,255,256,UINT64_C(0x100000000),UINT64_C(0x100000001),UINT64_C(0x8000000000000000),UINT64_MAX};const uint64_t values[]={0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x8000000080000000),UINT64_C(0x7fffffff7fffffff),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x7ff012347f812345)};
for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase tc=cases[ci];unsigned bytes=tc.width/8,lanes=tc.width/tc.bits;uint64_t s[8],a[8],count[8];unsigned char out[400],want[64];uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<lanes)-1)};
for(unsigned sample=0;sample<16;sample++){for(unsigned i=0;i<8;i++){s[i]=values[(i+sample+3)%8];a[i]=values[(i+sample)%8];count[i]=counts[(i+sample)%16];}if(tc.kind==1){count[0]=counts[sample];count[1]=UINT64_C(0xfedcba9876543210)^sample;}else if(tc.bits==32){for(unsigned lane=0;lane<lanes;lane++)((uint32_t*)count)[lane]=(uint32_t)counts[(lane+sample)%16];}
for(unsigned mi=0;mi<7;mi++){unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);memset(out,0xa5,sizeof out);tc.reference(s,a,count,want,masks[mi]);tc.run(s,a,count,out,masks[mi]);if(_mm_getcsr()!=csr||memcmp(out,want,bytes)){fprintf(stderr,"%s sample=%u mask=%x system mismatch\n",tc.name,sample,masks[mi]);return 1;}for(unsigned lane=0;lane<lanes;lane++){uint64_t x=read_lane(a,tc.bits,lane),n=tc.kind==0?tc.immediate:tc.kind==1?count[0]:read_lane(tc.alias?(void*)a:(void*)count,tc.bits,lane);uint64_t expected=(masks[mi]&(1u<<lane))?scalar(x,n,tc.bits,tc.operation):read_lane(tc.alias?(void*)a:(void*)s,tc.bits,lane);if(read_lane(out,tc.bits,lane)!=expected){fprintf(stderr,"%s sample=%u lane=%u n=%llx expected=%llx got=%llx\n",tc.name,sample,lane,(unsigned long long)n,(unsigned long long)expected,(unsigned long long)read_lane(out,tc.bits,lane));return 2;}}for(unsigned j=bytes;j<64;j++)if(out[j]!=0xa5)return 3;if(memcmp(out+64,s,bytes)||memcmp(out+128,a,bytes)||memcmp(out+192,count,tc.kind==1?16:bytes)||out[256]!=masks[mi]||(tc.alias&&memcmp(out+320,s,64)))return 4;}}
}_mm_setcsr(saved);return 0;}
"#;
    (ir, c)
}
#[test]
fn vl_shifts_and_rotations_match_scalar_and_system_counts_aliases_and_spills() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(!features.avx2 && !features.avx512bw && !features.avx512dq);
    let directory = std::env::temp_dir().join(format!("keel-vl-shift-{}", std::process::id()));
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
            .args([
                "-O1",
                "-fno-tree-vectorize",
                "-mavx512f",
                "-mavx512vl",
                "-c",
            ])
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
            fs::write(directory.join("shift.o"), object).unwrap();
            if native {
                let output = Command::new("cc")
                    .arg(directory.join("oracle.o"))
                    .arg(directory.join("shift.o"))
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
                        .success(),
                    "{allocation:?}"
                );
            }
        }
    }
}
#[test]
fn vl_shift_contracts_immediates_and_per_function_features() {
    assert_eq!(
        Intrinsic::ALL
            .iter()
            .filter(|id| id.x86_avx512_vl_shift().is_some())
            .count(),
        52
    );
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_vl_shift().is_some())
    {
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure && !inst.may_trap());
        assert_eq!(inst.memory_effects(), MemoryEffects::None);
    }
    let source = "func @operation(i32x4,i8,i32x4)->i32x4 {\nblock0(v0:i32x4,v1:i8,v2:i32x4):\n v3=iconst.i32 1\n v4=intrinsic x86_avx512vl_sllid128(v0,v1,v2,v3)\n return v4\n}\n";
    for count in [-1, 256] {
        let invalid = source.replace("iconst.i32 1", &format!("iconst.i32 {count}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&invalid).unwrap()).is_err());
    }
    let dynamic = source
        .replace("i32x4)->i32x4", "i32x4,i32)->i32x4")
        .replace("v2:i32x4):", "v2:i32x4,v3:i32):")
        .replace(" v3=iconst.i32 1\n", "");
    assert!(keel_ir::verify_module(&keel_ir::parse(&dynamic).unwrap()).is_err());
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
        Features::NONE.with_features("avx2").unwrap(),
    ] {
        assert!(compile(source, features).is_err());
    }
    compile(
        &source.replace("->i32x4 {", "->i32x4 target_features(\"avx512vl\") {"),
        Features::NONE,
    )
    .unwrap();
    assert!(
        compile(
            &source.replace("->i32x4 {", "->i32x4 target_features(\"-avx512vl\") {"),
            Features::NONE.with_features("avx512vl").unwrap()
        )
        .is_err()
    );
}
