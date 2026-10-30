//! Scalar conversion/flag oracles retain XMM payloads and observe MXCSR.
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
fn fixtures() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut cases = String::new();
    for id in Intrinsic::ALL
        .iter()
        .copied()
        .filter(|id| id.x86_avx512_scalar_convert().is_some())
    {
        let (opcode, index) = id.x86_avx512_scalar_convert().unwrap();
        let info = id.info();
        let flags = opcode.contains("comis");
        let int_to_float = opcode.starts_with("vcvtsi2") || opcode.starts_with("vcvtusi2");
        let float_width = index == 4;
        let fp_to_int = info.returns[0].is_int() && !flags;
        for control in [4, 8, 9, 10, 11]
            .into_iter()
            .filter(|&value| id.valid_x86_avx512_scalar_convert_control(value))
        {
            let name = format!("{}_{}", info.name, control);
            let target = info.returns[0];
            let mut body = format!(" v5=load.{} align 1 v0\n", info.params[0]);
            let arguments = if int_to_float || flags {
                body += &format!(" v6=load.{} align 1 v1\n", info.params[1]);
                "v5,v6,v9"
            } else if float_width {
                body += &format!(
                    " v6=load.{} align 1 v1\n v7=load.{} align 1 v2\n",
                    info.params[2], info.params[3]
                );
                "v5,v4,v6,v7,v9"
            } else {
                "v5,v9"
            };
            ir += &format!(
                "func @{name}(ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:i8):\n v20=load.i64x2 align 1 v0\n v21=load.i64x2 align 1 v1\n v22=load.i64x2 align 1 v2\n{body} v9=iconst.i32 {control}\n v10=intrinsic {}({arguments})\n store align 1 v10,v3\n v11=iconst.i64 16\n v12=ptradd v3,v11\n store align 1 v20,v12\n v13=ptradd v12,v11\n store align 1 v21,v13\n v14=ptradd v13,v11\n store align 1 v22,v14\n return\n}}\n",
                info.name
            );
            c += &format!("extern void {name}(void*,void*,void*,void*,uint8_t);\n");
            let mut oracle = String::new();
            let input_double = if float_width {
                opcode == "vcvtsd2ss"
            } else {
                opcode.contains("sd")
            };
            let fp =
                if opcode.contains("sd") && !float_width || float_width && opcode == "vcvtss2sd" {
                    "pd"
                } else {
                    "ps"
                };
            let vector = if fp == "pd" { "__m128d" } else { "__m128" };
            if int_to_float {
                let bits = info.params[1].bytes().unwrap() * 8;
                let unsigned = opcode.starts_with("vcvtusi");
                let ct = format!("{}int{bits}_t", if unsigned { "u" } else { "" });
                let sf = if fp == "pd" { "sd" } else { "ss" };
                let letter = if unsigned { "u" } else { "i" };
                oracle += &format!("{vector} a=_mm_loadu_{fp}(sp);{ct} x;memcpy(&x,ap,sizeof(x));");
                let expr = if bits == 32 && sf == "sd" {
                    format!("_mm_cvt{}32_sd(a,x)", if unsigned { "u" } else { "si" })
                } else {
                    format!("_mm_cvt_round{letter}{bits}_{sf}(a,x,{control})")
                };
                oracle += &format!("{vector} r={expr};_mm_storeu_{fp}(out,r);");
            } else if fp_to_int {
                let bits = target.bytes().unwrap() * 8;
                let unsigned = opcode.contains("2usi");
                let ct = format!("{}int{bits}_t", if unsigned { "u" } else { "" });
                let sf = if input_double { "sd" } else { "ss" };
                let tr = if opcode.starts_with("vcvtt") { "t" } else { "" };
                let sign = if unsigned { "u" } else { "i" };
                oracle += &format!(
                    "{vector} a=_mm_loadu_{fp}(sp);{ct} r=_mm_cvt{tr}_round{sf}_{sign}{bits}(a,{control});memcpy(out,&r,sizeof(r));"
                );
            } else if float_width {
                let sourcefp = if input_double { "pd" } else { "ps" };
                let sourcevec = if input_double { "__m128d" } else { "__m128" };
                let sf = if input_double { "sd_ss" } else { "ss_sd" };
                oracle += &format!(
                    "{vector} s=_mm_loadu_{fp}(sp),a=_mm_loadu_{fp}(ap);{sourcevec} b=_mm_loadu_{sourcefp}(bp);{vector} r=_mm_mask_cvt_round{sf}(s,k,a,b,{control});_mm_storeu_{fp}(out,r);"
                );
            } else {
                let sae = if control == 8 { "%{sae%}, " } else { "" };
                oracle += &format!(
                    "{vector} a=_mm_loadu_{fp}(sp),b=_mm_loadu_{fp}(ap);unsigned char cf,pf,zf;__asm__ volatile(\"{opcode} {sae}%4,%3;setb %0;setp %1;sete %2\":\"=qm\"(cf),\"=qm\"(pf),\"=qm\"(zf):\"x\"(a),\"x\"(b):\"cc\",\"memory\");*(uint8_t*)out=cf|(pf<<2)|(zf<<6);"
                );
            }
            c += &format!(
                "__attribute__((noinline)) static void oracle_{name}(void*sp,void*ap,void*bp,void*out,uint8_t k){{{oracle}}}\n"
            );
            let bytes = target.bytes().unwrap();
            let masks = if float_width { "0,1,0xfe,0xff" } else { "0xff" };
            cases += &format!(
                "{{uint64_t s[2],a[2],b[2],out[8]={{0}},want[2]={{0}};uint8_t masks[]={{{masks}}};for(unsigned sample=0;sample<16;sample++){{s[0]={};s[1]=UINT64_C(0xfff876547fc12345);a[0]={};a[1]=UINT64_C(0x7ff1234580000000);b[0]={};b[1]=UINT64_C(0xfff543217fc98765);for(unsigned rounding=0;rounding<4;rounding++)for(unsigned mi=0;mi<sizeof(masks);mi++){{unsigned csr=(saved&~0x603f)|(rounding<<13);_mm_setcsr(csr);oracle_{name}(s,a,b,want,masks[mi]);unsigned expected=_mm_getcsr()&0x3f;_mm_setcsr(csr);{name}(s,a,b,out,masks[mi]);unsigned actual=_mm_getcsr()&0x3f;if(memcmp(out,want,{bytes})||actual!=expected){{fprintf(stderr,\"{name} sample=%u round=%u mask=%x flags=%x/%x\\n\",sample,rounding,masks[mi],actual,expected);return 1;}}if(memcmp(out+2,s,16)||memcmp(out+4,a,16)||memcmp(out+6,b,16))return 2;}}}}}}\n",
                if int_to_float {
                    "UINT64_C(0x7ff543217fc12345)"
                } else if input_double {
                    "doubles[sample]"
                } else {
                    "floats[sample]"
                },
                if int_to_float {
                    "integers[sample]"
                } else if flags {
                    if input_double {
                        "doubles[(sample+5)%16]"
                    } else {
                        "floats[(sample+5)%16]"
                    }
                } else {
                    "UINT64_C(0x7ff98765ffc34567)"
                },
                if input_double {
                    "doubles[sample]"
                } else {
                    "floats[sample]"
                }
            );
        }
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();uint64_t integers[16]={{0,1,0x7fffffff,0x80000000,0xffffffff,0x100000001,UINT64_C(0x7fffffffffffffff),UINT64_C(0x8000000000000000),UINT64_MAX,16777217,9007199254740993ULL,UINT64_C(0x100000000000001),3,17,31,63}};uint64_t doubles[16]={{0,UINT64_C(0x8000000000000000),UINT64_C(0x3ff8000000000000),UINT64_C(0xbff8000000000000),UINT64_C(0x41e0000000000000),UINT64_C(0x43e0000000000000),UINT64_C(0x43f0000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8123456789abc),UINT64_C(0x7ff0123456789abc),1,UINT64_C(0x3fe0000000000000),UINT64_C(0xbfe0000000000000),UINT64_C(0x3ff0000010000000),UINT64_C(0xc3e0000000000001)}};uint64_t floats[16]={{0,0x80000000u,0x3fc00000u,0xbfc00000u,0x4f000000u,0x5f000000u,0x5f800000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1,0x3f000000u,0xbf000000u,0x3f800001u,0xdf000001u}};{cases}_mm_setcsr(saved);return 0;}}\n"
    );
    (ir, c)
}
#[test]
fn scalar_evex_conversion_flags_upper_bits_and_mxcsr() {
    let (source, c) = fixtures();
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for f in &module.functions {
        for (_, i) in f.layout_insts() {
            if let InstData::Intrinsic { id, .. } = f.inst_data(i) {
                if id.x86_avx512_scalar_convert().is_some() {
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
    let dir = std::env::temp_dir().join(format!("keel-scalar-convert-{}", std::process::id()));
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
            fs::write(dir.join("convert.o"), object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native scalar EVEX conversions");
                    continue;
                }
                let output = Command::new("cc")
                    .args(["-O2", "-mavx512f", "-frounding-math", "-ffp-contract=off"])
                    .arg(dir.join("harness.c"))
                    .arg(dir.join("convert.o"))
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
