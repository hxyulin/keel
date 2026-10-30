//! Scalar EVEX oracles check every upper bit and inactive exception suppression.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut cases = String::new();
    for (suffix, shape, table, ctype, uint, lanes, bits) in [
        (
            "ss",
            "f32x4",
            "i32x4",
            "__m128",
            "uint32_t",
            4,
            "0,0x80000000u,0x3f800000u,0xc0000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x00800000u,0x3f000000u,0xbf000000u,0x3fc00000u,0xbfc00000u,0x40200000u,0xc0200000u,0x7f812345u,0xffc54321u",
        ),
        (
            "sd",
            "f64x2",
            "i64x2",
            "__m128d",
            "uint64_t",
            2,
            "0,0x8000000000000000ULL,0x3ff0000000000000ULL,0xc000000000000000ULL,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x0010000000000000ULL,0x3fe0000000000000ULL,0xbfe0000000000000ULL,0x3ff8000000000000ULL,0xbff8000000000000ULL,0x4004000000000000ULL,0xc004000000000000ULL,0x7ff0123456789abcULL,0xfff8543212345678ULL",
        ),
    ] {
        let packed = if suffix == "ss" { "ps" } else { "pd" };
        for op in [
            "add",
            "sub",
            "mul",
            "div",
            "sqrt",
            "min",
            "max",
            "cmp",
            "getexp",
            "getmant",
            "scalef",
            "rcp14",
            "rsqrt14",
            "roundscale",
            "fixupimm",
        ] {
            let er = matches!(op, "add" | "sub" | "mul" | "div" | "sqrt" | "scalef");
            let compare = op == "cmp";
            let controls: &[i32] = if matches!(op, "rcp14" | "rsqrt14") {
                &[4]
            } else if er {
                &[4, 8, 9, 10, 11]
            } else {
                &[4, 8]
            };
            let immediates: &[i32] = match op {
                "getmant" => &[0, 10],
                "roundscale" => &[3, 0x48],
                "fixupimm" => &[0, 255],
                "cmp" => &[0, 4, 7, 3, 17, 31],
                _ => &[0],
            };
            for &control in controls {
                for &imm in immediates {
                    for alias in [false, true] {
                        let name = format!("scalar_{op}_{suffix}_{control}_{imm}_{alias}");
                        let second = if op == "fixupimm" { table } else { shape };
                        let a = if alias { "v5" } else { "v6" };
                        let mut arguments = if compare {
                            format!("{a},v7,v4,v8,v9")
                        } else {
                            format!("v5,v4,{a},v7")
                        };
                        if !compare {
                            if matches!(op, "getmant" | "roundscale" | "fixupimm") {
                                arguments += ",v8";
                            }
                            if !matches!(op, "rcp14" | "rsqrt14") {
                                arguments += ",v9";
                            }
                        }
                        ir += &format!(
                            "func @{name}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v5=load.{shape} align 1 v0\n v6=load.{shape} align 1 v1\n v7=load.{second} align 1 v2\n v8=iconst.i32 {imm}\n v9=iconst.i32 {control}\n v10=intrinsic x86_avx512_{op}{suffix}({arguments})\n store align 1 v10,v3\n v11=iconst.i64 16\n v12=ptradd v3,v11\n store align 1 v5,v12\n v13=ptradd v12,v11\n store align 1 v6,v13\n v14=ptradd v13,v11\n store align 1 v7,v14\n return\n}}\n"
                        );
                        c += &format!("extern void {name}(void*,void*,void*,void*,uint8_t);\n");
                        let a = if alias { "s" } else { "a" };
                        let expr = match op {
                            "cmp" => {
                                format!("_mm_mask_cmp_round_{suffix}_mask(k,{a},b,{imm},{control})")
                            }
                            "rcp14" | "rsqrt14" => format!("_mm_mask_{op}_{suffix}(s,k,{a},b)"),
                            "getmant" => format!(
                                "_mm_mask_getmant_round_{suffix}(s,k,{a},b,{},{},{control})",
                                imm & 3,
                                imm >> 2
                            ),
                            "roundscale" => format!(
                                "_mm_mask_roundscale_round_{suffix}(s,k,{a},b,{imm},{control})"
                            ),
                            "fixupimm" => format!(
                                "_mm_mask_fixupimm_round_{suffix}(s,k,{a},t,{imm},{control})"
                            ),
                            _ => format!("_mm_mask_{op}_round_{suffix}(s,k,{a},b,{control})"),
                        };
                        let finish = if compare {
                            format!("*(uint8_t*)out={expr};")
                        } else {
                            format!("{ctype} r={expr};_mm_storeu_{packed}(out,r);")
                        };
                        c += &format!(
                            "__attribute__((noinline)) static void oracle_{name}(void*sp,void*ap,void*bp,void*out,uint8_t k){{{ctype} s=_mm_loadu_{packed}(sp),a=_mm_loadu_{packed}(ap),b=_mm_loadu_{packed}(bp);__m128i t=_mm_loadu_si128(bp);{finish}}}\n"
                        );
                        cases += &format!(
                            "{{{uint} bits[16]={{{bits}}},s[{lanes}],a[{lanes}],b[{lanes}],want[{lanes}]={{0}},out[4*{lanes}]={{0}};uint8_t masks[]={{0,1,0xfe,0xff}};for(unsigned shift=0;shift<16;shift++){{for(unsigned i=0;i<{lanes};i++){{s[i]=bits[(i+shift)%16];a[i]=bits[(i+shift*3+1)%16];b[i]={};}}for(unsigned mi=0;mi<4;mi++){{unsigned csr=(saved&~0x603f)|((shift%4)<<13);_mm_setcsr(csr);oracle_{name}(s,a,b,want,masks[mi]);unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{name}(s,a,b,out,masks[mi]);unsigned actual=_mm_getcsr()&0x3f;if(memcmp(out,want,{})||actual!=expected){{fprintf(stderr,\"{name} shift=%u mask=%x flags=%x/%x\\n\",shift,masks[mi],actual,expected);return 1;}}if(memcmp(out+{lanes},s,16)||memcmp(out+2*{lanes},a,16)||memcmp(out+3*{lanes},b,16))return 2;}}}}}}\n",
                            if op == "fixupimm" {
                                format!("({uint})0x76543210u+({uint})i*0x11111111u")
                            } else {
                                "bits[(i+shift+5)%16]".into()
                            },
                            if compare { 1 } else { 16 }
                        );
                    }
                }
            }
        }
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();{cases}_mm_setcsr(saved);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn scalar_evex_masks_upper_payloads_controls_and_mxcsr() {
    let (source, c) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for f in &module.functions {
        for (_, i) in f.layout_insts() {
            if let InstData::Intrinsic { id, .. } = f.inst_data(i) {
                if id.x86_avx512_scalar().is_some() {
                    assert!(!id.info().pure);
                    assert_eq!(f.inst_data(i).memory_effects(), MemoryEffects::Barrier);
                    assert!(f.inst_data(i).may_trap());
                }
            }
        }
    }
    assert!(keel_codegen::x86_64::compile(&module).is_err());
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-scalar-evex-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), c).unwrap();
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&keel_codegen::x86_64::REGISTERS, &EMPTY] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &alloc,
            };
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            fs::write(dir.join("scalar.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native scalar EVEX");
                    continue;
                }
                let output = Command::new("cc")
                    .args(["-O2", "-mavx512f", "-frounding-math", "-ffp-contract=off"])
                    .arg(dir.join("harness.c"))
                    .arg(dir.join("scalar.o"))
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
                assert!(status.success(), "{alloc:?}: {status}");
            }
        }
    }
    for control in [-1, 0, 7, 12, 256] {
        let bad = source.replace("iconst.i32 4", &format!("iconst.i32 {control}"));
        assert!(
            keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err(),
            "{control}"
        );
    }
}
