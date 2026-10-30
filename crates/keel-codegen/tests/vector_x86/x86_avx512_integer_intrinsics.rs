//! Raw integer controls agree with scalar bit semantics and system intrinsics.
use keel_asm_ir::x86::Features;
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use std::{fs, process::Command};

struct Case {
    id: Intrinsic,
    control: Option<u8>,
    alias: bool,
}
impl Case {
    fn name(&self) -> String {
        format!(
            "{}_{}_{}",
            self.id.info().name,
            self.control.unwrap_or(0),
            self.alias
        )
    }
    fn bits(&self) -> usize {
        if self.id.info().returns[0] == Type::I8
            || self.id.info().returns[0]
                .vector()
                .is_some_and(|v| v.lanes() == 8)
        {
            64
        } else {
            32
        }
    }
    fn operation(&self) -> String {
        let name = self
            .id
            .info()
            .name
            .trim_start_matches("x86_avx512_")
            .trim_start_matches("mask");
        if matches!(
            self.id,
            Intrinsic::X86Avx512MaskMuldq | Intrinsic::X86Avx512MaskMuludq
        ) {
            name.to_owned()
        } else {
            name[..name.len() - 1].to_owned()
        }
    }
}
fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for id in Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_integer())
    {
        let bits = if id.info().returns[0]
            .vector()
            .is_some_and(|v| v.lanes() == 8)
        {
            64
        } else {
            32
        };
        if let Some(index) = id.x86_avx512_integer_control() {
            let controls = if index == 5 {
                vec![0, 0x96, 0xca, 255]
            } else {
                vec![0, bits - 1, bits, 255]
            };
            for control in controls {
                cases.push(Case {
                    id,
                    control: Some(control),
                    alias: false,
                });
                if index == 5 && control == 0x96 {
                    cases.push(Case {
                        id,
                        control: Some(control),
                        alias: true,
                    });
                }
            }
        } else {
            cases.push(Case {
                id,
                control: None,
                alias: false,
            });
        }
    }
    cases
}
fn source(cases: &[Case]) -> String {
    let mut source = String::new();
    for case in cases {
        let info = case.id.info();
        let vector = if info.returns[0].is_vector() {
            info.returns[0]
        } else {
            info.params[0]
        };
        let mask = if case.bits() == 32 { "i16" } else { "i8" };
        let input = if matches!(
            case.id,
            Intrinsic::X86Avx512MaskMuldq | Intrinsic::X86Avx512MaskMuludq
        ) {
            "i32x16".to_owned()
        } else {
            vector.to_string()
        };
        let mut args = if case.id.is_x86_avx512_integer_test() {
            vec!["v7", "v8", "v5"]
        } else {
            vec!["v6", "v5", "v7"]
        };
        if case.id.x86_avx512_integer_control() == Some(5) {
            args.extend([if case.alias { "v7" } else { "v8" }, "v9", "v10"]);
        } else if case.control.is_some() {
            args.push("v10");
        } else if info.params.len() == 4 {
            args.push("v8");
        }
        let control = case
            .control
            .map(|control| format!(" v10 = iconst.i32 {control}\n"))
            .unwrap_or_default();
        source += &format!(
            "func @{}(ptr,ptr,ptr,ptr,ptr,{mask}) {{\nblock0(v0:ptr,v1:ptr,v2:ptr,v3:ptr,v4:ptr,v5:{mask}):\n v6 = load.{vector} align 1 v0\n v7 = load.{input} align 1 v1\n v8 = load.{input} align 1 v2\n v9 = load.{input} align 1 v3\n{control} v11 = intrinsic {}({})\n store align 1 v11,v4\n v12 = iconst.i64 64\n v13 = ptradd v4,v12\n store align 1 v6,v13\n v14 = ptradd v13,v12\n store align 1 v7,v14\n v15 = ptradd v14,v12\n store align 1 v8,v15\n v16 = ptradd v15,v12\n store align 1 v9,v16\n v17 = ptradd v16,v12\n store align 1 v5,v17\n return\n}}\n",
            case.name(),
            info.name,
            args.join(",")
        );
    }
    source
}
fn harness(cases: &[Case]) -> String {
    let mut harness = String::from(
        r#"
#include <immintrin.h>
#include <stdint.h>
#include <string.h>
#include <stdio.h>
static uint64_t scalar(unsigned operation,uint64_t a,uint64_t b,uint64_t c,unsigned control,unsigned bits) {
 uint64_t mask=bits==64?UINT64_MAX:UINT32_MAX,sign=UINT64_C(1)<<(bits-1),result=0;unsigned n=(unsigned)b&(bits-1);
 switch(operation) {
 case 0:for(unsigned i=0;i<bits;i++){unsigned index=((a>>i)&1)*4+((b>>i)&1)*2+((c>>i)&1);result|=(uint64_t)((control>>index)&1)<<i;}return result;
 case 1:n=control&(bits-1); /* immediate left rotation */
 case 3:return n?((a<<n)|(a>>(bits-n)))&mask:a;
 case 2:n=control&(bits-1); /* immediate right rotation */
 case 4:return n?((a>>n)|(a<<(bits-n)))&mask:a;
 case 5:return b>=bits?0:(a<<b)&mask;
 case 6:return b>=bits?0:a>>b;
 case 7:if(b>=bits)return a&sign?mask:0;if(!b)return a;return (a>>b)|((a&sign)?mask<<(bits-b):0);
 case 8:return a&sign?(-a)&mask:a;
 case 9:return (uint64_t)((int64_t)(int32_t)(uint32_t)a*(int64_t)(int32_t)(uint32_t)b);
 case 10:return (uint64_t)(uint32_t)a*(uint64_t)(uint32_t)b;
 default:return 0;
 }
}
"#,
    );
    let mut checks = String::new();
    for case in cases {
        let bits = case.bits();
        let lanes = 512 / bits;
        let ctype = format!("uint{bits}_t");
        let mask = if bits == 32 { "uint16_t" } else { "uint8_t" };
        harness += &format!(
            "extern void {}(void*,void*,void*,void*,void*,{mask});\n",
            case.name()
        );
        let operation = case.operation();
        let opnum = match operation.as_str() {
            "ternlog" => 0,
            "rol" => 1,
            "ror" => 2,
            "rolv" => 3,
            "rorv" => 4,
            "sllv" => 5,
            "srlv" => 6,
            "srav" => 7,
            "abs" => 8,
            "muldq" => 9,
            "muludq" => 10,
            "test" | "testn" => 11,
            _ => panic!("{operation}"),
        };
        let control = case.control.unwrap_or(0);
        let b = if case.alias { "a" } else { "b" };
        let system = match operation.as_str() {
            "ternlog" => format!(
                "_mm512_mask_mov_epi{bits}(m,k,_mm512_ternarylogic_epi{bits}(a,{b},c,{control}))"
            ),
            "rol" | "ror" => format!("_mm512_mask_{operation}_epi{bits}(m,k,a,{control})"),
            "rolv" | "rorv" | "sllv" | "srlv" | "srav" => {
                format!("_mm512_mask_{operation}_epi{bits}(m,k,a,b)")
            }
            "abs" => format!("_mm512_mask_abs_epi{bits}(m,k,a)"),
            "test" | "testn" => format!("_mm512_mask_{operation}_epi{bits}_mask(k,a,b)"),
            "muldq" => "_mm512_mask_mul_epi32(m,k,a,b)".to_owned(),
            "muludq" => "_mm512_mask_mul_epu32(m,k,a,b)".to_owned(),
            _ => unreachable!(),
        };
        let counts = if (3..=7).contains(&opnum) {
            "bv[i]=(uint64_t[]){0,1,BITS-1,BITS,BITS+1,255,UINT64_MAX,UINT64_C(0x8000000000000000)}[i%8];".replace("BITS",&bits.to_string())
        } else {
            String::new()
        };
        let result_check = if case.id.is_x86_avx512_integer_test() {
            let predicate = if operation == "test" { "!=0" } else { "==0" };
            format!(
                "{mask} expected=0,got,reference={system};for(unsigned i=0;i<{lanes};i++)if(((k>>i)&1)&&((av[i]&bv[i]){predicate}))expected|=1u<<i;memcpy(&got,out,sizeof got);if(got!=expected||got!=reference)return 10;for(unsigned i=sizeof got;i<64;i++)if(out[i]!=0xa5)return 11;"
            )
        } else {
            let rhs = if case.alias { "av[i]" } else { "bv[i]" };
            format!(
                "{ctype} expected[{lanes}],reference[{lanes}];for(unsigned i=0;i<{lanes};i++)expected[i]=((k>>i)&1)?({ctype})scalar({opnum},av[i],{rhs},cv[i],{control},{bits}):mv[i];_mm512_storeu_si512(reference,{system});if(memcmp(out,expected,64)||memcmp(out,reference,64))return 12;"
            )
        };
        checks += &format!(
            "{{{ctype} mv[{lanes}],av[{lanes}],bv[{lanes}],cv[{lanes}];unsigned char out[384];for(unsigned i=0;i<{lanes};i++){{mv[i]=({ctype})UINT64_C(0x1234abcd9876fedc)+i;av[i]=({ctype})(UINT64_C(0x8000000080000000)+(uint64_t[]){{0,1,UINT64_MAX,0x7fffffff,0xffffffff,0x55555555,0xaaaaaaaa,0}}[i%8]);bv[i]=({ctype})(UINT64_C(0xa5558000ff000000)^((uint64_t)i*0x100001237));cv[i]=({ctype})(UINT64_C(0x55aa7fff0000ffff)-i*73);{counts}}}__m512i m=_mm512_loadu_si512(mv),a=_mm512_loadu_si512(av),b=_mm512_loadu_si512(bv),c=_mm512_loadu_si512(cv);{mask} patterns[]={{0,({mask})~0u,({mask})0xa55a,1u<<({lanes}-1)}};for(unsigned p=0;p<4;p++){{{mask} k=patterns[p],saved;memset(out,0xa5,sizeof out);{}(mv,av,bv,cv,out,k);{result_check}memcpy(&saved,out+320,sizeof saved);if(memcmp(out+64,mv,64)||memcmp(out+128,av,64)||memcmp(out+192,bv,64)||memcmp(out+256,cv,64)||saved!=k)return 13;}}}}\n",
            case.name()
        );
    }
    harness += &format!("int main(void){{{checks}return 0;}}\n");
    harness
}

#[test]
fn avx512_integer_controls_metadata_and_immediate_bounds() {
    let ids: Vec<_> = Intrinsic::ALL
        .into_iter()
        .filter(|id| id.is_x86_avx512_integer())
        .collect();
    assert_eq!(ids.len(), 24);
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
    let dynamic = keel_ir::parse(&good.replace("v10 = iconst.i32 0", "v10 = load.i32 v0")).unwrap();
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
fn avx512_integer_controls_native_scalar_and_system_oracles() {
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
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[0],
        float_callee_saved: &[],
    };
    let directory = std::env::temp_dir().join(format!(
        "keel-avx512-integer-controls-{}",
        std::process::id()
    ));
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
                    .args(["-mavx512f", "-mno-avx512dq", "-O1"])
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
