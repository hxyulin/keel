//! XMM/YMM masking preserves exact widths, ignores high mask bits and requires VL.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, module_features},
};
use keel_ir::{Intrinsic, Type};
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
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/mman.h>\n#include <sys/wait.h>\n#include <unistd.h>\n#include <signal.h>\n#include <sys/prctl.h>\n",
    );
    let mut cases = String::new();
    let mut memory_cases = String::new();
    for id in Intrinsic::ALL
        .iter()
        .copied()
        .filter(|id| id.is_x86_avx512_vl_core())
    {
        // The oracle follows the public operation contract, independently of
        // intrinsic parameter metadata and the backend opcode lookup.
        let variant = format!("{id:?}");
        let variant = variant.strip_prefix("X86Avx512Vl").unwrap();
        let (operation, bits) = if let Some(operation) = variant.strip_suffix("128") {
            (operation.to_ascii_lowercase(), 128)
        } else {
            (
                variant.strip_suffix("256").unwrap().to_ascii_lowercase(),
                256,
            )
        };
        let (scalar, lane_bits) = match operation.as_str() {
            "maskaddd" | "masksubd" | "maskloadd" | "maskstored" | "cmpd" | "cmpud" => ("i32", 32),
            "maskaddq" | "masksubq" | "maskloadq" | "maskstoreq" | "cmpq" | "cmpuq" => ("i64", 64),
            "maskaddps" | "masksubps" | "maskloadps" | "maskstoreps" | "cmpps" => ("f32", 32),
            "maskaddpd" | "masksubpd" | "maskloadpd" | "maskstorepd" | "cmppd" => ("f64", 64),
            _ => unreachable!("declared VL core operation"),
        };
        let shape = Type::Vector(format!("{scalar}x{}", bits / lane_bits).parse().unwrap());
        let api_name = format!("x86_avx512vl_{operation}{bits}");
        assert_eq!(id.info().name, api_name);
        let vector = shape.vector().unwrap();
        let bytes = vector.bytes();
        let lanes = vector.lanes();
        let lane_bytes = vector.lane_type().bytes().unwrap();
        let floating = vector.lane_type().is_float();
        let compare = operation.starts_with("cmp");
        let store = operation.starts_with("maskstore");
        let memory = store || operation.starts_with("maskload");
        let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
        let suffix = if floating {
            if lane_bytes == 4 { "ps" } else { "pd" }
        } else if lane_bytes == 4 {
            "epi32"
        } else {
            "epi64"
        };
        let container = if bytes == 16 {
            if floating {
                if lane_bytes == 4 { "__m128" } else { "__m128d" }
            } else {
                "__m128i"
            }
        } else if floating {
            if lane_bytes == 4 { "__m256" } else { "__m256d" }
        } else {
            "__m256i"
        };
        let load = if floating {
            format!("{prefix}_loadu_{suffix}")
        } else {
            format!("{prefix}_loadu_si{}", bytes * 8)
        };
        let save = if floating {
            format!("{prefix}_storeu_{suffix}")
        } else {
            format!("{prefix}_storeu_si{}", bytes * 8)
        };
        let predicates = if compare {
            if floating { 32 } else { 8 }
        } else {
            1
        };
        for predicate in 0..predicates {
            let aliases = if !memory && (!compare || predicate == 0) {
                2
            } else {
                1
            };
            for alias in 0..aliases {
                let function = format!("{api_name}_{predicate}_{alias}");
                let source_a = if alias == 1 { "v5" } else { "v6" };
                let source_b = if compare && alias == 1 { "v5" } else { "v7" };
                let arguments = if compare {
                    format!("v5,{source_b},v4,v8")
                } else if memory {
                    "v1,v5,v4".into()
                } else {
                    format!("v5,v4,{source_a},v7")
                };
                let load_a = if memory {
                    String::new()
                } else {
                    format!(" v6=load.{shape} align 1 v1\n")
                };
                let instruction = if store {
                    format!(" intrinsic {api_name}({arguments})\n")
                } else {
                    format!(" v9=intrinsic {api_name}({arguments})\n store align 1 v9,v3\n")
                };
                ir += &format!(
                    "func @{function}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{shape} align 1 v0\n{load_a} v7=load.{shape} align 1 v2\n v8=iconst.i32 {predicate}\n{instruction} v10=iconst.i64 64\n v11=ptradd v3,v10\n store align 1 v5,v11\n v12=ptradd v11,v10\n store align 1 v7,v12\n return\n}}\n"
                );
                c += &format!("extern void {function}(void*,void*,void*,void*,uint8_t);\n");
                let expr = if compare {
                    let ep = if matches!(operation.as_str(), "cmpud" | "cmpuq") {
                        if lane_bytes == 4 { "epu32" } else { "epu64" }
                    } else {
                        suffix
                    };
                    let b = if alias == 1 { "s" } else { "b" };
                    format!("*(uint8_t*)out={prefix}_mask_cmp_{ep}_mask(k,s,{b},{predicate});")
                } else if memory {
                    if store {
                        format!("{prefix}_mask_storeu_{suffix}(ap,k,s);")
                    } else {
                        format!("{container} r={prefix}_mask_loadu_{suffix}(s,k,ap);{save}(out,r);")
                    }
                } else {
                    let operation = if operation.starts_with("maskadd") {
                        "add"
                    } else {
                        "sub"
                    };
                    let a = if alias == 1 { "s" } else { "a" };
                    format!(
                        "{container} r={prefix}_mask_{operation}_{suffix}(s,k,{a},b);{save}(out,r);"
                    )
                };
                c += &format!(
                    "__attribute__((noinline)) static void oracle_{function}(void*sp,void*ap,void*bp,void*out,uint8_t k){{{container} s={load}(sp),b={load}(bp);"
                );
                if !memory && !compare {
                    c += &format!("{container} a={load}(ap);");
                }
                c += &format!("{expr}}}\n");
                let initialize = if floating && lane_bytes == 4 {
                    format!(
                        "for(unsigned i=0;i<{lanes};i++){{((uint32_t*)s)[i]=floats[(i+shift)%16];((uint32_t*)a)[i]=floats[(i+shift+3)%16];((uint32_t*)b)[i]=floats[(i+shift*3+5)%16];}}"
                    )
                } else if floating {
                    format!(
                        "for(unsigned i=0;i<{lanes};i++){{s[i]=doubles[(i+shift)%16];a[i]=doubles[(i+shift+3)%16];b[i]=doubles[(i+shift*3+5)%16];}}"
                    )
                } else {
                    format!(
                        "for(unsigned i=0;i<{};i++){{s[i]=UINT64_C(0x80000000ffffffff)+i*17;a[i]=UINT64_C(0xdeadbeef01234567)+i*173+shift;b[i]=UINT64_C(0xa5a5a5a5fedcba98)-i*37-shift;}}",
                        bytes / 8
                    )
                };
                let compare_bytes = if compare { 1 } else { bytes };
                let output = if store {
                    format!("memcmp(a,copy,{bytes})")
                } else {
                    format!("memcmp(out,want,{compare_bytes})")
                };
                let high = format!("(uint8_t)~((1u<<{lanes})-1)");
                cases += &format!(
                    "{{uint64_t s[8],a[8],b[8],copy[8],out[24]={{0}},want[8]={{0}};uint8_t masks[]={{0,1,0x55,0xff,{high}}};for(unsigned shift=0;shift<4;shift++)for(unsigned mi=0;mi<5;mi++){{{initialize}memcpy(copy,a,{bytes});unsigned csr=(saved&~0x603f)|(shift<<13);_mm_setcsr(csr);oracle_{function}(s,copy,b,want,masks[mi]);unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{function}(s,a,b,out,masks[mi]);unsigned actual=_mm_getcsr()&0x3f;if({output}||actual!=expected){{fprintf(stderr,\"{function} mask=%x flags=%x/%x\\n\",masks[mi],actual,expected);return 1;}}if(memcmp((char*)out+64,s,{bytes})||memcmp((char*)out+128,b,{bytes}))return 2;}}}}\n"
                );
                if memory {
                    let result = if store {
                        format!("if(memcmp(p,merge,{lane_bytes}))return 3;")
                    } else {
                        format!(
                            "memcpy(want,merge,{bytes});memcpy(want,p,{lane_bytes});if(memcmp(out,want,{bytes}))return 3;"
                        )
                    };
                    memory_cases += &format!(
                        "{{uint64_t merge[8],b[8]={{0}},out[24]={{0}},want[8];memset(merge,0xa5,sizeof merge);unsigned char*p=m+page-{lane_bytes};memset(p,0x81,{lane_bytes});uint8_t high={high};{function}(merge,p,b,out,high|1);{result}{function}(merge,m+page,b,out,high);{function}(merge,m+page-{bytes},b,out,0xff);{function}(merge,m+1,b,out,0xff);pid_t child=fork();if(child==0){{prctl(PR_SET_DUMPABLE,0);{function}(merge,m+page,b,out,1);_exit(0);}}int status;if(child<0||waitpid(child,&status,0)!=child||!WIFSIGNALED(status)||(WTERMSIG(status)!=SIGSEGV&&WTERMSIG(status)!=SIGBUS))return 4;}}\n"
                    );
                }
            }
        }
    }
    // A live ZMM value may not share an allocation with a live XMM/YMM view.
    for (width, shape) in [(128, "f32x4"), (256, "f32x8")] {
        ir += &format!(
            "func @mixed{width}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.i64x8 align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{shape} align 1 v2\n v8=intrinsic x86_avx512vl_maskaddps{width}(v6,v4,v6,v7)\n store align 1 v8,v3\n v9=iconst.i64 64\n v10=ptradd v3,v9\n store align 1 v5,v10\n return\n}}\n"
        );
        c += &format!("extern void mixed{width}(void*,void*,void*,void*,uint8_t);\n");
        cases += &format!(
            "{{uint64_t wide[8],out[16];float a[8],b[8];for(unsigned i=0;i<8;i++){{wide[i]=UINT64_C(0xfedcba9876543210)+i;a[i]=i;b[i]=i+2;}}mixed{width}(wide,a,b,out,0xff);if(memcmp(out+8,wide,64))return 5;}}\n"
        );
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t doubles[16]={{0,UINT64_C(0x8000000000000000),UINT64_C(0x3ff8000000000000),UINT64_C(0xbff8000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0123456789abc),1,UINT64_C(0x3ff0000000000001),UINT64_C(0x7fefffffffffffff),UINT64_C(0x0010000000000000),UINT64_C(0x3fe0000000000000),UINT64_C(0xbfe0000000000000),UINT64_C(0x4004000000000000),UINT64_C(0xc004000000000000)}};uint32_t floats[16]={{0,0x80000000u,0x3fc00000u,0xbfc00000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1,0x3f800001u,0x7f7fffffu,0x00800000u,0x3f000000u,0xbf000000u,0x40200000u,0xc0200000u}};{cases}_mm_setcsr(saved);long page=sysconf(_SC_PAGESIZE);unsigned char*m=mmap(0,page*2,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);if(m==MAP_FAILED||mprotect(m,page,PROT_READ|PROT_WRITE))return 6;{memory_cases}munmap(m,page*2);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn vl_core_masks_widths_aliases_memory_and_mxcsr() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(features.avx512f && features.avx512vl && !features.avx2 && !features.fma);
    let directory = std::env::temp_dir().join(format!("keel-vl-core-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("oracle.c"), oracle).unwrap();
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
            fs::write(directory.join("vl.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512vl") {
                    eprintln!("SKIP native VL core: AVX512VL unavailable");
                    continue;
                }
                let output = Command::new("cc")
                    .args([
                        "-O2",
                        "-mavx512f",
                        "-mavx512vl",
                        "-frounding-math",
                        "-ffp-contract=off",
                    ])
                    .arg(directory.join("oracle.c"))
                    .arg(directory.join("vl.o"))
                    .arg("-o")
                    .arg(directory.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let status = Command::new(directory.join("run")).status().unwrap();
                assert!(status.success(), "{allocation:?}: {status}");
            }
        }
    }
}
#[test]
fn vl_core_requires_function_local_permissions_and_constant_predicates() {
    let body = "block0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n v4=load.i32x4 align 1 v0\n v5=load.i32x4 align 1 v1\n v6=intrinsic x86_avx512vl_maskaddd128(v4,v3,v4,v5)\n store align 1 v6,v2\n return\n";
    let source = format!(
        "func @enabled(ptr,ptr,ptr,i8) target_features(\"avx512vl\") {{\n{body}}}\nfunc @baseline(ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:i8):\n call @enabled(v0,v1,v2,v3)\n return\n}}\n"
    );
    let mut module = keel_ir::parse(&source).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let base = Features::NONE.with_features("avx512f").unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: base,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).unwrap();
    let encoding = module_features(&module, base).unwrap();
    assert!(encoding.avx512vl);
    assert!(keel_asm::x86::assemble_program_object_with_features(&program, base).is_err());
    keel_asm::x86::assemble_program_object_with_features(&program, encoding).unwrap();
    let caller = module
        .functions
        .iter()
        .find(|f| f.name == "baseline")
        .unwrap();
    assert!(!caller.layout_insts().any(|(_,i)|matches!(caller.inst_data(i),keel_ir::InstData::Intrinsic{id,..}if id.is_x86_avx512_vl_core())));
    for features in [
        Features::NONE,
        base,
        Features::NONE.with_features("avx2").unwrap(),
    ] {
        let missing =
            keel_ir::parse(&format!("func @missing(ptr,ptr,ptr,i8) {{\n{body}}}\n")).unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features,
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&missing, &EMPTY, &pipeline)
                .is_err()
        );
    }
    let disabled = keel_ir::parse(&source.replace(
        "target_features(\"avx512vl\")",
        "target_features(\"-avx512vl\")",
    ))
    .unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: encoding,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&disabled, &EMPTY, &pipeline).is_err()
    );
    for (operation, maximum) in [("cmpd128", 7), ("cmpps256", 31)] {
        for value in [-1, maximum + 1] {
            let shape = if operation.starts_with("cmpd") {
                "i32x4"
            } else {
                "f32x8"
            };
            let bad = format!(
                "func @bad({shape},{shape},i8)->i8 {{\nblock0(v0:{shape},v1:{shape},v2:i8):\n v3=iconst.i32 {value}\n v4=intrinsic x86_avx512vl_{operation}(v0,v1,v2,v3)\n return v4\n}}\n"
            );
            assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
        }
    }
}
