//! Baseline lane bit operations and wrapping integer accumulators.
use super::*;
use keel_ir::NeonIntrinsic as N;
impl FunctionCompiler<'_> {
    pub(super) fn neon_integer_misc(
        &mut self,
        op: N,
        ty: Type,
        args: &[Value],
        result: Option<Value>,
    ) {
        let (opcode, name) = match op {
            N::Cls => (Opcode::CLS_asimdmisc_R, "cls"),
            N::Rbit => (Opcode::RBIT_asimdmisc_R, "rbit"),
            N::Mla => (Opcode::MLA_asimdsame_only, "mla"),
            N::Mls => (Opcode::MLS_asimdsame_only, "mls"),
            N::Saba => (Opcode::SABA_asimdsame_only, "saba"),
            N::Uaba => (Opcode::UABA_asimdsame_only, "uaba"),
            N::Sadalp => (Opcode::SADALP_asimdmisc_P, "sadalp"),
            N::Uadalp => (Opcode::UADALP_asimdmisc_P, "uadalp"),
            _ => unreachable!("verified integer intrinsic"),
        };
        // Capture every operand before the destination, even for fully spilled
        // allocation and when a dying operand shares the result register.
        for (i, &arg) in args.iter().enumerate() {
            let scratch = [FS0, FS1, FS2][i];
            let source = self.use_freg(arg, scratch);
            self.move_vector(scratch, source);
        }
        let source = if args.len() == 1 { FS0 } else { FS1 };
        let shape = ty.vector().unwrap();
        let pair = matches!(op, N::Sadalp | N::Uadalp);
        let mut fields = vec![
            (FieldId::Q, u32::from(shape.bytes() == 16)),
            (FieldId::Rd, FS0.into()),
            (FieldId::Rn, source.into()),
        ];
        if op != N::Rbit {
            fields.push((
                FieldId::size,
                shape.lane_type().bytes().unwrap().trailing_zeros() - u32::from(pair),
            ));
        }
        if args.len() == 3 {
            fields.push((FieldId::Rm, FS2.into()));
        }
        let arrangement = |ty: Type| {
            let v = ty.vector().unwrap();
            format!(
                "{}{}",
                v.lanes(),
                match v.lane_type().bytes().unwrap() {
                    1 => 'b',
                    2 => 'h',
                    4 => 's',
                    8 => 'd',
                    _ => unreachable!(),
                }
            )
        };
        let dest = arrangement(ty);
        let input = arrangement(self.ty(args[if args.len() == 1 { 0 } else { 1 }]));
        self.out.native(
            || keel_asm_ir::Instruction::new(opcode, fields),
            |out| {
                write!(out, "{name}\tv{FS0}.{dest}, v{source}.{input}")?;
                if args.len() == 3 {
                    write!(out, ", v{FS2}.{input}")?;
                }
                Ok(())
            },
        );
        let d = self.def_freg(result);
        self.move_vector(d, FS0);
        self.def_done(result, d);
    }
}
