#[path = "../src/aggregate_return.rs"]
mod aggregate_return;

const CONSTRUCTOR: &str = "func @make(ptr sret {i64, i64}, i64) local {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 8
 v3 = ptradd v0, v2
 store v1, v0
 store v1, v3
 return
}\n";
const CALLER: &str = "func @test(i64) -> i64 {
 ss0 = stack_slot 16, align 8
 ss1 = stack_slot 16, align 8
block0(v0: i64):
 v1 = stack_addr ss0
 v2 = stack_addr ss1
 v3 = iconst.i64 8
 call @make(v1, v0)
 v4 = load.i64 v1
 store v4, v2
 v5 = ptradd v1, v3
 v6 = ptradd v2, v3
 v7 = load.i64 v5
 store v7, v6
 v8 = load.i64 v2
 return v8
}";
fn check(source: &str, expected: usize) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    assert_eq!(aggregate_return::run(&mut m), expected, "{source}");
    keel_ir::verify_module(&m).unwrap();
    if expected == 0 {
        assert_eq!(before, m.to_string());
    }
    m
}
#[test]
fn eliminates_exact_leaf_result_copy() {
    let m = check(&(CONSTRUCTOR.to_owned() + CALLER), 1);
    assert_eq!(
        m.functions[1]
            .layout_insts()
            .filter(|&(_, i)| matches!(m.functions[1].inst_data(i), keel_ir::InstData::Load { .. }))
            .count(),
        1
    );
    assert_eq!(aggregate_return::run(&mut m.clone()), 0);
}
#[test]
fn rejects_capture_observation_traps_incomplete_and_interposition() {
    for constructor in [
        CONSTRUCTOR.replace(" local", ""),
        CONSTRUCTOR.replace(" store v1, v3", ""),
        CONSTRUCTOR.replace(" store v1, v3", " store v1, v0"),
        CONSTRUCTOR.replace(" return", " v9 = ptrtoint.i64 v0\n return"),
        CONSTRUCTOR.replace(" return", " v9 = load.i64 v0\n return"),
        CONSTRUCTOR.replace(" return", " trap"),
        CONSTRUCTOR.replace(" return", " v9 = sdiv v1, v1\n return"),
        CONSTRUCTOR.replace(" return", " v9 = intrinsic frame_address()\n return"),
        CONSTRUCTOR.replace(" store v1, v0", " store volatile v1, v0"),
    ] {
        check(&(constructor + CALLER), 0);
    }
}
#[test]
fn rejects_escape_alias_snapshot_partial_and_extra_load_use() {
    for caller in [
        CALLER.replace(
            " ss1 = stack_slot 16, align 8",
            " ss1 = stack_slot 16, align 4",
        ),
        CALLER.replace("v2 = stack_addr ss1", "v2 = stack_addr ss0"),
        CALLER.replace(" store v7, v6", " store v7, v2"),
        CALLER.replace(" store v7, v6", ""),
        CALLER.replace(" store v4, v2", " store volatile v4, v2"),
        CALLER.replace(
            " v8 = load.i64 v2",
            " v9 = ptrtoint.i64 v1\n v8 = load.i64 v2",
        ),
        CALLER.replace(
            " v8 = load.i64 v2",
            " v9 = ptrtoint.i64 v2\n v8 = load.i64 v2",
        ),
        CALLER.replace(" return v8", " return v4"),
        CALLER.replace(
            " v4 = load.i64 v1",
            " v9 = intrinsic frame_address()\n v4 = load.i64 v1",
        ),
        CALLER.replace(
            " v4 = load.i64 v1",
            " v9 = intrinsic stack_save()\n v4 = load.i64 v1",
        ),
    ] {
        check(&(CONSTRUCTOR.to_owned() + &caller), 0);
    }
}
#[test]
fn rejects_unknown_calls_and_observers() {
    for effect in ["call @escape(v1)", "call @escape(v2)", "call @tick()"] {
        let caller = CALLER.replace(
            " v4 = load.i64 v1",
            &format!(" {effect}\n v4 = load.i64 v1"),
        );
        check(
            &("decl @escape(ptr)\ndecl @tick()\n".to_owned() + CONSTRUCTOR + &caller),
            0,
        );
    }
    let source = "decl @escape(ptr)\n".to_owned()
        + CONSTRUCTOR
        + &CALLER.replace(" return v8", " call @escape(v1)\n return v8");
    check(&source, 0);
}
#[test]
fn debug_module_is_untouched() {
    let mut m = keel_ir::parse(&(CONSTRUCTOR.to_owned() + CALLER)).unwrap();
    m.files.push("input.c".into());
    assert_eq!(aggregate_return::run(&mut m), 0);
}

#[test]
fn destination_call_after_copy_is_safe_because_constructor_cannot_observe_it() {
    let caller = CALLER.replace(" return v8", " call @escape(v2)\n return v8");
    check(
        &("decl @escape(ptr)\n".to_owned() + CONSTRUCTOR + &caller),
        1,
    );
}

#[test]
fn rejects_nonlocal_resumption_and_exception_paths() {
    for (decl, effect) in [
        ("decl @checkpoint() returns_twice\n", "call @checkpoint()"),
        (
            "decl data @fn\n",
            "v10 = symbol_addr @fn\n call_indirect v10() : () returns_twice",
        ),
        (
            "decl @personality()\ndecl @may_throw() -> i64\n",
            "v10 = invoke @may_throw(), block1, block2\nblock1:\n return v8\nblock2:\n v11, v12 = landingpad cleanup\n trap",
        ),
    ] {
        let caller = CALLER.replace(" return v8", &format!(" {effect}\n return v8"));
        // Invoke's continuation supplies its own terminators.
        let caller = if effect.contains("invoke") {
            caller.replace(" trap\n return v8", " trap").replace(
                "func @test(i64) -> i64 {",
                "func @test(i64) -> i64 personality @personality {",
            )
        } else {
            caller
        };
        check(&(decl.to_owned() + CONSTRUCTOR + &caller), 0);
    }
}
#[test]
fn rejects_atomic_accesses_and_preexisting_source_reads() {
    for effect in [
        "v10 = atomic_load.i64 relaxed v1",
        "v10 = atomic_load.i64 relaxed v2",
        "v10 = load.i64 v1",
    ] {
        let caller = CALLER.replace(
            " call @make(v1, v0)",
            &format!(" {effect}\n call @make(v1, v0)"),
        );
        check(&(CONSTRUCTOR.to_owned() + &caller), 0);
    }
}
#[test]
fn rejects_byval_source_snapshot_overlap() {
    let constructor = CONSTRUCTOR
        .replace(
            "sret {i64, i64}, i64",
            "sret {i64, i64}, i64, ptr byval {i64, i64}",
        )
        .replace("v1: i64):", "v1: i64, v10: ptr):");
    let caller = CALLER.replace("call @make(v1, v0)", "call @make(v1, v0, v1)");
    check(&(constructor + &caller), 0);
}
#[test]
fn live_result_alias_is_not_overwritten() {
    let mut m = keel_ir::parse(&(CONSTRUCTOR.to_owned() + CALLER)).unwrap();
    let f = &mut m.functions[1];
    let load = f
        .layout_insts()
        .find_map(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Load { .. }).then_some(i))
        .unwrap();
    let value = f.first_result(load);
    let replacement = f.block_params(f.entry_block().unwrap())[0];
    f.replace_all_uses(value, replacement);
    assert_eq!(aggregate_return::run(&mut m), 0);
    assert_eq!(m.functions[1].resolve(value), replacement);
}

#[test]
fn hard_work_budget_stops_with_valid_partial_progress() {
    let mut caller = "func @many(i64) {\n".to_owned();
    for n in 0..400 {
        caller += &format!(
            " ss{} = stack_slot 16, align 8\n ss{} = stack_slot 16, align 8\n",
            2 * n,
            2 * n + 1
        );
    }
    caller += "block0(v0: i64):\n v1 = iconst.i64 8\n";
    for n in 0..400 {
        let v = 2 + 6 * n;
        caller += &format!(
            " v{v} = stack_addr ss{}\n v{} = stack_addr ss{}\n call @make(v{v}, v0)\n v{} = load.i64 v{v}\n store v{}, v{}\n v{} = ptradd v{v}, v1\n v{} = ptradd v{}, v1\n v{} = load.i64 v{}\n store v{}, v{}\n",
            2 * n,
            v + 1,
            2 * n + 1,
            v + 2,
            v + 2,
            v + 1,
            v + 3,
            v + 4,
            v + 1,
            v + 5,
            v + 3,
            v + 5,
            v + 4
        );
    }
    caller += " return\n}";
    let mut m = keel_ir::parse(&(CONSTRUCTOR.to_owned() + &caller)).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let changed = aggregate_return::run(&mut m);
    assert!(
        changed > 0 && changed < 400,
        "budget did not bound scans: {changed}"
    );
    keel_ir::verify_module(&m).unwrap();
}
