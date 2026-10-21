//! AVX XMM forms only. W=0 and L=0, including WIG/LIG forms, for
//! predictable encodings. Reuses legacy address emission and final fixup bias.
use super::*;

impl<'a> Encoding<'a> {
    fn vex_rm(
        &mut self,
        map: u8,
        pp: u8,
        byte: u8,
        field: u8,
        source: Option<u8>,
        rm: &'a Operand,
    ) -> Result<(), String> {
        let (b, x) = match rm {
            R(r) => (r.number >> 3, 0),
            M(m) => (
                m.base
                    .filter(|r| r.class != RegisterClass::Rip)
                    .map_or(0, |r| r.number >> 3),
                m.index.map_or(0, |r| r.number >> 3),
            ),
            _ => return Err("expected XMM register or memory".into()),
        };
        if let M(m) = rm
            && let Some(segment) = m.segment
        {
            self.bytes.push(segment);
        }
        // None means reserved encoded vvvv=1111, not register 15.
        let v = source.map_or(0x78, |r| ((!r) & 15) << 3);
        let r = ((!field >> 3) & 1) << 7;
        if map == 1 && b == 0 && x == 0 {
            self.bytes.extend([0xc5, r | v | pp]);
        } else {
            self.bytes
                .extend([0xc4, r | ((!x & 1) << 6) | ((!b & 1) << 5) | map, v | pp]);
        }
        self.bytes.push(byte);
        match rm {
            R(r) => self.bytes.push(0xc0 | ((field & 7) << 3) | (r.number & 7)),
            M(m) => self.memory(field, m)?,
            _ => unreachable!(),
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    Nds,
    Compare,
    PackedMove,
    ScalarMove,
    Round,
}

pub(super) fn encode<'a>(e: &mut Encoding<'a>, i: &'a Instruction) -> Result<(), String> {
    let o = i.operands.as_slice();
    let err = || format!("unsupported AVX operands for {:?}", i.opcode);
    if i.opcode == Vzeroupper {
        if !o.is_empty() {
            return Err(err());
        }
        e.bytes.extend([0xc5, 0xf8, 0x77]);
        return Ok(());
    }
    use Form::*;
    let (pp, byte, w, form) = match i.opcode {
        Vaddps => (0, 0x58, 128, Nds),
        Vaddpd => (1, 0x58, 128, Nds),
        Vsubps => (0, 0x5c, 128, Nds),
        Vsubpd => (1, 0x5c, 128, Nds),
        Vmulps => (0, 0x59, 128, Nds),
        Vmulpd => (1, 0x59, 128, Nds),
        Vdivps => (0, 0x5e, 128, Nds),
        Vdivpd => (1, 0x5e, 128, Nds),
        Vaddss => (2, 0x58, 32, Nds),
        Vaddsd => (3, 0x58, 64, Nds),
        Vsubss => (2, 0x5c, 32, Nds),
        Vsubsd => (3, 0x5c, 64, Nds),
        Vmulss => (2, 0x59, 32, Nds),
        Vmulsd => (3, 0x59, 64, Nds),
        Vdivss => (2, 0x5e, 32, Nds),
        Vdivsd => (3, 0x5e, 64, Nds),
        Vsqrtss => (2, 0x51, 32, Nds),
        Vsqrtsd => (3, 0x51, 64, Nds),
        Vminss => (2, 0x5d, 32, Nds),
        Vminsd => (3, 0x5d, 64, Nds),
        Vmaxss => (2, 0x5f, 32, Nds),
        Vmaxsd => (3, 0x5f, 64, Nds),
        Vandps => (0, 0x54, 128, Nds),
        Vandpd => (1, 0x54, 128, Nds),
        Vorps => (0, 0x56, 128, Nds),
        Vorpd => (1, 0x56, 128, Nds),
        Vxorps => (0, 0x57, 128, Nds),
        Vxorpd => (1, 0x57, 128, Nds),
        Vroundss => (1, 0x0a, 32, Round),
        Vroundsd => (1, 0x0b, 64, Round),
        Vucomiss => (0, 0x2e, 32, Compare),
        Vucomisd => (1, 0x2e, 64, Compare),
        Vmovss => (2, 0x10, 32, ScalarMove),
        Vmovsd => (3, 0x10, 64, ScalarMove),
        Vmovaps => (0, 0x28, 128, PackedMove),
        Vmovups => (0, 0x10, 128, PackedMove),
        Vmovapd => (1, 0x28, 128, PackedMove),
        Vmovupd => (1, 0x10, 128, PackedMove),
        _ => return Err(err()),
    };
    if form == Nds || form == Round || form == ScalarMove && o.len() == 3 {
        if o.len() != if form == Round { 4 } else { 3 } || !xmm(&o[0]) || !xmm(&o[1]) {
            return Err(err());
        }
        check_xmm(&o[2], w)?;
        // Scalar register moves merge from src1; memory loads have only two operands.
        if form == ScalarMove && !xmm(&o[2]) {
            return Err(err());
        }
        let mut source = reg(&o[1])?.number;
        let mut rm = &o[2];
        // Commutative bitwise forms can keep a low register in r/m for VEX2.
        if matches!(i.opcode, Vandps | Vandpd | Vorps | Vorpd | Vxorps | Vxorpd)
            && source < 8
            && xmm(rm)
            && reg(rm)?.number >= 8
        {
            source = reg(rm)?.number;
            rm = &o[1];
        }
        let mut field = reg(&o[0])?.number;
        let mut byte = byte;
        if form == ScalarMove && field < 8 && reg(rm)?.number >= 8 {
            field = reg(rm)?.number;
            rm = &o[0];
            byte = 0x11;
        }
        e.vex_rm(
            if form == Round { 3 } else { 1 },
            pp,
            byte,
            field,
            Some(source),
            rm,
        )?;
        if form == Round {
            e.imm(immediate(&o[3])?, 1, false)?;
        }
    } else {
        if o.len() != 2 {
            return Err(err());
        }
        let (field, rm, store) = if xmm(&o[0]) {
            (reg(&o[0])?.number, &o[1], false)
        } else if matches!(form, PackedMove | ScalarMove) && matches!(&o[0], M(_)) && xmm(&o[1]) {
            (reg(&o[1])?.number, &o[0], true)
        } else {
            return Err(err());
        };
        check_xmm(rm, w)?;
        if form == ScalarMove && !matches!(rm, M(_)) {
            return Err(err());
        }
        if form == PackedMove && !store && field < 8 && xmm(rm) && reg(rm)?.number >= 8 {
            e.vex_rm(1, pp, byte + 1, reg(rm)?.number, None, &o[0])?;
        } else {
            e.vex_rm(1, pp, byte + u8::from(store), field, None, rm)?;
        }
    }
    Ok(())
}
