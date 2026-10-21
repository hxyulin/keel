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
