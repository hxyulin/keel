//! Sparse-switch default control flow with out-of-line case argument copies.
use keel_codegen::{CodegenPipeline, SelectionTier, regalloc::AllocationTier};

#[test]
fn dense_dispatch_tails_share_the_table_and_case_copies() {
    let cases = (0..64)
        .map(|case| format!("{case}: block{}(v4)", 5 + case % 12))
        .collect::<Vec<_>>()
        .join(", ");
    let mut source = format!(
        "func @dispatch(i64, i32, i64, i64) -> i64 cc(c) {{
block0(v0: i64, v1: i32, v2: i64, v3: i64):
    brif v1, block1, block2
block1:
    jump block3(v0, v2)
block2:
    jump block3(v0, v3)
block3(v4: i64, v5: i64):
    switch v4, block4, [{cases}]
block4:
    return v5
"
    );
    for block in 5..17 {
        source.push_str(&format!(
            "block{block}(v{}: i64):\n    return v{}\n",
            block + 2,
            block + 2
        ));
    }
    source.push('}');
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Bounded,
    };
    let pools = &keel_codegen::x86_64::REGISTERS;
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
    assert_eq!(text.matches("_table:\n").count(), 1, "{text}");
    assert_eq!(text.matches("jmp\trax").count(), 3, "{text}");

    let pools = &keel_codegen::aarch64::REGISTERS;
    let arm = keel_codegen::aarch64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let arm_program =
        keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let arm_object =
        keel_asm::assemble_program_object(&arm_program, keel_asm::TargetFeatures::default())
            .unwrap();
    assert_eq!(arm_object, keel_asm::assemble_object(&arm).unwrap());
    assert_eq!(arm.matches("_table:\n").count(), 1, "{arm}");
    assert_eq!(arm.matches("\tbr\tx17").count(), 3, "{arm}");

    let native = if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        Some(object)
    } else if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        Some(arm_object)
    } else {
        None
    };
    if let Some(native) = native {
        use std::{fs, path::Path, process::Command};
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("switch-dispatch-tails");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("dispatch.o"), native).unwrap();
        fs::write(dir.join("driver.c"), "long dispatch(long, int, long, long); int main(void) { for (long i = -1; i <= 64; ++i) for (int b = 0; b <= 1; ++b) if (dispatch(i,b,71,93) != (i >= 0 && i < 64 ? i : b ? 71 : 93)) return 1; return 0; }").unwrap();
        let output = Command::new("clang")
            .current_dir(&dir)
            .args(["driver.c", "dispatch.o", "-o", "run"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(Command::new(dir.join("run")).status().unwrap().success());
    }
}

#[test]
fn sparse_default_jumps_over_case_pads_on_both_targets() {
    let module = keel_ir::parse(
        "func @f(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 1
    switch v0, block1, [259: block2(v1), 288: block2(v0)]
block1:
    v2 = iconst.i32 0
    jump block2(v2)
block2(v3: i32):
    return v3
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let arm = keel_codegen::aarch64::compile_with_pipeline(
                &module,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::assemble_object(&arm).unwrap();
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::x86::assemble_object(&x86).unwrap();
            for (text, jump) in [(&arm, "b"), (&x86, "jmp")] {
                // An unmatched value traverses every comparison. The last
                // instruction before the first case pad must reach default,
                // rather than falling into the case's argument-copy path.
                let first_pad = text
                    .lines()
                    .position(|line| line.ends_with(':') && line.contains("_edge"))
                    .expect("case argument landing pad");
                let previous = text.lines().take(first_pad).last().unwrap().trim();
                let mut words = previous.split_whitespace();
                assert_eq!(words.next(), Some(jump), "{text}");
                assert!(
                    words
                        .next()
                        .unwrap()
                        .trim_matches('"')
                        .ends_with("f_block1"),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn x86_edges_whose_arguments_are_in_place_need_no_landing_pad() {
    // Both back edges pass block1's own parameter, so their copies are
    // no-ops and each branch can target block1 directly.
    let module = keel_ir::parse(
        "func @f(i32, i32, i32) -> i32 {
block0(v0: i32, v1: i32, v5: i32):
    jump block1(v1)
block1(v2: i32):
    v3 = iadd v2, v5
    brif v3, block2, block1(v2)
block2:
    switch v0, block3, [1: block1(v2), 2: block3, 3: block1(v2), 4: block3]
block3:
    return v3
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let x86 = keel_codegen::x86_64::compile_with_pipeline(
                &module,
                &keel_codegen::x86_64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            keel_asm::x86::assemble_object(&x86).unwrap();
            assert!(
                !x86.lines().any(|line| line.ends_with(':')
                    && (line.contains("_else") || line.contains("_edge"))),
                "{x86}"
            );
        }
    }
}

#[test]
fn masked_switches_fill_both_default_entries_before_dropping_bounds_checks() {
    use keel_codegen::{
        InstructionSelector, OptionsSelector, SelectionOptions, Target, cover::Cover,
    };
    let source = "func @bounded(i64) -> i64 cc(c) {
block0(v0:i64):
 v1 = iconst.i64 7
 v2 = and v0, v1
 switch v2, block2(v2), [1:block1(v2),2:block1(v2),3:block1(v2),4:block1(v2),5:block1(v2),6:block1(v2)]
block1(v3:i64):
 v4 = iconst.i64 10
 v5 = iadd v3, v4
 return v5
block2(v6:i64):
 v7 = iconst.i64 20
 v8 = iadd v6, v7
 return v8
}";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let mut reference = SelectionOptions::ALL;
    reference.covers.remove(Cover::BoundedSwitch);
    for target in Target::NATIVE {
        for options in [SelectionOptions::ALL, reference] {
            let selector = OptionsSelector(options);
            let selected = selector.select(
                &module.functions[0],
                keel_codegen::select::Target::Aarch64,
                &|_| vec![],
            );
            let proven = options.covers.contains(Cover::BoundedSwitch);
            // Ablation preserves the explicit checking route.
            assert_eq!(
                selected.owner(
                    module.functions[0].block_insts(module.functions[0].entry_block().unwrap())[2]
                ),
                proven.then_some(Cover::BoundedSwitch)
            );
            let assembly = keel_codegen::compile_with_pipeline(
                &module,
                target,
                &CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                },
            )
            .unwrap();
            assert_eq!(assembly.contains("\tcmp\t"), !proven, "{assembly}");
            if Some(target) == Target::host() {
                let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                    .join(format!("switch-range-{proven}"));
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("f.s"), assembly).unwrap();
                std::fs::write(dir.join("driver.c"), "#include <stdint.h>\nextern uint64_t bounded(uint64_t); int main(void) { uint64_t x=1; for(unsigned i=0;i<10000;i++){x^=x<<13;x^=x>>7;x^=x<<17;uint64_t k=x&7;if(bounded(x)!=k+((k>=1&&k<=6)?10:20))return 1;}return 0;}").unwrap();
                let build = std::process::Command::new("clang")
                    .current_dir(&dir)
                    .args(["f.s", "driver.c", "-o", "run"])
                    .output()
                    .unwrap();
                assert!(
                    build.status.success(),
                    "{}",
                    String::from_utf8_lossy(&build.stderr)
                );
                assert!(
                    std::process::Command::new(dir.join("run"))
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
        let unknown = keel_ir::parse(&source.replace("and v0, v1", "or v0, v1")).unwrap();
        let assembly =
            keel_codegen::compile_with_options(&unknown, target, SelectionOptions::ALL).unwrap();
        assert!(
            assembly.contains("\tcmp\t"),
            "unknown range lost bounds check: {assembly}"
        );
    }
}

#[test]
fn switch_bounds_merge_every_argument_even_from_the_same_predecessor() {
    use keel_codegen::{SelectionOptions, Target};
    let source = "func @merged(i64,i64,i32) -> i64 cc(c) {
entry(v0:i64,v1:i64,v2:i32):
 v3 = iconst.i64 7
 v4 = and v0, v3
 v5 = and v1, v3
 brif v2, dispatch(v4), dispatch(v5)
dispatch(v6:i64):
 switch v6, default, [0:valid,1:valid,2:valid,3:valid,4:valid,5:valid,6:valid,7:valid]
valid:
 return v6
default:
 v7 = iconst.i64 -1
 return v7
}";
    for (source, checked) in [
        (source.to_owned(), false),
        (source.replace("dispatch(v5)", "dispatch(v1)"), true),
    ] {
        let module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for target in Target::NATIVE {
            let text =
                keel_codegen::compile_with_options(&module, target, SelectionOptions::ALL).unwrap();
            assert_eq!(text.contains("\tcmp\t"), checked, "{text}");
        }
    }
}

#[test]
fn bounded_switches_keep_flag_definitions_for_opaque_observers() {
    use keel_codegen::{SelectionOptions, Target};
    for target in Target::NATIVE {
        let observers = if target == Target::X86_64LinuxGnu {
            vec![
                "v3 = asm \"sete %b0\", \"=r\"() -> i8\n v4 = uext.i64 v3",
                "v4 = intrinsic x86_rflags_read()",
            ]
        } else {
            vec!["v3 = asm \"cset %w0, eq\", \"=r\"() -> i8\n v4 = uext.i64 v3"]
        };
        for (index, observer) in observers.iter().enumerate() {
            let source = format!(
                "func @f(i64) -> i64 cc(c) {{\nblock0(v0:i64):\n v1 = iconst.i64 7\n v2 = and v0,v1\n switch v2, block2, [0:block1,1:block1,2:block1,3:block1,4:block1,5:block1,6:block1]\nblock1:\n {observer}\n return v4\nblock2:\n v5 = iconst.i64 13\n return v5\n}}"
            );
            let module = keel_ir::parse(&source).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let assembly =
                keel_codegen::compile_with_options(&module, target, SelectionOptions::ALL).unwrap();
            assert!(
                assembly.contains("\tcmp\t"),
                "flag definition disappeared: {assembly}"
            );
            if target != Target::Aarch64AppleDarwin || Target::host() != Some(target) {
                continue;
            }
            let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("switch-opaque-flags-{index}"));
            std::fs::create_dir_all(&dir).unwrap();
            // Set EQ before calling f. Its switch must replace those flags,
            // and its case's inline assembly observes the original table CMP.
            let wrapper = "\n.globl _invoke\n_invoke:\n stp x29,x30,[sp,#-16]!\n mov x29,sp\n cmp xzr,xzr\n bl _f\n ldp x29,x30,[sp],#16\n ret\n";
            std::fs::write(dir.join("f.s"), assembly + wrapper).unwrap();
            std::fs::write(dir.join("driver.c"), "extern unsigned long invoke(unsigned long); int main(void){for(unsigned long i=0;i<256;i++){unsigned long k=i&7;unsigned long expected=k==7?13:k==6;if(invoke(i)!=expected)return 1;}return 0;}").unwrap();
            let build = std::process::Command::new("clang")
                .current_dir(&dir)
                .args(["f.s", "driver.c", "-o", "run"])
                .output()
                .unwrap();
            assert!(
                build.status.success(),
                "{}",
                String::from_utf8_lossy(&build.stderr)
            );
            assert!(
                std::process::Command::new(dir.join("run"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }
}
