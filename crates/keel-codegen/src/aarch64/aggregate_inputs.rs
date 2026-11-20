use crate::abi::{self, ArgKind, ArgLoc, Slot};
use crate::aggregate_inputs::{self, ArgumentLocation, Placement};
use crate::regalloc::Loc;
use keel_ir::{Function, InstData, Purpose};
use std::borrow::Cow;

impl ArgumentLocation for ArgLoc {
    fn pair_registers(&self) -> Option<[Loc; 2]> {
        if let Self::InRegs(parts) = self
            && let [low @ Loc::Reg(_), high @ Loc::Reg(_)] = parts.as_slice()
        {
            Some([*low, *high])
        } else {
            None
        }
    }

    fn scalar_register(&self) -> Option<Loc> {
        match self {
            Self::Value(Slot::Reg(register)) => Some(*register),
            _ => None,
        }
    }
}

pub(super) fn promote(
    f: &Function,
    debug: bool,
    linux: bool,
    scheduled: bool,
) -> Cow<'_, Function> {
    // Multiple incoming copies can put immediate pressure on the spill frame.
    // Large dispatches already trade register lifetimes for a smaller frame.
    if !scheduled
        || debug
        || f.sig
            .params
            .iter()
            .filter(|p| matches!(p.purpose, Purpose::ByVal(_)))
            .take(2)
            .count() < 2
        || f.layout_insts().any(|(_, inst)| {
            matches!(f.inst_data(inst), InstData::Switch { cases, .. } if cases.len() >= 24)
        })
    {
        return Cow::Borrowed(f);
    }

    aggregate_inputs::promote(f, debug, |sig| {
        let kinds: Vec<_> = sig
            .params
            .iter()
            .map(|p| ArgKind::of(p.ty, &p.purpose))
            .collect();
        let (locs, stack) = abi::arg_locs_for(&kinds, kinds.len(), linux);
        Placement {
            locs,
            stack,
            stack_align: 16,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_words_preserve_stack_arguments_and_observable_homes() {
        let source = "
decl @observe(ptr)
func @read(ptr byval [2 x i64], i64, ptr byval [2 x i64], i64, i64, i64, ptr byval [2 x i64], i64) -> i64 cc(c) {
block0(v0:ptr, v1:i64, v2:ptr, v3:i64, v4:i64, v5:i64, v6:ptr, v7:i64):
    v8 = load.i64 v0
    v9 = iconst.i64 12
    v10 = ptradd v2, v9
    v11 = load.i32 align 4 v10
    v12 = uext.i64 v11
    v13 = load.i64 v6
    v14 = xor v8, v12
    v15 = xor v14, v13
    v16 = xor v15, v7
    return v16
}
";
        let module = keel_ir::parse(source).unwrap();
        let original = &module.functions[0];
        for linux in [false, true] {
            let candidate = promote(original, false, linux, true);
            assert!(matches!(candidate, Cow::Owned(_)));
            let byval_positions: Vec<_> = candidate
                .sig
                .params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| {
                    matches!(param.purpose, Purpose::ByVal(_)).then_some(index)
                })
                .collect();
            assert_eq!(byval_positions, [8]);
            keel_ir::verify_module(&keel_ir::parse(&format!("{candidate}")).unwrap()).unwrap();
            assert!(matches!(
                promote(original, false, linux, false),
                Cow::Borrowed(_)
            ));
        }

        let escaped = source.replace(
            "    v8 =",
            "    call @observe(v0)\n    call @observe(v2)\n    v8 =",
        );
        let escaped = keel_ir::parse(&escaped).unwrap();
        assert!(matches!(
            promote(&escaped.functions[0], false, false, true),
            Cow::Borrowed(_)
        ));
    }
}
