use keel_ir::{InstData, Module};
fn run(s: &str) -> Module {
    let mut m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(
        &mut m,
        &["stack-forward", "canonicalize", "instcombine", "dce"],
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn loads(m: &Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Load { .. }))
        .count()
}
#[test]
fn forwards_partial_union_write_and_removes_private_stores() {
    let m = run("func @f(i32) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: i32):
 v1 = stack_addr ss0
 v2 = iconst.i64 0
 store v2, v1
 store v0, v1
 v3 = load.i64 v1
 return v3
}");
    assert_eq!(loads(&m), 0);
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::Store { .. }))
    );
    assert!(m.to_string().contains("uext.i64 v0"), "{m}");
}
#[test]
fn preserves_uninitialized_bytes_and_reads_across_blocks() {
    for body in [
        "store v0, v1\n v3 = load.i64 v1\n return v3",
        "store v2, v1\n jump block1\nblock1:\n v3 = load.i64 v1\n return v3",
    ] {
        let m = run(&format!(
            "func @f(i32) -> i64 {{\n ss0 = stack_slot 8, align 8\nblock0(v0: i32):\n v1 = stack_addr ss0\n v2 = iconst.i64 0\n {body}\n}}"
        ));
        assert_eq!(loads(&m), 1);
        assert!(m.to_string().contains("store"));
    }
}
#[test]
fn escaped_addresses_and_volatile_atomic_accesses_are_untouched() {
    for effect in [
        "call @escape(v1)",
        "v5 = ptrtoint.i64 v1",
        "store volatile v2, v1",
        "v5 = atomic_load.i64 relaxed v1",
        "v5 = intrinsic frame_address()",
    ] {
        let m = run(&format!(
            "decl @escape(ptr)\nfunc @f() -> i64 {{\n ss0 = stack_slot 8, align 8\nblock0:\n v1 = stack_addr ss0\n v2 = iconst.i64 42\n store v2, v1\n {effect}\n v3 = load.i64 v1\n return v3\n}}"
        ));
        assert_eq!(loads(&m), 1, "{effect}");
    }
}
#[test]
fn private_stack_survives_unrelated_aliasing_call_but_public_writes_do_not() {
    let m = run("decl @effect(ptr)
func @f(ptr) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = stack_addr ss0
 v2 = iconst.i64 17
 store v2, v1
 call @effect(v0)
 v3 = load.i64 v1
 return v3
}");
    assert_eq!(loads(&m), 0);
    assert!(m.to_string().contains("call @effect"));
}
#[test]
fn overlapping_stores_observe_byte_offsets() {
    let m = run("func @f(i64, i16) -> i32 {
 ss0 = stack_slot 8, align 8
block0(v0: i64, v1: i16):
 v2 = stack_addr ss0
 v3 = iconst.i64 2
 v4 = ptradd v2, v3
 store v0, v2
 store v1, v4
 v5 = load.i32 v2
 return v5
}");
    assert_eq!(loads(&m), 0);
}

#[test]
fn debugger_visible_homes_keep_both_stores_and_loads() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    let mut m = keel_ir::parse(
        "func @f() -> i64 {
 ss0 = stack_slot 8, align 8
block0:
 v0 = stack_addr ss0
 v1 = iconst.i64 3
 store v1,v0
 v2 = load.i64 v0
 return v2
}",
    )
    .unwrap();
    m.debug_types.push(DebugTypeData::Base {
        name: "long".into(),
        encoding: Encoding::Signed,
        size: 8,
    });
    m.functions[0].debug_vars.push(DebugVar {
        name: "d".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
    assert_eq!(before, m.to_string());
}
