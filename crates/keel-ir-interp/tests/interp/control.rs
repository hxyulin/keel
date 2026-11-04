//! Branches, block arguments, calls, tail calls, traps and limits.

use super::{eval, module, run};
use keel_ir_interp::{Interpreter, Outcome, Trap, Val};

#[test]
fn block_arguments_are_copied_simultaneously() {
    // Swapping through a jump: fib by (a, b) -> (b, a + b).
    let src = "\
func @fib(i32) -> i64 {
block0(v0: i32):
    v1 = iconst.i64 0
    v2 = iconst.i64 1
    jump block1(v0, v1, v2)
block1(v3: i32, v4: i64, v5: i64):
    v6 = iconst.i32 0
    v7 = icmp.eq v3, v6
    brif v7, block2, block3
block2:
    return v4
block3:
    v8 = iadd v4, v5
    v9 = iconst.i32 1
    v10 = isub v3, v9
    jump block1(v10, v5, v8)
}
";
    assert_eq!(eval(src, "fib", &[Val::I32(50)]), Val::I64(12_586_269_025));
}

#[test]
fn switch_matches_bit_patterns_of_the_type() {
    let src = "\
func @f(i8) -> i32 {
block0(v0: i8):
    switch v0, block1, [-1: block2, 3: block3(v0)]
block1:
    v1 = iconst.i32 0
    return v1
block2:
    v2 = iconst.i32 1
    return v2
block3(v3: i8):
    v4 = uext.i32 v3
    return v4
}
";
    assert_eq!(eval(src, "f", &[Val::I8(0xff)]), Val::I32(1));
    assert_eq!(eval(src, "f", &[Val::I8(3)]), Val::I32(3));
    assert_eq!(eval(src, "f", &[Val::I8(4)]), Val::I32(0));
}

const CALLS: &str = "\
func @fact(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 1
    v2 = icmp.ule v0, v1
    brif v2, block1, block2
block1:
    return v1
block2:
    v3 = isub v0, v1
    v4 = call @fact(v3)
    v5 = imul v0, v4
    return v5
}
func @divmod(i32, i32) -> (i32, i32) {
block0(v0: i32, v1: i32):
    v2 = udiv v0, v1
    v3 = urem v0, v1
    return v2, v3
}
func @indirect(i32) -> i32 {
block0(v0: i32):
    v1 = symbol_addr @divmod
    v2, v3 = call_indirect v1(v0, v0) : (i32, i32) -> (i32, i32)
    return v3
}
func @bad_target() -> i32 {
block0:
    v0 = iconst.i64 64
    v1 = inttoptr.ptr v0
    v2 = call_indirect v1() : () -> i32
    return v2
}
func @count_down(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = iconst.i64 0
    v3 = icmp.eq v0, v2
    brif v3, block1, block2
block1:
    return v1
block2:
    v4 = iconst.i64 1
    v5 = isub v0, v4
    v6 = iadd v1, v4
    return_call @count_down(v5, v6)
}
func @spin() {
block0:
    jump block1
block1:
    jump block1
}
func @stop() -> i32 {
block0:
    trap
}
";

#[test]
fn calls_return_every_result() {
    assert_eq!(
        eval(CALLS, "fact", &[Val::I64(20)]),
        Val::I64(2_432_902_008_176_640_000)
    );
    assert_eq!(
        run(CALLS, "divmod", &[Val::I32(17), Val::I32(5)]),
        Outcome::Return(vec![Val::I32(3), Val::I32(2)])
    );
    assert_eq!(eval(CALLS, "indirect", &[Val::I32(7)]), Val::I32(0));
}

#[test]
fn traps_and_limits_are_outcomes() {
    let module = module(CALLS);
    let new = || Interpreter::new(&module);
    assert_eq!(
        new().call("bad_target", &[]),
        Outcome::Trap(Trap::BadCallTarget { addr: 64 })
    );
    assert_eq!(new().call("stop", &[]), Outcome::Trap(Trap::Unreachable));
    assert_eq!(
        new().with_step_limit(1000).call("spin", &[]),
        Outcome::StepLimit
    );
    assert_eq!(
        new().with_depth_limit(10).call("fact", &[Val::I64(20)]),
        Outcome::Trap(Trap::StackOverflow)
    );
    assert_eq!(
        new().call("divmod", &[Val::I32(1), Val::I32(0)]),
        Outcome::Trap(Trap::DivisionByZero)
    );
}

#[test]
fn tail_calls_run_in_constant_depth() {
    let module = module(CALLS);
    let mut interp = Interpreter::new(&module).with_depth_limit(2);
    assert_eq!(
        interp.call("count_down", &[Val::I64(10_000), Val::I64(0)]),
        Outcome::Return(vec![Val::I64(10_000)])
    );
}

#[test]
fn byval_parameters_get_a_private_copy() {
    let src = "\
func @clobber(ptr byval {i32, i32}) -> i32 {
block0(v0: ptr):
    v1 = iconst.i32 99
    store v1, v0
    v2 = iconst.i64 4
    v3 = ptradd v0, v2
    v4 = load.i32 v3
    return v4
}
func @f() -> (i32, i32) {
    ss0 = stack_slot 8, align 4
block0:
    v0 = stack_addr ss0
    v1 = iconst.i32 5
    store v1, v0
    v2 = iconst.i64 4
    v3 = ptradd v0, v2
    v4 = iconst.i32 6
    store v4, v3
    v5 = call @clobber(v0)
    v6 = load.i32 v0
    return v5, v6
}
";
    assert_eq!(
        run(src, "f", &[]),
        Outcome::Return(vec![Val::I32(6), Val::I32(5)])
    );
}

#[test]
fn invoke_continues_to_the_normal_destination() {
    let src = "\
decl @__gxx_personality_v0()
func @callee(i32) -> i32 {
block0(v0: i32):
    v1 = iconst.i32 1
    v2 = iadd v0, v1
    return v2
}
func @f(i32) -> i32 cc(c) personality @__gxx_personality_v0 {
block0(v0: i32):
    v1 = invoke @callee(v0), block1, block2
block1:
    return v1
block2:
    v2, v3 = landingpad cleanup
    trap
}
";
    assert_eq!(eval(src, "f", &[Val::I32(41)]), Val::I32(42));
}

#[test]
fn arguments_must_match_the_signature() {
    assert!(matches!(
        run(CALLS, "fact", &[Val::I32(1)]),
        Outcome::Invalid(_)
    ));
    assert!(matches!(run(CALLS, "fact", &[]), Outcome::Invalid(_)));
}

#[test]
fn a_function_runs_without_a_module() {
    let module = module(CALLS);
    let divmod = module
        .functions
        .iter()
        .find(|f| f.name == "divmod")
        .unwrap();
    assert_eq!(
        Interpreter::for_function(divmod).call("divmod", &[Val::I32(9), Val::I32(4)]),
        Outcome::Return(vec![Val::I32(2), Val::I32(1)])
    );
    let fact = module.functions.iter().find(|f| f.name == "fact").unwrap();
    assert_eq!(
        Interpreter::for_function(fact).call("fact", &[Val::I64(1)]),
        Outcome::Return(vec![Val::I64(1)])
    );
}

#[test]
fn indirect_tail_calls_and_pointer_comparisons() {
    let src = "\
func @twice(i64) -> i64 {
block0(v0: i64):
    v1 = iadd v0, v0
    return v1
}
func @f(i64) -> i64 {
block0(v0: i64):
    v1 = symbol_addr @twice
    return_call_indirect v1(v0) : (i64) -> i64
}
func @same(ptr, ptr) -> (i8, i8) {
block0(v0: ptr, v1: ptr):
    v2 = icmp.eq v0, v1
    v3 = icmp.ult v0, v1
    return v2, v3
}
";
    assert_eq!(eval(src, "f", &[Val::I64(21)]), Val::I64(42));
    assert_eq!(
        run(src, "same", &[Val::Ptr(8), Val::Ptr(16)]),
        Outcome::Return(vec![Val::I8(0), Val::I8(1)])
    );
}
