//! Scalar integer oracles cover pure VL arithmetic and floating bit transfers.
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
    MultiplyLow,
    SignedMinimum,
    UnsignedMinimum,
    SignedMaximum,
    UnsignedMaximum,
    And,
    AndNot,
    Or,
    Xor,
    Move,
}
impl Operation {
    fn unary(self) -> bool {
        matches!(self, Self::Move)
    }
    fn expression(self) -> &'static str {
        match self {
            Self::MultiplyLow => "x*y",
            Self::SignedMinimum => "((x^sign)<(y^sign)?x:y)",
            Self::UnsignedMinimum => "(x<y?x:y)",
            Self::SignedMaximum => "((x^sign)>(y^sign)?x:y)",
            Self::UnsignedMaximum => "(x>y?x:y)",
            Self::And => "x&y",
            Self::AndNot => "(~x)&y",
            Self::Or => "x|y",
            Self::Xor => "x^y",
            Self::Move => "x",
        }
    }
}
// Public shapes and arithmetic meanings do not depend on intrinsic metadata.
const CASES: [(Intrinsic, VectorType, Operation); 42] = [
    (
        Intrinsic::X86Avx512VlMaskMulld128,
        VectorType::I32x4,
        Operation::MultiplyLow,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinsd128,
        VectorType::I32x4,
        Operation::SignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinud128,
        VectorType::I32x4,
        Operation::UnsignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxsd128,
        VectorType::I32x4,
        Operation::SignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxud128,
        VectorType::I32x4,
        Operation::UnsignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndd128,
        VectorType::I32x4,
        Operation::And,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndnd128,
        VectorType::I32x4,
        Operation::AndNot,
    ),
    (
        Intrinsic::X86Avx512VlMaskOrd128,
        VectorType::I32x4,
        Operation::Or,
    ),
    (
        Intrinsic::X86Avx512VlMaskXord128,
        VectorType::I32x4,
        Operation::Xor,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinsq128,
        VectorType::I64x2,
        Operation::SignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinuq128,
        VectorType::I64x2,
        Operation::UnsignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxsq128,
        VectorType::I64x2,
        Operation::SignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxuq128,
        VectorType::I64x2,
        Operation::UnsignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndq128,
        VectorType::I64x2,
        Operation::And,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndnq128,
        VectorType::I64x2,
        Operation::AndNot,
    ),
    (
        Intrinsic::X86Avx512VlMaskOrq128,
        VectorType::I64x2,
        Operation::Or,
    ),
    (
        Intrinsic::X86Avx512VlMaskXorq128,
        VectorType::I64x2,
        Operation::Xor,
    ),
    (
        Intrinsic::X86Avx512VlMaskMoved128,
        VectorType::I32x4,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMoveq128,
        VectorType::I64x2,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMoveps128,
        VectorType::F32x4,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMovepd128,
        VectorType::F64x2,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMulld256,
        VectorType::I32x8,
        Operation::MultiplyLow,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinsd256,
        VectorType::I32x8,
        Operation::SignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinud256,
        VectorType::I32x8,
        Operation::UnsignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxsd256,
        VectorType::I32x8,
        Operation::SignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxud256,
        VectorType::I32x8,
        Operation::UnsignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndd256,
        VectorType::I32x8,
        Operation::And,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndnd256,
        VectorType::I32x8,
        Operation::AndNot,
    ),
    (
        Intrinsic::X86Avx512VlMaskOrd256,
        VectorType::I32x8,
        Operation::Or,
    ),
    (
        Intrinsic::X86Avx512VlMaskXord256,
        VectorType::I32x8,
        Operation::Xor,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinsq256,
        VectorType::I64x4,
        Operation::SignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMinuq256,
        VectorType::I64x4,
        Operation::UnsignedMinimum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxsq256,
        VectorType::I64x4,
        Operation::SignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskMaxuq256,
        VectorType::I64x4,
        Operation::UnsignedMaximum,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndq256,
        VectorType::I64x4,
        Operation::And,
    ),
    (
        Intrinsic::X86Avx512VlMaskAndnq256,
        VectorType::I64x4,
        Operation::AndNot,
    ),
    (
        Intrinsic::X86Avx512VlMaskOrq256,
        VectorType::I64x4,
        Operation::Or,
    ),
    (
        Intrinsic::X86Avx512VlMaskXorq256,
        VectorType::I64x4,
        Operation::Xor,
    ),
    (
        Intrinsic::X86Avx512VlMaskMoved256,
        VectorType::I32x8,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMoveq256,
        VectorType::I64x4,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMoveps256,
        VectorType::F32x8,
        Operation::Move,
    ),
    (
        Intrinsic::X86Avx512VlMaskMovepd256,
        VectorType::F64x4,
        Operation::Move,
    ),
];
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut cases = String::new();
    for (id, shape, operation) in CASES {
        let bytes = shape.bytes();
        let lanes = shape.lanes();
        let lane_bits = shape.lane_type().bytes().unwrap() * 8;
        let uint = if lane_bits == 32 {
            "uint32_t"
        } else {
            "uint64_t"
        };
        let name = id.info().name;
        let aliases = if operation.unary() { 2 } else { 3 };
        for alias in 0..aliases {
            let function = format!("{name}_{alias}");
            let input_a = if alias == 1 { "v5" } else { "v6" };
            let input_b = if alias == 2 { "v5" } else { "v7" };
            let args = if operation.unary() {
                format!("v5,v4,{input_a}")
            } else {
                format!("v5,v4,{input_a},{input_b}")
            };
            ir += &format!(
                "func @{function}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{shape} align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{shape} align 1 v2\n v8=intrinsic {name}({args})\n store align 1 v8,v3\n v9=iconst.i64 64\n v10=ptradd v3,v9\n store align 1 v5,v10\n v11=iconst.i64 32\n v12=ptradd v10,v11\n store align 1 v6,v12\n v13=ptradd v12,v11\n store align 1 v7,v13\n v14=ptradd v13,v11\n store align 1 v4,v14\n return\n}}\n"
            );
            c += &format!("extern void {function}(void*,void*,void*,void*,uint8_t);\n");
            let a = if alias == 1 { "merge" } else { "a" };
            let b = if alias == 2 { "merge" } else { "b" };
            let expression = operation.expression();
            let edges = if lane_bits == 32 {
                "edges32"
            } else {
                "edges64"
            };
            cases += &format!(
                "{{{uint} merge[{lanes}],a[{lanes}],b[{lanes}];unsigned char out[192];uint8_t masks[]={{0,1,0x55,0xff,(uint8_t)~((1u<<{lanes})-1)}};{uint} sign=({uint})1<<({lane_bits}-1);for(unsigned sample=0;sample<16;sample++){{for(unsigned lane=0;lane<{lanes};lane++){{merge[lane]={edges}[(lane+sample)%16];a[lane]={edges}[(lane*3+sample+1)%16];b[lane]={edges}[(lane+sample*5+7)%16];}}for(unsigned mi=0;mi<5;mi++){{memset(out,0xa5,sizeof out);unsigned csr=(saved&~0x80u)|0x3f;_mm_setcsr(csr);{function}(merge,a,b,out,masks[mi]);if(_mm_getcsr()!=csr)return 1;for(unsigned lane=0;lane<{lanes};lane++){{{uint} x={a}[lane],y={b}[lane],got;memcpy(&got,out+lane*sizeof(got),sizeof(got));{uint} expected=(masks[mi]&(1u<<lane))?({uint})({expression}):merge[lane];if(got!=expected){{fprintf(stderr,\"{function} sample=%u lane=%u mask=%x expected=%llx got=%llx\\n\",sample,lane,masks[mi],(unsigned long long)expected,(unsigned long long)got);return 2;}}}}for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5)return 3;if(memcmp(out+64,merge,{bytes})||memcmp(out+96,a,{bytes})||memcmp(out+128,b,{bytes})||out[160]!=masks[mi])return 4;_mm_setcsr(saved);}}}}}}\n"
            );
        }
    }
    for (width, shape) in [(128, "f32x4"), (256, "f32x8")] {
        ir += &format!(
            "func @mixed_move{width}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.i64x8 align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{shape} align 1 v2\n v8=intrinsic x86_avx512vl_maskmoveps{width}(v6,v4,v7)\n store align 1 v8,v3\n v9=iconst.i64 64\n v10=ptradd v3,v9\n store align 1 v5,v10\n return\n}}\n"
        );
        c += &format!("extern void mixed_move{width}(void*,void*,void*,void*,uint8_t);\n");
        cases += &format!(
            "{{uint64_t wide[8],out[16],a[4],b[4];for(unsigned i=0;i<8;i++)wide[i]=UINT64_C(0xfedcba9876543210)+i;for(unsigned i=0;i<4;i++){{a[i]=UINT64_C(0x7ff012347f812345)+i;b[i]=UINT64_C(0x80000000fff81234)-i;}}mixed_move{width}(wide,a,b,out,0xff);if(memcmp(out,b,{})||memcmp(out+8,wide,64))return 5;}}\n",
            width / 8
        );
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint32_t edges32[16]={{0,1,0x7fffffffu,0x80000000u,0xffffffffu,0x80000001u,0x7ffffffeu,0x55555555u,0xaaaaaaaau,0x7fc12345u,0x7f812345u,0xffc54321u,0xff812345u,0x3f800000u,0x80000000u,17}};uint64_t edges64[16]={{0,1,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,UINT64_C(0x8000000000000001),UINT64_C(0x7ffffffffffffffe),UINT64_C(0x5555555555555555),UINT64_C(0xaaaaaaaaaaaaaaaa),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0123456789abc),UINT64_C(0xfff8543212345678),UINT64_C(0xfff0543212345678),UINT64_C(0x3ff0000000000000),UINT64_C(0x8000000000000000),17}};{cases}_mm_setcsr(saved);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn vl_integer_and_moves_match_scalar_bits_without_mxcsr_effects() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for function in &module.functions {
        for (_, inst) in function.layout_insts() {
            if let InstData::Intrinsic { id, .. } = function.inst_data(inst) {
                assert!(id.x86_avx512_vl_integer_move().is_some());
                assert!(id.info().pure);
                assert_eq!(
                    function.inst_data(inst).memory_effects(),
                    MemoryEffects::None
                );
                assert!(!function.inst_data(inst).may_trap());
            }
        }
    }
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512vl").unwrap();
    assert!(features.avx512f && !features.avx2 && !features.fma);
    let directory =
        std::env::temp_dir().join(format!("keel-vl-integer-move-{}", std::process::id()));
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
            fs::write(directory.join("integer-move.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512vl") {
                    eprintln!("SKIP native VL integer/move: AVX512VL unavailable");
                    continue;
                }
                let output = Command::new("cc")
                    .args(["-O2", "-fno-tree-vectorize", "-mavx512f", "-mavx512vl"])
                    .arg(directory.join("oracle.c"))
                    .arg(directory.join("integer-move.o"))
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
fn vl_integer_and_moves_require_vl_and_exact_public_shapes() {
    for (id, shape, operation) in CASES {
        let operands = if operation.unary() {
            "v0,v2,v1"
        } else {
            "v0,v2,v0,v1"
        };
        let source = format!(
            "func @operation({shape},{shape},i8)->{shape} {{\nblock0(v0:{shape},v1:{shape},v2:i8):\n v3=intrinsic {}({operands})\n return v3\n}}\n",
            id.info().name
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
                "{}",
                id.info().name
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
    }
}
