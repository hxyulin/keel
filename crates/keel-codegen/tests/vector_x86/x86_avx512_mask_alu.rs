//! Scalar mask contracts checked against bit arithmetic and system assembly.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects};
use std::{fs, process::Command};

struct Case {
    id: Intrinsic,
    control: Option<u8>,
    same: bool,
}
impl Case {
    fn name(&self) -> String {
        format!(
            "{}_{}_{}",
            self.id.info().name,
            self.control.unwrap_or(0),
            self.same
        )
    }
}
fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_mask_alu())
    {
        if id.has_x86_avx512_mask_shift_control() {
            for control in [0, 1, 15, 16, 255] {
                result.push(Case {
                    id,
                    control: Some(control),
                    same: false,
                });
            }
        } else {
            result.push(Case {
                id,
                control: None,
                same: false,
            });
            if id.info().params.len() == 2 {
                result.push(Case {
                    id,
                    control: None,
                    same: true,
                });
            }
        }
    }
    result
}
fn source(cases: &[Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let control = case
            .control
            .map(|control| format!(" v3 = iconst.i32 {control}\n"))
            .unwrap_or_default();
        let mut args = vec!["v0"];
        if case.control.is_some() {
            args.push("v3");
        } else if case.id.info().params.len() == 2 {
            args.push(if case.same { "v0" } else { "v1" });
        }
        source += &format!(
            "func @{}(i16,i16,ptr) {{\nblock0(v0:i16,v1:i16,v2:ptr):\n{control} v4 = intrinsic {}({})\n store align 1 v4,v2\n v5 = iconst.i64 8\n v6 = ptradd v2,v5\n store align 1 v0,v6\n v7 = ptradd v6,v5\n store align 1 v1,v7\n return\n}}\n",
            case.name(),
            case.id.info().name,
            args.join(",")
        );
    }
    source
}
fn harness(cases: &[Case]) -> String {
    let mut harness = String::from("#include <stdint.h>\n#include <string.h>\n");
    let mut checks = String::new();
    for case in cases {
        let name = case.name();
        let op = case.id.info().name.trim_start_matches("x86_avx512_");
        harness += &format!("extern void {name}(uint16_t,uint16_t,void*);\n");
        let a = "a";
        let b = if case.same { "a" } else { "b" };
        let control = case.control.unwrap_or(0);
        let expected = match op {
            "kandw" => format!("({a}&{b})"),
            "kandnw" => format!("((~{a})&{b})"),
            "korw" => format!("({a}|{b})"),
            "kxorw" => format!("({a}^{b})"),
            "kxnorw" => format!("~({a}^{b})"),
            "knotw" => format!("~{a}"),
            "kshiftlw" => {
                if control < 16 {
                    format!("((unsigned){a}<<{control})")
                } else {
                    "0".to_owned()
                }
            }
            "kshiftrw" => {
                if control < 16 {
                    format!("({a}>>{control})")
                } else {
                    "0".to_owned()
                }
            }
            "kunpckbw" => format!("(({a}&255)<<8)|({b}&255)"),
            "ktestw" => format!("(({a}&{b})==0)|((((~{a})&{b})==0)<<1)"),
            "kortestw" => format!("(({a}|{b})==0)|((({a}|{b})==65535)<<1)"),
            _ => unreachable!(),
        };
        let load = "kmovw %k[a],%%k1\\n\\t kmovw %k[b],%%k2\\n\\t ";
        let reference = if matches!(op, "ktestw" | "kortestw") {
            let instructions = if op == "kortestw" {
                "kortestw %%k2,%%k1\\n\\t sete %[z]\\n\\t setc %[c]".to_owned()
            } else {
                "kandw %%k2,%%k1,%%k3\\n\\t kortestw %%k3,%%k3\\n\\t sete %[z]\\n\\t kandnw %%k2,%%k1,%%k3\\n\\t kortestw %%k3,%%k3\\n\\t sete %[c]".to_owned()
            };
            format!(
                "unsigned char z,cf;__asm__ volatile(\"{load}{instructions}\":[z]\"=q\"(z),[c]\"=q\"(cf):[a]\"r\"((unsigned){a}),[b]\"r\"((unsigned){b}):\"k1\",\"k2\",\"k3\",\"cc\");unsigned reference=z|(cf<<1);"
            )
        } else {
            let operation = if case.control.is_some() {
                format!("{op} ${control},%%k1,%%k3")
            } else if op == "knotw" {
                format!("{op} %%k1,%%k3")
            } else {
                format!("{op} %%k2,%%k1,%%k3")
            };
            format!(
                "unsigned reference;__asm__ volatile(\"{load}{operation}\\n\\t kmovw %%k3,%k[out]\":[out]\"=r\"(reference):[a]\"r\"((unsigned){a}),[b]\"r\"((unsigned){b}):\"k1\",\"k2\",\"k3\");"
            )
        };
        let result_bytes = case.id.info().returns[0].bytes().unwrap();
        checks += &format!(
            "{{uint16_t pairs[][2]={{{{0,0}},{{65535,65535}},{{0,65535}},{{65535,0}},{{0xaaaa,0x5555}},{{0x1234,0xabcd}},{{1,0x8000}},{{0x8000,1}}}};for(unsigned p=0;p<8;p++){{uint16_t a=pairs[p][0],b=pairs[p][1],saved_a,saved_b;unsigned char out[32];memset(out,0xa5,sizeof out);{name}(a,b,out);unsigned expected=(uint16_t)({expected}),got=0;memcpy(&got,out,{result_bytes});{reference}if(got!=expected||got!=reference)return 10;memcpy(&saved_a,out+8,2);memcpy(&saved_b,out+16,2);if(saved_a!=a||saved_b!=b)return 11;for(unsigned i={result_bytes};i<8;i++)if(out[i]!=0xa5)return 12;}}}}\n"
        );
    }
    harness += &format!("int main(void){{{checks}return 0;}}\n");
    harness
}

#[test]
fn foundation_mask_algebra_metadata_and_controls() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_mask_alu())
        .collect();
    assert_eq!(ids.len(), 11);
    for id in ids {
        let instruction = InstData::Intrinsic { id, args: vec![] };
        assert!(id.info().pure);
        assert!(!instruction.may_trap());
        assert_eq!(instruction.memory_effects(), MemoryEffects::None);
    }
    let good = source(&cases());
    let module = keel_ir::parse(&good).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let selector = FeatureSelector {
        inner: &SelectionTier::Full,
        features: Features::NONE,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &AllocationTier::Full,
    };
    assert!(
        keel_codegen::compile_program_with_pipeline(
            &module,
            keel_ir::Target::X86_64LinuxGnu,
            &pipeline
        )
        .is_err()
    );
    assert!(keel_codegen::compile(&module, keel_ir::Target::Aarch64LinuxGnu).is_err());
    let dynamic = keel_ir::parse(&good.replace("v3 = iconst.i32 0", "v3 = uext.i32 v0")).unwrap();
    assert!(keel_ir::verify_module(&dynamic).is_err());
    for invalid in [-1, 256] {
        let bad = keel_ir::parse(&good.replace("iconst.i32 0", &format!("iconst.i32 {invalid}")))
            .unwrap();
        assert!(keel_ir::verify_module(&bad).is_err());
        let selector = FeatureSelector {
            inner: &SelectionTier::Full,
            features: Features::NONE.with_features("avx512f").unwrap(),
        };
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &AllocationTier::Full,
        };
        assert!(
            keel_codegen::compile_program_with_pipeline(
                &bad,
                keel_ir::Target::X86_64LinuxGnu,
                &pipeline
            )
            .is_err()
        );
    }
}

#[test]
fn foundation_mask_algebra_scalar_and_system_oracles() {
    let cases = cases();
    let mut module = keel_ir::parse(&source(&cases)).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let features = Features::NONE.with_features("avx512f").unwrap();
    assert!(!features.avx2 && !features.avx512vl);
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let one = Pools {
        caller_saved: &[6],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let directory =
        std::env::temp_dir().join(format!("keel-avx512-mask-alu-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("harness.c"), harness(&cases)).unwrap();
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for (pool_name, pools) in [
            ("normal", &keel_codegen::x86_64::REGISTERS),
            ("empty", &empty),
            ("one", &one),
        ] {
            let selector = FeatureSelector {
                inner: &SelectionTier::Full,
                features,
            };
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
            assert!(
                !text
                    .lines()
                    .any(|line| line.split_whitespace().next() == Some("ktestw")),
                "KTESTW requires DQ"
            );
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            let object =
                keel_asm::x86::assemble_program_object_with_features(&program, features).unwrap();
            assert_eq!(
                object,
                keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
            );
            let name = format!("{allocator:?}-{pool_name}");
            let path = directory.join(format!("{name}.o"));
            fs::write(&path, object).unwrap();
            if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
                #[cfg(target_arch = "x86_64")]
                if !std::is_x86_feature_detected!("avx512f") {
                    eprintln!("SKIP native: AVX512F unavailable");
                    continue;
                }
                let executable = directory.join(&name);
                let output = Command::new("cc")
                    .args(["-mavx512f", "-mno-avx512dq", "-mno-avx512bw", "-O1"])
                    .arg(directory.join("harness.c"))
                    .arg(path)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let status = Command::new(executable).status().unwrap();
                assert!(status.success(), "{name}: {status}");
            }
        }
    }
}
