//! Bounded x86 multiply-add selection; native/oracle checks require a host lease.
use keel_codegen::select::{Machine, Selection, Target as SelectTarget};
use keel_codegen::{SelectionOptions, Target, compile_with_options};

fn source(ty: &str, factor: i64, displacement: i64) -> String {
    format!(
        "func @f({ty}) -> {ty} {{
block0(v0: {ty}):
 v1 = iconst.{ty} {factor}
 v2 = imul v0, v1
 v3 = iconst.{ty} {displacement}
 v4 = iadd v2, v3
 return v4
}}"
    )
}
fn count(text: &str, opcode: &str) -> usize {
    text.lines()
        .filter(|l| l.trim_start().starts_with(&format!("{opcode}\t")))
        .count()
}
fn selected(src: &str) -> bool {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let plan = Selection::compute_with_options(
        &module.functions[0],
        SelectTarget::X86,
        SelectionOptions::ALL,
    );
    plan.matches
        .values()
        .any(|m| matches!(m.machine, Machine::LeaMul3 { .. }))
}
#[test]
fn exact_factors_widths_displacements_and_commutations() {
    for ty in ["i32", "i64"] {
        for displacement in [i32::MIN as i64, -1, 0, 7, i32::MAX as i64] {
            let src = source(ty, 3, displacement);
            for src in [
                src.clone(),
                src.replace("imul v0, v1", "imul v1, v0"),
                src.replace("iadd v2, v3", "iadd v3, v2"),
            ] {
                assert!(selected(&src), "{src}");
            }
        }
    }
    for (ty, factor, displacement) in [
        ("i8", 3, 7),
        ("i16", 3, 7),
        ("i64", 2, 7),
        ("i64", 5, 7),
        ("i64", 9, 7),
        ("i64", -3, 7),
        ("i64", 3, 2147483648),
        ("i64", 3, -2147483649),
        ("i64", 4294967299, 7),
    ] {
        assert!(!selected(&source(ty, factor, displacement)));
    }
    // i32 constants are bit patterns; normalize before testing disp32/factor.
    assert!(selected(&source("i32", 4294967299, 4294967303)));
}
#[test]
fn shared_cross_block_and_separately_breakable_products_fall_back() {
    let src = source("i64", 3, 7);
    for negative in [
        src.replace("return v4", "v5 = iadd v4, v2\n return v5"),
        src.replace("v3 = iconst", "jump block1\nblock1:\n v3 = iconst"),
        format!(
            "file f0 = \"lea.c\"\n{}",
            src.replace("imul v0, v1", "imul v0, v1 !f0:2:1")
                .replace("iadd v2, v3", "iadd v2, v3 !f0:3:1")
        ),
    ] {
        assert!(!selected(&negative), "{negative}");
    }
}
#[test]
fn retained_actual_bridge_ir_selects_only_in_tree_modes_and_both_targets_assemble() {
    // Exact retained actual C bridge output at the approved base, not invented IR.
    // Fresh C->IR activation is a separate mandatory leased acceptance check.
    let module = keel_ir::parse(include_str!("fixtures/bridge_madd.ir")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in Target::ALL {
        for trees in [false, true] {
            for folded_operands in [false, true] {
                for register_hints in [false, true] {
                    let options = SelectionOptions {
                        trees,
                        folded_operands,
                        register_hints,
                    };
                    let text = compile_with_options(&module, target, options).unwrap();
                    if target == Target::X86_64LinuxGnu {
                        assert_eq!(count(&text, "imul"), usize::from(!trees), "{text}");
                        assert_eq!(count(&text, "lea"), usize::from(trees), "{text}");
                        if trees {
                            let lea = text
                                .lines()
                                .find(|l| l.trim_start().starts_with("lea\t"))
                                .unwrap();
                            assert!(lea.contains(" * 2") && lea.contains('7'), "{lea}");
                        }
                    }
                    let selector = keel_codegen::OptionsSelector(options);
                    let program = keel_codegen::compile_program_with_pipeline(
                        &module,
                        target,
                        &keel_codegen::CodegenPipeline {
                            selector: &selector,
                            allocator: &keel_codegen::regalloc::AllocationTier::Full,
                        },
                    )
                    .unwrap();
                    let object = keel_asm::assemble_program_object(
                        &program,
                        keel_asm::TargetFeatures::default(),
                    )
                    .unwrap();
                    assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
                }
            }
        }
    }
}
#[test]
fn aliases_resolve_leaves_and_never_publish_a_dead_root() {
    let module = keel_ir::parse(
        &source("i64", 3, 7)
            .replace("func @f(i64)", "func @f(i64, i64)")
            .replace("block0(v0: i64)", "block0(v0: i64, v9: i64)"),
    )
    .unwrap();
    let mut f = module.functions[0].clone();
    let block = f.blocks().next().unwrap();
    let params = f.block_params(block).to_vec();
    f.replace_all_uses(params[0], params[1]);
    let insts = f.block_insts(block).to_vec();
    let plan = Selection::compute_with_options(&f, SelectTarget::X86, SelectionOptions::ALL);
    assert_eq!(plan.operands(&f, insts[3]), vec![params[1]]);
    let result = f.inst_results(insts[3])[0];
    f.replace_all_uses(result, params[1]);
    let plan = Selection::compute_with_options(&f, SelectTarget::X86, SelectionOptions::ALL);
    assert!(!plan.matches.contains_key(&insts[3]));
}
