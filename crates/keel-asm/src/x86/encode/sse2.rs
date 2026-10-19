//! Legacy SSE/SSE2 forms used by compiler output. All are x86-64 baseline;
//! MMX, VEX, and post-SSE2 opcodes deliberately use separate identities.
use super::*;

fn imm8(e: &mut Encoding, o: &Operand) -> Result<(), String> {
    let n = constant(immediate(o)?)
        .filter(|n| (0..=255).contains(n))
        .ok_or("SSE control requires a constant imm8 in 0..255")?;
    e.number(n, 1);
    Ok(())
}

pub(super) fn encode(e: &mut Encoding, i: &Instruction) -> Result<(), String> {
    let o = i.operands.as_slice();
    let op = i.opcode;
    let invalid = || {
        format!("unsupported operands for {op}: expected XMM registers or matching memory width")
    };
    let [a, b, ..] = o else { return Err(invalid()) };
    if matches!(op, Pmovmskb | Movmskps | Movmskpd) {
        if o.len() != 2 || !gpr(a) || !matches!(width(a), 32 | 64) || !xmm(b) {
            return Err(invalid());
        }
        // Even the r64 spelling writes a zero-extended 32-bit result; REX.W=0.
        return e.rm(
            if matches!(op, Pmovmskb | Movmskpd) {
                0x66
            } else {
                0
            },
            &[0x0f, if op == Pmovmskb { 0xd7 } else { 0x50 }],
            32,
            reg(a)?.number,
            b,
            None,
        );
    }
    if matches!(
        op,
        Psllw | Pslld | Psllq | Psrlw | Psrld | Psrlq | Psraw | Psrad | Pslldq | Psrldq
    ) {
        if o.len() != 2 || !xmm(a) {
            return Err(invalid());
        }
        let (byte, group, reg_byte) = match op {
            Psllw => (0x71, 6, 0xf1),
            Pslld => (0x72, 6, 0xf2),
            Psllq => (0x73, 6, 0xf3),
            Psrlw => (0x71, 2, 0xd1),
            Psrld => (0x72, 2, 0xd2),
            Psrlq => (0x73, 2, 0xd3),
            Psraw => (0x71, 4, 0xe1),
            Psrad => (0x72, 4, 0xe2),
            Pslldq => (0x73, 7, 0),
            _ => (0x73, 3, 0),
        };
        if matches!(b, I(_)) {
            e.rm(0x66, &[0x0f, byte], 32, group, a, None)?;
            return imm8(e, b);
        }
        if reg_byte == 0 {
            return Err(invalid());
        }
        check_xmm(b, 128)?;
        return e.rm(0x66, &[0x0f, reg_byte], 32, reg(a)?.number, b, None);
    }
    // prefix, load opcode, store opcode (zero means no store), memory bits,
    // control immediate (None = no immediate, Some(None) = explicit operand).
    let (prefix, byte, store, bits, control) = match op {
        Movdqa => (0x66, 0x6f, 0x7f, 128, None),
        Movdqu => (0xf3, 0x6f, 0x7f, 128, None),
        Pxor => (0x66, 0xef, 0, 128, None),
        Pand => (0x66, 0xdb, 0, 128, None),
        Pandn => (0x66, 0xdf, 0, 128, None),
        Por => (0x66, 0xeb, 0, 128, None),
        Pcmpeqb => (0x66, 0x74, 0, 128, None),
        Pcmpeqw => (0x66, 0x75, 0, 128, None),
        Pcmpeqd => (0x66, 0x76, 0, 128, None),
        Pcmpgtb => (0x66, 0x64, 0, 128, None),
        Pcmpgtw => (0x66, 0x65, 0, 128, None),
        Pcmpgtd => (0x66, 0x66, 0, 128, None),
        Pshufd => (0x66, 0x70, 0, 128, Some(None)),
        Pshuflw => (0xf2, 0x70, 0, 128, Some(None)),
        Pshufhw => (0xf3, 0x70, 0, 128, Some(None)),
        Punpcklbw => (0x66, 0x60, 0, 128, None),
        Punpcklwd => (0x66, 0x61, 0, 128, None),
        Punpckldq => (0x66, 0x62, 0, 128, None),
        Punpcklqdq => (0x66, 0x6c, 0, 128, None),
        Punpckhbw => (0x66, 0x68, 0, 128, None),
        Punpckhwd => (0x66, 0x69, 0, 128, None),
        Punpckhdq => (0x66, 0x6a, 0, 128, None),
        Punpckhqdq => (0x66, 0x6d, 0, 128, None),
        Unpcklps => (0, 0x14, 0, 128, None),
        Unpckhps => (0, 0x15, 0, 128, None),
        Unpcklpd => (0x66, 0x14, 0, 128, None),
        Unpckhpd => (0x66, 0x15, 0, 128, None),
        Shufps => (0, 0xc6, 0, 128, Some(None)),
        Shufpd => (0x66, 0xc6, 0, 128, Some(None)),
        Andnps => (0, 0x55, 0, 128, None),
        Andnpd => (0x66, 0x55, 0, 128, None),
        Minps => (0, 0x5d, 0, 128, None),
        Minpd => (0x66, 0x5d, 0, 128, None),
        Maxps => (0, 0x5f, 0, 128, None),
        Maxpd => (0x66, 0x5f, 0, 128, None),
        Cmpps => (0, 0xc2, 0, 128, Some(None)),
        Cmppd => (0x66, 0xc2, 0, 128, Some(None)),
        Cmpss => (0xf3, 0xc2, 0, 32, Some(None)),
        Cmpsd => (0xf2, 0xc2, 0, 64, Some(None)),
        Cmpltss => (0xf3, 0xc2, 0, 32, Some(Some(1))),
        Cmpnltps => (0, 0xc2, 0, 128, Some(Some(5))),
        Pmaddwd => (0x66, 0xf5, 0, 128, None),
        Pmulhuw => (0x66, 0xe4, 0, 128, None),
        Pmulhw => (0x66, 0xe5, 0, 128, None),
        Pmuludq => (0x66, 0xf4, 0, 128, None),
        Cvtdq2ps => (0, 0x5b, 0, 128, None),
        Cvtps2dq => (0x66, 0x5b, 0, 128, None),
        Cvttps2dq => (0xf3, 0x5b, 0, 128, None),
        Cvtps2pd => (0, 0x5a, 0, 64, None),
        Cvtpd2ps => (0x66, 0x5a, 0, 128, None),
        Cvtdq2pd => (0xf3, 0xe6, 0, 64, None),
        Cvtpd2dq => (0xf2, 0xe6, 0, 128, None),
        Cvttpd2dq => (0x66, 0xe6, 0, 128, None),
        _ => return Err(invalid()),
    };
    if o.len() != if control == Some(None) { 3 } else { 2 } {
        return Err(invalid());
    }
    let (r, m, byte) = if xmm(a) {
        (reg(a)?, b, byte)
    } else if store != 0 && matches!(a, M(_)) && xmm(b) {
        (reg(b)?, a, store)
    } else {
        return Err(invalid());
    };
    check_xmm(m, bits)?;
    e.rm(prefix, &[0x0f, byte], 32, r.number, m, None)?;
    match control {
        Some(Some(n)) => e.number(n, 1),
        Some(None) => imm8(e, &o[2])?,
        None => {}
    }
    Ok(())
}
