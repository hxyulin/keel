//! Runs test functions on the reference interpreter in `keel-ir-interp`.
//! Integer and pointer inputs are bit patterns, truncated to each parameter's
//! width. Missing inputs are zero.

#![allow(dead_code)]

use keel_ir::{Function, Type};
use keel_ir_interp::{Interpreter, Outcome, Val};

/// The function's arguments built from `inputs`.
pub fn args(func: &Function, inputs: &[u128]) -> Vec<Val> {
    let entry = func.entry_block().expect("function has a body");
    func.block_params(entry)
        .iter()
        .enumerate()
        .map(|(i, &param)| {
            let bits = inputs.get(i).copied().unwrap_or(0);
            match func.value_type(param) {
                Type::Ptr => Val::Ptr(bits as u64),
                ty => Val::int(ty, bits as i128),
            }
        })
        .collect()
}

/// How `func` ends on `inputs`.
pub fn outcome(func: &Function, inputs: &[u64]) -> Outcome {
    let inputs: Vec<u128> = inputs.iter().map(|&x| u128::from(x)).collect();
    Interpreter::for_function(func).call(&func.name, &args(func, &inputs))
}

/// The first result of `func` on `inputs`, zero-extended.
///
/// # Panics
///
/// If the function traps or does not return an integer or pointer.
pub fn execute_wide(func: &Function, inputs: &[u128]) -> u128 {
    let outcome = Interpreter::for_function(func).call(&func.name, &args(func, inputs));
    first_result(outcome, func)
}

/// [`execute_wide`] for inputs and results of at most 64 bits.
pub fn execute(func: &Function, inputs: &[u64]) -> u64 {
    first_result(outcome(func, inputs), func) as u64
}

/// The bits of the first returned value.
///
/// # Panics
///
/// If `outcome` is not a return of an integer or pointer.
pub fn first_result(outcome: Outcome, func: &Function) -> u128 {
    match outcome {
        Outcome::Return(values) => values
            .first()
            .and_then(Val::as_u128)
            .unwrap_or_else(|| panic!("@{} returned {values:?}", func.name)),
        other => panic!("@{} ended with {other:?}", func.name),
    }
}

/// Whether `func` traps on `inputs`.
pub fn traps(func: &Function, inputs: &[u64]) -> bool {
    matches!(outcome(func, inputs), Outcome::Trap(_))
}
