//! Operations outside the interpreter's model stop with `Unsupported`.

use super::run;
use keel_ir_interp::Outcome;

fn unsupported(src: &str, name: &str) -> String {
    match run(src, name, &[]) {
        Outcome::Unsupported(what) => what,
        other => panic!("expected Unsupported, got {other:?}"),
    }
}

#[test]
fn external_calls() {
    let src = "\
decl @putchar(i32) -> i32
func @f() -> i32 {
block0:
    v0 = iconst.i32 65
    v1 = call @putchar(v0)
    return v1
}
";
    assert!(unsupported(src, "f").contains("@putchar"));
    assert!(matches!(run(src, "putchar", &[]), Outcome::Unsupported(_)));
}

#[test]
fn undefined_data() {
    let src = "\
decl data @environ
func @f() -> ptr {
block0:
    v0 = symbol_addr @environ
    return v0
}
";
    assert!(unsupported(src, "f").contains("@environ"));
}

#[test]
fn target_intrinsics_and_f80() {
    let debugtrap = "func @f() {\nblock0:\n    intrinsic debugtrap()\n    return\n}\n";
    assert!(unsupported(debugtrap, "f").contains("debugtrap"));
    let f80 = "func @f() -> f80 {\nblock0:\n    v0 = fconst.f80 16383, 0x8000000000000000\n    return v0\n}\n";
    assert!(unsupported(f80, "f").contains("f80"));
}

#[test]
fn variadic_arguments() {
    let src = "\
func @f(i32, ...) -> i32 {
    ss0 = stack_slot 32, align 8
block0(v0: i32):
    v1 = stack_addr ss0
    intrinsic va_start(v1)
    v2 = va_arg.i32 v1
    return v2
}
func @g() -> i32 {
block0:
    v0 = iconst.i32 1
    v1 = call @f(v0, v0)
    return v1
}
";
    assert!(unsupported(src, "g").contains("va_start"));
}
