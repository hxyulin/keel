//! Independent VL FP descriptors cover raw min/max bits and masked exceptions.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, VectorType};
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
#[derive(Clone, Copy)]
enum Operation {
    Multiply,
    Divide,
    SquareRoot,
    Minimum,
    Maximum,
}
impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Multiply => "mul",
            Self::Divide => "div",
            Self::SquareRoot => "sqrt",
            Self::Minimum => "min",
            Self::Maximum => "max",
        }
    }
    fn unary(self) -> bool {
        matches!(self, Self::SquareRoot)
    }
    fn extreme(self) -> bool {
        matches!(self, Self::Minimum | Self::Maximum)
    }
}
// Shapes and operations are public API contracts, independent of info() and lowering.
const CASES: [(Intrinsic, VectorType, Operation); 20] = [
    (
        Intrinsic::X86Avx512VlMaskMulps128,
        VectorType::F32x4,
        Operation::Multiply,
    ),
    (
        Intrinsic::X86Avx512VlMaskMulpd128,
        VectorType::F64x2,
        Operation::Multiply,
    ),
    (
        Intrinsic::X86Avx512VlMaskDivps128,
        VectorType::F32x4,
        Operation::Divide,
    ),
    (
        Intrinsic::X86Avx512VlMaskDivpd128,
        VectorType::F64x2,
        Operation::Divide,
    ),
    (
        Intrinsic::X86Avx512VlMaskSqrtps128,
        VectorType::F32x4,
        Operation::SquareRoot,
    ),
    (
        Intrinsic::X86Avx512VlMaskSqrtpd128,
        VectorType::F64x2,
        Operation::SquareRoot,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinps128,
        VectorType::F32x4,
        Operation::Minimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinpd128,
        VectorType::F64x2,
        Operation::Minimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxps128,
        VectorType::F32x4,
        Operation::Maximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxpd128,
        VectorType::F64x2,
        Operation::Maximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMulps256,
        VectorType::F32x8,
        Operation::Multiply,
    ),
    (
        Intrinsic::X86Avx512VlMaskMulpd256,
        VectorType::F64x4,
        Operation::Multiply,
    ),
    (
        Intrinsic::X86Avx512VlMaskDivps256,
        VectorType::F32x8,
        Operation::Divide,
    ),
    (
        Intrinsic::X86Avx512VlMaskDivpd256,
        VectorType::F64x4,
        Operation::Divide,
    ),
    (
        Intrinsic::X86Avx512VlMaskSqrtps256,
        VectorType::F32x8,
        Operation::SquareRoot,
    ),
    (
        Intrinsic::X86Avx512VlMaskSqrtpd256,
        VectorType::F64x4,
        Operation::SquareRoot,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinps256,
        VectorType::F32x8,
        Operation::Minimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinpd256,
        VectorType::F64x4,
        Operation::Minimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxps256,
        VectorType::F32x8,
        Operation::Maximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxpd256,
        VectorType::F64x4,
        Operation::Maximum,
    ),
];
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <sys/wait.h>\n#include <unistd.h>\n#include <signal.h>\n#include <sys/prctl.h>\n",
    );
    let mut cases = String::new();
    let mut traps = String::new();
    let mut denormal_cases = String::new();
    for (id, shape, operation) in CASES {
        let bytes = shape.bytes();
        let bits = bytes * 8;
        let lanes = shape.lanes();
        let double = shape.lane_type() == keel_ir::Type::F64;
        let suffix = if double { "pd" } else { "ps" };
        let prefix = if bytes == 16 { "_mm" } else { "_mm256" };
        let container = match (double, bytes) {
            (false, 16) => "__m128",
            (true, 16) => "__m128d",
            (false, 32) => "__m256",
            (true, 32) => "__m256d",
            _ => unreachable!(),
        };
        let uint = if double { "uint64_t" } else { "uint32_t" };
        let name = format!("x86_avx512vl_mask{}{suffix}{bits}", operation.name());
        assert_eq!(id.info().name, name);
        for alias in 0..3 {
            let function = format!("{name}_{alias}");
            let a = if alias == 1 { "v5" } else { "v6" };
            let b = if alias == 2 { "v5" } else { "v7" };
            let arguments = if operation.unary() {
                format!("v5,v4,{a}")
            } else {
                format!("v5,v4,{a},{b}")
            };
            ir += &format!(
                "func @{function}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{shape} align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{shape} align 1 v2\n v8=intrinsic {name}({arguments})\n store align 1 v8,v3\n v9=iconst.i64 64\n v10=ptradd v3,v9\n store align 1 v5,v10\n v11=iconst.i64 32\n v12=ptradd v10,v11\n store align 1 v6,v12\n v13=ptradd v12,v11\n store align 1 v7,v13\n v14=ptradd v13,v11\n store align 1 v4,v14\n return\n}}\n"
            );
            c += &format!("extern void {function}(void*,void*,void*,void*,uint8_t);\n");
            let a = if alias == 1 { "s" } else { "a" };
            let b = if alias == 2 { "s" } else { "b" };
            let expression = if operation.unary() {
                format!("{prefix}_mask_{}_{suffix}(s,k,{a})", operation.name())
            } else {
                format!("{prefix}_mask_{}_{suffix}(s,k,{a},{b})", operation.name())
            };
            c += &format!(
                "__attribute__((noinline)) static void oracle_{function}(void*sp,void*ap,void*bp,void*out,uint8_t k){{{container} s={prefix}_loadu_{suffix}(sp),a={prefix}_loadu_{suffix}(ap),b={prefix}_loadu_{suffix}(bp);{container} result={expression};{prefix}_storeu_{suffix}(out,result);}}\n"
            );
            let initialize = if double {
                format!(
                    "for(unsigned i=0;i<{lanes};i++){{s[i]=doubles[(i+sample)%16];a[i]=doubles[(i+sample+1)%16];b[i]=doubles[(i+sample*3+5)%16];}}"
                )
            } else {
                format!(
                    "for(unsigned i=0;i<{lanes};i++){{((uint32_t*)s)[i]=floats[(i+sample)%16];((uint32_t*)a)[i]=floats[(i+sample+1)%16];((uint32_t*)b)[i]=floats[(i+sample*3+5)%16];}}"
                )
            };
            let source_a = if alias == 1 { "s" } else { "a" };
            let source_b = if alias == 2 { "s" } else { "b" };
            let sign = if double {
                "UINT64_C(0x8000000000000000)"
            } else {
                "0x80000000u"
            };
            let exponent = if double {
                "UINT64_C(0x7ff0000000000000)"
            } else {
                "0x7f800000u"
            };
            let fraction = if double {
                "UINT64_C(0x000fffffffffffff)"
            } else {
                "0x007fffffu"
            };
            let extremes = if operation.extreme() {
                format!(
                    "for(unsigned lane=0;lane<{lanes};lane++){{{uint} x,y,got,merge;memcpy(&x,(char*){source_a}+lane*sizeof(x),sizeof(x));memcpy(&y,(char*){source_b}+lane*sizeof(y),sizeof(y));memcpy(&got,(char*)out+lane*sizeof(got),sizeof(got));memcpy(&merge,(char*)s+lane*sizeof(merge),sizeof(merge));/* DAZ normalizes denormal operands before second-source selection. */if(mode&4){{if(!(x&{exponent})&&(x&{fraction}))x&={sign};if(!(y&{exponent})&&(y&{fraction}))y&={sign};}}int nanx=(x&{exponent})=={exponent}&&(x&{fraction});int nany=(y&{exponent})=={exponent}&&(y&{fraction});int zeros=!(x&~({uint}){sign})&&!(y&~({uint}){sign});if(!(masks[mi]&(1u<<lane))){{if(got!=merge)return 3;}}else if((nanx||nany||zeros)&&got!=y){{fprintf(stderr,\"{function} independent min/max sample=%u mode=%x lane=%u x=%llx y=%llx got=%llx\\n\",sample,mode,lane,(unsigned long long)x,(unsigned long long)y,(unsigned long long)got);return 4;}}}}"
                )
            } else {
                String::new()
            };
            cases += &format!(
                "{{uint64_t s[8],a[8],b[8],out[24]={{0}},want[8]={{0}};uint8_t masks[]={{0,1,0x55,0xff,(uint8_t)~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<16;sample++)for(unsigned mode=0;mode<16;mode++)for(unsigned mi=0;mi<5;mi++){{{initialize}unsigned csr=(saved&~0xe07f)|((mode&3)<<13)|((mode&4)?0x40:0)|((mode&8)?0x8000:0);_mm_setcsr(csr);oracle_{function}(s,a,b,want,masks[mi]);unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{function}(s,a,b,out,masks[mi]);unsigned actual=_mm_getcsr()&0x3f;if(memcmp(out,want,{bytes})||actual!=expected){{fprintf(stderr,\"{function} sample=%u mode=%x mask=%x flags=%x/%x\\n\",sample,mode,masks[mi],actual,expected);return 1;}}if(memcmp((char*)out+64,s,{bytes})||memcmp((char*)out+96,a,{bytes})||memcmp((char*)out+128,b,{bytes})||((uint8_t*)out)[160]!=masks[mi])return 2;{extremes}}}}}\n"
            );
            if alias == 0 && operation.extreme() {
                let qnan = if double {
                    "UINT64_C(0x7ff8123456789abc)"
                } else {
                    "0x7fc12345u"
                };
                let numeric = if matches!(operation, Operation::Minimum) {
                    "negative?denormal:0"
                } else {
                    "negative?0:denormal"
                };
                denormal_cases += &format!(
                    "{{{uint} s[{lanes}],a[{lanes}],b[{lanes}];uint64_t out[24],oracle[8];for(unsigned unordered=0;unordered<2;unordered++)for(unsigned negative=0;negative<2;negative++)for(unsigned daz=0;daz<2;daz++){{{uint} signbit=negative?({uint}){sign}:0;{uint} denormal=signbit|1;for(unsigned i=0;i<{lanes};i++){{s[i]=~({uint})0-i*19;a[i]=unordered?{qnan}:0;b[i]=denormal;}}unsigned csr=(saved&~0x807f)|(daz?0x40:0);_mm_setcsr(csr);oracle_{function}(s,a,b,oracle,1);unsigned expected_flags=_mm_getcsr()&0x3f;_mm_setcsr(csr);{function}(s,a,b,out,1);unsigned actual_flags=_mm_getcsr()&0x3f;{uint} got;memcpy(&got,out,sizeof(got));{uint} expected=daz?signbit:(unordered?denormal:({numeric}));if(got!=expected||memcmp(out,oracle,{bytes})||actual_flags!=expected_flags){{fprintf(stderr,\"{function} denormal second source unordered=%u sign=%u DAZ=%u expected=%llx got=%llx\\n\",unordered,negative,daz,(unsigned long long)expected,(unsigned long long)got);return 8;}}}}}}\n"
                );
            }
            if alias == 0 {
                let snan = if double {
                    "UINT64_C(0x7ff0123456789abc)"
                } else {
                    "0x7f812345u"
                };
                traps += &format!(
                    "{{{uint} s[{lanes}],a[{lanes}],b[{lanes}];uint64_t out[24];for(unsigned i=0;i<{lanes};i++)s[i]=a[i]=b[i]={snan};for(unsigned active=0;active<2;active++){{pid_t child=fork();if(child==0){{prctl(PR_SET_DUMPABLE,0);_mm_setcsr(saved&~0xbf);{function}(s,a,b,out,active?1:(uint8_t)~((1u<<{lanes})-1));_exit(0);}}int status;if(child<0||waitpid(child,&status,0)!=child)return 5;if(active){{if(!WIFSIGNALED(status)||WTERMSIG(status)!=SIGFPE)return 6;}}else if(!WIFEXITED(status)||WEXITSTATUS(status))return 7;}}}}\n"
                );
            }
        }
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t doubles[16]={{0,UINT64_C(0x8000000000000000),UINT64_C(0x3ff8000000000000),UINT64_C(0xbff8000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0123456789abc),1,UINT64_C(0x3ff0000000000001),UINT64_C(0x7fefffffffffffff),UINT64_C(0x0010000000000000),UINT64_C(0x3fe0000000000000),UINT64_C(0xbfe0000000000000),UINT64_C(0x4004000000000000),UINT64_C(0xc004000000000000)}};uint32_t floats[16]={{0,0x80000000u,0x3fc00000u,0xbfc00000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1,0x3f800001u,0x7f7fffffu,0x00800000u,0x3f000000u,0xbf000000u,0x40200000u,0xc0200000u}};{cases}_mm_setcsr(saved);{denormal_cases}_mm_setcsr(saved);{traps}_mm_setcsr(saved);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn vl_float_arithmetic_preserves_raw_bits_mxcsr_and_masked_exceptions() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for function in &module.functions {
        for (_, inst) in function.layout_insts() {
            if let InstData::Intrinsic { id, .. } = function.inst_data(inst) {
                assert!(id.x86_avx512_vl_float_arithmetic().is_some());
                assert!(!id.info().pure);
                assert_eq!(
                    function.inst_data(inst).memory_effects(),
                    MemoryEffects::Barrier
                );
                assert!(function.inst_data(inst).may_trap());
            }
        }
    }
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(features.avx512f && !features.avx2 && !features.fma);
    let directory = std::env::temp_dir().join(format!("keel-vl-float-{}", std::process::id()));
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
            fs::write(directory.join("vl-float.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512vl") {
                    eprintln!("SKIP native VL floating arithmetic: AVX512VL unavailable");
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
                    .arg(directory.join("vl-float.o"))
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
fn vl_float_arithmetic_requires_vl_and_rejects_extra_controls() {
    for (id, shape, operation) in CASES {
        let name = id.info().name;
        let operands = if operation.unary() {
            "v0,v2,v1"
        } else {
            "v0,v2,v0,v1"
        };
        let source = format!(
            "func @operation({shape},{shape},i8)->{shape} {{\nblock0(v0:{shape},v1:{shape},v2:i8):\n v3=intrinsic {name}({operands})\n return v3\n}}\n"
        );
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for features in [
            Features::NONE,
            Features::NONE.with_features("avx512f").unwrap(),
            Features::NONE.with_features("avx2").unwrap(),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            assert!(
                keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline)
                    .is_err(),
                "{name}"
            );
        }
        let local = keel_ir::parse(&source.replace(
            &format!("->{shape} {{"),
            &format!("->{shape} target_features(\"avx512vl\") {{"),
        ))
        .unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512f").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        keel_codegen::x86_64::compile_program_with_pipeline(&local, &EMPTY, &pipeline).unwrap();
        let disabled = keel_ir::parse(&source.replace(
            &format!("->{shape} {{"),
            &format!("->{shape} target_features(\"-avx512vl\") {{"),
        ))
        .unwrap();
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512vl").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::x86_64::compile_program_with_pipeline(&disabled, &EMPTY, &pipeline)
                .is_err()
        );
        let bad = source
            .replace(" v3=intrinsic", " v4=iconst.i32 8\n v3=intrinsic")
            .replace(&format!("({operands})"), &format!("({operands},v4)"));
        assert!(
            keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err(),
            "{name}"
        );
    }
}
