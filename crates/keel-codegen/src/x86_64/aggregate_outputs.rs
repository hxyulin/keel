//! Replan the System V call before passing a pair as two SSA words.
use super::sysv::{self, ArgLoc, RegPiece, Slot};
use crate::aggregate_outputs::{Plan, is_pair};
use keel_ir::{AbiParam, CallConv, Signature, Type};

pub(super) fn plan(old: &Signature) -> Option<Plan> {
    if old.cc != CallConv::C || old.variadic || old.returns_twice || old.x86_vector_abi.is_some() {
        return None;
    }
    if !old.params.iter().any(is_pair) {
        return None;
    }
    let placement = |sig: &Signature| sysv::signature_arg_locs(sig, &super::param_kinds(sig));
    let before = placement(old);
    let split: Vec<_> = old
        .params
        .iter()
        .zip(&before.locs)
        .map(|(param, loc)| {
            is_pair(param)
                && (matches!(loc, ArgLoc::InRegs(_))
                    || matches!(loc, ArgLoc::OnStack(_))
                        && usize::from(before.ints) == sysv::INT_ARGS.len())
        })
        .collect();
    if !split.contains(&true) {
        return None;
    }
    let on_stack = |loc: &ArgLoc| matches!(loc, ArgLoc::Value(Slot::Stack(_)) | ArgLoc::OnStack(_));
    let mut order: Vec<_> = (0..old.params.len()).collect();
    // A rolled-back pair can be placed on the stack before the last GP
    // register is assigned. In the backend's local signature, assign the
    // registers first so two separate scalar words cannot claim it.
    if split
        .iter()
        .zip(&before.locs)
        .any(|(&split, loc)| split && on_stack(loc))
    {
        order.sort_by_key(|&index| on_stack(&before.locs[index]));
    }
    let mut signature = old.clone();
    signature.params.clear();
    for &index in &order {
        if split[index] {
            signature
                .params
                .extend([AbiParam::new(Type::I64), AbiParam::new(Type::I64)]);
        } else {
            signature.params.push(old.params[index].clone());
        }
    }
    let after = placement(&signature);
    if before.stack != after.stack
        || before.stack_align != after.stack_align
        || before.ints != after.ints
        || before.floats != after.floats
    {
        return None;
    }
    let mut positions = after.locs.iter();
    for &index in &order {
        let loc = &before.locs[index];
        let low = positions.next()?;
        if !split[index] {
            if loc != low {
                return None;
            }
            continue;
        }
        let high = positions.next()?;
        let same = match (loc, low, high) {
            (
                ArgLoc::InRegs(regs),
                ArgLoc::Value(Slot::Regs(lo, None)),
                ArgLoc::Value(Slot::Regs(hi, None)),
            ) => {
                regs.as_slice()
                    == [
                        RegPiece {
                            offset: 0,
                            loc: *lo,
                        },
                        RegPiece {
                            offset: 8,
                            loc: *hi,
                        },
                    ]
            }
            (
                ArgLoc::OnStack(from),
                ArgLoc::Value(Slot::Stack(lo)),
                ArgLoc::Value(Slot::Stack(hi)),
            ) => from == lo && *hi == from + 8,
            _ => false,
        };
        if !same {
            return None;
        }
    }
    Some(Plan {
        signature,
        split,
        order,
    })
}
