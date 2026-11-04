//! Matchers over IR values shared by the passes.

use keel_ir::{Function, InstData, Value, ValueDef};

/// The immediate of the `iconst` defining `v`, looking through aliases.
/// It is the canonical form the verifier enforces: sign-extended from the
/// constant's type, so `iconst.i8 255` reads as -1.
pub(crate) fn int_const(f: &Function, v: Value) -> Option<i64> {
    let ValueDef::Result(inst, _) = f.value_def(f.resolve(v)) else {
        return None;
    };
    match *f.inst_data(inst) {
        InstData::Iconst { imm, .. } => Some(imm),
        _ => None,
    }
}

/// The `iconst` defining `v` as unsigned bits of its type: `iconst.i8 -1`
/// reads as 255.
pub(crate) fn int_bits(f: &Function, v: Value) -> Option<u128> {
    let imm = int_const(f, v)?;
    let width = 8 * f.value_type(f.resolve(v)).bytes()?;
    Some(imm as i128 as u128 & (u128::MAX >> (128 - width)))
}
