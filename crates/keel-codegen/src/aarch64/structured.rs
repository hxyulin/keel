//! Native structured SIMD memory with exact architectural footprints.
use super::*;
use keel_ir::NeonIntrinsic;

impl FunctionCompiler<'_> {
    pub(super) fn neon_memory(&mut self, op: NeonIntrinsic, ty: Type, args: &[Value], results: &[Value]) {
        let m = op.memory().unwrap();
        let shape = ty.vector().unwrap();
        let size = shape.lane_type().bytes().unwrap().trailing_zeros();
        let lane = if m.lane {
            let ValueDef::Result(i, _) = self.func.value_def(self.func.resolve(*args.last().unwrap())) else { unreachable!() };
            let InstData::Iconst { imm, .. } = *self.func.inst_data(i) else { unreachable!() };
            imm as u32
        } else { 0 };
        // v0-v7 are never allocated. Capture the complete register list before
        // touching any result, including spilled or aliased source operands.
        // Using ABI scratch registers here avoids constraining allocation to a
        // consecutive list and leaves all three reserved FP reload temps free.
        if m.store || m.lane {
            for component in 0..m.count {
                let source = self.use_freg(args[1 + component as usize], FS0);
                self.move_vector(component, source);
            }
        }
        let address = self.use_reg(args[0], S0);
        // LD2/3/4 .1d is reserved. With one lane, interleaved order equals
        // contiguous-list order, so LD1/ST1 lists preserve the same footprint.
        let interleaved = m.interleaved && shape.lanes() != 1;
        let (opcode, name) = if m.lane {
            match (m.store, m.count, size) {
                (false, 1, 0) => (Opcode::LD1_asisdlso_B1_1b, "ld1"),
                (false, 1, 1) => (Opcode::LD1_asisdlso_H1_1h, "ld1"),
                (false, 1, 2) => (Opcode::LD1_asisdlso_S1_1s, "ld1"),
                (false, 1, 3) => (Opcode::LD1_asisdlso_D1_1d, "ld1"),
                (false, 2, 0) => (Opcode::LD2_asisdlso_B2_2b, "ld2"),
                (false, 2, 1) => (Opcode::LD2_asisdlso_H2_2h, "ld2"),
                (false, 2, 2) => (Opcode::LD2_asisdlso_S2_2s, "ld2"),
                (false, 2, 3) => (Opcode::LD2_asisdlso_D2_2d, "ld2"),
                (false, 3, 0) => (Opcode::LD3_asisdlso_B3_3b, "ld3"),
                (false, 3, 1) => (Opcode::LD3_asisdlso_H3_3h, "ld3"),
                (false, 3, 2) => (Opcode::LD3_asisdlso_S3_3s, "ld3"),
                (false, 3, 3) => (Opcode::LD3_asisdlso_D3_3d, "ld3"),
                (false, 4, 0) => (Opcode::LD4_asisdlso_B4_4b, "ld4"),
                (false, 4, 1) => (Opcode::LD4_asisdlso_H4_4h, "ld4"),
                (false, 4, 2) => (Opcode::LD4_asisdlso_S4_4s, "ld4"),
                (false, 4, 3) => (Opcode::LD4_asisdlso_D4_4d, "ld4"),
                (true, 1, 0) => (Opcode::ST1_asisdlso_B1_1b, "st1"),
                (true, 1, 1) => (Opcode::ST1_asisdlso_H1_1h, "st1"),
                (true, 1, 2) => (Opcode::ST1_asisdlso_S1_1s, "st1"),
                (true, 1, 3) => (Opcode::ST1_asisdlso_D1_1d, "st1"),
                (true, 2, 0) => (Opcode::ST2_asisdlso_B2_2b, "st2"),
                (true, 2, 1) => (Opcode::ST2_asisdlso_H2_2h, "st2"),
                (true, 2, 2) => (Opcode::ST2_asisdlso_S2_2s, "st2"),
                (true, 2, 3) => (Opcode::ST2_asisdlso_D2_2d, "st2"),
                (true, 3, 0) => (Opcode::ST3_asisdlso_B3_3b, "st3"),
                (true, 3, 1) => (Opcode::ST3_asisdlso_H3_3h, "st3"),
                (true, 3, 2) => (Opcode::ST3_asisdlso_S3_3s, "st3"),
                (true, 3, 3) => (Opcode::ST3_asisdlso_D3_3d, "st3"),
                (true, 4, 0) => (Opcode::ST4_asisdlso_B4_4b, "st4"),
                (true, 4, 1) => (Opcode::ST4_asisdlso_H4_4h, "st4"),
                (true, 4, 2) => (Opcode::ST4_asisdlso_S4_4s, "st4"),
                (true, 4, 3) => (Opcode::ST4_asisdlso_D4_4d, "st4"),
                _ => unreachable!(),
            }
        } else if m.replicate {
            match m.count {
                1 => (Opcode::LD1R_asisdlso_R1, "ld1r"),
                2 => (Opcode::LD2R_asisdlso_R2, "ld2r"),
                3 => (Opcode::LD3R_asisdlso_R3, "ld3r"),
                4 => (Opcode::LD4R_asisdlso_R4, "ld4r"),
                _ => unreachable!(),
            }
        } else {
            match (m.store, interleaved, m.count) {
                (false, false, 1) => (Opcode::LD1_asisdlse_R1_1v, "ld1"),
                (false, false, 2) => (Opcode::LD1_asisdlse_R2_2v, "ld1"),
                (false, false, 3) => (Opcode::LD1_asisdlse_R3_3v, "ld1"),
                (false, false, 4) => (Opcode::LD1_asisdlse_R4_4v, "ld1"),
                (false, true, 1) => (Opcode::LD1_asisdlse_R1_1v, "ld1"),
                (false, true, 2) => (Opcode::LD2_asisdlse_R2, "ld2"),
                (false, true, 3) => (Opcode::LD3_asisdlse_R3, "ld3"),
                (false, true, 4) => (Opcode::LD4_asisdlse_R4, "ld4"),
                (true, false, 1) => (Opcode::ST1_asisdlse_R1_1v, "st1"),
                (true, false, 2) => (Opcode::ST1_asisdlse_R2_2v, "st1"),
                (true, false, 3) => (Opcode::ST1_asisdlse_R3_3v, "st1"),
                (true, false, 4) => (Opcode::ST1_asisdlse_R4_4v, "st1"),
                (true, true, 1) => (Opcode::ST1_asisdlse_R1_1v, "st1"),
                (true, true, 2) => (Opcode::ST2_asisdlse_R2, "st2"),
                (true, true, 3) => (Opcode::ST3_asisdlse_R3, "st3"),
                (true, true, 4) => (Opcode::ST4_asisdlse_R4, "st4"),
                _ => unreachable!(),
            }
        };
        let fields = [(FieldId::Q, if m.lane { (lane << size) >> 3 } else { u32::from(shape.bytes() == 16) }),
                      (FieldId::Rn, address.into()), (FieldId::Rt, 0)];
        let element = ["b", "h", "s", "d"][size as usize];
        let arrangement = if m.lane { element.to_string() } else { format!("{}{element}", shape.lanes()) };
        let list = (0..m.count).map(|r| format!("v{r}.{arrangement}")).collect::<Vec<_>>().join(", ");
        let text = |out: &mut crate::text::Text| {
            write!(out, "{name}\t{{{list}}}")?;
            if m.lane { write!(out, "[{lane}]")?; }
            write!(out, ", [x{address}]")
        };
        if !m.lane {
            self.native(opcode, [fields[0], fields[1], fields[2], (FieldId::size, size)], text);
        } else if size < 2 {
            let offset = lane << size;
            self.native(opcode, [fields[0], fields[1], fields[2], (FieldId::S, (offset >> 2) & 1), (FieldId::size, offset & 3)], text);
        } else if size == 2 {
            self.native(opcode, [fields[0], fields[1], fields[2], (FieldId::S, lane & 1)], text);
        } else {
            self.native(opcode, fields, text);
        }
        for (component, &result) in results.iter().enumerate() {
            let d = self.def_freg(Some(result));
            self.move_vector(d, component as u8);
            self.def_done(Some(result), d);
        }
    }
}
