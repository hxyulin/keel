//! Use the incoming integer registers for immutable, nonescaping byval pairs.
//! The backend-local signature must preserve every argument location.
use crate::regalloc::Loc;
use keel_ir::fxhash::FxHashMap;
use keel_ir::{
    AbiParam, BinaryOp, CallConv, ConvertOp, Function, InsertPoint, Inst, InstData, Intrinsic,
    Purpose, Signature, Type, Value,
};
use std::borrow::Cow;

#[derive(Default)]
struct Reads {
    offsets: Vec<Inst>,
    loads: Vec<(Inst, usize, Type)>,
    escaped: bool,
}

pub(crate) struct Placement<L> {
    pub(crate) locs: Vec<L>,
    pub(crate) stack: u32,
    pub(crate) stack_align: u32,
}

pub(crate) trait ArgumentLocation: PartialEq {
    fn pair_registers(&self) -> Option<[Loc; 2]>;
    fn scalar_register(&self) -> Option<Loc>;
}

impl<L: ArgumentLocation> Placement<L> {
    fn preserves(&self, replacement: &Self, split: &[bool]) -> bool {
        if self.stack != replacement.stack || self.stack_align != replacement.stack_align {
            return false;
        }

        let mut new_locations = replacement.locs.iter();
        for (&split, old) in split.iter().zip(&self.locs) {
            if split {
                let Some(registers) = old.pair_registers() else {
                    return false;
                };
                for register in registers {
                    if new_locations.next().and_then(L::scalar_register) != Some(register) {
                        return false;
                    }
                }
            } else if new_locations.next() != Some(old) {
                return false;
            }
        }
        new_locations.next().is_none()
    }
}

pub(crate) fn promote<L: ArgumentLocation>(
    f: &Function,
    debug: bool,
    placement: impl Fn(&Signature) -> Placement<L>,
) -> Cow<'_, Function> {
    if debug
        || f.sig.cc != CallConv::C
        || f.attrs.optnone
        || f.attrs.naked
        || f.attrs.interrupt
        || f.num_values() > 100_000
        || f.sig.variadic
        || f.sig.returns_twice
        || f.sig.x86_vector_abi.is_some()
        || f.personality.is_some()
        || !f.debug_vars.is_empty()
        || !f.restrict_params.is_empty()
        || !f.attrs.contract.parameters.is_empty()
        || !f.attrs.contract.parameter_ranges.is_empty()
        || !f.sig.params.iter().any(|p| {
            matches!(&p.purpose,
            Purpose::ByVal(l) if crate::aggregate_results::is_integer_pair(l))
        })
    {
        return Cow::Borrowed(f);
    }
    let Some(entry) = f.entry_block() else {
        return Cow::Borrowed(f);
    };
    let before = placement(&f.sig);
    let params = f.block_params(entry);
    let mut addresses = FxHashMap::<Value, (usize, usize)>::default();
    let mut reads = Vec::<Reads>::new();
    for (index, (&v, (param, loc))) in params
        .iter()
        .zip(f.sig.params.iter().zip(&before.locs))
        .enumerate()
    {
        reads.push(Reads::default());
        if matches!(&param.purpose, Purpose::ByVal(l) if crate::aggregate_results::is_integer_pair(l))
            && loc.pair_registers().is_some()
            && f.resolve(v) == v
        {
            addresses.insert(v, (index, 0));
        }
    }
    if addresses.is_empty() {
        return Cow::Borrowed(f);
    }
    // Map address arithmetic directly based on the incoming copy. Other
    // derivations and every non-read use block scalarization of that copy.
    for (_, inst) in f.layout_insts() {
        let data = f.inst_data(inst);
        if matches!(
            data,
            InstData::InlineAsm { .. }
                | InstData::Intrinsic {
                    id: Intrinsic::FrameAddress,
                    ..
                }
        ) || crate::lowering::returns_twice(f, data)
            || data.branch_dests().iter().any(|d| d.block == entry)
        {
            return Cow::Borrowed(f);
        }
        if let InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [base, delta],
        } = *data
            && let Some(&(param, 0)) = addresses.get(&f.resolve(base))
            && let Some(offset @ 0..=15) = crate::fold::iconst(f, delta)
        {
            let v = f.first_result(inst);
            if f.resolve(v) == v {
                addresses.insert(v, (param, offset as usize));
                reads[param].offsets.push(inst);
            }
        }
    }
    for (_, inst) in f.layout_insts() {
        match f.inst_data(inst) {
            InstData::Binary {
                op: BinaryOp::Ptradd,
                ..
            } if addresses.contains_key(&f.first_result(inst)) => {}
            InstData::Load { ty, addr, flags } if !flags.volatile && flags.align <= 8 => {
                if let Some(&(param, offset)) = addresses.get(&f.resolve(*addr)) {
                    let value = f.first_result(inst);
                    let width = match ty {
                        Type::Ptr => 8,
                        Type::I8 | Type::I16 | Type::I32 | Type::I64 => {
                            ty.bytes().unwrap() as usize
                        }
                        _ => {
                            reads[param].escaped = true;
                            continue;
                        }
                    };
                    if (offset % 8) + width <= 8
                        && offset.is_multiple_of(flags.align as usize)
                        && f.resolve(value) == value
                    {
                        reads[param].loads.push((inst, offset, *ty));
                    } else {
                        reads[param].escaped = true;
                    }
                }
            }
            data => data.for_each_operand(|value| {
                if let Some(&(param, _)) = addresses.get(&f.resolve(value)) {
                    reads[param].escaped = true;
                }
            }),
        }
    }
    let split: Vec<_> = reads
        .iter()
        .map(|r| !r.escaped && !r.loads.is_empty())
        .collect();
    if !split.iter().any(|&yes| yes) {
        return Cow::Borrowed(f);
    }
    let mut sig = f.sig.clone();
    sig.params.clear();
    for (&need, p) in split.iter().zip(&f.sig.params) {
        if need {
            sig.params
                .extend([AbiParam::new(Type::I64), AbiParam::new(Type::I64)]);
        } else {
            sig.params.push(p.clone());
        }
    }
    if !before.preserves(&placement(&sig), &split) {
        return Cow::Borrowed(f);
    }

    let mut out = f.clone();
    for i in (0..params.len()).rev() {
        out.remove_block_param(entry, i);
    }
    out.sig = sig;
    for (&v, (&need, reads)) in params.iter().zip(split.iter().zip(reads)) {
        if !need {
            let new = out.append_block_param(entry, f.value_type(v));
            if f.resolve(v) == v {
                out.replace_all_uses(v, new);
            }
            continue;
        }
        let lanes = [
            out.append_block_param(entry, Type::I64),
            out.append_block_param(entry, Type::I64),
        ];
        for (load, offset, ty) in reads.loads {
            let at = InsertPoint::Before(load);
            let mut bits = lanes[offset / 8];
            let shift = (offset % 8) * 8;
            if shift != 0 {
                let k = out.insert_inst(at, InstData::iconst(Type::I64, shift as i64));
                let op = out.insert_inst(
                    at,
                    InstData::Binary {
                        op: BinaryOp::Ushr,
                        args: [bits, out.first_result(k)],
                    },
                );
                bits = out.first_result(op);
            }
            if ty != Type::I64 {
                let op = if ty == Type::Ptr {
                    ConvertOp::IntToPtr
                } else {
                    ConvertOp::Trunc
                };
                let value = out.insert_inst(at, InstData::Convert { op, ty, arg: bits });
                bits = out.first_result(value);
            }
            out.replace_all_uses(out.first_result(load), bits);
            out.remove_inst(load);
        }
        for inst in reads.offsets {
            out.remove_inst(inst);
        }
    }
    Cow::Owned(out)
}
