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
