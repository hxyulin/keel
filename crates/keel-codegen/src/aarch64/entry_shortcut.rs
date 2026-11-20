//! A guarded conversion of a tagged integer can return before building a frame.
use super::*;

pub(super) struct TaggedConversion {
    tag_offset: usize,
    tag: u8,
    status: u16,
}

impl TaggedConversion {
    pub(super) fn analyze(func: &Function) -> Option<Self> {
        if func.sig.cc != CallConv::C
            || func.sig.variadic
            || func.sig.returns_twice
            || func.attrs.optnone
            || func.attrs.naked
            || func.attrs.interrupt
            || func.personality.is_some()
            || func.sig.params.len() != 2
            || func
                .sig
                .params
                .iter()
                .any(|p| p.ty != Type::Ptr || p.purpose != Purpose::Normal)
            || func.sig.returns.len() != 1
            || func.sig.returns[0].ty != Type::I32
        {
            return None;
        }
        let entry = func.entry_block()?;
        let &[object, result] = func.block_params(entry) else {
            return None;
        };
        let &[offset, tag_addr, load, extend, tag, compare, branch] = func.block_insts(entry)
        else {
            return None;
        };
        let value = |i| func.first_result(i);
        let data = |i| func.inst_data(i);
        let integral = |inst| match *data(inst) {
            InstData::Iconst { imm, .. } => Some(imm),
            _ => None,
        };
        let tag_offset = usize::try_from(integral(offset)?).ok()?;
        let tag_value = u8::try_from(integral(tag)?).ok()?;
        if tag_offset >= 4096
            || !matches!(*data(tag_addr), InstData::Binary {
            op: BinaryOp::Ptradd, args
        } if args == [object, value(offset)])
        {
            return None;
        }
        // The original path may read the discriminator again when the
        // shortcut doesn't apply. Only an ordinary, nonobservable read can
        // be repeated. The load width and its address stay exact.
        if !matches!(*data(load), InstData::Load {
            ty: Type::I8, addr, flags,
        } if addr == value(tag_addr) && !flags.volatile && (flags.notrap || flags.trap_ub))
            || !matches!(*data(extend), InstData::Convert {
                op: ConvertOp::Uext, ty: Type::I32, arg,
            } if arg == value(load))
        {
            return None;
        }
        let InstData::Brif { cond, dests } = data(branch) else {
            return None;
        };
        let InstData::Icmp {
            cond: relation,
            args,
        } = *data(compare)
        else {
            return None;
        };
        if *cond != value(compare) || args != [value(extend), value(tag)] {
            return None;
        }
        let fast = match relation {
            IntCC::Eq => &dests[0],
            IntCC::Ne => &dests[1],
            _ => return None,
        };
        let &[payload, cast, store, status, ret] = func.block_insts(fast.block) else {
            return None;
        };
        let status_code = u16::try_from(integral(status)?).ok()?;
        if !fast.args.is_empty()
            || !matches!(*data(payload), InstData::Load { ty: Type::I64, addr, flags }
                if addr == object && !flags.volatile)
            || !matches!(*data(cast), InstData::Convert { op: ConvertOp::Sitofp, ty: Type::F64, arg }
                if arg == value(payload))
            || !matches!(*data(store), InstData::Store { addr, value: stored, flags }
                if addr == result && stored == value(cast) && !flags.volatile)
            || !matches!(data(ret), InstData::Return { args }
                if args.as_slice() == [value(status)])
        {
            return None;
        }
        Some(Self {
            tag_offset,
            tag: tag_value,
            status: status_code,
        })
    }

    pub(super) fn emit(self, fc: &mut FunctionCompiler<'_>) {
        let label = format!("L{}_framed_entry", fc.func.name);
        // A single compact unwind entry cannot describe both ranges.
        // Emit both CFA states so that the assembler keeps DWARF unwind CFI.
        fc.cfi(Cfi::DefCfaOffset(0));
        fc.mach(
            "ldrb",
            [
                Reg::w(16).into(),
                Address::offset(Reg::x(0), self.tag_offset).into(),
            ],
        );
        fc.mach("cmp", [Reg::w(16).into(), Arg::imm(self.tag)]);
        fc.conditional("ne", Name::Text(&label));
        fc.mach("ldr", [Reg::d(0).into(), Address::base(Reg::x(0)).into()]);
        fc.native(
            Opcode::SCVTF_asisdmisc_R,
            [(FieldId::sz, 1), (FieldId::Rd, 0), (FieldId::Rn, 0)],
            |out| out.write_str("scvtf\td0, d0"),
        );
        fc.mach("str", [Reg::d(0).into(), Address::base(Reg::x(1)).into()]);
        fc.materialize(0, self.status.into(), false);
        fc.native(Opcode::RET_64R_branch_reg, [(FieldId::Rn, 30)], |out| {
            out.write_str("ret")
        });
        fc.out.label(label);
    }
}
