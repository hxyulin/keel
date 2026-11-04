//! Interpreter semantics, one module per instruction family, on textual IR.

#[path = "interp/control.rs"]
mod control;
#[path = "interp/float.rs"]
mod float;
#[path = "interp/integer.rs"]
mod integer;
#[path = "interp/memory.rs"]
mod memory;
#[path = "interp/unsupported.rs"]
mod unsupported;
#[path = "interp/vector.rs"]
mod vector;

use keel_ir::Module;
use keel_ir_interp::{Interpreter, Outcome, Val};

/// Parses and verifies `src`.
pub fn module(src: &str) -> Module {
    let module = keel_ir::parse(src).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
    if let Err(errors) = keel_ir::verify_module(&module) {
        panic!("{errors:?}\n{src}");
    }
    module
}

pub fn run(src: &str, name: &str, args: &[Val]) -> Outcome {
    Interpreter::new(&module(src)).call(name, args)
}

/// The single value `name` returns.
pub fn eval(src: &str, name: &str, args: &[Val]) -> Val {
    match run(src, name, args) {
        Outcome::Return(mut values) if values.len() == 1 => values.pop().unwrap(),
        other => panic!("{other:?}\n{src}"),
    }
}

/// `@f(a, b) = op a, b`, with the result type `ret`.
pub fn binary_fn(op: &str, a: &str, b: &str, ret: &str) -> String {
    format!(
        "func @f({a}, {b}) -> {ret} {{\nblock0(v0: {a}, v1: {b}):\n    v2 = {op} v0, v1\n    return v2\n}}\n"
    )
}

/// `@f(a) = op a`, with the result type `ret`.
pub fn unary_fn(op: &str, a: &str, ret: &str) -> String {
    format!("func @f({a}) -> {ret} {{\nblock0(v0: {a}):\n    v1 = {op} v0\n    return v1\n}}\n")
}
