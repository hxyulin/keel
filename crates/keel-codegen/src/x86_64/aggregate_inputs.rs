use super::sysv::{self, ArgLoc, Slot};
use crate::aggregate_inputs::{self, ArgumentLocation, Placement};
use crate::regalloc::Loc;
use keel_ir::Function;
use std::borrow::Cow;

impl ArgumentLocation for ArgLoc {
    fn pair_registers(&self) -> Option<[Loc; 2]> {
        if let Self::InRegs(parts) = self
            && let [low, high] = parts.as_slice()
            && matches!((low.loc, high.loc), (Loc::Reg(_), Loc::Reg(_)))
        {
            Some([low.loc, high.loc])
        } else {
            None
        }
    }

    fn scalar_register(&self) -> Option<Loc> {
        match self {
            Self::Value(Slot::Regs(register, None)) => Some(*register),
            _ => None,
        }
    }
}

pub(super) fn promote(f: &Function, debug: bool) -> Cow<'_, Function> {
    aggregate_inputs::promote(f, debug, |sig| {
        let placement = sysv::signature_arg_locs(
            sig,
            &sig.params
                .iter()
                .map(|p| sysv::signature_param_kind(sig, p))
                .collect::<Vec<_>>(),
        );
        Placement {
            locs: placement.locs,
            stack: placement.stack,
            stack_align: placement.stack_align,
        }
    })
}
