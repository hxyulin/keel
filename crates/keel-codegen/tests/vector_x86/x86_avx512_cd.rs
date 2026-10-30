//! Conflict checks use all earlier source lanes, independently of the output mask.
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
// Public API meanings and types are explicit rather than inferred from compiler metadata.
const CASES: [(Intrinsic, VectorType, bool); 12] = [
    (
        Intrinsic::X86Avx512CdMaskConflictd128,
        VectorType::I32x4,
        true,
    ),
    (
        Intrinsic::X86Avx512CdMaskConflictq128,
        VectorType::I64x2,
        true,
    ),
    (
        Intrinsic::X86Avx512CdMaskLzcntd128,
        VectorType::I32x4,
        false,
    ),
    (
        Intrinsic::X86Avx512CdMaskLzcntq128,
        VectorType::I64x2,
        false,
    ),
    (
        Intrinsic::X86Avx512CdMaskConflictd256,
        VectorType::I32x8,
        true,
    ),
    (
        Intrinsic::X86Avx512CdMaskConflictq256,
        VectorType::I64x4,
        true,
    ),
    (
        Intrinsic::X86Avx512CdMaskLzcntd256,
        VectorType::I32x8,
        false,
    ),
    (
        Intrinsic::X86Avx512CdMaskLzcntq256,
        VectorType::I64x4,
        false,
    ),
    (
        Intrinsic::X86Avx512CdMaskConflictd512,
        VectorType::I32x16,
        true,
    ),
    (
        Intrinsic::X86Avx512CdMaskConflictq512,
        VectorType::I64x8,
        true,
    ),
    (
        Intrinsic::X86Avx512CdMaskLzcntd512,
        VectorType::I32x16,
        false,
    ),
    (
        Intrinsic::X86Avx512CdMaskLzcntq512,
        VectorType::I64x8,
        false,
    ),
];
// Source scalar types are part of the public broadcast contract.
const BROADCASTS: [(Intrinsic, VectorType, &str); 6] = [
    (
        Intrinsic::X86Avx512CdBroadcastmb128,
        VectorType::I64x2,
        "i8",
    ),
    (
        Intrinsic::X86Avx512CdBroadcastmw128,
        VectorType::I32x4,
        "i16",
    ),
    (
        Intrinsic::X86Avx512CdBroadcastmb256,
        VectorType::I64x4,
        "i8",
    ),
    (
        Intrinsic::X86Avx512CdBroadcastmw256,
        VectorType::I32x8,
        "i16",
    ),
    (
        Intrinsic::X86Avx512CdBroadcastmb512,
        VectorType::I64x8,
        "i8",
    ),
    (
        Intrinsic::X86Avx512CdBroadcastmw512,
        VectorType::I32x16,
        "i16",
    ),
];
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut checks = String::new();
    for (id, shape, conflict) in CASES {
        let bytes = shape.bytes();
        let lanes = shape.lanes();
        let bits = bytes * 8 / u32::from(lanes);
        let uint = if bits == 32 { "uint32_t" } else { "uint64_t" };
        let mask = if lanes == 16 { "i16" } else { "i8" };
        let cmask = if lanes == 16 { "uint16_t" } else { "uint8_t" };
        let width = bytes * 8;
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            _ => "_mm512",
        };
        let api = if conflict { "conflict" } else { "lzcnt" };
        for alias in [false, true] {
            let name = format!("{}_{}", id.info().name, u8::from(alias));
            let input = if alias { "v4" } else { "v5" };
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:{mask}):\n v4=load.{shape} align 1 v0\n v5=load.{shape} align 1 v1\n v6=load.i64x8 align 1 v0\n v7=intrinsic {}(v4,v3,{input})\n store align 1 v7,v2\n v8=iconst.i64 64\n v9=ptradd v2,v8\n store align 1 v6,v9\n v10=ptradd v9,v8\n store align 1 v4,v10\n v11=ptradd v10,v8\n store align 1 v5,v11\n return\n}}\n",
                id.info().name
            );
            c += &format!(
                "extern void {name}(void*,void*,void*,{cmask});\nstatic void oracle_{name}(void*s,void*a,void*out,{cmask} k){{__m{width}i x,y;memcpy(&x,s,{bytes});memcpy(&y,a,{bytes});x={prefix}_mask_{api}_epi{bits}(x,k,{});memcpy(out,&x,{bytes});}}\n",
                if alias { "x" } else { "y" }
            );
            let source = if alias { "s" } else { "a" };
            let expression = if conflict {
                format!(
                    "for(unsigned j=0;j<i;j++)if({source}[j]=={source}[i])expected|=({uint})1<<j;"
                )
            } else {
                format!(
                    "expected={bits};for(unsigned j=0;j<{bits};j++)if(({source}[i]>>({bits}-1-j))&1){{expected=j;break;}}"
                )
            };
            checks += &format!(
                "{{{uint} s[64/sizeof({uint})],a[64/sizeof({uint})],want[{lanes}];unsigned char out[256];{cmask} masks[]={{0,1,2,0x55,0xaaaa,({cmask})~0u,({cmask})~((1u<<{lanes})-1)}};for(unsigned sample=0;sample<7;sample++){{for(unsigned i=0;i<64/sizeof({uint});i++){{s[i]=sample==0?7:sample==1?0:sample==2?({uint})1<<(i%{bits}):sample==3?({uint})~0u:sample==4?(i%2):sample==5?(i%3):({uint})1<<({bits}-1);}}for(unsigned i=0;i<64/sizeof({uint});i++)a[i]=s[(i+1)%(64/sizeof({uint}))];for(unsigned mi=0;mi<7;mi++){{unsigned csr=saved|0x3f;_mm_setcsr(csr);memset(out,0xa5,sizeof out);oracle_{name}(s,a,want,masks[mi]);{name}(s,a,out,masks[mi]);if(_mm_getcsr()!=csr)return 1;for(unsigned i=0;i<{lanes};i++){{{uint} expected=0,got;{expression}if(!(masks[mi]&(1u<<i)))expected=s[i];memcpy(&got,out+i*sizeof(got),sizeof(got));if(got!=expected||got!=want[i]){{fprintf(stderr,\"{name} sample=%u mask=%x lane=%u expected=%llx got=%llx\\n\",sample,masks[mi],i,(unsigned long long)expected,(unsigned long long)got);return 2;}}}}for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5)return 3;if(memcmp(out+64,s,64)||memcmp(out+128,s,{bytes})||memcmp(out+192,a,{bytes}))return 4;_mm_setcsr(saved);}}}}}}\n"
            );
        }
    }
    for (id, shape, scalar) in BROADCASTS {
        let name = id.info().name;
        let width = shape.bytes() * 8;
        let bytes = shape.bytes();
        let lanes = shape.lanes();
        let (uint, cmask, api, maximum) = if scalar == "i8" {
            ("uint64_t", "uint8_t", "broadcastmb_epi64", 255)
        } else {
            ("uint32_t", "uint16_t", "broadcastmw_epi32", 65535)
        };
        let prefix = match width {
            128 => "_mm",
            256 => "_mm256",
            _ => "_mm512",
        };
        ir += &format!(
            "func @{name}(ptr,ptr,{scalar}) {{\nblock0(v0:ptr,v1:ptr,v2:{scalar}):\n v3=load.i64x8 align 1 v0\n v4=intrinsic {name}(v2)\n store align 1 v4,v1\n v5=iconst.i64 64\n v6=ptradd v1,v5\n store align 1 v3,v6\n v7=ptradd v6,v5\n store align 1 v2,v7\n return\n}}\n"
        );
        c += &format!("extern void {name}(void*,void*,{cmask});\n");
        checks += &format!(
            "{{uint64_t wide[8]={{0,1,2,3,4,5,6,UINT64_MAX}};unsigned char out[192];unsigned masks[]={{0,1,0x7f,0x80,0xff,0x7fff,0x8000,0xffff}};for(unsigned mi=0;mi<8;mi++){{{cmask} k=({cmask})masks[mi];__m{width}i oracle={prefix}_{api}(k);{uint} expected=({uint})(k&{maximum}u);unsigned csr=saved|0x3f;_mm_setcsr(csr);memset(out,0xa5,sizeof out);{name}(wide,out,k);if(_mm_getcsr()!=csr||memcmp(out,&oracle,{bytes}))return 5;for(unsigned lane=0;lane<{lanes};lane++){{{uint} got;memcpy(&got,out+lane*sizeof got,sizeof got);if(got!=expected){{fprintf(stderr,\"{name} mask=%x lane=%u expected=%llx got=%llx\\n\",k,lane,(unsigned long long)expected,(unsigned long long)got);return 6;}}}}for(unsigned i={bytes};i<64;i++)if(out[i]!=0xa5)return 7;if(memcmp(out+64,wide,64)||memcmp(out+128,&k,sizeof k))return 8;_mm_setcsr(saved);}}}}\n"
        );
    }
    c += &format!("int main(void){{unsigned saved=_mm_getcsr();{checks}return 0;}}\n");
    (ir, c)
}
#[test]
fn cd_conflict_and_leading_zero_match_independent_and_system_oracles() {
    let (source, oracle) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for function in &module.functions {
        for (_, inst) in function.layout_insts() {
            if let InstData::Intrinsic { id, .. } = function.inst_data(inst) {
                assert!(id.x86_avx512_cd().is_some());
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
    let features = Features::NONE.with_features("avx512cd,avx512vl").unwrap();
    assert!(features.avx512f && !features.avx2 && !features.lzcnt);
    let directory = std::env::temp_dir().join(format!("keel-cd-{}", std::process::id()));
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
            fs::write(directory.join("cd.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512cd")
                    || !std::is_x86_feature_detected!("avx512vl")
                {
                    eprintln!("SKIP native CD: unavailable");
                    continue;
                }
                let output = Command::new("cc")
                    .args([
                        "-O2",
                        "-fno-tree-vectorize",
                        "-mavx512f",
                        "-mavx512cd",
                        "-mavx512vl",
                    ])
                    .arg(directory.join("oracle.c"))
                    .arg(directory.join("cd.o"))
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
fn cd_requires_cd_and_narrow_forms_require_vl_per_function() {
    for (id, shape, _) in CASES {
        let mask = if shape == VectorType::I32x16 {
            "i16"
        } else {
            "i8"
        };
        let source = format!(
            "func @operation({shape},{shape},{mask})->{shape} {{\nblock0(v0:{shape},v1:{shape},v2:{mask}):\n v3=intrinsic {}(v0,v2,v1)\n return v3\n}}\n",
            id.info().name
        );
        let compile = |source: &str, features| {
            let m = keel_ir::parse(source).unwrap();
            keel_ir::verify_module(&m).unwrap();
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            };
            keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &pipeline)
        };
        for features in [
            Features::NONE,
            Features::NONE.with_features("avx512f,avx512vl").unwrap(),
        ] {
            assert!(compile(&source, features).is_err());
        }
        let cd = Features::NONE.with_features("avx512cd").unwrap();
        assert_eq!(compile(&source, cd).is_ok(), shape.bytes() == 64);
        let f = Features::NONE.with_features("avx512f").unwrap();
        let enabled = source.replace(
            &format!("->{shape} {{"),
            &format!("->{shape} target_features(\"avx512cd,avx512vl\") {{"),
        );
        compile(&enabled, f).unwrap();
        let disabled = source.replace(
            &format!("->{shape} {{"),
            &format!("->{shape} target_features(\"-avx512cd\") {{"),
        );
        assert!(
            compile(
                &disabled,
                Features::NONE.with_features("avx512cd,avx512vl").unwrap()
            )
            .is_err()
        );
    }
    for (id, shape, scalar) in BROADCASTS {
        let source = format!(
            "func @operation({scalar})->{shape} {{\nblock0(v0:{scalar}):\n v1=intrinsic {}(v0)\n return v1\n}}\n",
            id.info().name
        );
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
        assert!(
            compile(
                &source,
                Features::NONE.with_features("avx512f,avx512vl").unwrap()
            )
            .is_err()
        );
        assert_eq!(
            compile(&source, Features::NONE.with_features("avx512cd").unwrap()).is_ok(),
            shape.bytes() == 64
        );
        let enabled = source.replace(
            &format!("->{shape} {{"),
            &format!("->{shape} target_features(\"avx512cd,avx512vl\") {{"),
        );
        compile(&enabled, Features::NONE).unwrap();
        for feature in ["-avx512cd", "-avx512vl"] {
            let disabled = source.replace(
                &format!("->{shape} {{"),
                &format!("->{shape} target_features(\"{feature}\") {{"),
            );
            assert_eq!(
                compile(
                    &disabled,
                    Features::NONE.with_features("avx512cd,avx512vl").unwrap()
                )
                .is_err(),
                feature == "-avx512cd" || shape.bytes() < 64
            );
        }
    }
}
