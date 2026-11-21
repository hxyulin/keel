use super::sysv::{self, ArgLoc, Slot};
use crate::aggregate_inputs::{self, ArgumentLocation, Placement};
use crate::regalloc::Loc;
use keel_ir::Function;
use std::borrow::Cow;

impl ArgumentLocation for ArgLoc {
    fn aggregate_registers(&self, words: usize) -> Option<(Loc, Option<Loc>)> {
        let Self::InRegs(parts) = self else {
            return None;
        };
        match (words, parts.as_slice()) {
            (1, [low]) if low.offset == 0 && matches!(low.loc, Loc::Reg(_)) => {
                Some((low.loc, None))
            }
            (2, [low, high])
                if low.offset == 0
                    && high.offset == 8
                    && matches!((low.loc, high.loc), (Loc::Reg(_), Loc::Reg(_))) =>
            {
                Some((low.loc, Some(high.loc)))
            }
            _ => None,
        }
    }

    fn scalar_register(&self) -> Option<Loc> {
        match self {
            Self::Value(Slot::Regs(register, None)) => Some(*register),
            _ => None,
        }
    }
}

pub(super) fn promote(f: &Function, debug: bool, call_alignment: u32) -> Cow<'_, Function> {
    aggregate_inputs::promote(f, debug, |sig| {
        let placement = sysv::signature_arg_locs(
            sig,
            &sig.params
                .iter()
                .map(|p| sysv::signature_param_kind(sig, p))
                .collect::<Vec<_>>(),
        );
        let stack_align = placement.stack_align(call_alignment);
        Placement {
            locs: placement.locs,
            stack: placement.stack,
            stack_align,
        }
    })
}
