//! DQ mask algebra uses scalar values and exact flag/result widths.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::{FeatureSelector, Features},
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const ONE: Pools = Pools {
    caller_saved: &[7],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const FORMS: &[(&str, &str, &str)] = &[
    ("kandb", "Binary", "i8"),
    ("kandnb", "Binary", "i8"),
    ("korb", "Binary", "i8"),
    ("kxorb", "Binary", "i8"),
    ("kxnorb", "Binary", "i8"),
    ("knotb", "Unary", "i8"),
    ("kshiftlb", "Shift", "i8"),
    ("kshiftrb", "Shift", "i8"),
    ("ktestb", "Test", "i8"),
    ("kortestb", "Test", "i8"),
    ("kaddb", "Binary", "i8"),
    ("kaddw", "Binary", "i16"),
];
fn fixtures() -> (String, String) {
    let mut source = String::new();
    let mut c = String::from(
        "#include <immintrin.h>\n#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n",
    );
    let mut checks = String::new();
    for &(op, kind, ty) in FORMS {
        let word = ty == "i16";
        let ct = if word { "uint16_t" } else { "uint8_t" };
        let width = if word { 2 } else { 1 };
        let controls: &[u32] = if kind == "Shift" {
            &[0, 7, 8, 255]
        } else {
            &[0]
        };
        let aliases: &[bool] = if kind == "Binary" || kind == "Test" {
            &[false, true]
        } else {
            &[false]
        };
        for &count in controls {
            for &same in aliases {
                let name = format!("{op}_{count}_{}", u8::from(same));
                let b = if same { "v0" } else { "v1" };
                let args = if kind == "Unary" {
                    "v0".to_owned()
                } else if kind == "Shift" {
                    "v0,v3".to_owned()
                } else {
                    format!("v0,{b}")
                };
                source += &format!(
                    "func @{name}({ty},{ty},ptr) {{\nblock0(v0:{ty},v1:{ty},v2:ptr):\n v3 = iconst.i32 {count}\n v4 = intrinsic x86_avx512dq_{op}({args})\n store align 1 v4,v2\n v5 = iconst.i64 8\n v6 = ptradd v2,v5\n store align 1 v0,v6\n v7 = ptradd v6,v5\n store align 1 v1,v7\n return\n}}\n"
                );
                c += &format!("extern void {name}({ct},{ct},void*);\n");
                let b = if same { "a" } else { "b" };
                let model = match op {
                    "kandb" => format!("a&{b}"),
                    "kandnb" => format!("~a&{b}"),
                    "korb" => format!("a|{b}"),
                    "kxorb" => format!("a^{b}"),
                    "kxnorb" => format!("~(a^{b})"),
                    "knotb" => "~a".into(),
                    "kshiftlb" => {
                        if count >= 8 {
                            "0".into()
                        } else {
                            format!("a<<{count}")
                        }
                    }
                    "kshiftrb" => {
                        if count >= 8 {
                            "0".into()
                        } else {
                            format!("a>>{count}")
                        }
                    }
                    "ktestb" => format!("((a&{b})==0)|((((~a&{b})&255)==0)<<1)"),
                    "kortestb" => format!("((a|{b})==0)|(((a|{b})==255)<<1)"),
                    "kaddb" | "kaddw" => format!("a+{b}"),
                    _ => unreachable!(),
                };
                let na = if word { 8 } else { 256 };
                let nb = if word {
                    8
                } else if kind == "Unary" || kind == "Shift" {
                    1
                } else {
                    256
                };
                let av = if word { "edges[ai]" } else { "ai" };
                let bv = if word {
                    "edges[bi]"
                } else if nb == 1 {
                    "165"
                } else {
                    "bi"
                };
                checks += &format!(
                    "for(unsigned ai=0;ai<{na};ai++)for(unsigned bi=0;bi<{nb};bi++){{unsigned a={av},b={bv};unsigned char out[32];memset(out,0xa5,sizeof out);unsigned csr=(saved|63)&~0x80u;_mm_setcsr(csr);{name}(({ct})a,({ct})b,out);if(_mm_getcsr()!=csr)return 1;{ct} got,live_a,live_b,expected=({ct})({model});memcpy(&got,out,{width});memcpy(&live_a,out+8,{width});memcpy(&live_b,out+16,{width});if(got!=expected||live_a!=a||live_b!=b){{fprintf(stderr,\"{name} a%x b%x got%x expected%x\\n\",a,b,(unsigned)got,(unsigned)expected);return 2;}}for(unsigned i={width};i<8;i++)if(out[i]!=0xa5||out[8+i]!=0xa5||out[16+i]!=0xa5)return 3;for(unsigned i=24;i<32;i++)if(out[i]!=0xa5)return 4;_mm_setcsr(saved);}}\n"
                );
            }
        }
    }
    c += &format!(
        "int main(void){{unsigned saved=_mm_getcsr();unsigned edges[8]={{0,1,255,256,32767,32768,65534,65535}};{checks}_mm_setcsr(saved);return 0;}}\n"
    );
    (source, c)
}
#[test]
fn dq_mask_alu_exhaustive_bytes_flags_shifts_and_spills() {
    let (source, harness) = fixtures();
    let m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let features = Features::NONE.with_features("avx512f,avx512dq").unwrap();
    assert!(!features.avx2 && !features.fma);
    let dir = std::env::temp_dir().join(format!("keel-avx512-dq-mask-alu-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    let native = cfg!(all(target_arch = "x86_64", target_os = "linux"));
    #[cfg(target_arch = "x86_64")]
    let native = native && std::is_x86_feature_detected!("avx512dq");
    if native {
        // Compile the immutable oracle once, then relink each allocation variant.
        let output = Command::new("cc")
            .args(["-O1", "-mavx512f", "-mavx512dq", "-frounding-math", "-c"])
            .arg(dir.join("harness.c"))
            .arg("-o")
            .arg(dir.join("harness.o"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(12000)
                .collect::<String>()
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
        (SelectionTier::Full, AllocationTier::Full, &ONE, true),
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
        for opcode in [
            "kandb", "kandnb", "korb", "kxorb", "kxnorb", "knotb", "kshiftlb", "kshiftrb",
            "ktestb", "kortestb", "kaddb", "kaddw",
        ] {
            assert!(text.contains(opcode), "missing {opcode}");
        }
        let program =
            keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &p).unwrap();
        let object =
            keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
        assert_eq!(
            object,
            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
        );
        fs::write(dir.join("convert.o"), object).unwrap();
        if native {
            let output = Command::new("cc")
                .arg(dir.join("harness.o"))
                .arg(dir.join("convert.o"))
                .arg("-o")
                .arg(dir.join("run"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(12000)
                    .collect::<String>()
            );
            let status = Command::new(dir.join("run")).status().unwrap();
            assert!(status.success(), "DQ special oracle: {status}");
        }
    }
}
#[test]
fn dq_mask_alu_contracts_and_immediates() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.x86_avx512_dq_mask_alu().is_some())
        .collect();
    assert_eq!(ids.len(), 12);
    for id in ids {
        let data = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure);
        assert!(!data.is_pinned() && !data.may_trap());
        assert_eq!(data.memory_effects(), MemoryEffects::None);
    }
    let (source, _) = fixtures();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for n in [-1, 256] {
        let bad = source.replace("iconst.i32 255", &format!("iconst.i32 {n}"));
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE
            .with_features("avx512f,avx512vl,avx512bw")
            .unwrap(),
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::x86_64::compile_program_with_pipeline(&module, &EMPTY, &pipeline).is_err()
    );
}
