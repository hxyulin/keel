//! Generic EVEX prefix and operand emission from reviewed imported forms.
use super::*;
use keel_asm_ir::x86::evex;

pub(super) fn encode(i: &Instruction) -> Result<Encoding<'_>, String> {
    let f = evex::form(i).ok_or_else(|| {
        format!(
            "unsupported AVX-512F operands or decorators for {}",
            i.opcode
        )
    })?;
    let Evex(op) = i.opcode else { unreachable!() };
    let mut out = Encoding::default();
    let mut field = f.form.reg_mask.trailing_zeros() as u8;
    let mut source = None;
    let mut rm = None;
    for (spec, operand) in f.form.operands.iter().zip(&i.operands) {
        match spec.encoding {
            "modrm_reg" => {
                let R(r) = operand else {
                    return Err("ModRM.reg requires a register".into());
                };
                field = r.number;
            }
            "modrm_rm" => rm = Some(operand),
            "ndsndd" => {
                let R(r) = operand else {
                    return Err("vvvv requires a register".into());
                };
                source = Some(r.number);
            }
            _ => {}
        }
    }
    let rm = rm.ok_or("AVX-512F form requires ModRM")?;
    let pp = match f.form.prefix {
        0x66 => 1,
        0xf3 => 2,
        0xf2 => 3,
        _ => 0,
    };
    if f.form.vex {
        out.vex_rm_bits(
            VexPrefix {
                map: f.form.map,
                pp,
                field,
                source,
                vector256: f.form.vector == 256,
                wide: f.form.w == 1,
            },
            f.form.byte,
            rm,
            op.options.address32
                || matches!(rm, M(m) if m.base.into_iter().chain(m.index).any(|r| r.width == 32)),
        )?;
    } else {
        let (b, x, v_high) = match rm {
            R(r) => (
                (r.number >> 3) & 1,
                (r.number >> 4) & 1,
                source.map_or(0, |r| r >> 4),
            ),
            M(m) => {
                if let Some(segment) = m.segment {
                    out.bytes.push(segment);
                }
                if op.options.address32
                    || m.base.into_iter().chain(m.index).any(|r| {
                        matches!(r.class, RegisterClass::Gpr | RegisterClass::Rip) && r.width == 32
                    })
                {
                    out.bytes.push(0x67);
                }
                let vsib = f
                    .form
                    .operands
                    .iter()
                    .any(|s| s.kind.starts_with("mem_vsib"));
                (
                    m.base
                        .filter(|r| r.class != RegisterClass::Rip)
                        .map_or(0, |r| (r.number >> 3) & 1),
                    m.index.map_or(0, |r| (r.number >> 3) & 1),
                    if vsib {
                        m.index.map_or(0, |r| r.number >> 4)
                    } else {
                        source.map_or(0, |r| r >> 4)
                    },
                )
            }
            _ => return Err("EVEX r/m requires a register or memory".into()),
        };
        let ll = op.options.rounding.map_or(
            match f.form.vector {
                512 => 2,
                256 => 1,
                _ => 0,
            },
            |r| r.bits(),
        );
        out.bytes.extend([
            0x62,
            ((!field >> 3) & 1) << 7
                | ((!x & 1) << 6)
                | ((!b & 1) << 5)
                | (((!field >> 4) & 1) << 4)
                | f.form.map,
            (u8::from(f.form.w == 1) << 7) | (source.map_or(15, |r| !r & 15) << 3) | 4 | pp,
            (u8::from(op.options.zero) << 7)
                | (ll << 5)
                | (u8::from(op.options.broadcast != 0 || op.options.rounding.is_some()) << 4)
                | ((!v_high & 1) << 3)
                | op.options.mask.unwrap_or(0),
            f.form.byte,
        ]);
        match rm {
            R(r) => out.bytes.push(0xc0 | ((field & 7) << 3) | (r.number & 7)),
            M(m) => out.memory_scaled_address(
                field,
                m,
                f.displacement_scale(op.options.broadcast != 0),
                op.options.address32,
            )?,
            _ => unreachable!(),
        }
    }
    for (spec, operand) in f.form.operands.iter().zip(&i.operands) {
        if spec.kind == "imm" {
            let I(a) = operand else {
                return Err("immediate required".into());
            };
            out.imm(a, spec.bytes(32) as u8, false)?;
        }
    }
    out.finish()?;
    Ok(out)
}
