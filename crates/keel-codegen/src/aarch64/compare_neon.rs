//! Exact zero-operand selection without covering or moving either producer.
use super::*;
use keel_ir::NeonIntrinsic as N;
fn zero(func: &Function, value: Value, float: bool) -> bool {
    let value = func.resolve(value);
    let ValueDef::Result(i, _) = func.value_def(value) else {
        return false;
    };
    match func.inst_data(i) {
        InstData::VectorSplat { arg, .. } => zero(func, *arg, float),
        InstData::Iconst { imm: 0, .. } => !float,
        InstData::Fconst { bits: 0, .. } => float,
        _ => false,
    }
}
fn arrangement(ty: Type) -> String {
    let v = ty.vector().unwrap();
    let e = match v.lane_type().bytes().unwrap() {
        1 => 'b',
        2 => 'h',
        4 => 's',
        8 => 'd',
        _ => unreachable!(),
    };
    format!("{}{e}", v.lanes())
}
impl FunctionCompiler<'_> {
    pub(super) fn vector_compare_zero(
        &mut self,
        cond: IntCC,
        args: [Value; 2],
        result: Option<Value>,
    ) -> bool {
        let (source, cond) = if zero(self.func, args[1], false) {
            (args[0], cond)
        } else if zero(self.func, args[0], false) {
            (
                args[1],
                match cond {
                    IntCC::Slt => IntCC::Sgt,
                    IntCC::Sgt => IntCC::Slt,
                    IntCC::Sle => IntCC::Sge,
                    IntCC::Sge => IntCC::Sle,
                    IntCC::Eq | IntCC::Ne => cond,
                    _ => return false,
                },
            )
        } else {
            return false;
        };
        let (opcode, name) = match cond {
            IntCC::Eq | IntCC::Ne => (Opcode::CMEQ_asimdmisc_Z, "cmeq"),
            IntCC::Sge => (Opcode::CMGE_asimdmisc_Z, "cmge"),
            IntCC::Sgt => (Opcode::CMGT_asimdmisc_Z, "cmgt"),
            IntCC::Sle => (Opcode::CMLE_asimdmisc_Z, "cmle"),
            IntCC::Slt => (Opcode::CMLT_asimdmisc_Z, "cmlt"),
            _ => return false,
        };
        let ty = self.ty(source);
        let v = ty.vector().unwrap();
        let a = self.use_freg(source, FS0);
        let d = self.def_freg(result);
        let arr = arrangement(ty);
        self.native(
            opcode,
            [
                (FieldId::Q, u32::from(v.bytes() == 16)),
                (
                    FieldId::size,
                    v.lane_type().bytes().unwrap().trailing_zeros(),
                ),
                (FieldId::Rd, d.into()),
                (FieldId::Rn, a.into()),
            ],
            |out| write!(out, "{name}\tv{d}.{arr}, v{a}.{arr}, #0"),
        );
        if cond == IntCC::Ne {
            self.native(
                Opcode::NOT_asimdmisc_R,
                [
                    (FieldId::Q, u32::from(v.bytes() == 16)),
                    (FieldId::Rd, d.into()),
                    (FieldId::Rn, d.into()),
                ],
                |out| {
                    write!(
                        out,
                        "mvn\tv{d}.{}, v{d}.{}",
                        if v.bytes() == 16 { "16b" } else { "8b" },
                        if v.bytes() == 16 { "16b" } else { "8b" }
                    )
                },
            );
        }
        self.def_done(result, d);
        true
    }
    pub(super) fn neon_float_zero(
        &mut self,
        op: N,
        ty: Type,
        args: &[Value],
        result: Option<Value>,
    ) -> bool {
        if !matches!(op, N::Fcmeq | N::Fcmge | N::Fcmgt) {
            return false;
        }
        let (source, reversed) = if zero(self.func, args[1], true) {
            (args[0], false)
        } else if zero(self.func, args[0], true) {
            (args[1], true)
        } else {
            return false;
        };
        let input = self.ty(source);
        let lane = input.vector().map_or(input, |v| v.lane_type());
        let scalar = input.vector().is_none_or(|v| v.lanes() == 1);
        let (opcode, name) = match (op, reversed, scalar) {
            (N::Fcmeq, _, false) => (Opcode::FCMEQ_asimdmisc_FZ, "fcmeq"),
            (N::Fcmeq, _, true) => (Opcode::FCMEQ_asisdmisc_FZ, "fcmeq"),
            (N::Fcmge, false, false) => (Opcode::FCMGE_asimdmisc_FZ, "fcmge"),
            (N::Fcmge, false, true) => (Opcode::FCMGE_asisdmisc_FZ, "fcmge"),
            (N::Fcmgt, false, false) => (Opcode::FCMGT_asimdmisc_FZ, "fcmgt"),
            (N::Fcmgt, false, true) => (Opcode::FCMGT_asisdmisc_FZ, "fcmgt"),
            (N::Fcmge, true, false) => (Opcode::FCMLE_asimdmisc_FZ, "fcmle"),
            (N::Fcmge, true, true) => (Opcode::FCMLE_asisdmisc_FZ, "fcmle"),
            (N::Fcmgt, true, false) => (Opcode::FCMLT_asimdmisc_FZ, "fcmlt"),
            (N::Fcmgt, true, true) => (Opcode::FCMLT_asisdmisc_FZ, "fcmlt"),
            _ => unreachable!(),
        };
        let a = self.use_freg(source, FS0);
        let mut fields = vec![
            (FieldId::sz, u32::from(lane == Type::F64)),
            (FieldId::Rd, FS2.into()),
            (FieldId::Rn, a.into()),
        ];
        if !scalar {
            fields.push((FieldId::Q, u32::from(input.bytes() == Some(16))));
        }
        self.out.native(
            || keel_asm_ir::Instruction::new(opcode, fields),
            |out| {
                if scalar {
                    let e = if lane == Type::F64 { 'd' } else { 's' };
                    write!(out, "{name}\t{e}{FS2}, {e}{a}, #0.0")
                } else {
                    let arr = arrangement(input);
                    write!(out, "{name}\tv{FS2}.{arr}, v{a}.{arr}, #0.0")
                }
            },
        );
        if ty.is_int() {
            let d = self.def_reg(result);
            self.mach("fmov", [Reg::x(d).into(), Reg::d(FS2).into()]);
            self.def_done(result, d);
        } else {
            let d = self.def_freg(result);
            self.move_vector(d, FS2);
            self.def_done(result, d);
        }
        true
    }
}
