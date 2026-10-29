//! Memory address selection must remove an address definition, not move an effect.
use keel_codegen::{SelectionOptions, Target, compile_with_options};

fn source(ty: &str, offset: i64, tail: &str) -> String {
    let ret = if matches!(ty, "i8" | "i16") {
        "i64"
    } else {
        ty
    };
    let tail = if matches!(ty, "i8" | "i16") {
        tail.replace("return v3", "v9 = uext.i64 v3\n return v9")
            .replace("return v5", "v9 = uext.i64 v5\n return v9")
    } else {
        tail.to_owned()
    };
    format!(
        "func @f(ptr) -> {ret} {{
block0(v0: ptr):
 v1 = iconst.i64 {offset}
 v2 = ptradd v0, v1
 v3 = load.{ty} v2
 {tail}
}}"
    )
}
fn output(src: &str, target: Target, options: SelectionOptions) -> String {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    compile_with_options(&module, target, options).unwrap()
}
fn instruction_count(asm: &str, mnemonic: &str) -> usize {
    asm.lines()
        .filter(|l| l.trim_start().starts_with(&format!("{mnemonic}\t")))
        .count()
}
#[test]
fn displacement_limits_and_selection_tiers() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for (ty, offset, arm_matches) in [
            ("i8", 8, true),
            ("i8", 4095, true),
            ("i8", 4096, false),
            ("i16", 8190, true),
            ("i16", 8192, false),
            ("i32", 16380, true),
            ("i32", 16384, false),
            ("i64", 32760, true),
            ("ptr", 32760, true),
            ("ptr", 32768, false),
            ("i64", 32768, false),
            ("i64", 7, false),
            ("i8", -1, false),
            ("i8", -2147483648, false),
            ("i8", -2147483649, false),
            ("i8", 2147483648, false),
        ] {
            let src = source(ty, offset, "return v3");
            let baseline = output(&src, target, SelectionOptions::default());
            for trees in [false, true] {
                for folded_operands in [false, true] {
                    for register_hints in [false, true] {
                        let selected = output(
                            &src,
                            target,
                            SelectionOptions {
                                trees,
                                folded_operands,
                                register_hints,
                            },
                        );
                        let matches = trees
                            && if target == Target::Aarch64AppleDarwin {
                                arm_matches
                            } else {
                                i32::try_from(offset).is_ok()
                            };
                        // The original ptradd is a single add (or an add with a materialized constant).
                        assert_eq!(
                            instruction_count(&selected, "add"),
                            instruction_count(&baseline, "add") - usize::from(matches),
                            "{target:?} {ty} {offset}\n{selected}"
                        );
                    }
                }
            }
        }
    }
}
#[test]
fn shared_and_cross_block_addresses_remain_materialized() {
    let shared = source("i8", 8, "v4 = load.i8 v2\n v5 = iadd v3, v4\n return v5");
    let cross =
        source("i8", 8, "return v3").replace("v3 = load", "jump block1\nblock1:\n v3 = load");
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        for src in [&shared, &cross] {
            let selected = output(src, target, SelectionOptions::ALL);
            let baseline = output(src, target, SelectionOptions::default());
            assert_eq!(
                instruction_count(&selected, "add"),
                instruction_count(&baseline, "add"),
                "{selected}"
            );
        }
    }
}
#[test]
fn narrow_load_sign_extension_is_preserved() {
    for ty in ["i8", "i16"] {
        let src = source(ty, 8, "v4 = sext.i64 v3\n return v4").replacen(
            &format!("-> {ty}"),
            "-> i64",
            1,
        );
        for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
            let selected = output(&src, target, SelectionOptions::ALL);
            let baseline = output(&src, target, SelectionOptions::default());
            let op = if target == Target::Aarch64AppleDarwin {
                if ty == "i8" { "sxtb" } else { "sxth" }
            } else {
                "movsx"
            };
            assert_eq!(
                instruction_count(&selected, op),
                instruction_count(&baseline, op),
                "{selected}"
            );
            assert!(instruction_count(&selected, op) > 0, "{selected}");
        }
    }
}

const BOOLEAN: &str = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 v2 = icmp.slt v0, v1
 v3 = uext.i32 v2
 v4 = iconst.i32 0
 v5 = icmp.ne v3, v4
 brif v5, block1, block2
block1:
 return v0
block2:
 return v1
}";
#[test]
fn extended_integer_conditions_branch_directly() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let set = if target == Target::Aarch64AppleDarwin {
            "cset"
        } else {
            "setl"
        };
        for cc in ["ne", "eq"] {
            let src = BOOLEAN.replace("icmp.ne", &format!("icmp.{cc}"));
            let baseline = output(&src, target, SelectionOptions::default());
            let selected = output(&src, target, SelectionOptions::ALL);
            let set = if target == Target::Aarch64AppleDarwin {
                "cset"
            } else {
                "setl"
            };
            assert_eq!(instruction_count(&baseline, set), 1, "{baseline}");
            assert_eq!(instruction_count(&selected, set), 0, "{selected}");
            assert_eq!(instruction_count(&selected, "cmp"), 1, "{selected}");
        }
        // Boolean conversion is only valid for a known 0/1 producer; signed
        // narrow data, truncation, other comparisons, sharing and intervening
        // operations must preserve their original materializations.
        for src in [
            BOOLEAN.replace("uext.i32", "sext.i32"),
            BOOLEAN.replace("icmp.ne", "icmp.slt"),
            BOOLEAN.replace("iconst.i32 0", "iconst.i32 2"),
            BOOLEAN.replace("return v0", "v6 = uext.i64 v2\n return v6"),
            BOOLEAN
                .replace("v3 = uext", "v6 = iadd v0, v1\n v3 = uext")
                .replace("return v0", "return v6"),
        ] {
            let selected = output(&src, target, SelectionOptions::ALL);
            assert_eq!(instruction_count(&selected, set), 1, "{selected}");
        }
    }
}

#[test]
fn allocation_reads_base_and_compare_leaves_at_the_consumer() {
    use keel_codegen::select::{Selection, Target as SelectTarget};
    for target in [SelectTarget::Aarch64, SelectTarget::X86] {
        for src in [source("i64", 8, "return v3"), BOOLEAN.to_owned()] {
            let module = keel_ir::parse(&src).unwrap();
            let f = &module.functions[0];
            let insts = f.block_insts(f.blocks().next().unwrap());
            let selection = Selection::compute_with_options(f, target, SelectionOptions::ALL);
            let params = f.block_params(f.blocks().next().unwrap());
            if src.contains("ptradd") {
                assert!(selection.covered(insts[1]));
                assert_eq!(selection.operands(f, insts[2]), vec![params[0]]);
            } else {
                assert!(selection.covered(insts[0]));
                assert!(selection.covered(insts[1]));
                assert_eq!(selection.operands(f, insts[3]), params);
            }
        }
    }
}
#[test]
fn atomics_stay_base_only_and_volatile_accesses_stay_in_place() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let src = source("i64", 8, "return v3");
        let atomic = src.replace("load.i64", "atomic_load.i64 acquire");
        let selected = output(&atomic, target, SelectionOptions::ALL);
        assert_eq!(instruction_count(&selected, "add"), 1, "{selected}");
        let volatile = src.replace("load.i64", "load.i64 volatile align 8");
        let selected = output(&volatile, target, SelectionOptions::ALL);
        assert_eq!(instruction_count(&selected, "add"), 0, "{selected}");
        let loads = if target == Target::Aarch64AppleDarwin {
            "ldr"
        } else {
            "mov"
        };
        assert!(instruction_count(&selected, loads) > 0, "{selected}");
    }
}
#[test]
fn calls_and_other_flag_producers_prevent_condition_covers() {
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let set = if target == Target::Aarch64AppleDarwin {
            "cset"
        } else {
            "setl"
        };
        for src in [
            format!(
                "decl @clobber()\n{}",
                BOOLEAN.replace("v3 = uext", "call @clobber()\n v3 = uext")
            ),
            BOOLEAN.replace("v3 = uext", "v6 = icmp.eq v0, v1\n v3 = uext"),
            BOOLEAN.replace("v3 = uext", "jump block3\nblock3:\n v3 = uext"),
        ] {
            let selected = output(&src, target, SelectionOptions::ALL);
            assert!(instruction_count(&selected, set) > 0, "{selected}");
        }
    }
}

#[test]
fn shared_zero_operand_and_materialized_outer_condition_keep_exact_folds() {
    let src = "func @f(i32) -> i32 {
block0(v0: i32):
 v1 = iconst.i32 0
 v2 = icmp.eq v0, v1
 v3 = uext.i32 v2
 v4 = icmp.eq v3, v1
 v5 = uext.i32 v4
 return v5
}";
    for target in [Target::Aarch64AppleDarwin, Target::X86_64LinuxGnu] {
        let selected = output(src, target, SelectionOptions::ALL);
        assert_eq!(instruction_count(&selected, "cmp"), 1, "{selected}");
        let set = if target == Target::Aarch64AppleDarwin {
            "cset"
        } else {
            "setne"
        };
        assert_eq!(instruction_count(&selected, set), 1, "{selected}");
    }
}
#[test]
fn resolved_aliases_publish_the_same_base_operand() {
    use keel_codegen::select::{Selection, Target as SelectTarget};
    let module = keel_ir::parse(
        "func @f(ptr, ptr) -> i64 {
block0(v0: ptr, v1: ptr):
 v2 = iconst.i64 8
 v3 = ptradd v1, v2
 v4 = load.i64 v3
 return v4
}",
    )
    .unwrap();
    let mut f = module.functions[0].clone();
    let block = f.blocks().next().unwrap();
    let params = f.block_params(block).to_vec();
    f.replace_all_uses(params[1], params[0]);
    let insts = f.block_insts(block);
    for target in [SelectTarget::Aarch64, SelectTarget::X86] {
        let selection = Selection::compute_with_options(&f, target, SelectionOptions::ALL);
        assert_eq!(selection.operands(&f, insts[2]), vec![params[0]]);
    }
}

#[test]
fn aarch64_pointer_stack_slots_fold_their_offset() {
    let src = "decl @use(ptr)
func @f(ptr) -> ptr {
    ss0 = stack_slot 8, align 8
    ss1 = stack_slot 8, align 8
block0(v0: ptr):
    v1 = stack_addr ss0
    v2 = stack_addr ss1
    store v0, v1
    store v0, v2
    call @use(v1)
    v3 = load.ptr v1
    v4 = load.ptr v2
    v5 = ptrtoint.i64 v3
    v6 = ptrtoint.i64 v4
    v7 = iadd v5, v6
    v8 = inttoptr.ptr v7
    return v8
}";
    let asm = output(src, Target::Aarch64AppleDarwin, SelectionOptions::default());
    assert_eq!(instruction_count(&asm, "add"), 2, "{asm}");
    assert!(asm.contains("\tstr\tx9, [sp, #8]\n"), "{asm}");
    assert!(asm.contains("\tldr\tx10, [sp, #8]\n"), "{asm}");
}
