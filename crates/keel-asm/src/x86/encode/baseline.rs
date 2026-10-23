//! Baseline MMX forms and the remaining legacy SIMD forms. MMX never aliases XMM.
use super::*;
fn mmx(o: &Operand) -> bool {
    matches!(o, R(r) if r.class == RegisterClass::Mmx)
}
fn packed_byte(op: keel_asm_ir::x86::Opcode) -> Option<u8> {
    Some(match op {
        Packssdw => 0x6b,
        Packsswb => 0x63,
        Packuswb => 0x67,
        Paddb => 0xfc,
        Paddw => 0xfd,
        Paddd => 0xfe,
        Paddq => 0xd4,
        Paddsb => 0xec,
        Paddsw => 0xed,
        Paddusb => 0xdc,
        Paddusw => 0xdd,
        Psubb => 0xf8,
        Psubw => 0xf9,
        Psubd => 0xfa,
        Psubq => 0xfb,
        Psubsb => 0xe8,
        Psubsw => 0xe9,
        Psubusb => 0xd8,
        Psubusw => 0xd9,
        Pavgb => 0xe0,
        Pavgw => 0xe3,
        Pmaxsw => 0xee,
        Pmaxub => 0xde,
        Pminsw => 0xea,
        Pminub => 0xda,
        Psadbw => 0xf6,
        Pand => 0xdb,
        Pandn => 0xdf,
        Por => 0xeb,
        Pxor => 0xef,
        Pcmpeqb => 0x74,
        Pcmpeqw => 0x75,
        Pcmpeqd => 0x76,
        Pcmpgtb => 0x64,
        Pcmpgtw => 0x65,
        Pcmpgtd => 0x66,
        Pmaddwd => 0xf5,
        Pmulhuw => 0xe4,
        Pmulhw => 0xe5,
        Pmullw => 0xd5,
        Pmuludq => 0xf4,
        Punpcklbw => 0x60,
        Punpcklwd => 0x61,
        Punpckldq => 0x62,
        Punpckhbw => 0x68,
        Punpckhwd => 0x69,
        Punpckhdq => 0x6a,
        _ => return None,
    })
}
pub(super) fn handles(i: &Instruction) -> bool {
    extra(i.opcode)
        || matches!(
            i.opcode,
            Packssdw
                | Packsswb
                | Packuswb
                | Paddsb
                | Paddsw
                | Paddusb
                | Paddusw
                | Psubsb
                | Psubsw
                | Psubusb
                | Psubusw
                | Pavgb
                | Pavgw
                | Pmaxsw
                | Pmaxub
                | Pminsw
                | Pminub
                | Psadbw
                | Pshufw
                | Pinsrw
                | Pextrw
        )
        || i.operands.iter().any(mmx)
            && (packed_byte(i.opcode).is_some()
                || matches!(
                    i.opcode,
                    Movd | Movq
                        | Pmovmskb
                        | Psllw
                        | Pslld
                        | Psllq
                        | Psrlw
                        | Psrld
                        | Psrlq
                        | Psraw
                        | Psrad
                ))
}
fn control(e: &mut Encoding<'_>, o: &Operand) -> Result<(), String> {
    let n = constant(immediate(o)?)
        .filter(|n| (0..=255).contains(n))
        .ok_or("SIMD control requires a constant imm8 in 0..255")?;
    e.number(n, 1);
    Ok(())
}
fn vector(o: &Operand, mm: bool, bits: u16) -> Result<(), String> {
    if (if mm { mmx(o) } else { xmm(o) }) || matches!(o, M(m) if m.width == 0 || m.width == bits) {
        Ok(())
    } else {
        Err("SIMD register class or memory width mismatch".into())
    }
}
pub(super) fn encode<'a>(e: &mut Encoding<'a>, i: &'a Instruction) -> Result<(), String> {
    let op = i.opcode;
    let o = i.operands.as_slice();
    let bad = || format!("invalid baseline SIMD operands for {op}");
    if extra(op) {
        return encode_extra(e, i);
    }
    let [a, b, ..] = o else { return Err(bad()) };
    let mm = mmx(a);
    let prefix = if mm { 0 } else { 0x66 };
    let bits = if mm { 64 } else { 128 };
    if let Some(byte) = packed_byte(op) {
        if o.len() != 2 || !(mm || xmm(a)) {
            return Err(bad());
        }
        vector(
            b,
            mm,
            if mm && matches!(op, Punpcklbw | Punpcklwd | Punpckldq) {
                32
            } else {
                bits
            },
        )?;
        // The original MMX low unpack forms read only m32, despite using mm64 registers.
        // Intel's memory operand size is m32 for these forms.
        return e.rm(prefix, &[0x0f, byte], 32, reg(a)?.number, b, None);
    }
    if matches!(
        op,
        Psllw | Pslld | Psllq | Psrlw | Psrld | Psrlq | Psraw | Psrad
    ) {
        if o.len() != 2 || !mm {
            return Err(bad());
        }
        let (byte, group, variable) = match op {
            Psllw => (0x71, 6, 0xf1),
            Pslld => (0x72, 6, 0xf2),
            Psllq => (0x73, 6, 0xf3),
            Psrlw => (0x71, 2, 0xd1),
            Psrld => (0x72, 2, 0xd2),
            Psrlq => (0x73, 2, 0xd3),
            Psraw => (0x71, 4, 0xe1),
            _ => (0x72, 4, 0xe2),
        };
        if matches!(b, I(_)) {
            e.rm(0, &[0x0f, byte], 32, group, a, None)?;
            return control(e, b);
        }
        vector(b, true, 64)?;
        return e.rm(0, &[0x0f, variable], 32, reg(a)?.number, b, None);
    }
    match op {
        Pshufw => {
            if o.len() != 3 || !mm {
                return Err(bad());
            }
            vector(b, true, 64)?;
            e.rm(0, &[0x0f, 0x70], 32, reg(a)?.number, b, None)?;
            control(e, &o[2])
        }
        Pinsrw => {
            if o.len() != 3
                || !(mm || xmm(a))
                || !(gpr(b) && matches!(width(b), 32 | 64)
                    || matches!(b,M(m) if m.width==0 || m.width==16))
            {
                return Err(bad());
            }
            e.rm(prefix, &[0x0f, 0xc4], 32, reg(a)?.number, b, None)?;
            control(e, &o[2])
        }
        Pextrw => {
            if o.len() != 3 || !gpr(a) || !matches!(width(a), 32 | 64) || !(mmx(b) || xmm(b)) {
                return Err(bad());
            }
            e.rm(
                if mmx(b) { 0 } else { 0x66 },
                &[0x0f, 0xc5],
                32,
                reg(a)?.number,
                b,
                None,
            )?;
            control(e, &o[2])
        }
        Pmovmskb => {
            if o.len() != 2 || !gpr(a) || !matches!(width(a), 32 | 64) || !mmx(b) {
                return Err(bad());
            }
            e.rm(0, &[0x0f, 0xd7], 32, reg(a)?.number, b, None)
        }
        Movd | Movq => {
            if o.len() != 2 {
                return Err(bad());
            }
            if op == Movq && mm && (mmx(b) || matches!(b, M(_))) {
                vector(b, true, 64)?;
                return e.rm(0, &[0x0f, 0x6f], 32, reg(a)?.number, b, None);
            }
            if op == Movq && matches!(a, M(_)) && mmx(b) {
                vector(a, true, 64)?;
                return e.rm(0, &[0x0f, 0x7f], 32, reg(b)?.number, a, None);
            }
            let (r, m, byte) = if mm {
                (a, b, 0x6e)
            } else if mmx(b) {
                (b, a, 0x7e)
            } else {
                return Err(bad());
            };
            let w = if op == Movd { 32 } else { 64 };
            check_width(m, w)?;
            e.rm(0, &[0x0f, byte], w, reg(r)?.number, m, None)
        }
        _ => Err(bad()),
    }
}

fn extra(op: keel_asm_ir::x86::Opcode) -> bool {
    matches!(
        op,
        Movq6e
            | Movq7e
            | MovqD6
            | Movq7f
            | MovssStore
            | MovsdStore
            | MovapsStore
            | MovupsStore
            | MovapdStore
            | MovupdStore
            | MovdqaStore
            | MovdquStore
            | Comiss
            | Comisd
            | Cvtss2si
            | Cvtsd2si
            | Cvtpi2ps
            | Cvtpi2pd
            | Cvtps2pi
            | Cvtpd2pi
            | Cvttps2pi
            | Cvttpd2pi
            | Rcpps
            | Rcpss
            | Rsqrtps
            | Rsqrtss
            | Movhps
            | Movlps
            | Movhpd
            | Movlpd
            | Movlhps
            | Movdq2q
            | Movq2dq
            | Movntq
            | Movntdq
            | Movntps
            | Movntpd
            | Movnti
            | Maskmovq
            | Maskmovdqu
            | Prefetchnta
            | Prefetcht0
            | Prefetcht1
            | Prefetcht2
            | Fxsave
            | Fxsave64
            | Fxrstor
            | Fxrstor64
            | Cmpeqps
            | Cmpltps
            | Cmpleps
            | Cmpunordps
            | Cmpneqps
            | Cmpnleps
            | Cmpordps
            | Cmpeqpd
            | Cmpltpd
            | Cmplepd
            | Cmpunordpd
            | Cmpneqpd
            | Cmpnltpd
            | Cmpnlepd
            | Cmpordpd
            | Cmpeqss
            | Cmpless
            | Cmpunordss
            | Cmpneqss
            | Cmpnltss
            | Cmpnless
            | Cmpordss
            | Cmpunordsd
            | Cmpnltsd
            | Cmpnlesd
            | Cmpltsd
            | Cmpneqsd
            | Cmpeqsd
            | Cmpordsd
            | Cmpltss
            | Cmplesd
            | Cmpnltps
    )
}

fn encode_extra<'a>(e: &mut Encoding<'a>, i: &'a Instruction) -> Result<(), String> {
    let op = i.opcode;
    let o = i.operands.as_slice();
    let bad = || format!("invalid operands for {op}");
    if matches!(
        op,
        Fxsave
            | Fxsave64
            | Fxrstor
            | Fxrstor64
            | Prefetchnta
            | Prefetcht0
            | Prefetcht1
            | Prefetcht2
    ) {
        let [a @ M(m)] = o else { return Err(bad()) };
        let fx = matches!(op, Fxsave | Fxsave64 | Fxrstor | Fxrstor64);
        if m.width != 0 && m.width != if fx { 0 } else { 8 } {
            return Err(bad());
        }
        let group = match op {
            Fxsave | Fxsave64 | Prefetchnta => 0,
            Fxrstor | Fxrstor64 | Prefetcht0 => 1,
            Prefetcht1 => 2,
            _ => 3,
        };
        return e.rm(
            0,
            &[0x0f, if fx { 0xae } else { 0x18 }],
            if matches!(op, Fxsave64 | Fxrstor64) {
                64
            } else {
                32
            },
            group,
            a,
            None,
        );
    }
    let [a, b, ..] = o else { return Err(bad()) };
    if o.len() != 2 {
        return Err(bad());
    }
    match op {
        Movq6e | Movq7e => {
            let (r, m) = if op == Movq6e { (a, b) } else { (b, a) };
            if !(mmx(r) || xmm(r)) {
                return Err(bad());
            }
            check_width(m, 64)?;
            return e.rm(
                if xmm(r) { 0x66 } else { 0 },
                &[0x0f, if op == Movq6e { 0x6e } else { 0x7e }],
                64,
                reg(r)?.number,
                m,
                None,
            );
        }
        MovqD6 | Movq7f => {
            let mm = op == Movq7f;
            if !(if mm { mmx(b) } else { xmm(b) }) {
                return Err(bad());
            }
            vector(a, mm, 64)?;
            return e.rm(
                if mm { 0 } else { 0x66 },
                &[0x0f, if mm { 0x7f } else { 0xd6 }],
                32,
                reg(b)?.number,
                a,
                None,
            );
        }
        MovssStore | MovsdStore | MovapsStore | MovupsStore | MovapdStore | MovupdStore
        | MovdqaStore | MovdquStore => {
            if !xmm(b) {
                return Err(bad());
            }
            let (prefix, byte, bits) = match op {
                MovssStore => (0xf3, 0x11, 32),
                MovsdStore => (0xf2, 0x11, 64),
                MovapsStore => (0, 0x29, 128),
                MovupsStore => (0, 0x11, 128),
                MovapdStore => (0x66, 0x29, 128),
                MovupdStore => (0x66, 0x11, 128),
                MovdqaStore => (0x66, 0x7f, 128),
                _ => (0xf3, 0x7f, 128),
            };
            check_xmm(a, bits)?;
            return e.rm(prefix, &[0x0f, byte], 32, reg(b)?.number, a, None);
        }
        Movntq | Movntdq | Movntps | Movntpd | Movnti => {
            if !matches!(a, M(_)) {
                return Err(bad());
            }
            let (prefix, byte, bits, w) = match op {
                Movntq if mmx(b) => (0, 0xe7, 64, 32),
                Movntdq if xmm(b) => (0x66, 0xe7, 128, 32),
                Movntps if xmm(b) => (0, 0x2b, 128, 32),
                Movntpd if xmm(b) => (0x66, 0x2b, 128, 32),
                Movnti if gpr(b) && matches!(width(b), 32 | 64) => (0, 0xc3, width(b), width(b)),
                _ => return Err(bad()),
            };
            if width(a) != 0 && width(a) != bits {
                return Err(bad());
            }
            return e.rm(prefix, &[0x0f, byte], w, reg(b)?.number, a, None);
        }
        Maskmovq | Maskmovdqu => {
            let mm = op == Maskmovq;
            if !(if mm {
                mmx(a) && mmx(b)
            } else {
                xmm(a) && xmm(b)
            }) {
                return Err(bad());
            }
            return e.rm(
                if mm { 0 } else { 0x66 },
                &[0x0f, 0xf7],
                32,
                reg(a)?.number,
                b,
                None,
            );
        }
        Movdq2q | Movq2dq => {
            if !(if op == Movdq2q {
                mmx(a) && xmm(b)
            } else {
                xmm(a) && mmx(b)
            }) {
                return Err(bad());
            }
            return e.rm(
                if op == Movdq2q { 0xf2 } else { 0xf3 },
                &[0x0f, 0xd6],
                32,
                reg(a)?.number,
                b,
                None,
            );
        }
        Movlhps => {
            if !xmm(a) || !xmm(b) {
                return Err(bad());
            }
            return e.rm(0, &[0x0f, 0x16], 32, reg(a)?.number, b, None);
        }
        Movhps | Movlps | Movhpd | Movlpd => {
            let (r, m, store) = if xmm(a) && matches!(b, M(_)) {
                (a, b, false)
            } else if matches!(a, M(_)) && xmm(b) {
                (b, a, true)
            } else {
                return Err(bad());
            };
            if width(m) != 0 && width(m) != 64 {
                return Err(bad());
            }
            let byte = if matches!(op, Movhps | Movhpd) {
                0x16
            } else {
                0x12
            } + u8::from(store);
            return e.rm(
                if matches!(op, Movhpd | Movlpd) {
                    0x66
                } else {
                    0
                },
                &[0x0f, byte],
                32,
                reg(r)?.number,
                m,
                None,
            );
        }
        Cvtss2si | Cvtsd2si => {
            if !gpr(a) || !matches!(width(a), 32 | 64) {
                return Err(bad());
            }
            check_xmm(b, if op == Cvtss2si { 32 } else { 64 })?;
            return e.rm(
                if op == Cvtss2si { 0xf3 } else { 0xf2 },
                &[0x0f, 0x2d],
                width(a),
                reg(a)?.number,
                b,
                None,
            );
        }
        Cvtpi2ps | Cvtpi2pd => {
            if !xmm(a) {
                return Err(bad());
            }
            vector(b, true, 64)?;
            return e.rm(
                if op == Cvtpi2ps { 0 } else { 0x66 },
                &[0x0f, 0x2a],
                32,
                reg(a)?.number,
                b,
                None,
            );
        }
        Cvtps2pi | Cvttps2pi | Cvtpd2pi | Cvttpd2pi => {
            if !mmx(a) {
                return Err(bad());
            }
            let pd = matches!(op, Cvtpd2pi | Cvttpd2pi);
            check_xmm(b, if pd { 128 } else { 64 })?;
            return e.rm(
                if pd { 0x66 } else { 0 },
                &[
                    0x0f,
                    if matches!(op, Cvttps2pi | Cvttpd2pi) {
                        0x2c
                    } else {
                        0x2d
                    },
                ],
                32,
                reg(a)?.number,
                b,
                None,
            );
        }
        _ => {}
    }
    let (prefix, byte, bits, predicate) = match op {
        Comiss => (0, 0x2f, 32, None),
        Comisd => (0x66, 0x2f, 64, None),
        Rcpps => (0, 0x53, 128, None),
        Rcpss => (0xf3, 0x53, 32, None),
        Rsqrtps => (0, 0x52, 128, None),
        Rsqrtss => (0xf3, 0x52, 32, None),
        Cmpeqps => (0, 0xc2, 128, Some(0)),
        Cmpltps => (0, 0xc2, 128, Some(1)),
        Cmpleps => (0, 0xc2, 128, Some(2)),
        Cmpunordps => (0, 0xc2, 128, Some(3)),
        Cmpneqps => (0, 0xc2, 128, Some(4)),
        Cmpnltps => (0, 0xc2, 128, Some(5)),
        Cmpnleps => (0, 0xc2, 128, Some(6)),
        Cmpordps => (0, 0xc2, 128, Some(7)),
        Cmpeqpd => (0x66, 0xc2, 128, Some(0)),
        Cmpltpd => (0x66, 0xc2, 128, Some(1)),
        Cmplepd => (0x66, 0xc2, 128, Some(2)),
        Cmpunordpd => (0x66, 0xc2, 128, Some(3)),
        Cmpneqpd => (0x66, 0xc2, 128, Some(4)),
        Cmpnltpd => (0x66, 0xc2, 128, Some(5)),
        Cmpnlepd => (0x66, 0xc2, 128, Some(6)),
        Cmpordpd => (0x66, 0xc2, 128, Some(7)),
        Cmpeqss => (0xf3, 0xc2, 32, Some(0)),
        Cmpltss => (0xf3, 0xc2, 32, Some(1)),
        Cmpless => (0xf3, 0xc2, 32, Some(2)),
        Cmpunordss => (0xf3, 0xc2, 32, Some(3)),
        Cmpneqss => (0xf3, 0xc2, 32, Some(4)),
        Cmpnltss => (0xf3, 0xc2, 32, Some(5)),
        Cmpnless => (0xf3, 0xc2, 32, Some(6)),
        Cmpordss => (0xf3, 0xc2, 32, Some(7)),
        Cmpeqsd => (0xf2, 0xc2, 64, Some(0)),
        Cmpltsd => (0xf2, 0xc2, 64, Some(1)),
        Cmplesd => (0xf2, 0xc2, 64, Some(2)),
        Cmpunordsd => (0xf2, 0xc2, 64, Some(3)),
        Cmpneqsd => (0xf2, 0xc2, 64, Some(4)),
        Cmpnltsd => (0xf2, 0xc2, 64, Some(5)),
        Cmpnlesd => (0xf2, 0xc2, 64, Some(6)),
        Cmpordsd => (0xf2, 0xc2, 64, Some(7)),
        _ => return Err(bad()),
    };
    if !xmm(a) {
        return Err(bad());
    }
    check_xmm(b, bits)?;
    e.rm(prefix, &[0x0f, byte], 32, reg(a)?.number, b, None)?;
    if let Some(n) = predicate {
        e.number(n, 1);
    }
    Ok(())
}
