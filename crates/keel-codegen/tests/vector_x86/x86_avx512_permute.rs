//! Runtime selectors, immediate lane boundaries and two-table aliases preserve bits.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type, X86Avx512PermuteKind as K};
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
        "#include <immintrin.h>\n#include <stdint.h>\n#include <string.h>\n#include <stdio.h>\n",
    );
    let mut checks = String::new();
    for id in Intrinsic::ALL {
        let Some((opcode, kind)) = id.x86_avx512_permute() else {
            continue;
        };
        let shape = id.info().returns[0].vector().unwrap();
        let data = shape.to_string();
        let q = shape.lane_type().bytes() == Some(8);
        let (mask, cmask, index, ctype, lanes, bytes) = if q {
            ("i8", "uint8_t", "i64x8", "uint64_t", 8, 8)
        } else {
            ("i16", "uint16_t", "i32x16", "uint32_t", 16, 4)
        };
        let indexed = matches!(kind, K::Full | K::Local | K::TwoIndex | K::TwoTable);
        let aliases = indexed && shape.lane_type().is_int();
        for alias in [false, true].into_iter().filter(|&a| !a || aliases) {
            let controls: &[i64] = if kind.control_index().is_some() {
                &[0, 1, 15, 16, 78, 177, 255]
            } else {
                &[0]
            };
            for &control in controls {
                let name = format!("{}_{}_{}", id.info().name, control, u8::from(alias));
                let ix = if alias { "v7" } else { "v8" };
                let args = match kind {
                    K::Full | K::Local => format!("v6,v5,v7,{ix}"),
                    K::TwoIndex | K::TwoTable => format!("v6,v5,v7,{ix},v9"),
                    K::Immediate => "v6,v5,v7,v10".into(),
                    K::PairImmediate => "v6,v5,v7,v9,v10".into(),
                    K::Pair => "v6,v5,v7,v9".into(),
                    K::Unary => "v6,v5,v7".into(),
                };
                ir += &format!(
                    "func @{name}(ptr,ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:{mask}):\n v6 = load.{data} align 1 v0\n v7 = load.{data} align 1 v1\n v8 = load.{index} align 1 v2\n v9 = load.{data} align 1 v3\n v10 = iconst.i32 {control}\n v11 = intrinsic {}({args})\n store align 1 v11,v4\n v12 = iconst.i64 64\n v13 = ptradd v4,v12\n store align 1 v7,v13\n v14 = ptradd v13,v12\n store align 1 v9,v14\n v15 = ptradd v14,v12\n store align 1 v8,v15\n v16 = ptradd v15,v12\n store align 1 v6,v16\n v17 = ptradd v16,v12\n store align 1 v5,v17\n return\n}}\n",
                    id.info().name
                );
                let assembly = match kind {
                    K::Full => format!("{opcode} %[a],%[i],%[o]"),
                    K::Local => format!("{opcode} %[i],%[a],%[o]"),
                    K::TwoIndex => format!("{opcode} %[b],%[a],%[o]"),
                    K::TwoTable => format!("{opcode} %[b],%[i],%[o]"),
                    K::Immediate => format!("{opcode} ${control},%[a],%[o]"),
                    K::PairImmediate => format!("{opcode} ${control},%[b],%[a],%[o]"),
                    K::Pair => format!("{opcode} %[b],%[a],%[o]"),
                    K::Unary => format!("{opcode} %[a],%[o]"),
                };
                let constraint = if matches!(kind, K::TwoIndex | K::TwoTable) {
                    "+&v"
                } else {
                    "=&v"
                };
                let init = match kind {
                    K::TwoIndex => "vo=vi;",
                    K::TwoTable => "vo=va;",
                    _ => "",
                };
                c += &format!(
                    "extern void {name}(void*,void*,void*,void*,void*,{cmask});\n__attribute__((noinline)) static void ref_{name}(void*o,const void*m,const void*a,const void*i,const void*b,{cmask} k){{__m512i va,vi,vb,vo;memcpy(&va,a,64);memcpy(&vi,i,64);memcpy(&vb,b,64);{init}__asm__ volatile(\"{assembly}\":[o]\"{constraint}\"(vo):[a]\"v\"(va),[i]\"v\"(vi),[b]\"v\"(vb):\"memory\");memcpy(o,&vo,64);for(unsigned j=0;j<{lanes};j++)if(!((k>>j)&1))memcpy((char*)o+j*{bytes},(const char*)m+j*{bytes},{bytes});}}\n"
                );
                let abase = if q {
                    "0x7ff0000000000001ULL"
                } else {
                    "0x7f800001U"
                };
                let bbase = if q {
                    "0xfff8000000000011ULL"
                } else {
                    "0xffc00011U"
                };
                let high = if q {
                    "0x8000000000000000ULL"
                } else {
                    "0x80000000U"
                };
                let ref_index = if alias { "a" } else { "idx" };
                checks += &format!(
                    "{{{ctype} a[{lanes}],b[{lanes}],idx[{lanes}],merge[{lanes}];unsigned char out[336],expected[64];for(unsigned sample=0;sample<5;sample++){{for(unsigned j=0;j<{lanes};j++){{a[j]=({ctype})({abase}+j*17);b[j]=({ctype})({bbase}+j*31);merge[j]=({ctype})(0xa5a59abcdef01234ULL+j);idx[j]=sample==0?j:sample==1?j+{lanes}:sample==2?~({ctype})j:sample==3?({ctype})({high}|(j*7)):({ctype})((j%6)==0?0:(j%6)==1?{lanes}-1:(j%6)==2?{lanes}:(j%6)==3?{lanes}*2-1:(j%6)==4?{lanes}*2:~({ctype})0);}}for(unsigned pattern=0;pattern<4;pattern++){{{cmask} k=pattern==0?0:pattern==1?({cmask})~0u:pattern==2?({cmask})0xaaaa:({cmask})(1u<<({lanes}-1));_mm_setcsr(0x1f80);{name}(merge,a,idx,b,out,k);unsigned status=_mm_getcsr()&63;ref_{name}(expected,merge,a,{ref_index},b,k);if(memcmp(out,expected,64)||status){{fprintf(stderr,\"{name} sample%u mask%u flags%x\\n\",sample,pattern,status);return 10;}}if(memcmp(out+64,a,64)||memcmp(out+128,b,64)||memcmp(out+192,idx,64)||memcmp(out+256,merge,64)||memcmp(out+320,&k,sizeof(k)))return 11;}}}}}}\n"
                );
            }
        }
    }
    c +=
        &format!("int main(void){{unsigned csr=_mm_getcsr();{checks}_mm_setcsr(csr);return 0;}}\n");
    (ir, c)
}
#[test]
fn avx512_permutation_runtime_immediate_aliases_and_spills() {
    let (source, harness) = sources();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx512vl && !features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-permute-{}", std::process::id()));
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
            .args(["-O1", "-mavx512f", "-c"])
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
        fs::write(dir.join("permute.o"), object).unwrap();
        if native {
            let output = Command::new("cc")
                .arg(dir.join("harness.o"))
                .arg(dir.join("permute.o"))
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
            assert!(status.success(), "permutation oracle: {status}");
        }
    }
}
#[test]
fn avx512_permutation_controls_and_effects() {
    let mut count = 0;
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_permute().is_some())
    {
        count += 1;
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure);
        assert!(!data.is_pinned() && !data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::None);
        assert!(matches!(id.info().params[1], Type::I8 | Type::I16));
    }
    assert_eq!(count, 38);
    let (source, _) = sources();
    for control in ["-1", "256"] {
        let bad = source.replace("iconst.i32 0", &format!("iconst.i32 {control}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let m = keel_ir::parse(&source).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE.with_features("avx2").unwrap(),
    };
    let p = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(keel_codegen::x86_64::compile_program_with_pipeline(&m, &EMPTY, &p).is_err());
}
