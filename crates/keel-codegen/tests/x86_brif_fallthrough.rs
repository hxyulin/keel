//! Branch inversion must skip only edges whose copies are all no-ops.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const SPILLED: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};

fn source(ty: &str, edges: bool, same: bool) -> String {
    let (setup, yes, no, params, result) = if edges {
        (
            format!("v2 = iconst.{ty} 37\n v3 = iconst.{ty} 91"),
            "(v2)",
            "(v3)",
            format!("(v4: {ty})"),
            "v4",
        )
    } else {
        (String::new(), "", "", String::new(), "v1")
    };
    // RPO puts block2 (the false edge) directly after block0.
    format!(
        "func @f({ty}, {ty}, ptr) -> {ty}{} {{
block0(v0: {ty}, v1: {ty}, v5: ptr):
 {setup}
 brif v0, block1{yes}, {}{no}
block1{params}:
 store {result}, v5
 return {result}
{} }}",
        if matches!(ty, "i8" | "i16") {
            " uext"
        } else {
            ""
        },
        if same { "block1" } else { "block2" },
        if same {
            String::new()
        } else {
            format!(
                "block2{}:\n store {}, v5\n return {}\n",
                if edges {
                    format!("(v6: {ty})")
                } else {
                    String::new()
                },
                if edges { "v6" } else { "v0" },
                if edges { "v6" } else { "v0" }
            )
        }
    )
}

fn compile(
    src: &str,
    tier: SelectionTier,
    alloc: AllocationTier,
    pools: &Pools,
) -> (String, Vec<u8>) {
    let module = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &tier,
        allocator: &alloc,
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    assert_eq!(program.stats().text_bytes, 0, "no opaque text fallback");
    let object = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object(&text).unwrap(),
        "{text}"
    );
    (text, object)
}

fn matrix(mut f: impl FnMut(SelectionTier, AllocationTier, &Pools)) {
    for tier in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for alloc in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&SPILLED, &keel_codegen::x86_64::REGISTERS] {
                f(tier, alloc, pools);
            }
        }
    }
}

#[test]
fn false_fallthrough_has_one_inverted_branch_for_every_truth_width() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        matrix(|tier, alloc, pools| {
            let (text, _) = compile(&source(ty, false, false), tier, alloc, pools);
            assert!(
                text.lines()
                    .any(|l| l.contains("\tjne\t") && l.contains("block1")),
                "{text}"
            );
            assert!(
                !text.contains("\tjmp\t"),
                "redundant fallthrough jump: {text}"
            );
            assert!(!text.contains("_else:"), "{text}");
        });
    }
}

#[test]
fn real_edge_copies_keep_landing_pads_including_wide_spills_and_same_target() {
    for ty in ["i8", "i64", "i128"] {
        for same in [false, true] {
            matrix(|tier, alloc, pools| {
                let (text, _) = compile(&source(ty, true, same), tier, alloc, pools);
                // Both edges pass different constants, including when they
                // share a target: neither may silently take the other's copy.
                assert!(text.contains("_else:"), "{text}");
                assert!(text.contains("\tje\t"), "{text}");
                assert!(text.contains("\tjmp\t"), "{text}");
            });
        }
    }
}

#[test]
fn fused_compare_and_materialized_condition_preserve_polarity() {
    for cc in [
        "eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge",
    ] {
        for materialized in [false, true] {
            let src = format!(
                "func @f(i64, i64, ptr) -> i8 uext {{
block0(v0: i64, v1: i64, v5: ptr):
 v2 = icmp.{cc} v0, v1
 brif v2, block1, block2
block1:
 {} v3 = iconst.i8 37
 return v3
block2:
 v4 = iconst.i8 91
 return v4
}}",
                if materialized { "store v2, v5\n" } else { "" }
            );
            matrix(|tier, alloc, pools| {
                let (text, _) = compile(&src, tier, alloc, pools);
                assert!(!text.contains("\tjmp\t"), "{text}");
                if materialized {
                    assert!(text.contains("\tjne\t"), "{text}");
                }
            });
        }
    }
}

#[test]
fn floating_compare_flags_keep_ordered_and_unordered_polarity() {
    for cc in ["eq", "ne", "lt", "le", "gt", "ge", "ord", "uno"] {
        let src = format!(
            "func @f(f64, f64) -> i64 {{
block0(v0: f64, v1: f64):
 v2 = fcmp.{cc} v0, v1
 brif v2, block1, block2
block1:
 v3 = iconst.i64 37
 return v3
block2:
 v4 = iconst.i64 91
 return v4
}}"
        );
        matrix(|tier, alloc, pools| {
            let (text, _) = compile(&src, tier, alloc, pools);
            assert!(!text.contains("\tjmp\t"), "{text}");
            assert!(text.contains("\tucomisd\t"), "{text}");
        });
    }
}

#[test]
fn no_op_block_arguments_allow_inversion_and_inline_asm_stays_rejected() {
    let src = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 brif v0, block1(v0), block2
block1(v2: i64):
 return v2
block2:
 return v0
}";
    // A single-predecessor parameter is an alias under the default allocator.
    matrix(|tier, alloc, pools| {
        let (text, _) = compile(src, tier, alloc, pools);
        assert!(!text.contains("\tjmp\t"), "{text}");
    });
    let src = src.replace(
        "return v2",
        "v3 = asm \"sete %b0\", \"=r\"() -> i8\n v4 = uext.i64 v3\n return v4",
    );
    let module = keel_ir::parse(&src).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(
        keel_codegen::x86_64::compile(&module)
            .unwrap_err()
            .to_string()
            .contains("inline assembly not supported")
    );
}

#[test]
fn false_edge_copies_run_inline_but_true_edge_copies_prevent_inversion() {
    let src = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 brif v0, block1, block2(v1)
block1:
 return v0
block2(v2: i64):
 return v2
}";
    matrix(|tier, alloc, pools| {
        let (text, _) = compile(src, tier, alloc, pools);
        assert!(text.contains("\tjne\t"), "{text}");
        assert!(!text.contains("_else:"), "{text}");
        assert!(!text.contains("\tjmp\t"), "{text}");
        let reversed = src.replace("block1, block2(v1)", "block2(v1), block1");
        let (text, _) = compile(&reversed, tier, alloc, pools);
        // The destination parameter reuses v0's location, so the true
        // edge must copy v1 rather than jumping straight to its return.
        assert!(text.contains("\tje\t"), "{text}");
        assert!(text.contains("\tjmp\t"), "{text}");
    });
}

#[test]
fn same_target_with_different_arguments_skips_only_the_false_copy() {
    let src = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 brif v0, block1(v0), block1(v1)
block1(v2: i64):
 return v2
}";
    matrix(|tier, alloc, pools| {
        let (text, _) = compile(src, tier, alloc, pools);
        assert!(text.contains("\tjne\t"), "{text}");
        assert!(!text.contains("\tjmp\t"), "{text}");
        assert!(!text.contains("_else:"), "{text}");
    });
}

#[test]
fn trap_stays_on_its_original_edge() {
    let src = "func @f(i64) -> i64 {
block0(v0: i64):
 brif v0, block1, block2
block1:
 trap
block2:
 return v0
}";
    matrix(|tier, alloc, pools| {
        let (text, _) = compile(src, tier, alloc, pools);
        assert!(
            text.lines()
                .any(|l| l.contains("\tjne\t") && l.contains("block1")),
            "{text}"
        );
        assert!(text.contains("\tud2"), "{text}");
        assert!(!text.contains("\tjmp\t"), "{text}");
    });
}

#[test]
fn allocator_only_edge_transfers_are_not_lost_or_moved_to_the_other_edge() {
    use keel_codegen::regalloc::{Allocation, AllocationInput, Loc, RegisterAllocator};
    struct EdgeTransfer {
        successor: usize,
        noop: bool,
    }
    impl RegisterAllocator for EdgeTransfer {
        fn name(&self) -> &'static str {
            "test-edge-transfer"
        }
        fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
            let mut alloc = AllocationTier::Full.allocate(input);
            let slot = Loc::Stack(alloc.spill_slots);
            alloc.spill_slots += 1;
            let block = input.func.entry_block().unwrap();
            let inst = *input.func.block_insts(block).last().unwrap();
            let src = alloc.locs[0][1].unwrap();
            alloc
                .edits
                .edge_mut(inst, self.successor)
                .push((if self.noop { src } else { slot }, src));
            alloc
        }
    }
    let module = keel_ir::parse(&source("i64", false, false)).unwrap();
    for successor in [0, 1] {
        for noop in [false, true] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &EdgeTransfer { successor, noop },
            };
            let text =
                keel_codegen::x86_64::compile_with_pipeline(&module, &SPILLED, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&module, &SPILLED, &pipeline)
                    .unwrap();
            assert_eq!(
                keel_asm::x86::assemble_program_object(&program).unwrap(),
                keel_asm::x86::assemble_object(&text).unwrap()
            );
            assert_eq!(text.contains("\tjmp\t"), successor == 0 && !noop, "{text}");
            assert_eq!(text.contains("\tjne\t"), successor == 1 || noop, "{text}");
            assert!(!text.contains("_else:"), "{text}");
        }
    }
}

#[cfg_attr(
    not(any(
        all(target_arch = "x86_64", target_os = "linux"),
        all(target_arch = "aarch64", target_os = "macos")
    )),
    ignore = "requires Linux x86-64 or macOS ARM64"
)]
#[test]
fn native_both_outcomes_and_edge_stores_match_independent_c_oracle() {
    use std::{fs, process::Command};
    let mut src = String::new();
    let mut c = String::from("#include <stdint.h>\n#include <assert.h>\n");
    let mut checks = String::new();
    for (ty, cty) in [
        ("i8", "uint8_t"),
        ("i16", "uint16_t"),
        ("i32", "uint32_t"),
        ("i64", "uint64_t"),
        ("i128", "__uint128_t"),
    ] {
        for edges in [false, true] {
            for same in [false, true] {
                let name = format!("f{ty}_{}_{}", edges as u8, same as u8);
                src += &source(ty, edges, same).replace("@f(", &format!("@{name}("));
                c += &format!("extern {cty} {name}({cty}, {cty}, {cty}*);\n");
                checks += &format!(
                    "{{ {cty} out=0, x=({cty})values[i], y=({cty})values[j]; {cty} want={}; assert({name}(x,y,&out)==want); assert(out==want); }}\n",
                    if edges {
                        "x ? 37 : 91"
                    } else if same {
                        "y"
                    } else {
                        "x ? y : x"
                    }
                );
            }
        }
    }
    src += "func @same_noop(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
 brif v0, block1(v0), block1(v1)
block1(v2: i64):
 return v2
}";
    c += "extern uint64_t same_noop(uint64_t,uint64_t);\n";
    checks += "{ uint64_t x=values[i], y=values[j]; assert(same_noop(x,y)==(x ? x : y)); }\n";
    src += "func @guarded_load(i64, ptr) -> i64 {
block0(v0: i64, v1: ptr):
 brif v0, block1, block2
block1:
 v2 = load.i64 v1
 return v2
block2:
 v3 = iconst.i64 91
 return v3
}";
    c += "extern uint64_t guarded_load(uint64_t,uint64_t*);\n";
    checks +=
        "{ uint64_t x=values[i]; assert(guarded_load(0,0)==91); assert(guarded_load(1,&x)==x); }\n";
    c += &format!(
        "int main(void) {{ __uint128_t values[]={{0,1,255,256,65536,((__uint128_t)1)<<64,~(__uint128_t)0}}; for(int i=0;i<7;i++) for(int j=0;j<7;j++) {{ {checks} }} }}"
    );
    let dir = std::env::temp_dir().join(format!("keel-x86-brif-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("oracle.c"), c).unwrap();
    matrix(|tier, alloc, pools| {
        // Execute on the host only. On ARM this validates the shared IR
        // fixtures and independent oracle; it makes no x86 execution claim.
        let object = if cfg!(target_arch = "x86_64") {
            compile(&src, tier, alloc, pools).1
        } else {
            let module = keel_ir::parse(&src).unwrap();
            keel_ir::verify_module(&module).unwrap();
            let pipeline = CodegenPipeline {
                selector: &tier,
                allocator: &alloc,
            };
            let pools = if pools.caller_saved.is_empty() {
                &SPILLED
            } else {
                &keel_codegen::aarch64::REGISTERS
            };
            let program =
                keel_codegen::aarch64::compile_program_with_pipeline(&module, pools, &pipeline)
                    .unwrap();
            keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
                .unwrap()
        };
        fs::write(dir.join("test.o"), object).unwrap();
        assert!(
            Command::new("cc")
                .arg("-O2")
                .arg(dir.join("oracle.c"))
                .arg(dir.join("test.o"))
                .arg("-o")
                .arg(dir.join("test"))
                .status()
                .unwrap()
                .success()
        );
        assert!(Command::new(dir.join("test")).status().unwrap().success());
    });
    fs::remove_dir_all(dir).unwrap();
}
