//! C scalar shifts accept runtime unsigned-int counts; rotations require imm8.
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
fn fixtures() -> (String, String) {
    let mut source = String::from("#include <immintrin.h>\n");
    let mut oracle = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\ntypedef void(*function)(void*,void*,void*,void*,unsigned long long,uint8_t);\nstruct testcase{function run,reference;unsigned width,bits,operation,kind,immediate,merge;const char*name;};\n",
    );
    let mut cases = String::new();
    for (width, prefix) in [(128, "_mm"), (256, "_mm256")] {
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
            ("rol", 3, 3),
            ("ror", 4, 3),
            ("rolv", 3, 2),
            ("rorv", 4, 2),
        ] {
            for bits in [32, 64] {
                let mut forms = vec!["mask", "maskz"];
                if operation >= 3 || bits == 64 && operation == 2 {
                    forms.push("plain");
                }
                let immediates = if kind == 3 {
                    vec![0, 1, bits - 1, bits, bits + 1, 127, 255]
                } else {
                    vec![0]
                };
                for form in forms.clone() {
                    for immediate in &immediates {
                        let name = format!("consumer_{stem}{bits}_{width}_{form}_{immediate}");
                        let c = if kind == 0 {
                            "(count++)".to_string()
                        } else if kind == 3 {
                            immediate.to_string()
                        } else {
                            "c".to_string()
                        };
                        let expr = match form {
                            "mask" => format!("{prefix}_mask_{stem}_epi{bits}(s,k,a,{c})"),
                            "maskz" => format!("{prefix}_maskz_{stem}_epi{bits}(k,a,{c})"),
                            _ => format!("{prefix}_{stem}_epi{bits}(a,{c})"),
                        };
                        let loadcount = if kind == 1 {
                            format!("__m128i c=*(const __m128i*)cp;")
                        } else if kind == 2 {
                            format!("__m{width}i c=*(const __m{width}i*)cp;")
                        } else {
                            String::new()
                        };
                        let body = format!(
                            "__m{width}i s=*(const __m{width}i*)sp,a=*(const __m{width}i*)ap;{loadcount}{prefix}_storeu_si{width}(out,{expr});*(unsigned long long*)((char*)out+64)=count;"
                        );
                        source += &format!(
                            "__attribute__((target(\"avx512vl\"))) void {name}(void*sp,void*ap,void*cp,void*out,unsigned long long count,__mmask8 k){{{body}}}\n"
                        );
                        oracle += &format!(
                            "extern void {name}(void*,void*,void*,void*,unsigned long long,uint8_t);\nstatic void reference_{name}(void*sp,void*ap,void*cp,void*out,unsigned long long count,__mmask8 k){{{body}}}\n"
                        );
                        cases += &format!(
                            "{{{name},reference_{name},{width},{bits},{operation},{kind},{immediate},{},\"{name}\"}},\n",
                            if form == "mask" {
                                1
                            } else if form == "maskz" {
                                0
                            } else {
                                2
                            }
                        );
                    }
                }
            }
        }
    }
    oracle += &format!("static struct testcase cases[]={{\n{cases}}};\n");
    oracle += r#"
static uint64_t read_lane(void*p,unsigned bits,unsigned lane){uint64_t x=0;memcpy(&x,(char*)p+lane*(bits/8),bits/8);return x;}
static uint64_t scalar(uint64_t x,uint64_t n,unsigned bits,unsigned operation){uint64_t mask=bits==64?UINT64_MAX:UINT32_MAX;x&=mask;if(operation>=3){n&=bits-1;if(!n)return x;return(operation==3?(x<<n)|(x>>(bits-n)):(x>>n)|(x<<(bits-n)))&mask;}if(n>=bits)return operation==2&&(x>>(bits-1))?mask:0;if(!n)return x;if(operation==0)return(x<<n)&mask;if(operation==1)return x>>n;return((x>>n)|((x>>(bits-1))?(mask^(mask>>n)):0))&mask;}
int main(void){unsigned saved=_mm_getcsr();const uint64_t counts[]={0,1,31,32,33,63,64,65,127,128,255,256,UINT64_C(0x100000000),UINT64_C(0x100000001),UINT64_C(0x8000000000000000),UINT64_MAX};const uint64_t edges[]={0,1,UINT64_MAX,UINT64_C(0x8000000000000000),UINT64_C(0x8000000080000000),UINT64_C(0x7fffffff7fffffff),UINT64_C(0x55555555aaaaaaaa),UINT64_C(0x7ff012347f812345)};for(unsigned ci=0;ci<sizeof cases/sizeof(*cases);ci++){struct testcase tc=cases[ci];unsigned lanes=tc.width/tc.bits;uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),count[8] __attribute__((aligned(64)));unsigned char out[96],want[96];uint8_t masks[]={0,1,2,0x55,0xaa,0xff,(uint8_t)~((1u<<lanes)-1)};for(unsigned sample=0;sample<16;sample++){for(unsigned i=0;i<8;i++){s[i]=edges[(i+sample+3)%8];a[i]=edges[(i+sample)%8];count[i]=counts[(i+sample)%16];}if(tc.kind==1){count[0]=counts[sample];count[1]=UINT64_C(0xfedcba9876543210)^sample;}else if(tc.bits==32){for(unsigned lane=0;lane<lanes;lane++)((uint32_t*)count)[lane]=(uint32_t)counts[(lane+sample)%16];}for(unsigned mi=0;mi<7;mi++){memset(out,0xa5,sizeof out);memset(want,0xa5,sizeof want);unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);tc.reference(s,a,count,want,counts[sample],masks[mi]);tc.run(s,a,count,out,counts[sample],masks[mi]);if(_mm_getcsr()!=csr||memcmp(out,want,sizeof out)){fprintf(stderr,"%s sample=%u mask=%x system mismatch\n",tc.name,sample,masks[mi]);return 1;}for(unsigned lane=0;lane<lanes;lane++){uint64_t n=tc.kind==0?(uint32_t)counts[sample]:tc.kind==1?count[0]:tc.kind==2?read_lane(count,tc.bits,lane):tc.immediate;uint64_t expected=(tc.merge==2||(masks[mi]&(1u<<lane)))?scalar(read_lane(a,tc.bits,lane),n,tc.bits,tc.operation):tc.merge?read_lane(s,tc.bits,lane):0;if(read_lane(out,tc.bits,lane)!=expected)return 2;}uint64_t after;memcpy(&after,out+64,8);if(after!=counts[sample]+(tc.kind==0))return 3;}}}_mm_setcsr(saved);return 0;}
"#;
    (source, oracle)
}
#[test]
fn vl_shift_wrappers_match_scalar_and_system_runtime_unsigned_counts() {
    let (source, oracle) = fixtures();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let features = module_features(&module, base).unwrap();
    assert!(features.avx512vl && !features.avx2 && !features.avx512bw && !features.avx512dq);
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
    let directory = std::env::temp_dir().join(format!("keel-c-vl-shift-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(directory.join("shift.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512vl") {
                eprintln!("SKIP native C VL shifts: unavailable");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O1", "-fno-tree-vectorize", "-mavx512f", "-mavx512vl"])
                .arg(directory.join("oracle.c"))
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
                    .success()
            );
        }
    }
}
#[test]
fn vl_rotation_immediates_reject_out_of_range_and_runtime_counts() {
    for count in ["-1", "256", "n"] {
        let source = format!(
            "#include <immintrin.h>\n__attribute__((target(\"avx512vl\"))) __m128i f(__m128i a,int n){{return _mm_mask_rol_epi32(a,-1,a,{count});}}\n"
        );
        match parse(&source, Target::X86_64LinuxGnu) {
            Err(_) => {}
            Ok(module) => assert!(keel_ir::verify_module(&module).is_err()),
        }
    }
}
