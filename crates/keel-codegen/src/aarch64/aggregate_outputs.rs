//! Prove the location of each word before replacing a register byval pair.
use crate::abi::{self, ArgKind, ArgLoc, Slot};
use crate::aggregate_outputs::{Plan, is_pair};
use crate::regalloc::Loc;
use keel_ir::{AbiParam, CallConv, Signature, Type};

pub(super) fn plan(old: &Signature, linux: bool) -> Option<Plan> {
    if old.cc != CallConv::C
        || old.variadic
        || old.returns_twice
        || old.x86_vector_abi.is_some()
        || !old.params.iter().any(is_pair)
    {
        return None;
    }
    let placement = |sig: &Signature| {
        let kinds: Vec<_> = sig
            .params
            .iter()
            .map(|p| ArgKind::of(p.ty, &p.purpose))
            .collect();
        abi::CallLayout::new(&kinds, sig.params.len(), linux)
    };
    let before = placement(old);
    let split: Vec<_> = old
        .params
        .iter()
        .zip(&before.locations)
        .map(|(param, location)| {
            is_pair(param)
                && matches!(location, ArgLoc::InRegs(regs)
                if matches!(regs.as_slice(), [Loc::Reg(_), Loc::Reg(_)]))
        })
        .collect();
    if !split.contains(&true) {
        return None;
    }

    let mut signature = old.clone();
    signature.params.clear();
    for (parameter, &split) in old.params.iter().zip(&split) {
        if split {
            signature
                .params
                .extend([AbiParam::new(Type::I64), AbiParam::new(Type::I64)]);
        } else {
            signature.params.push(parameter.clone());
        }
    }
    let after = placement(&signature);
    if before.stack_bytes != after.stack_bytes || before.copy_bytes != after.copy_bytes {
        return None;
    }
    let mut offset = 0;
    for (index, (location, &split)) in before.locations.iter().zip(&split).enumerate() {
        if split {
            let ArgLoc::InRegs(regs) = location else {
                unreachable!()
            };
            if after.locations[offset..offset + 2]
                != [
                    ArgLoc::Value(Slot::Reg(regs[0])),
                    ArgLoc::Value(Slot::Reg(regs[1])),
                ]
            {
                return None;
            }
            offset += 2;
        } else {
            if *location != after.locations[offset]
                || before.copy_offsets.get(index).copied().flatten()
                    != after.copy_offsets.get(offset).copied().flatten()
            {
                return None;
            }
            offset += 1;
        }
    }
    Some(Plan {
        signature,
        order: (0..old.params.len()).collect(),
        split,
    })
}
