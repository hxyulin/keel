//! Scalar FMA checks encoded arithmetic and the hardware destination-a upper lanes.
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
fn sources() -> (String, String) {
    let mut ir = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n#include <stdlib.h>\nstatic const uint32_t words[]={0x3f800001,0x3f7ffffe,0xbf800000,0x00800000,1,0x7f7fffff,0x80000000,0,0x7f800000,0xff800000,0x7fc12345,0x7f800001,0x3f000000,0xbf000000,0x40000000,0xc0000000};\nstatic const uint64_t qwords[]={0x3ff0000000000001ULL,0x3feffffffffffffeULL,0xbff0000000000000ULL,0x0010000000000000ULL,1,0x7fefffffffffffffULL,0x8000000000000000ULL,0,0x7ff0000000000000ULL,0xfff0000000000000ULL,0x7ff8123456789abcULL,0x7ff0000000000001ULL,0x3fe0000000000000ULL,0xbfe0000000000000ULL,0x4000000000000000ULL,0xc000000000000000ULL};\n",
    );
    let mut checks = String::new();
    for family in ["vfmadd", "vfmsub", "vfnmadd", "vfnmsub"] {
        for arrangement in [132, 213, 231] {
            for (suffix, shape, vtype, ctype, lanes, bytes, values) in [
                ("ss", "f32x4", "__m128", "uint32_t", 4, 4, "words"),
                ("sd", "f64x2", "__m128d", "uint64_t", 2, 8, "qwords"),
            ] {
                let opcode = format!("{family}{arrangement}{suffix}");
                for control in [4, 8, 9, 10, 11] {
                    let name = format!("{opcode}_{control}");
                    ir += &format!(
                        "func @{name}(ptr,ptr,ptr,ptr,ptr,i8) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:i8):\n v6 = load.{shape} align 1 v0\n v7 = load.{shape} align 1 v1\n v8 = load.{shape} align 1 v2\n v9 = load.{shape} align 1 v3\n v10 = iconst.i32 {control}\n v11 = intrinsic x86_avx512_{opcode}(v6,v5,v7,v8,v9,v10)\n store align 1 v11,v4\n v12 = iconst.i64 16\n v13 = ptradd v4,v12\n store align 1 v7,v13\n v14 = ptradd v13,v12\n store align 1 v8,v14\n v15 = ptradd v14,v12\n store align 1 v9,v15\n v16 = ptradd v15,v12\n store align 1 v6,v16\n v17 = ptradd v16,v12\n store align 1 v5,v17\n return\n}}\n"
                    );
                    let rounding = match control {
                        8 => "%{rn-sae%}, ",
                        9 => "%{rd-sae%}, ",
                        10 => "%{ru-sae%}, ",
                        11 => "%{rz-sae%}, ",
                        _ => "",
                    };
                    c += &format!(
                        "extern void {name}(void*,void*,void*,void*,void*,uint8_t);\n__attribute__((noinline)) static void ref_{name}(void*o,const void*s,const void*a,const void*b,const void*c,uint8_t k){{{vtype} va,vb,vc;memcpy(&va,a,16);memcpy(&vb,b,16);memcpy(&vc,c,16);__asm__ volatile(\"{opcode} {rounding}%[c],%[b],%[a] %{{%[k]%}}\":[a]\"+&v\"(va):[b]\"v\"(vb),[c]\"v\"(vc),[k]\"Yk\"(k):\"memory\");if(memcmp((const unsigned char*)&va+{bytes},(const unsigned char*)a+{bytes},16-{bytes})){{fputs(\"hardware scalar FMA upper lanes differ from destination a\\n\",stderr);abort();}}memcpy(o,&va,16);if(!(k&1))memcpy(o,s,{bytes});}}\n"
                    );
                    checks += &format!(
                        "{{{ctype} a[{lanes}],b[{lanes}],d[{lanes}],s[{lanes}];unsigned char out[96],expected[16];for(unsigned sample=0;sample<16;sample++){{for(unsigned i=0;i<{lanes};i++){{a[i]={values}[(sample+i)%16];b[i]={values}[(sample+i+5)%16];d[i]={values}[(sample+i+11)%16];s[i]=sample%5==1?0:sample%5==2?a[i]:sample%5==3?d[i]:sample%5==4?b[i]:({ctype})0xa5a59abcdef01234ULL;}}for(unsigned round=0;round<4;round++)for(unsigned pattern=0;pattern<5;pattern++){{uint8_t k=pattern==0?0:pattern==1?1:pattern==2?2:pattern==3?128:255;unsigned csr=0x1f80|(round<<13);_mm_setcsr(csr);{name}(s,a,b,d,out,k);unsigned status=_mm_getcsr()&63;_mm_setcsr(csr);ref_{name}(expected,s,a,b,d,k);unsigned wanted=_mm_getcsr()&63;if(memcmp(out,expected,16)||status!=wanted||(!(k&1)&&status)||({control}&8&&status)){{fprintf(stderr,\"{name} sample%u round%u pattern%u flags%x wanted%x\\n\",sample,round,pattern,status,wanted);return 10;}}if(memcmp(out+16,a,16)||memcmp(out+32,b,16)||memcmp(out+48,d,16)||memcmp(out+64,s,16)||memcmp(out+80,&k,1))return 11;}}}}}}\n"
                    );
                }
            }
        }
    }
    c +=
        &format!("int main(void){{unsigned csr=_mm_getcsr();{checks}_mm_setcsr(csr);return 0;}}\n");
    (ir, c)
}
#[test]
fn avx512_scalar_fma_encoded_order_upper_lanes_and_masks() {
    let (source, harness) = sources();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx512vl && !features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-scalar-fma-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = {
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        {
            std::is_x86_feature_detected!("avx512f")
        }
        #[cfg(not(all(target_arch = "x86_64", target_os = "linux")))]
        {
            false
        }
    };
    if native {
        let output = Command::new("cc")
            .args(["-O1", "-mavx512f", "-frounding-math", "-c"])
            .arg(dir.join("harness.c"))
            .arg("-o")
            .arg(dir.join("harness.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for (tier, alloc, pools, opt) in [
        (
            SelectionTier::Baseline,
            AllocationTier::Fast,
            &keel_codegen::x86_64::REGISTERS,
            false,
        ),
        (
            SelectionTier::Full,
            AllocationTier::Full,
            &keel_codegen::x86_64::REGISTERS,
            true,
        ),
        (SelectionTier::Full, AllocationTier::Full, &EMPTY, true),
    ] {
        let mut module = m.clone();
        if opt {
            keel_opt::optimize(&mut module).unwrap();
        }
        let selector = FeatureSelector {
            inner: &tier,
            features,
        };
        let p = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &p).unwrap();
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("scalar-fma.o"), object).unwrap();
        if native {
            let output = Command::new("cc")
                .arg(dir.join("harness.o"))
                .arg(dir.join("scalar-fma.o"))
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
            assert!(status.success(), "scalar FMA oracle: {status}");
        }
    }
}
#[test]
fn avx512_scalar_fma_contracts() {
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_scalar_fma_opcode().is_some())
    {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(!id.info().pure);
        assert!(data.is_pinned() && data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::Barrier);
        assert_eq!(id.info().params[1], keel_ir::Type::I8);
    }
    let (source, _) = sources();
    for control in ["0", "7", "12"] {
        let bad = source.replace("iconst.i32 4", &format!("iconst.i32 {control}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let m = keel_ir::parse(&source).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2,fma").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
}
