//! BMI GPR encodings, independent of AVX. See docs/x86-bmi.md for ISA references.
use super::*;
use keel_asm_ir::x86::Opcode;

pub(super) fn encode(e: &mut Encoding, op: Opcode, operands: &[Operand]) -> Result<(), String> {
    let invalid = || {
        format!(
            "unsupported operands for {op:?}: expected matching 32/64-bit GPRs and at most one r/m source"
        )
    };
    let Some(dst) = operands
        .first()
        .filter(|o| gpr(o) && matches!(width(o), 32 | 64))
    else {
        return Err(invalid());
    };
    let dst = reg(dst)?;
    let w = dst.width;
    // (opcode, map, pp, ModRM.reg, logical (uninverted) vvvv, r/m)
    let (byte, map, pp, field, v, rm) = match (op, operands) {
        (Tzcnt, [_, src]) => {
            check_width(src, w)?;
            return e.rm(0xf3, &[0x0f, 0xbc], w, dst.number, src, None);
        }
        (Blsi | Blsmsk | Blsr, [_, src]) => (
            0xf3,
            2,
            0,
            match op {
                Blsi => 3,
                Blsmsk => 2,
                _ => 1,
            },
            dst.number,
            src,
        ),
        (Andn | Pext | Pdep | Mulx, [_, a, b]) if gpr(a) && width(a) == w => (
            match op {
                Andn => 0xf2,
                Mulx => 0xf6,
                _ => 0xf5,
            },
            2,
            match op {
                Pext => 2,
                Pdep | Mulx => 3,
                _ => 0,
            },
            dst.number,
            reg(a)?.number,
            b,
        ),
        (Bextr | Bzhi | Shlx | Shrx | Sarx, [_, a, b]) if gpr(b) && width(b) == w => (
            if op == Bzhi { 0xf5 } else { 0xf7 },
            2,
            match op {
                Shlx => 1,
                Sarx => 2,
                Shrx => 3,
                _ => 0,
            },
            dst.number,
            reg(b)?.number,
            a,
        ),
        (Rorx, [_, src, I(imm)]) => {
            // Immediate must be a literal imm8. The hardware masks it to 5/6 bits.
            let Some(n) = constant(imm).filter(|n| (0..=255).contains(n)) else {
                return Err("rorx requires a constant imm8 in 0..255".into());
            };
            check_width(src, w)?;
            // Reserved VEX.vvvv is encoded 1111 (logical zero).
            vex(e, 3, 3, 0xf0, w, dst.number, 0, src)?;
            e.number(n, 1);
            return Ok(());
        }
        _ => return Err(invalid()),
    };
    check_width(rm, w)?;
    vex(e, map, pp, byte, w, field, v, rm)
}

#[allow(clippy::too_many_arguments)]
fn vex(
    e: &mut Encoding,
    map: u8,
    pp: u8,
    opcode: u8,
    w: u16,
    field: u8,
    v: u8,
    rm: &Operand,
) -> Result<(), String> {
    let (b, x) = match rm {
        R(r) => (r.number >> 3, 0),
        M(m) => (
            m.base
                .filter(|r| r.class != RegisterClass::Rip)
                .map_or(0, |r| r.number >> 3),
            m.index.map_or(0, |r| r.number >> 3),
        ),
        _ => return Err("BMI source must be a GPR or memory".into()),
    };
    if let M(m) = rm
        && let Some(segment) = m.segment
    {
        e.bytes.push(segment);
    }
    // All BMI VEX instructions use L=0; map 0F38/0F3A requires 3-byte VEX.
    e.bytes.extend([
        0xc4,
        ((!((field >> 3) & 1) & 1) << 7) | ((!x & 1) << 6) | ((!b & 1) << 5) | map,
        (u8::from(w == 64) << 7) | ((!v & 15) << 3) | pp,
        opcode,
    ]);
    match rm {
        R(r) => e.bytes.push(0xc0 | ((field & 7) << 3) | (r.number & 7)),
        M(m) => e.memory(field, m)?,
        _ => unreachable!(),
    }
    Ok(())
}
