use keel_ir::{Block, InstData, Module};

// Reduced source fixture for the retained C reload_restrict header/body shape.
// The C fixture alongside this test is the frontend/native activation input.
const LOOP: &str = "func @kernel(ptr, ptr, i64, i64) -> i64 cc(c) restrict_params(0, 1) {
block0(v0: ptr, v1: ptr, v2: i64, v3: i64):
 v4 = iconst.i64 0
 jump block1(v4, v3)
block1(v5: i64, v6: i64):
 v7 = icmp.ult v5, v2
 brif v7, block2, block3(v6)
block2:
 v8 = load.i64 v0
 v9 = iadd v6, v8
 store v9, v1
 v10 = iconst.i64 1
 v11 = iadd v5, v10
 jump block1(v11, v9)
block3(v12: i64):
 return v12
}";

fn optimize(source: &str) -> Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["loop-memory"]).unwrap();
    let text = m.to_string();
    keel_ir::verify_module(&keel_ir::parse(&text).unwrap()).unwrap();
    m
}

fn load_block(m: &Module) -> Block {
    let f = &m.functions[0];
    f.layout_insts()
        .find_map(|(b, i)| matches!(f.inst_data(i), InstData::Load { .. }).then_some(b))
        .unwrap()
}

#[test]
fn guarded_load_dominates_body_and_false_edge_bypasses_it_with_exit_arguments() {
    let mut m = optimize(LOOP);
    let f = &m.functions[0];
    let cfg = keel_ir::dominators::ControlFlowGraph::compute(f);
    let dom = keel_ir::dominators::DominatorTree::compute(f, &cfg);
    let load = load_block(&m);
    let body = f
        .blocks()
        .find(|&b| {
            f.block_insts(b)
                .iter()
                .any(|&i| matches!(f.inst_data(i), InstData::Store { .. }))
        })
        .unwrap();
    assert_ne!(load, body);
    assert!(dom.dominates(load, body));
    let entry = f.entry_block().unwrap();
    let InstData::Brif { dests, .. } = f.inst_data(*f.block_insts(entry).last().unwrap()) else {
        panic!("entry guard")
    };
    assert_eq!(dests[0].block, load);
    assert_eq!(dests[1].args, vec![f.block_params(entry)[3]]);
    assert!(!dom.dominates(load, dests[1].block));
    let once = m.to_string();
    keel_opt::run_passes(&mut m, &["loop-memory"]).unwrap();
    assert_eq!(once, m.to_string());
    keel_opt::optimize(&mut m).unwrap(); // verifier runs after every cleanup pass
}

#[test]
fn preserves_opposite_branch_orientation_and_clones_integer_exit_computation() {
    let source = LOOP
        .replace(
            "v7 = icmp.ult v5, v2",
            "v30 = iconst.i64 7\n v31 = iadd v6, v30\n v7 = icmp.uge v5, v2",
        )
        .replace(
            "brif v7, block2, block3(v6)",
            "brif v7, block3(v31), block2",
        );
    let m = optimize(&source);
    let f = &m.functions[0];
    let entry = f.entry_block().unwrap();
    let InstData::Brif { dests, .. } = f.inst_data(*f.block_insts(entry).last().unwrap()) else {
        panic!()
    };
    assert_eq!(dests[1].block, load_block(&m));
    let keel_ir::ValueDef::Result(i, _) = f.value_def(dests[0].args[0]) else {
        panic!()
    };
    assert_eq!(f.inst_block(i), entry);
}

#[test]
fn unknown_aliases_barriers_and_earlier_faults_leave_loop_unchanged() {
    let cases = [
        LOOP.replace(" restrict_params(0, 1)", ""),
        LOOP.replace("restrict_params(0, 1)", "restrict_params(0)"),
        LOOP.replace("store v9, v1", "store v9, v0"),
        LOOP.replace("load.i64 v0", "load.i64 volatile v0"),
        LOOP.replace("store v9", "store volatile v9"),
        LOOP.replace(" v8 = load", " store v6, v1\n v8 = load"),
        LOOP.replace(" v8 = load", " v30 = udiv v6, v5\n v8 = load"),
        LOOP.replace(" v8 = load", " v30 = load.i8 v1\n v8 = load"),
        LOOP.replace(
            " v8 = load",
            " v30 = atomic_load.i64 relaxed v1\n v8 = load",
        ),
        LOOP.replace(" store v9", " fence seq_cst\n store v9"),
        format!(
            "decl @effect()\n{}",
            LOOP.replace(" store v9", " call @effect()\n store v9")
        ),
        LOOP.replace(" store v9", " intrinsic stack_restore(v1)\n store v9"),
        LOOP.replace(" v7 = icmp", " v30 = load.i64 v0\n v7 = icmp"),
        LOOP.replace(" v7 = icmp", " v30 = udiv v6, v5\n v7 = icmp"),
    ];
    for source in cases {
        let original = keel_ir::parse(&source).unwrap().to_string();
        assert_eq!(original, optimize(&source).to_string(), "{source}");
    }
}

#[test]
fn conditional_loads_multiple_exits_and_loop_local_addresses_are_rejected() {
    let cases = [
        LOOP.replace(
            "block2:\n v8",
            "block2:\n brif v7, block4, block3(v6)\nblock4:\n v8",
        ),
        LOOP.replace(
            "v8 = load.i64 v0",
            "v30 = ptradd v0, v5\n v8 = load.i64 v30",
        ),
        LOOP.replace(
            "jump block1(v11, v9)",
            "brif v7, block1(v11, v9), block3(v9)",
        ),
    ];
    for source in cases {
        let before = keel_ir::parse(&source).unwrap().to_string();
        assert_eq!(before, optimize(&source).to_string(), "{source}");
    }
}

#[test]
fn readonly_restricted_aliases_are_not_assumed_unequal_and_nontrapping_is_not_a_barrier_waiver() {
    // The read may alias a second read; there is no modifying access here.
    let source = LOOP.replace(" store v9, v1\n", "");
    assert_ne!(
        load_block(&optimize(&source)),
        load_block(&keel_ir::parse(&source).unwrap())
    );
    let source = LOOP
        .replace("load.i64 v0", "load.i64 notrap v0")
        .replace(" store v9", " fence seq_cst\n store v9");
    assert_eq!(
        keel_ir::parse(&source).unwrap().to_string(),
        optimize(&source).to_string()
    );
}

#[test]
fn named_pass_only() {
    assert!(keel_opt::find_pass("loop-memory").is_some());
    assert!(!keel_opt::DEFAULT_PIPELINE.contains(&"loop-memory"));
}

#[test]
fn repairs_direct_header_liveouts_in_return_exit_without_rewriting_loop_uses() {
    for source in [
        LOOP.replace("block3(v6)", "block3")
            .replace("block3(v12: i64):\n return v12", "block3:\n return v6"),
        LOOP.replace(" v7 = icmp", " v30 = iadd v6, v5\n v7 = icmp")
            .replace("block3(v6)", "block3")
            .replace(
                "block3(v12: i64):\n return v12",
                "block3:\n v31 = iadd v30, v6\n return v31",
            ),
    ] {
        let m = optimize(&source);
        let f = &m.functions[0];
        let entry = f.entry_block().unwrap();
        let InstData::Brif { dests, .. } = f.inst_data(*f.block_insts(entry).last().unwrap())
        else {
            panic!()
        };
        let exit = dests[1].block;
        assert_eq!(
            f.block_params(exit).len(),
            if source.contains("v30") { 2 } else { 1 }
        );
        let header = f
            .blocks()
            .find(|&b| {
                f.block_params(b).len() == 2
                    && matches!(
                        f.block_insts(b).last().map(|&i| f.inst_data(i)),
                        Some(InstData::Brif { .. })
                    )
                    && b != entry
            })
            .unwrap();
        let original_iv = f.block_params(header)[0];
        assert!(f.block_insts(header).iter().any(|&i| matches!(f.inst_data(i), InstData::Icmp { args, .. } if args.contains(&original_iv))));
        let mut cleanup = m.clone();
        keel_opt::optimize(&mut cleanup).unwrap();
    }
}

#[test]
fn locations_are_preserved_on_guard_clones_and_moved_load() {
    let source = format!(
        "file f0 = \"loop.c\"\n{}",
        LOOP.replace("icmp.ult v5, v2", "icmp.ult v5, v2 !f0:4:8")
            .replace("load.i64 v0", "load.i64 v0 !f0:5:12")
    );
    let m = optimize(&source);
    let f = &m.functions[0];
    let mut comparisons = 0;
    for (_, i) in f.layout_insts() {
        match f.inst_data(i) {
            InstData::Icmp { .. } => {
                comparisons += 1;
                assert_eq!(f.inst_loc(i).unwrap().line, 4);
            }
            InstData::Load { .. } => assert_eq!(f.inst_loc(i).unwrap().line, 5),
            _ => {}
        }
    }
    assert_eq!(comparisons, 2);
}

#[test]
fn nested_loops_are_skipped() {
    let source = LOOP.replace(" jump block1(v4, v3)", " jump block4(v4)\nblock4(v40: i64):\n v41 = icmp.ult v40, v2\n brif v41, block5, block6\nblock5:\n jump block1(v4, v3)")
        .replace("block3(v12: i64):\n return v12", "block3(v12: i64):\n v42 = iconst.i64 1\n v43 = iadd v40, v42\n jump block4(v43)\nblock6:\n return v3");
    assert_eq!(
        keel_ir::parse(&source).unwrap().to_string(),
        optimize(&source).to_string()
    );
}

#[test]
fn existing_exit_args_and_multiple_direct_liveouts_preserve_exit_effect_order() {
    let source = LOOP
        .replace(" v7 = icmp", " v30 = iadd v6, v5\n v7 = icmp")
        .replace(
            " return v12",
            " v31 = iadd v30, v6\n store v12, v1\n v32 = iadd v31, v12\n return v32",
        );
    let m = optimize(&source);
    let f = &m.functions[0];
    let entry = f.entry_block().unwrap();
    let InstData::Brif { dests, .. } = f.inst_data(*f.block_insts(entry).last().unwrap()) else {
        panic!()
    };
    let exit = dests[1].block;
    assert_eq!(f.block_params(exit).len(), 3);
    assert_eq!(dests[1].args.len(), 3);
    assert_eq!(dests[1].args[0], f.block_params(entry)[3]);
    assert_eq!(dests[1].args[2], f.block_params(entry)[3]);
    let ops: Vec<_> = f
        .block_insts(exit)
        .iter()
        .map(|&i| match f.inst_data(i) {
            InstData::Binary { .. } => "add",
            InstData::Store { .. } => "store",
            InstData::Return { .. } => "return",
            _ => "unexpected",
        })
        .collect();
    assert_eq!(ops, ["add", "store", "add", "return"]);
    for (_, i) in f.layout_insts() {
        for dest in f.inst_data(i).branch_dests() {
            if dest.block == exit {
                assert_eq!(dest.args.len(), 3);
            }
        }
    }
}

#[test]
fn constant_offset_disjoint_store_hoists_but_partial_and_wrapped_overlap_do_not() {
    for (offset, hoists) in [(8, true), (4, false), (-4, false)] {
        let source = LOOP
            .replace(" restrict_params(0, 1)", "")
            .replace(
                " v4 = iconst.i64 0",
                &format!(" v30 = iconst.i64 {offset}\n v31 = ptradd v0, v30\n v4 = iconst.i64 0"),
            )
            .replace("store v9, v1", "store v9, v31");
        let before = keel_ir::parse(&source).unwrap();
        let after = optimize(&source);
        assert_eq!(
            load_block(&before) != load_block(&after),
            hoists,
            "offset {offset}"
        );
    }
}

#[test]
fn debug_variable_descriptions_and_escaped_restricted_roots_are_skipped() {
    let debug = format!(
        "dtype dt0 = base \"unsigned long\" unsigned 8\n{}",
        LOOP.replace(
            "block0(v0:",
            " ss0 = stack_slot 8, align 8\n dvar \"saved\" dt0 ss0\nblock0(v0:"
        )
    );
    let escape = LOOP.replace(
        " v4 = iconst.i64 0",
        " v30 = icmp.eq v0, v1\n v4 = iconst.i64 0",
    );
    for source in [debug, escape] {
        let before = keel_ir::parse(&source).unwrap().to_string();
        assert_eq!(before, optimize(&source).to_string());
    }
}

#[derive(Debug, PartialEq, Eq)]
enum FirstObservable {
    Load(u64),
    Return(Vec<u64>),
}

// This evaluator stops before any memory access. In particular it can
// demonstrate a spurious null load without executing an invalid C/IR access.
// Bind definitions by their raw identity, but read operands through aliases:
// an instruction whose result was replaced must not redefine its alias target.
fn first_observable(f: &keel_ir::Function, bound: u64, seed: u64) -> FirstObservable {
    use keel_ir::{BinaryOp, IntCC};
    use std::collections::HashMap;
    let mut block = f.entry_block().unwrap();
    let mut values: HashMap<_, _> = f
        .block_params(block)
        .iter()
        .copied()
        .zip([0, 8, bound, seed])
        .collect();
    for _ in 0..32 {
        let mut next = None;
        for &i in f.block_insts(block) {
            let get = |v| values[&f.resolve(v)];
            let result = match f.inst_data(i) {
                InstData::Iconst { imm, .. } => Some(*imm as u64),
                InstData::Binary {
                    op: BinaryOp::Iadd,
                    args,
                } => Some(get(args[0]).wrapping_add(get(args[1]))),
                InstData::Icmp {
                    cond: IntCC::Ult,
                    args,
                } => Some(u64::from(get(args[0]) < get(args[1]))),
                InstData::Load { addr, .. } => return FirstObservable::Load(get(*addr)),
                InstData::Return { args } => {
                    return FirstObservable::Return(args.iter().map(|&v| get(v)).collect());
                }
                InstData::Jump { dest } => {
                    next = Some(dest);
                    None
                }
                InstData::Brif { cond, dests } => {
                    next = Some(&dests[usize::from(get(*cond) == 0)]);
                    None
                }
                other => panic!("unexpected instruction before first observable: {other:?}"),
            };
            if let Some(value) = result {
                values.insert(f.first_result(i), value);
            }
            if next.is_some() {
                break;
            }
        }
        let dest = next.expect("terminator");
        let args: Vec<_> = dest.args.iter().map(|&v| values[&f.resolve(v)]).collect();
        block = dest.block;
        for (&param, value) in f.block_params(block).iter().zip(args) {
            values.insert(param, value);
        }
    }
    panic!("first observable not reached within bound")
}

#[test]
fn aliased_header_definitions_are_rejected_in_memory_without_changing_guard_or_load_trace() {
    let mut failures = Vec::new();
    for case in [
        "result-bound",
        "result-param",
        "result-earlier-result",
        "result-chain-bound",
        "param-bound",
        "param-earlier-param",
    ] {
        let source = if case == "result-earlier-result" {
            LOOP.replace(
                " v7 = icmp",
                " v29 = iconst.i64 0\n v30 = iconst.i64 1\n v7 = icmp",
            )
            .replace("return v12", "return v29")
        } else if case == "result-chain-bound" {
            LOOP.replace(
                " v7 = icmp",
                " v29 = iconst.i64 2\n v30 = iconst.i64 1\n v7 = icmp",
            )
        } else if case.starts_with("result-") {
            LOOP.replace(" v7 = icmp", " v30 = iconst.i64 1\n v7 = icmp")
        } else {
            LOOP.to_string()
        };
        let mut module = keel_ir::parse(&source).unwrap();
        let f = &mut module.functions[0];
        let entry = f.entry_block().unwrap();
        let header = f.blocks().nth(1).unwrap();
        let bound = f.block_params(entry)[2];
        let iv = f.block_params(header)[0];
        let sum = f.block_params(header)[1];
        let (old, replacement) = if case.starts_with("result-") {
            let old = f
                .block_insts(header)
                .iter()
                .find_map(|&i| {
                    matches!(f.inst_data(i), InstData::Iconst { imm: 1, .. })
                        .then(|| f.first_result(i))
                })
                .unwrap();
            let replacement = match case {
                "result-bound" => bound,
                "result-param" => iv,
                _ => f.first_result(f.block_insts(header)[0]),
            };
            (old, replacement)
        } else {
            (sum, if case == "param-bound" { bound } else { iv })
        };
        f.replace_all_uses(old, replacement);
        if case == "result-chain-bound" {
            f.replace_all_uses(replacement, bound);
        }
        assert_ne!(old, f.resolve(old));
        keel_ir::verify(f).unwrap();
        // Do not print/parse round-trip: text resolves operands and erases
        // precisely the valid Function API alias state being tested.
        let before = format!("{f:?}");
        let trip_bound = u64::from(case == "result-param");
        let expected = first_observable(f, trip_bound, 1);
        let changed = keel_opt::find_pass("loop-memory").unwrap().run(f);
        let actual = first_observable(f, trip_bound, 1);
        println!(
            "{case}: bound={trip_bound} seed=1 before={expected:?} after={actual:?} changed={changed}"
        );
        if changed || before != format!("{f:?}") || expected != actual {
            failures.push(case);
        }
        keel_ir::verify(f).unwrap();
        assert_eq!(f.resolve(old), f.resolve(replacement));
    }
    assert!(
        failures.is_empty(),
        "aliased header cases changed: {failures:?}"
    );
}
