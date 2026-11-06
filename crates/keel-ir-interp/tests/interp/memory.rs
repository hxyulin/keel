//! Loads, stores, stack slots, data objects, memory intrinsics and atomics.

use super::{eval, module, run};
use keel_ir_interp::{Interpreter, Outcome, Trap, Val};

#[test]
fn stack_slot_round_trip_is_little_endian() {
    let src = "\
func @f(i32) -> (i32, i8) {
    ss0 = stack_slot 4, align 4
block0(v0: i32):
    v1 = stack_addr ss0
    store align 4 v0, v1
    v2 = load.i32 align 4 v1
    v3 = load.i8 v1
    return v2, v3
}
";
    assert_eq!(
        run(src, "f", &[Val::I32(0x0102_0304)]),
        Outcome::Return(vec![Val::I32(0x0102_0304), Val::I8(4)])
    );
}

#[test]
fn every_scalar_type_round_trips() {
    for (ty, val) in [
        ("i16", Val::I16(0xbeef)),
        ("i64", Val::I64(u64::MAX - 1)),
        ("i128", Val::I128(u128::MAX - 5)),
        ("f32", Val::F32(0x7fc0_1234)),
        ("f64", Val::f64(-2.5)),
        ("ptr", Val::Ptr(0x1234_5678)),
    ] {
        let src = format!(
            "func @f({ty}) -> {ty} {{\n    ss0 = stack_slot 16, align 16\nblock0(v0: {ty}):\n    v1 = stack_addr ss0\n    store v0, v1\n    v2 = load.{ty} v1\n    return v2\n}}\n"
        );
        assert_eq!(eval(&src, "f", std::slice::from_ref(&val)), val, "{ty}");
    }
}

#[test]
fn accesses_outside_an_allocation_trap() {
    let src = "\
func @f(i64) -> i64 {
    ss0 = stack_slot 16, align 8
block0(v0: i64):
    v1 = stack_addr ss0
    v2 = ptradd v1, v0
    v3 = load.i64 v2
    return v3
}
";
    assert_eq!(eval(src, "f", &[Val::I64(8)]), Val::I64(0));
    assert!(matches!(
        run(src, "f", &[Val::I64(12)]),
        Outcome::Trap(Trap::OutOfBounds { size: 8, .. })
    ));
    assert!(matches!(
        run(src, "f", &[Val::I64(-1i64 as u64)]),
        Outcome::Trap(Trap::OutOfBounds { .. })
    ));
}

#[test]
fn null_and_dangling_pointers_trap() {
    let src = "\
func @null() -> i32 {
block0:
    v0 = iconst.i64 0
    v1 = inttoptr.ptr v0
    v2 = load.i32 v1
    return v2
}
func @escape() -> ptr {
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    return v0
}
func @dangling() -> i64 {
block0:
    v0 = call @escape()
    v1 = load.i64 v0
    return v1
}
";
    assert_eq!(
        run(src, "null", &[]),
        Outcome::Trap(Trap::OutOfBounds { addr: 0, size: 4 })
    );
    assert!(matches!(
        run(src, "dangling", &[]),
        Outcome::Trap(Trap::OutOfBounds { .. })
    ));
}

#[test]
fn broken_align_promise_traps() {
    let src = "\
func @f() -> i32 {
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    v1 = iconst.i64 2
    v2 = ptradd v0, v1
    v3 = load.i32 align 4 v2
    return v3
}
";
    assert!(matches!(
        run(src, "f", &[]),
        Outcome::Trap(Trap::Misaligned { align: 4, .. })
    ));
}

#[test]
fn pointer_integer_round_trip() {
    let src = "\
func @f(ptr) -> ptr {
block0(v0: ptr):
    v1 = ptrtoint.i64 v0
    v2 = iconst.i64 3
    v3 = iadd v1, v2
    v4 = inttoptr.ptr v3
    return v4
}
";
    assert_eq!(eval(src, "f", &[Val::Ptr(0x1000)]), Val::Ptr(0x1003));
}

#[test]
fn data_objects_are_initialized_and_persist() {
    let src = "\
data @msg = const align 1 { b\"hi\\0\" }
data @table = align 8 { ptr @msg + 1, i32 7, zero 4 }
data @counter = align 8 { i64 40 }
func @second_char() -> i8 {
block0:
    v0 = symbol_addr @table
    v1 = load.ptr v0
    v2 = load.i8 v1
    return v2
}
func @bump() -> i64 {
block0:
    v0 = symbol_addr @counter
    v1 = load.i64 v0
    v2 = iconst.i64 1
    v3 = iadd v1, v2
    store v3, v0
    return v3
}
func @write_const() {
block0:
    v0 = symbol_addr @msg
    v1 = iconst.i8 0
    store v1, v0
    return
}
";
    let module = module(src);
    let mut interp = Interpreter::new(&module);
    assert_eq!(
        interp.call("second_char", &[]),
        Outcome::Return(vec![Val::I8(b'i')])
    );
    assert_eq!(
        interp.call("bump", &[]),
        Outcome::Return(vec![Val::I64(41)])
    );
    assert_eq!(
        interp.call("bump", &[]),
        Outcome::Return(vec![Val::I64(42)])
    );
    let table = interp.symbol_addr("table").unwrap();
    assert_eq!(
        interp.memory().read(table + 8, 4).unwrap(),
        &7u32.to_le_bytes()
    );
    let msg = interp.symbol_addr("msg").unwrap();
    assert_eq!(
        interp.call("write_const", &[]),
        Outcome::Trap(Trap::WriteToConstant { addr: msg })
    );
}

#[test]
fn caller_memory_is_visible() {
    let src = "\
func @sum(ptr, i64) -> i32 {
block0(v0: ptr, v1: i64):
    v2 = iconst.i32 0
    v3 = iconst.i64 0
    jump block1(v3, v2)
block1(v4: i64, v5: i32):
    v6 = icmp.ult v4, v1
    brif v6, block2, block3
block2:
    v7 = iconst.i64 4
    v8 = imul v4, v7
    v9 = ptradd v0, v8
    v10 = load.i32 v9
    v11 = iadd v5, v10
    v12 = iconst.i64 1
    v13 = iadd v4, v12
    jump block1(v13, v11)
block3:
    return v5
}
";
    let module = module(src);
    let mut interp = Interpreter::new(&module);
    let bytes: Vec<u8> = [3u32, 4, 5].iter().flat_map(|x| x.to_le_bytes()).collect();
    let addr = interp.memory_mut().alloc_bytes(&bytes, 4);
    assert_eq!(
        interp.call("sum", &[Val::Ptr(addr), Val::I64(3)]),
        Outcome::Return(vec![Val::I32(12)])
    );
    assert!(matches!(
        interp.call("sum", &[Val::Ptr(addr), Val::I64(4)]),
        Outcome::Trap(Trap::OutOfBounds { .. })
    ));
}

#[test]
fn memory_intrinsics() {
    let src = "\
func @f() -> (i32, i32, i8, i32) {
    ss0 = stack_slot 8, align 8
    ss1 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    v1 = stack_addr ss1
    v2 = iconst.i8 7
    v3 = iconst.i64 8
    intrinsic memset(v0, v2, v3)
    intrinsic memcpy(v1, v0, v3)
    v4 = load.i32 v1
    v5 = iconst.i8 9
    v6 = iconst.i64 3
    v7 = ptradd v1, v6
    store v5, v7
    v8 = intrinsic memcmp(v0, v1, v3)
    v9 = iconst.i64 1
    v10 = ptradd v1, v9
    intrinsic memmove(v10, v1, v6)
    v11 = load.i8 v7
    v12 = intrinsic memcmp(v1, v1, v3)
    return v4, v8, v11, v12
}
";
    assert_eq!(
        run(src, "f", &[]),
        Outcome::Return(vec![
            Val::I32(0x0707_0707),
            Val::I32(-2i32 as u32),
            Val::I8(7),
            Val::I32(0)
        ])
    );
}

#[test]
fn dynamic_stack_is_freed_by_restore() {
    let src = "\
func @f(i8) -> i64 {
block0(v0: i8):
    v1 = intrinsic stack_save()
    v2 = iconst.i64 32
    v3 = intrinsic stack_alloc(v2)
    v4 = iconst.i64 5
    store v4, v3
    brif v0, block1, block2
block1:
    intrinsic stack_restore(v1)
    jump block2
block2:
    v5 = load.i64 v3
    return v5
}
";
    assert_eq!(eval(src, "f", &[Val::I8(0)]), Val::I64(5));
    assert!(matches!(
        run(src, "f", &[Val::I8(1)]),
        Outcome::Trap(Trap::OutOfBounds { .. })
    ));
}

#[test]
fn lifetime_markers_bound_slot_access() {
    let src = "\
func @f(i8) -> i32 {
    ss0 = stack_slot 4, align 4
block0(v0: i8):
    v1 = stack_addr ss0
    intrinsic lifetime_end(v1)
    brif v0, block1, block2
block1:
    intrinsic lifetime_start(v1)
    jump block2
block2:
    v2 = load.i32 v1
    return v2
}
";
    assert_eq!(eval(src, "f", &[Val::I8(1)]), Val::I32(0));
    assert!(matches!(
        run(src, "f", &[Val::I8(0)]),
        Outcome::Trap(Trap::OutOfBounds { .. })
    ));
}

#[test]
fn atomics() {
    let src = "\
func @f(i32) -> (i32, i32, i32, i8, i32, i8, i32, i8, i32) {
    ss0 = stack_slot 4, align 4
block0(v0: i32):
    v1 = stack_addr ss0
    atomic_store release v0, v1
    v2 = iconst.i32 5
    v3 = atomic_rmw.add seq_cst v1, v2
    v4 = iconst.i32 -3
    v5 = atomic_rmw.smin relaxed v1, v4
    v6, v9 = atomic_cas acq_rel acquire v1, v4, v2
    v7, v10 = atomic_cas acq_rel acquire v1, v4, v0
    v11, v12 = atomic_cas weak release relaxed v1, v2, v4
    fence seq_cst
    v8 = atomic_load.i32 acquire v1
    return v3, v5, v6, v9, v7, v10, v11, v12, v8
}
";
    // The first compare-and-swap stores, the second fails and stores
    // nothing, and the weak one stores.
    assert_eq!(
        run(src, "f", &[Val::I32(10)]),
        Outcome::Return(vec![
            Val::I32(10),
            Val::I32(15),
            Val::I32(-3i32 as u32),
            Val::I8(1),
            Val::I32(5),
            Val::I8(0),
            Val::I32(5),
            Val::I8(1),
            Val::I32(-3i32 as u32)
        ])
    );
}

#[test]
fn atomics_need_natural_alignment() {
    let src = "\
func @f() -> i64 {
    ss0 = stack_slot 16, align 8
block0:
    v0 = stack_addr ss0
    v1 = iconst.i64 4
    v2 = ptradd v0, v1
    v3 = iconst.i64 1
    v4 = atomic_rmw.xchg seq_cst v2, v3
    return v4
}
";
    assert!(matches!(
        run(src, "f", &[]),
        Outcome::Trap(Trap::Misaligned { align: 8, .. })
    ));
}

#[test]
fn load_trace_records_each_executed_load_in_order() {
    let m = module(
        "\
func @f(ptr) -> i32 {
block0(v0: ptr):
    v1 = iconst.i64 4
    v2 = ptradd v0, v1
    v3 = load.i32 v2
    v4 = load.i32 v0
    v5 = iadd v3, v4
    return v5
}
",
    );
    let mut interp = Interpreter::new(&m).with_load_trace();
    let base = interp
        .memory_mut()
        .alloc_bytes(&[1, 0, 0, 0, 2, 0, 0, 0], 4);
    assert_eq!(
        interp.call("f", &[Val::Ptr(base)]),
        Outcome::Return(vec![Val::I32(3)])
    );
    assert_eq!(interp.load_trace(), [base + 4, base]);
    assert!(Interpreter::new(&m).load_trace().is_empty());
}
