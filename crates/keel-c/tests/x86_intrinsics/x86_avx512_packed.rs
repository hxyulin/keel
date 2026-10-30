//! Standard wrappers retain typed operands, raw FP controls and aligned memory.
use super::pclmul::parse;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{Intrinsic, Target, Type};
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
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <sys/wait.h>\n#include <unistd.h>\n#include <signal.h>\n#include <sys/prctl.h>\n",
    );
    let mut cases = String::new();
    let mut faults = String::new();
    for id in Intrinsic::ALL
        .iter()
        .copied()
        .filter(|id| id.x86_avx512_packed_opcode().is_some())
    {
        let opcode = id.x86_avx512_packed_opcode().unwrap();
        let info = id.info();
        let store = info.returns.is_empty();
        let memory = info.params[0] == Type::Ptr;
        let stream = opcode.starts_with("vmovnt");
        let blend = info.params[0].is_int();
        let moved = !memory && !blend && info.params.len() == 3;
        let shape = if store {
            info.params[1]
        } else {
            info.returns[0]
        };
        let lane = shape.vector().unwrap().lane_type();
        let mask = if lane.bytes() == Some(4) {
            "__mmask16"
        } else {
            "__mmask8"
        };
        let suffix = if lane == Type::F32 {
            "ps"
        } else if lane == Type::F64 {
            "pd"
        } else if lane == Type::I32 {
            "epi32"
        } else {
            "epi64"
        };
        let vec = if suffix == "ps" {
            "__m512"
        } else if suffix == "pd" {
            "__m512d"
        } else {
            "__m512i"
        };
        let save = if lane.is_float() {
            format!("_mm512_storeu_{suffix}")
        } else {
            "_mm512_storeu_si512".into()
        };
        for control in [4, 8].into_iter().filter(|&n| {
            id.x86_avx512_packed_control().is_none() && n == 4
                || id.valid_x86_avx512_packed_control(n)
        }) {
            let name = format!("consumer_{}_{}", info.name, control);
            let mut body = format!("{vec} s=*(const {vec}*)sp,b=*(const {vec}*)bp;");
            if !memory {
                body += &format!("{vec} a=*(const {vec}*)ap;");
            }
            let expr = if memory {
                if stream {
                    if store {
                        format!(
                            "_mm512_stream_{}(ap,s);",
                            if lane.is_float() { suffix } else { "si512" }
                        )
                    } else {
                        format!("{vec} r=_mm512_stream_load_si512(ap);{save}(out,r);")
                    }
                } else if store {
                    format!("_mm512_mask_store_{suffix}(ap,k,s);")
                } else {
                    format!("{vec} r=_mm512_mask_load_{suffix}(s,k,ap);{save}(out,r);")
                }
            } else {
                let value = if blend {
                    format!("_mm512_mask_blend_{suffix}(k,s,b)")
                } else if moved {
                    format!("_mm512_mask_mov_{suffix}(s,k,a)")
                } else if lane.is_float() {
                    let operation = opcode
                        .trim_start_matches('v')
                        .trim_end_matches(if suffix == "ps" { "ps" } else { "pd" });
                    if operation == "sqrt" {
                        format!("_mm512_mask_sqrt_round_{suffix}(s,k,a,{control})")
                    } else {
                        format!("_mm512_mask_{operation}_round_{suffix}(s,k,a,b,{control})")
                    }
                } else {
                    let operation = if opcode == "vpmulld" {
                        "mullo"
                    } else if opcode.starts_with("vpmin") {
                        "min"
                    } else if opcode.starts_with("vpmax") {
                        "max"
                    } else if opcode.starts_with("vpandn") {
                        "andnot"
                    } else if opcode.starts_with("vpand") {
                        "and"
                    } else if opcode.starts_with("vpor") {
                        "or"
                    } else {
                        "xor"
                    };
                    let unsigned = opcode.starts_with("vpminu") || opcode.starts_with("vpmaxu");
                    let ep = if unsigned {
                        if lane == Type::I32 { "epu32" } else { "epu64" }
                    } else {
                        suffix
                    };
                    format!("_mm512_mask_{operation}_{ep}(s,k,a,b)")
                };
                format!("{vec} r={value};{save}(out,r);")
            };
            body += &expr;
            body += &format!("{save}((char*)out+64,s);{save}((char*)out+128,b);");
            source +=
                &format!("void {name}(void*sp,void*ap,void*bp,void*out,{mask} k){{{body}}}\n");
            oracle += &format!(
                "extern void {name}(void*,void*,void*,void*,{mask});\n__attribute__((noinline)) static void oracle_{name}(void*sp,void*ap,void*bp,void*out,{mask} k){{{body}}}\n"
            );
            let compare = if store {
                "memcmp(a,copy,64)"
            } else {
                "memcmp(out,want,64)"
            };
            let masks = if stream {
                "0xffff"
            } else {
                "0,1,0xa55a,0xffff"
            };
            let init = if lane == Type::F32 {
                "for(unsigned i=0;i<16;i++){((uint32_t*)s)[i]=floats[(i+shift)%16];((uint32_t*)a)[i]=floats[(i+shift+1)%16];((uint32_t*)b)[i]=floats[(i+shift*3+5)%16];}"
            } else if lane == Type::F64 {
                "for(unsigned i=0;i<8;i++){s[i]=doubles[(i+shift)%16];a[i]=doubles[(i+shift+1)%16];b[i]=doubles[(i+shift*3+5)%16];}"
            } else {
                "for(unsigned i=0;i<8;i++){s[i]=UINT64_C(0x80000000ffffffff)+i*17;a[i]=UINT64_C(0xdeadbeef01234567)+i*173+shift;b[i]=UINT64_C(0xa5a5a5a5fedcba98)-i*37-shift;}"
            };
            cases += &format!(
                "{{uint64_t s[8] __attribute__((aligned(64))),a[8] __attribute__((aligned(64))),b[8] __attribute__((aligned(64))),copy[8] __attribute__((aligned(64))),out[24] __attribute__((aligned(64))),want[24] __attribute__((aligned(64)));{mask} masks[]={{{masks}}};for(unsigned shift=0;shift<4;shift++)for(unsigned mi=0;mi<sizeof(masks)/sizeof(*masks);mi++){{{init}memcpy(copy,a,64);unsigned csr=(saved&~0x603f)|(shift<<13);_mm_setcsr(csr);oracle_{name}(s,copy,b,want,masks[mi]);_mm_sfence();unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{name}(s,a,b,out,masks[mi]);_mm_sfence();unsigned actual=_mm_getcsr()&0x3f;if({compare}||actual!=expected){{fprintf(stderr,\"{name} mask=%x flags=%x/%x\\n\",(unsigned)masks[mi],actual,expected);return 1;}}if(memcmp(out+8,s,64)||memcmp(out+16,b,64))return 2;}}}}\n"
            );
            if memory {
                faults += &format!(
                    "{{uint64_t s[8] __attribute__((aligned(64)))={{0}},b[8] __attribute__((aligned(64)))={{0}},out[24] __attribute__((aligned(64)))={{0}};{name}(s,m+page-64,b,out,({mask})~0u);_mm_sfence();pid_t child=fork();if(child==0){{prctl(PR_SET_DUMPABLE,0);{name}(s,m+1,b,out,({mask})~0u);_mm_sfence();_exit(0);}}int status;if(child<0||waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 3;"
                );
                if !stream {
                    faults += &format!("{name}(s,m+page,b,out,0);");
                }
                faults += "}\n";
            }
        }
    }
    source += "int evaluation_count;__m512i counted_vector(void){evaluation_count++;return (__m512i){0};}__mmask16 counted_mask(void){evaluation_count++;return 0xffff;}int packed_evaluate_once(void){evaluation_count=0;__m512i r=_mm512_mask_min_epi32(counted_vector(),counted_mask(),counted_vector(),counted_vector());return evaluation_count+((int*)&r)[0];}\n";
    oracle += "extern int packed_evaluate_once(void);\n";
    // Repeated scalar values still contain distinct NaN payloads in the upper lanes.
    oracle += &format!(
        "int main(void){{if(packed_evaluate_once()!=4)return 5;unsigned saved=_mm_getcsr();uint64_t doubles[16]={{0,UINT64_C(0x8000000000000000),UINT64_C(0x3ff8000000000000),UINT64_C(0xbff8000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0123456789abc),1,UINT64_C(0x3ff0000000000001),UINT64_C(0x7fefffffffffffff),UINT64_C(0x0010000000000000),UINT64_C(0x3fe0000000000000),UINT64_C(0xbfe0000000000000),UINT64_C(0x4004000000000000),UINT64_C(0xc004000000000000)}};uint32_t floats[16]={{0,0x80000000u,0x3fc00000u,0xbfc00000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1,0x3f800001u,0x7f7fffffu,0x00800000u,0x3f000000u,0xbf000000u,0x40200000u,0xc0200000u}};{cases}_mm_setcsr(saved);long page=sysconf(_SC_PAGESIZE);unsigned char*m=mmap(0,2*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m==MAP_FAILED||mprotect(m,page,PROT_READ|PROT_WRITE))return 4;{faults}munmap(m,2*page);return 0;}}\n"
    );
    (source, oracle)
}
#[test]
fn packed51_standard_c_wrappers_preserve_bits_flags_and_alignment() {
    let (source, oracle) = fixtures();
    let module = parse(&source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    for id in Intrinsic::ALL
        .iter()
        .filter(|id| id.x86_avx512_packed_opcode().is_some())
    {
        assert!(ir.contains(id.info().name), "missing {}", id.info().name);
    }
    let features = Features::NONE.with_features("avx512f").unwrap();
    let dir = std::env::temp_dir().join(format!("keel-c-packed51-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), oracle).unwrap();
    for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        fs::write(dir.join("consumer.o"), object).unwrap();
        if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
            #[cfg(target_arch = "x86_64")]
            if !std::is_x86_feature_detected!("avx512f") {
                eprintln!("SKIP native packed C wrappers");
                continue;
            }
            let output = Command::new("cc")
                .args(["-O2", "-mavx512f", "-frounding-math", "-ffp-contract=off"])
                .arg(dir.join("oracle.c"))
                .arg(dir.join("consumer.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "{status}");
        }
    }
    assert!(parse(&source, Target::Aarch64LinuxGnu).is_err());
}
#[test]
fn packed_convenience_wrappers_and_control_bounds() {
    let source = "#include <immintrin.h>\nint calls;__m512i ivec(void){calls++;return (__m512i){0};}__mmask16 mask(void){calls++;return 0xffff;}int once(void){calls=0;__m512i r=_mm512_mask_min_epi32(ivec(),mask(),ivec(),ivec());return calls+((int*)&r)[0];}\nvoid bitwise(__m512*o,__m512*a,__m512*b){*o=_mm512_andnot_ps(*a,*b);}\nvoid minmax(__m512d*o,__m512d*a,__m512d*b){*o=_mm512_max_pd(*a,*b);}\nvoid zero(__m512i*o,__m512i*a,__m512i*b){*o=_mm512_maskz_xor_epi64(0x55,*a,*b);}\nvoid aligned(void*p,void*q){_mm512_store_si512(q,_mm512_load_si512(p));}\n";
    let module = parse(source, Target::X86_64LinuxGnu).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let ir = module.to_string();
    assert!(ir.contains("x86_avx512_maskandnd"));
    assert!(ir.contains("x86_avx512_maskmaxpd"));
    assert!(ir.contains("x86_avx512_maskalignedloadq"));
    assert!(ir.contains("x86_avx512_maskalignedstoreq"));
    for expression in [
        "_mm512_mask_add_round_ps(a,k,a,b,r)",
        "_mm512_mask_add_round_ps(a,k,a,b,12)",
        "_mm512_mask_min_round_ps(a,k,a,b,9)",
    ] {
        let bad = format!(
            "#include <immintrin.h>\n__m512 f(__m512 a,__m512 b,__mmask16 k,int r){{return {expression};}}"
        );
        assert!(parse(&bad, Target::X86_64LinuxGnu).is_err(), "{expression}");
    }
}
