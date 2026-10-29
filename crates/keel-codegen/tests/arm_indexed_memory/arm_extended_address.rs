use keel_asm::{TargetFeatures, assemble_object, assemble_program_object};
use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::{CodegenPipeline, SelectionTier};

const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
fn source(name: &str, signed: bool, shift: u8) -> String {
    format!(
        "func @{name}(ptr,i32)->ptr cc(c) {{
block0(v0:ptr,v1:i32):
 v2 = {}.i64 v1
 v3 = iconst.i64 {shift}
 v4 = shl v2,v3
 v5 = ptradd v0,v4
 return v5
}}",
        if signed { "sext" } else { "uext" }
    )
}
fn compile(
    src: &str,
    sel: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let pipeline = CodegenPipeline {
        selector: &sel,
        allocator: &alloc,
    };
    let s = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let p = keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    let obj = assemble_program_object(&p, TargetFeatures::default()).unwrap();
    assert_eq!(obj, assemble_object(&s).unwrap(), "{s}");
    (s, obj)
}
#[test]
fn exact_i32_extensions_and_scales_have_object_parity_in_all_tiers() {
    for signed in [false, true] {
        for shift in 0..=4 {
            for sel in [SelectionTier::Baseline, SelectionTier::Full] {
                for alloc in [AllocationTier::Fast, AllocationTier::Full] {
                    for pools in [&EMPTY, &keel_codegen::aarch64::REGISTERS] {
                        let (s, _) = compile(&source("f", signed, shift), sel, alloc, pools);
                        assert!(s.contains(if signed { ", sxtw" } else { ", uxtw" }), "{s}");
                        assert!(!s.contains("\tlsl\t"), "{s}");
                    }
                }
            }
        }
    }
}
#[test]
fn shared_extensions_out_of_range_scales_and_intervening_effects_stay_explicit() {
    for src in [
        source("f", true, 5),
        source("f", false, 4).replace(" return v5", " v6 = inttoptr.ptr v2\n return v6"),
        source("f", true, 4).replace(" v3 =", " store v1,v0\n v3 ="),
    ] {
        let (s, _) = compile(&src, SelectionTier::Baseline, AllocationTier::Full, &EMPTY);
        assert!(!s.contains(", uxtw") && !s.contains(", sxtw"), "{s}");
    }
}
#[test]
fn custom_constraints_and_distinct_locations_preserve_original_definitions() {
    use keel_codegen::{
        InstructionSelector,
        regalloc::{Constraint, OperandKind, ValuePart},
        select::{Selection, Target},
    };
    use keel_ir::{EntityRef, Function, InstData, Value};
    struct Constrained(usize);
    impl InstructionSelector for Constrained {
        fn name(&self) -> &'static str {
            "extended-add-boundary"
        }
        fn select(
            &self,
            f: &Function,
            t: Target,
            folds: &dyn Fn(&InstData) -> Vec<Value>,
        ) -> Selection {
            let mut p = SelectionTier::Baseline.select(f, t, folds);
            let i = f.layout_insts().nth(self.0).unwrap().1;
            let mut args = Vec::new();
            f.inst_data(i).for_each_operand(|v| args.push(v));
            p.set_constraint(
                i,
                ValuePart::low(args[0]),
                OperandKind::Use,
                Constraint::Register,
            );
            p
        }
    }
    for at in [0, 2, 3] {
        let m = keel_ir::parse(&source("f", false, 4)).unwrap();
        let selector = Constrained(at);
        let alloc = AllocationTier::Full;
        let pipeline = CodegenPipeline {
            selector: &selector,
            allocator: &alloc,
        };
        let s = keel_codegen::aarch64::compile_with_pipeline(
            &m,
            &keel_codegen::aarch64::REGISTERS,
            &pipeline,
        )
        .unwrap();
        assert!(!s.contains(", uxtw"), "{s}");
        let mut m = m;
        m.files.push("address.c".into());
        let f = &mut m.functions[0];
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        for (n, i) in insts.into_iter().enumerate() {
            f.set_inst_loc(
                i,
                Some(keel_ir::SourceLoc {
                    file: keel_ir::SourceFile::new(0),
                    line: if n == at { 11 } else { 10 },
                    col: 1,
                    scope: None,
                }),
            );
        }
        let s = keel_codegen::aarch64::compile_with_pipeline(
            &m,
            &keel_codegen::aarch64::REGISTERS,
            &CodegenPipeline {
                selector: &SelectionTier::Baseline,
                allocator: &alloc,
            },
        )
        .unwrap();
        assert!(!s.contains(", uxtw"), "{s}");
    }
}
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_signed_unsigned_wraparound_and_spilled_leaves_match_integer_oracle() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-extended-add-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut ir = String::new();
    let mut declarations = String::new();
    let mut checks = String::new();
    for signed in [false, true] {
        for shift in 0..=4 {
            let name = format!("f{}_{}", u8::from(signed), shift);
            ir.push_str(
                &source(&name, signed, shift)
                    .replace("ptr,i32", "ptr,i64")
                    .replace("v1:i32", "v1:i64")
                    .replace(" v2 =", " v100 = trunc.i32 v1\n v2 =")
                    .replace(".i64 v1", ".i64 v100"),
            );
            ir.push('\n');
            declarations.push_str(&format!("extern void *{name}(void*,int64_t);\n"));
            checks.push_str(&format!(
                "if((uintptr_t){name}((void*)b,x)!=(b+(({cast})<<{shift}))) return __LINE__;\n",
                cast = if signed {
                    "(uint64_t)(int64_t)(int32_t)x"
                } else {
                    "(uint64_t)(uint32_t)x"
                }
            ));
        }
    }
    let c = format!(
        "#include <stdint.h>\n{declarations}\nint main(void){{ uint64_t state=1;for(int j=0;j<20000;j++){{state^=state<<13;state^=state>>7;state^=state<<17;uint64_t b=state;int64_t x=(int64_t)state;if(j<5){{int32_t edge[]={{0,1,-1,INT32_MIN,INT32_MAX}};x=edge[j];}}{checks}}}return 0;}}"
    );
    fs::write(dir.join("driver.c"), c).unwrap();
    for alloc in [AllocationTier::Fast, AllocationTier::Full] {
        for (n, pools) in [&EMPTY, &keel_codegen::aarch64::REGISTERS]
            .into_iter()
            .enumerate()
        {
            let (_, obj) = compile(&ir, SelectionTier::Full, alloc, pools);
            fs::write(dir.join("f.o"), obj).unwrap();
            let exe = dir.join(format!("run{n}"));
            assert!(
                Command::new("/usr/bin/clang")
                    .arg("-O2")
                    .arg(dir.join("driver.c"))
                    .arg(dir.join("f.o"))
                    .arg("-o")
                    .arg(&exe)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(Command::new(&exe).status().unwrap().success());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
