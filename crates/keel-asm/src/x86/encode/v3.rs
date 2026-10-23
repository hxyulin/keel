//! Form-driven VEX encoding. The pinned inventory owns operand roles, L/W,
//! memory widths and form-specific features; legacy BMI identities stay separate.
use super::*;
use keel_asm_ir::x86::{V3_FORMS, V3Opcode};

pub(super) fn encode<'a>(e: &mut Encoding<'a>, i: &'a Instruction) -> Result<(), String> {
    if let V3(op) = i.opcode {
        use V3Opcode::*;
        let o = i.operands.as_slice();
        match (op, o) {
            (Lzcnt, [R(r), rm])
                if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) =>
            {
                check_width(rm, r.width)?;
                e.rm(0xf3, &[0x0f, 0xbd], r.width, r.number, rm, None)?;
                if r.width == 16 {
                    let start = usize::from(matches!(rm,M(m) if m.segment.is_some()));
                    e.bytes.swap(start, start + 1);
                }
                return Ok(());
            }
            (Movbe, [dst, src]) => {
                let (r, rm, store) = match (dst, src) {
                    (R(r), M(_)) => (*r, src, false),
                    (M(_), R(r)) => (*r, dst, true),
                    _ => return Err("MOVBE requires register and memory".into()),
                };
                if r.class != RegisterClass::Gpr || !matches!(r.width, 16 | 32 | 64) {
                    return Err("invalid MOVBE register".into());
                }
                check_width(rm, r.width)?;
                return e.rm(
                    0,
                    &[0x0f, 0x38, if store { 0xf1 } else { 0xf0 }],
                    r.width,
                    r.number,
                    rm,
                    None,
                );
            }
            (Xgetbv | Xsetbv, []) => {
                e.bytes
                    .extend([0x0f, 0x01, if op == Xgetbv { 0xd0 } else { 0xd1 }]);
                return Ok(());
            }
            (Xsave | Xsave64 | Xrstor | Xrstor64, [rm @ M(m)]) if m.width == 0 => {
                return e.rm(
                    0,
                    &[0x0f, 0xae],
                    if matches!(op, Xsave64 | Xrstor64) {
                        64
                    } else {
                        32
                    },
                    if matches!(op, Xsave | Xsave64) { 4 } else { 5 },
                    rm,
                    None,
                );
            }
            _ => {}
        }
    }
    let f = V3_FORMS
        .iter()
        .filter(|f| f.matches(i))
        .min_by_key(|f| (u8::from(f.w), f.map))
        .ok_or_else(|| format!("unsupported v3 operands for {}", i.opcode))?;
    let o = &i.operands;
    let mut field = if f.field < 0 {
        f.digit
    } else {
        reg(&o[f.field as usize])?.number
    };
    let mut source = if f.source < 0 {
        None
    } else {
        Some(reg(&o[f.source as usize])?.number)
    };
    if f.vsib != 0 {
        let M(m) = &o[f.rm as usize] else {
            unreachable!()
        };
        let index = m.index.unwrap().number;
        if index == field || source == Some(index) || source == Some(field) {
            return Err("gather destination, mask and index must be distinct SIMD indices".into());
        }
    }
    if f.rm < 0 {
        e.bytes.extend([0xc5, 0xf8 | (u8::from(f.l) << 2), f.byte]);
        return Ok(());
    }
    let mut rm = &o[f.rm as usize];
    let mut byte = f.byte;
    // LLVM/GAS select the shorter equivalent store-direction register move.
    if matches!(f.byte, 0x10 | 0x28 | 0x6f)
        && f.source < 0
        && f.map == 1
        && f.field == 0
        && field < 8
        && matches!(rm,R(r) if matches!(r.class,RegisterClass::Xmm|RegisterClass::Ymm) && r.number>=8)
    {
        field = reg(rm)?.number;
        rm = &o[0];
        byte = if f.byte == 0x6f { 0x7f } else { f.byte + 1 };
    }
    if matches!(i.opcode, Vandps | Vandpd | Vorps | Vorpd | Vxorps | Vxorpd)
        && source.is_some_and(|r| r < 8)
        && matches!(rm,R(r) if r.number>=8)
    {
        source = Some(reg(rm)?.number);
        rm = &o[f.source as usize];
    }
    if matches!(i.opcode, V3(V3Opcode::Vmovq))
        && f.byte == 0x7e
        && f.pp == 2
        && field < 8
        && matches!(rm,R(r) if r.class==RegisterClass::Xmm && r.number>=8)
    {
        return e.vex_rm_bits(1, 1, 0xd6, reg(rm)?.number, None, &o[0], false, false);
    }
    e.vex_rm_bits(f.map, f.pp, byte, field, source, rm, f.l, f.w)?;
    if f.imm >= 0 {
        if f.imm_reg {
            e.bytes.push(reg(&o[f.imm as usize])?.number << 4)
        } else {
            e.imm(immediate(&o[f.imm as usize])?, 1, false)?
        }
    }
    Ok(())
}
