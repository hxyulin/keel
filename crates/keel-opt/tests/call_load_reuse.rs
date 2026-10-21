use keel_ir::{InstData, Module};
fn optimize(src: &str) -> Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn loads(m: &Module) -> usize {
    let f = m.functions.iter().find(|f| f.name == "f").unwrap();
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
        .count()
}
fn source(helper: &str, middle: &str) -> String {
    format!(
        "{helper}\nfunc @f(ptr, i64) -> i64 {{\nblock0(v0: ptr, v1: i64):\n v2 = load.i64 v0\n {middle}\n v3 = load.i64 v0\n return v3\n}}"
    )
}
const LEAF: &str = "func @leaf(ptr, i64) -> i64 local {\nblock0(v0: ptr, v1: i64):\n v2 = iadd v1, v1\n return v2\n}";
#[test]
fn direct_body_proof_keeps_call_and_first_read() {
    for helper in [LEAF.to_string(), LEAF.replace("local", "noninterposable")] {
        let m = optimize(&source(&helper, "v4 = call @leaf(v0, v1)"));
        assert_eq!(loads(&m), 1);
        let text = m.to_string();
        assert!(text.contains("call @leaf"));
        assert!(text.contains("return v2"));
    }
    assert_eq!(
        loads(&optimize(&source(
            &LEAF.replace(" local", ""),
            "v4 = call @leaf(v0, v1)"
        ))),
        2
    );
}
#[test]
fn impure_recursive_and_faulting_bodies_are_not_summaries() {
    for body in [
        "store v1, v0\n return v1",
        "v2 = load.i64 v0\n return v2",
        "v2 = sdiv v1, v1\n return v2",
        "v2 = call @leaf(v0, v1)\n return v2",
        "v2 = intrinsic frame_address()\n return v1",
        "intrinsic debugtrap()\n return v1",
        "v2 = atomic_load.i64 relaxed v0\n return v2",
        "trap",
    ] {
        let helper =
            format!("func @leaf(ptr, i64) -> i64 local {{\nblock0(v0: ptr, v1: i64):\n {body}\n}}");
        assert_eq!(
            loads(&optimize(&source(&helper, "v4 = call @leaf(v0, v1)"))),
            2,
            "{body}"
        );
    }
}
#[test]
fn caller_barriers_and_read_types() {
    for middle in [
        "store v1, v0",
        "v4 = load.i64 volatile v0",
        "v4 = atomic_load.i64 relaxed v0",
        "fence seq_cst",
        "v4 = intrinsic frame_address()",
        "intrinsic stack_restore(v0)",
        "intrinsic debugtrap()",
        "v4 = sdiv v1, v1",
        "asm \"\", \"~{memory}\"()",
    ] {
        assert_eq!(
            loads(&optimize(&source(LEAF, middle))),
            if middle.contains("load.i64 volatile") {
                3
            } else {
                2
            },
            "{middle}"
        );
    }
    let src = source(LEAF, "v4 = call @leaf(v0, v1)")
        .replace("v3 = load.i64", "v3 = load.i32")
        .replace("func @f(ptr, i64) -> i64", "func @f(ptr, i64) -> i32");
    assert_eq!(loads(&optimize(&src)), 2);
}
#[test]
fn joins_require_same_available_dominating_value_on_every_edge() {
    let src = format!(
        "{LEAF}\nfunc @f(ptr, i64) -> i64 {{
block0(v0: ptr, v1: i64):
 v2 = load.i64 v0
 brif v1, block1, block2
block1:
 v3 = call @leaf(v0, v1)
 jump block3
block2:
 jump block3
block3:
 v4 = load.i64 v0
 return v4
}}"
    );
    assert_eq!(loads(&optimize(&src)), 1);
    assert_eq!(
        loads(&optimize(
            &src.replace("block2:\n jump", "block2:\n store v1, v0\n jump")
        )),
        2
    );
    let separate = src
        .replace(" v2 = load.i64 v0\n", "")
        .replace("block1:\n", "block1:\n v2 = load.i64 v0\n")
        .replace("block2:\n", "block2:\n v5 = load.i64 v0\n");
    assert_eq!(loads(&optimize(&separate)), 3);
    let looped = src.replace("block2:\n jump block3", "block2:\n brif v1, block2, block3");
    assert_eq!(loads(&optimize(&looped)), 2);
}
#[test]
fn default_pipelines_unchanged_and_pass_repeatable() {
    assert!(!keel_opt::DEFAULT_PIPELINE.contains(&"call-load-reuse"));
    assert!(
        !keel_opt::OptimizationTier::Fast
            .pipeline()
            .contains(&"call-load-reuse")
    );
    let mut m = optimize(&source(LEAF, "v4 = call @leaf(v0, v1)"));
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
    assert_eq!(before, m.to_string());
}
#[test]
fn unknown_indirect_and_returns_twice_calls_remain_barriers() {
    for (helper, middle) in [
        (
            "decl @unknown(ptr, i64) -> i64".to_string(),
            "v4 = call @unknown(v0, v1)",
        ),
        (
            LEAF.to_string(),
            "v4 = call_indirect v0(v0, v1) : (ptr, i64) -> i64",
        ),
        (
            LEAF.replace(" local", " returns_twice local"),
            "v4 = call @leaf(v0, v1)",
        ),
    ] {
        assert_eq!(loads(&optimize(&source(&helper, middle))), 2);
    }
    let src = source(LEAF, "v4 = call @leaf(v0, v1)").replace(
        "func @f(ptr, i64) -> i64",
        "func @f(ptr, i64) -> i64 returns_twice",
    );
    assert_eq!(loads(&optimize(&src)), 2);
}
#[test]
fn located_instructions_and_redirected_results_are_not_proof() {
    let src = source(LEAF, "v4 = call @leaf(v0, v1)");
    for located in [true, false] {
        let mut m = keel_ir::parse(&src).unwrap();
        let file = m.add_file("observer.c");
        let f = &mut m.functions[1];
        let load = f
            .layout_insts()
            .find_map(|(_, i)| matches!(f.inst_data(i), InstData::Load { .. }).then_some(i))
            .unwrap();
        if located {
            f.set_inst_loc(
                load,
                Some(keel_ir::SourceLoc {
                    file,
                    line: 1,
                    col: 1,
                    scope: None,
                }),
            );
        } else {
            let unrelated = f.block_params(f.entry_block().unwrap())[1];
            f.replace_all_uses(f.first_result(load), unrelated);
        }
        keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
        assert_eq!(loads(&m), 2);
    }
}
#[test]
fn invoke_is_not_a_proven_ordinary_call() {
    let src = format!(
        "decl @personality()\n{LEAF}\nfunc @f(ptr, i64) -> i64 personality @personality {{
block0(v0: ptr, v1: i64):
 v2 = load.i64 v0
 v3 = invoke @leaf(v0, v1), block1, block2
block1:
 v4 = load.i64 v0
 return v4
block2:
 v5, v6 = landingpad cleanup
 trap
}}"
    );
    assert_eq!(loads(&optimize(&src)), 2);
}

#[test]
fn fact_and_edge_budgets_lose_information_conservatively() {
    for (edges, expected) in [(8, 1), (9, 2)] {
        let mut src = "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 v0\n".to_string();
        for n in 1..=edges {
            src.push_str(&format!(" jump block{n}\nblock{n}:\n"));
        }
        src.push_str(" v2 = load.i64 v0\n return v2\n}");
        assert_eq!(loads(&optimize(&src)), expected);
    }
    for (count, expected) in [(128, 128), (129, 130)] {
        let mut src = "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n".to_string();
        for n in 0..count {
            src.push_str(&format!(
                " v{} = iconst.i64 {}\n v{} = ptradd v0, v{}\n v{} = load.i64 v{}\n",
                3 * n + 1,
                n * 8,
                3 * n + 2,
                3 * n + 1,
                3 * n + 3,
                3 * n + 2
            ));
        }
        src.push_str(&format!(
            " v{} = load.i64 v2\n return v{}\n}}",
            3 * count + 1,
            3 * count + 1
        ));
        assert_eq!(loads(&optimize(&src)), expected);
    }
}

#[test]
fn returned_pointer_is_allowed_but_is_not_a_nocapture_promise() {
    let helper = "func @id(ptr) -> ptr local {\nblock0(v0: ptr):\n return v0\n}";
    let src = source(helper, "v4 = call @id(v0)");
    assert_eq!(loads(&optimize(&src)), 1);
    // A use of that result can mutate the original object. No escape fact is
    // exported to let another pass ignore this store.
    assert_eq!(
        loads(&optimize(&src.replace(
            "v4 = call @id(v0)",
            "v4 = call @id(v0)\n store v1, v4"
        ))),
        2
    );
}

#[test]
fn resolved_pointer_aliases_match_but_frame_observers_disable_the_function() {
    let mut m = keel_ir::parse(&source(LEAF, "v4 = call @leaf(v0, v1)")).unwrap();
    let f = &mut m.functions[1];
    let entry = f.entry_block().unwrap();
    let pointer = f.block_params(entry)[0];
    // An extra pointer parameter aliases the original parameter in this in-memory
    // module; exact keys must resolve aliases rather than infer new addresses.
    let alias = f.append_block_param(entry, keel_ir::Type::Ptr);
    f.sig
        .params
        .push(keel_ir::AbiParam::new(keel_ir::Type::Ptr));
    let second = f
        .layout_insts()
        .filter_map(|(_, i)| matches!(f.inst_data(i), InstData::Load { .. }).then_some(i))
        .nth(1)
        .unwrap();
    if let InstData::Load { addr, .. } = f.inst_data_mut(second) {
        *addr = alias;
    }
    f.replace_all_uses(alias, pointer);
    keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
    assert_eq!(loads(&m), 1);
    let src = source(LEAF, "v4 = call @leaf(v0, v1)").replace(
        " v2 = load.i64 v0",
        " v5 = intrinsic frame_address()\n v2 = load.i64 v0",
    );
    assert_eq!(loads(&optimize(&src)), 2);
}

fn switch_budget_fixture(blocks: usize, destinations: usize) -> String {
    let mut s = "func @f(ptr, i64) -> i64 {\nblock0(v0: ptr, v1: i64):\n v2 = load.i64 v0\n v3 = load.i64 v0\n".to_string();
    for b in 0..blocks {
        if b != 0 {
            s.push_str(&format!("block{b}:\n"));
        }
        let cases = (0..destinations - 1)
            .map(|n| format!("{n}: block{}", b + 1))
            .collect::<Vec<_>>()
            .join(", ");
        s.push_str(&format!(" switch v1, block{}, [{cases}]\n", b + 1));
    }
    s.push_str(&format!("block{blocks}:\n return v3\n}}"));
    s
}

#[test]
fn switch_destination_budgets_reject_whole_caller_before_mutation() {
    // Default destination counts even when every case shares that target.
    for (destinations, expected) in [(1024, 1), (1025, 2)] {
        assert_eq!(
            loads(&optimize(&switch_budget_fixture(1, destinations))),
            expected
        );
    }
    for (blocks, expected) in [(16, 1), (17, 2)] {
        assert_eq!(
            loads(&optimize(&switch_budget_fixture(blocks, 1024))),
            expected
        );
    }
}

#[test]
fn removed_block_indices_still_spend_cfg_allocation_budget() {
    let mut m = keel_ir::parse(&source(LEAF, "v4 = call @leaf(v0, v1)")).unwrap();
    let f = m.functions.iter_mut().find(|f| f.name == "f").unwrap();
    for _ in 0..8192 {
        let b = f.create_block();
        f.remove_block(b);
    }
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(before, m.to_string());
    assert_eq!(loads(&m), 2);
}

// Every layer is kept: the effect proof is independent of inlining and never
// turns a body summary into permission to move a read before its original site.
fn chain(depth: usize) -> String {
    let mut src = LEAF.to_string();
    let mut previous = "leaf".to_string();
    for n in 0..depth {
        src.push_str(&format!(
            "\nfunc @chain{n}(ptr, i64) -> i64 local {{\nblock0(v0: ptr, v1: i64):\n v2 = call @{previous}(v0, v1)\n v3 = iadd v2, v1\n return v3\n}}"
        ));
        previous = format!("chain{n}");
    }
    source(&src, &format!("v4 = call @{previous}(v0, v1)"))
}

#[test]
fn transitive_depth_budget_is_inclusive_and_order_independent() {
    for (depth, expected) in [(1, 1), (8, 1), (9, 2)] {
        let src = chain(depth);
        let mut m = optimize(&src);
        assert_eq!(loads(&m), expected, "depth {depth}");
        for f in &m.functions {
            if f.name != "leaf" {
                assert_eq!(
                    f.layout_insts()
                        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
                        .count(),
                    1
                );
            }
        }
        let before = m.to_string();
        keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
        assert_eq!(before, m.to_string());
        let mut reversed = keel_ir::parse(&src).unwrap();
        reversed.functions.reverse();
        keel_opt::run_passes(&mut reversed, &["call-load-reuse"]).unwrap();
        keel_ir::verify_module(&reversed).unwrap();
        assert_eq!(loads(&reversed), expected);
    }
}

#[test]
fn transitive_barriers_propagate_through_every_wrapper() {
    for body in [
        "store v1, v0\n return v1",
        "v2 = load.i64 v0\n return v2",
        "v2 = load.i64 volatile v0\n return v2",
        "v2 = atomic_load.i64 relaxed v0\n return v2",
        "asm \"\", \"~{memory}\"()\n return v1",
        "v2 = call_indirect v0(v0, v1) : (ptr, i64) -> i64\n return v2",
        "v2 = sdiv v1, v1\n return v2",
        "intrinsic debugtrap()\n return v1",
        "v2 = call @chain0(v0, v1)\n return v2", // mutual cycle
    ] {
        let src = chain(2).replace("v2 = iadd v1, v1\n return v2", body);
        assert_eq!(loads(&optimize(&src)), 2, "{body}");
    }
    for old in ["local", "noninterposable"] {
        let src = chain(2).replacen(" local", &format!(" {old}"), 1);
        assert_eq!(loads(&optimize(&src)), 1);
        assert_eq!(
            loads(&optimize(&src.replacen(&format!(" {old}"), "", 1))),
            2
        );
    }
}

#[test]
fn transitive_proof_requires_unique_exact_legal_binding() {
    let src = chain(2);
    for mode in 0..6 {
        let mut m = keel_ir::parse(&src).unwrap();
        match mode {
            0 => m.functions[0].weak_odr = true,
            1 => {
                let copy = m.functions[0].clone();
                m.functions.push(copy);
            }
            2 => {
                let reference = m.functions[1].callees().next().unwrap().0;
                m.functions[1].callee_mut(reference).sig.cc = keel_ir::CallConv::Fast;
            }
            3 => {
                let reference = m.functions[1].callees().next().unwrap().0;
                m.functions[1].callee_mut(reference).attrs.noreturn = true;
            }
            4 => m.functions[0].sig.variadic = true,
            5 => {
                let file = m.add_file("nested.c");
                let f = &mut m.functions[0];
                let i = f.layout_insts().next().unwrap().1;
                f.set_inst_loc(
                    i,
                    Some(keel_ir::SourceLoc {
                        file,
                        line: 1,
                        col: 1,
                        scope: None,
                    }),
                );
            }
            _ => unreachable!(),
        }
        // Some cases intentionally bypass verification to exercise defensive
        // treatment of malformed linkage and mismatched/duplicate bindings.
        keel_opt::find_pass("call-load-reuse")
            .unwrap()
            .run_module(&mut m);
        assert_eq!(loads(&m), 2, "mode {mode}");
    }
    let src =
        "func @leaf(ptr byval {i64}, i64) -> i64 local {\nblock0(v0: ptr, v1: i64):\n return v1\n}";
    assert_eq!(loads(&optimize(&source(src, "v4 = call @leaf(v0, v1)"))), 2);
}

#[test]
fn transitive_join_requires_both_availability_and_readability() {
    let src = chain(2).replace(
        " v4 = call @chain1(v0, v1)\n v3 = load.i64 v0\n return v3",
        " brif v1, block1, block2\nblock1:\n v4 = call @chain1(v0, v1)\n jump block3\nblock2:\n jump block3\nblock3:\n v3 = load.i64 v0\n return v3",
    );
    assert_eq!(loads(&optimize(&src)), 1);
    for barrier in [
        "store v1, v0",
        "intrinsic debugtrap()",
        "v5 = load.i64 volatile v0",
    ] {
        let s = src.replace("block2:\n jump", &format!("block2:\n {barrier}\n jump"));
        assert_eq!(
            loads(&optimize(&s)),
            if barrier.contains("volatile") { 3 } else { 2 }
        );
    }
    // No read in one arm means the join cannot acquire successful readability.
    let s = src
        .replace(" v2 = load.i64 v0\n", "")
        .replace("block1:\n", "block1:\n v2 = load.i64 v0\n");
    assert_eq!(loads(&optimize(&s)), 2);
}

#[test]
fn transitive_candidate_body_work_is_bounded() {
    for (insts, expected) in [(128, 1), (129, 2)] {
        let mut helper = LEAF.to_string();
        helper.push_str("\nfunc @wrapper(ptr, i64) -> i64 local {\nblock0(v0: ptr, v1: i64):\n v2 = call @leaf(v0, v1)\n");
        for n in 0..insts - 2 {
            helper.push_str(&format!(" v{} = iadd v1, v1\n", n + 3));
        }
        helper.push_str(" return v2\n}");
        assert_eq!(
            loads(&optimize(&source(&helper, "v4 = call @wrapper(v0, v1)"))),
            expected
        );
    }
    // An SCC with a successful outgoing leaf proof must still never complete.
    let src = chain(2).replace("v3 = iadd v2, v1", "v3 = call @chain1(v0, v2)");
    assert_eq!(loads(&optimize(&src)), 2);
}
