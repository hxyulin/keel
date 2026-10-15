mod vex;

use Operand::{Immediate as I, Memory as M, Register as R};
use keel_asm_ir::x86::{
    Address, Instruction, Memory, Modifier, Opcode::*, Operand, Register, RegisterClass,
};

#[derive(Debug)]
pub(super) struct Fixup {
    pub offset: usize,
    pub width: u8,
    pub address: Address,
    pub kind: u32,
}
#[derive(Default)]
pub(super) struct Encoding {
    pub bytes: Vec<u8>,
    pub fixups: Vec<Fixup>,
}
fn width(o: &Operand) -> u16 {
    match o {
        R(r) => r.width,
        M(m) => m.width,
        _ => 0,
    }
}
fn gpr(o: &Operand) -> bool {
    matches!(o, R(r) if r.class == RegisterClass::Gpr)
}
fn xmm(o: &Operand) -> bool {
    matches!(o, R(r) if r.class == RegisterClass::Xmm)
}
fn reg(o: &Operand) -> Result<Register, String> {
    if let R(r) = o {
        Ok(*r)
    } else {
        Err("expected register".into())
    }
}
fn immediate(o: &Operand) -> Result<&Address, String> {
    if let I(a) = o {
        Ok(a)
    } else {
        Err("expected immediate".into())
    }
}
fn constant(a: &Address) -> Option<i128> {
    if a.expression.add.is_none() && a.expression.subtract.is_none() && a.modifier == Modifier::None
    {
        Some(a.expression.constant)
    } else {
        None
    }
}
impl Encoding {
    fn number(&mut self, n: i128, bytes: u8) {
        self.bytes
            .extend_from_slice(&n.to_le_bytes()[..usize::from(bytes)]);
    }
    fn imm(&mut self, a: &Address, bytes: u8, signed: bool) -> Result<(), String> {
        if let Some(n) = constant(a) {
            let bits = 8 * u32::from(bytes);
            if n < -(1i128 << (bits - 1)) || n >= (1i128 << (bits - u32::from(signed))) {
                return Err(format!(
                    "immediate {n} does not fit {} bits{}",
                    bits,
                    if signed { " signed" } else { "" }
                ));
            }
            self.number(n, bytes);
        } else {
            let kind = match (a.modifier, bytes, signed) {
                (Modifier::None, 8, _) => 1,
                (Modifier::None, 4, false) => 10,
                (Modifier::None, 4, true) => 11,
                (Modifier::TpOff, 4, _) => 23,
                (Modifier::DtpOff, 8, _) => 17,
                _ => return Err("unsupported immediate relocation".into()),
            };
            self.fixups.push(Fixup {
                offset: self.bytes.len(),
                width: bytes,
                address: a.clone(),
                kind,
            });
            self.number(0, bytes);
        }
        Ok(())
    }
    fn branch(&mut self, a: &Address, plt: bool, width: u8) -> Result<(), String> {
        if !matches!(a.modifier, Modifier::None | Modifier::Plt) {
            return Err("invalid branch relocation modifier".into());
        }
        self.fixups.push(Fixup {
            offset: self.bytes.len(),
            width,
            address: a.clone(),
            kind: if width == 1 {
                15
            } else if plt || a.modifier == Modifier::Plt {
                4
            } else {
                2
            },
        });
        self.number(0, width);
        Ok(())
    }
    /// Emit legacy prefixes, REX, opcode and ModRM/SIB. `reg` is an opcode extension or register field.
    fn rm(
        &mut self,
        prefix: u8,
        opcode: &[u8],
        bits: u16,
        field: u8,
        rm: &Operand,
        field_byte: Option<Register>,
    ) -> Result<(), String> {
        let (b, x) = match rm {
            R(r) if r.class != RegisterClass::Rip => (r.number >> 3, 0),
            M(m) => (
                m.base
                    .filter(|r| r.class != RegisterClass::Rip)
                    .map_or(0, |r| r.number >> 3),
                m.index.map_or(0, |r| r.number >> 3),
            ),
            _ => return Err("expected register or memory".into()),
        };
        if let M(m) = rm
            && let Some(segment) = m.segment
        {
            self.bytes.push(segment)
        }
        if prefix != 0 {
            self.bytes.push(prefix)
        }
        if bits == 16 && prefix != 0x66 {
            self.bytes.push(0x66)
        }
        let rex = 0x40 | if bits == 64 { 8 } else { 0 } | ((field >> 3) << 2) | (x << 1) | b;
        let byte_regs = [field_byte, if let R(r) = rm { Some(*r) } else { None }];
        let force = byte_regs
            .into_iter()
            .flatten()
            .any(|r| r.width == 8 && r.number >= 4 && !r.high_byte);
        if rex != 0x40 || force {
            if byte_regs.into_iter().flatten().any(|r| r.high_byte) {
                return Err("high-byte register cannot be encoded with REX".into());
            }
            self.bytes.push(rex);
        }
        self.bytes.extend_from_slice(opcode);
        match rm {
            R(r) => self.bytes.push(0xc0 | ((field & 7) << 3) | (r.number & 7)),
            M(m) => self.memory(field, m)?,
            _ => unreachable!(),
        }
        Ok(())
    }
    fn memory(&mut self, field: u8, m: &Memory) -> Result<(), String> {
        let a = &m.displacement;
        let symbolic = a.expression.add.is_some()
            || a.expression.subtract.is_some()
            || a.modifier != Modifier::None;
        let d = a.expression.constant;
        if m.base.is_some_and(|r| r.class == RegisterClass::Rip) {
            if a.expression.subtract.is_some() {
                return Err("symbol differences in RIP-relative operands are unsupported".into());
            }
            self.bytes.push(((field & 7) << 3) | 5);
            if symbolic {
                let kind = match a.modifier {
                    Modifier::None => 2,
                    Modifier::GotPcRel => 9,
                    Modifier::GotTpOff => 22,
                    _ => return Err("invalid RIP-relative modifier".into()),
                };
                self.fixups.push(Fixup {
                    offset: self.bytes.len(),
                    width: 4,
                    address: a.clone(),
                    kind,
                });
                self.number(0, 4);
            } else {
                // A numeric RIP displacement is relative to the next instruction.
                self.imm(a, 4, true)?;
            }
            return Ok(());
        }
        let base = m.base.map(|r| r.number & 7);
        let mode = if base.is_none() || (!symbolic && d == 0 && base != Some(5)) {
            0
        } else if !symbolic && (-128..128).contains(&d) {
            1
        } else {
            2
        };
        let sib = m.index.is_some() || base == Some(4) || base.is_none();
        self.bytes
            .push((mode << 6) | ((field & 7) << 3) | if sib { 4 } else { base.unwrap() });
        if sib {
            self.bytes.push(
                (m.scale.trailing_zeros() as u8) << 6
                    | (m.index.map_or(4, |r| r.number & 7) << 3)
                    | base.unwrap_or(5),
            );
        }
        if mode == 1 {
            self.number(d, 1)
        } else if mode == 2 || base.is_none() {
            self.imm(a, 4, true)?
        }
        Ok(())
    }
}

pub(super) fn encode(i: &Instruction, short: bool) -> Result<Encoding, String> {
    validate(i)?;
    let mut e = Encoding::default();
    let o = &i.operands;
    let op = i.opcode;
    if i.lock {
        if !matches!(
            op,
            Add | Or
                | Adc
                | Sbb
                | And
                | Sub
                | Xor
                | Neg
                | Not
                | Inc
                | Dec
                | Xadd
                | Cmpxchg
                | Xchg
                | Bts
                | Btr
                | Btc
        ) || !matches!(o.first(), Some(M(_)))
        {
            return Err("lock requires a lockable memory destination".into());
        }
        e.bytes.push(0xf0);
    }
    let err = || format!("unsupported operands for {op:?}: {o:?}");
    match (op, o.as_slice()) {
        (Ret, []) => e.bytes.push(0xc3),
        (Ret, [I(a)]) => {
            e.bytes.push(0xc2);
            e.imm(a, 2, false)?;
        }
        (Leave, []) => e.bytes.push(0xc9),
        (Nop, []) => e.bytes.push(0x90),
        (Ud2, []) => e.bytes.extend([0x0f, 0x0b]),
        (Cqo, []) => e.bytes.extend([0x48, 0x99]),
        (Cdq, []) => e.bytes.push(0x99),
        (Cbw, []) => e.bytes.extend([0x66, 0x98]),
        (Cwde, []) => e.bytes.push(0x98),
        (Cdqe, []) => e.bytes.extend([0x48, 0x98]),
        (Mfence, []) => e.bytes.extend([0x0f, 0xae, 0xf0]),
        (Lfence, []) => e.bytes.extend([0x0f, 0xae, 0xe8]),
        (Sfence, []) => e.bytes.extend([0x0f, 0xae, 0xf8]),
        (Pause, []) => e.bytes.extend([0xf3, 0x90]),
        (Syscall, []) => e.bytes.extend([0x0f, 0x05]),
        (Int3, []) => e.bytes.push(0xcc),
        (Push | Pop, [R(r)]) if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 64) => {
            if r.width == 16 {
                e.bytes.push(0x66)
            }
            if r.number >= 8 {
                e.bytes.push(0x41)
            }
            e.bytes
                .push((if op == Push { 0x50 } else { 0x58 }) + (r.number & 7));
        }
        (Push, [I(a)]) => {
            let small = constant(a).is_some_and(|n| (-128..128).contains(&n));
            e.bytes.push(if small { 0x6a } else { 0x68 });
            e.imm(a, if small { 1 } else { 4 }, true)?;
        }
        (Push | Pop, [m @ M(_)]) if matches!(width(m), 0 | 64 | 16) => e.rm(
            0,
            &[if op == Push { 0xff } else { 0x8f }],
            if width(m) == 16 { 16 } else { 32 },
            if op == Push { 6 } else { 0 },
            m,
            None,
        )?,
        (Call | Jmp, [I(a)]) => {
            let short = short && op == Jmp;
            e.bytes.push(if op == Call {
                0xe8
            } else if short {
                0xeb
            } else {
                0xe9
            });
            e.branch(a, true, if short { 1 } else { 4 })?;
        }
        (Jcc(cc), [I(a)]) if cc < 16 => {
            if short {
                e.bytes.push(0x70 + cc);
            } else {
                e.bytes.extend([0x0f, 0x80 + cc]);
            }
            e.branch(a, false, if short { 1 } else { 4 })?;
        }
        (Call | Jmp, [a])
            if (gpr(a) && width(a) == 64) || matches!(a,M(m) if matches!(m.width,0|64)) =>
        {
            e.rm(0, &[0xff], 32, if op == Call { 2 } else { 4 }, a, None)?
        }
        (Setcc(cc), [a]) if cc < 16 && (width(a) == 8 || matches!(a,M(m) if m.width==0)) => {
            e.rm(0, &[0x0f, 0x90 + cc], 8, 0, a, None)?
        }
        (Cmovcc(cc), [a, b]) if cc < 16 && gpr(a) && matches!(width(a), 16 | 32 | 64) => {
            check_width(b, width(a))?;
            let r = reg(a)?;
            e.rm(0, &[0x0f, 0x40 + cc], r.width, r.number, b, None)?;
        }
        (Lea, [R(r), b @ M(_)])
            if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) =>
        {
            e.rm(0, &[0x8d], r.width, r.number, b, None)?
        }
        (Mov | Movabs, [R(r), I(a)]) if r.class == RegisterClass::Gpr => {
            if op == Movabs && r.width != 64 {
                return Err(err());
            }
            if r.width == 64
                && op != Movabs
                && constant(a).is_some_and(|n| i32::try_from(n).is_ok())
            {
                e.rm(0, &[0xc7], 64, 0, &o[0], None)?;
                e.imm(a, 4, true)?;
            } else {
                if r.width == 16 {
                    e.bytes.push(0x66)
                }
                let rex = 0x40 | if r.width == 64 { 8 } else { 0 } | (r.number >> 3);
                if rex != 0x40 || (r.width == 8 && r.number >= 4 && !r.high_byte) {
                    if r.high_byte {
                        return Err(err());
                    }
                    e.bytes.push(rex)
                }
                e.bytes
                    .push((if r.width == 8 { 0xb0 } else { 0xb8 }) + (r.number & 7));
                e.imm(a, (r.width / 8) as u8, false)?;
            }
        }
        (Mov, [a, I(b)]) if matches!(a,M(m) if m.width!=0 && m.width<=64) => {
            let w = width(a);
            e.rm(0, &[if w == 8 { 0xc6 } else { 0xc7 }], w, 0, a, None)?;
            e.imm(b, if w == 64 { 4 } else { (w / 8) as u8 }, w == 64)?;
        }
        (Mov, [a, b]) if gpr(a) || gpr(b) => {
            let (r, m, load) = if gpr(b) {
                (reg(b)?, a, false)
            } else {
                (reg(a)?, b, true)
            };
            check_width(m, r.width)?;
            e.rm(
                0,
                &[if r.width == 8 { 0x88 } else { 0x89 } + if load { 2 } else { 0 }],
                r.width,
                r.number,
                m,
                Some(r),
            )?;
        }
        (Movsx | Movzx | Movsxd, [R(r), b]) if r.class == RegisterClass::Gpr => {
            let w = width(b);
            if !matches!(w, 8 | 16 | 32)
                || r.width <= w
                || (w == 32 && op != Movsxd)
                || (op == Movsxd && (w != 32 || r.width != 64))
            {
                return Err(err());
            }
            if op == Movsxd {
                e.rm(0, &[0x63], r.width, r.number, b, None)?
            } else {
                e.rm(
                    0,
                    &[
                        0x0f,
                        if op == Movsx { 0xbe } else { 0xb6 } + if w == 16 { 1 } else { 0 },
                    ],
                    r.width,
                    r.number,
                    b,
                    None,
                )?
            }
        }
        (Add | Or | Adc | Sbb | And | Sub | Xor | Cmp | Test, [a, b]) => {
            let w = infer_width(a, b)?;
            let group = match op {
                Add => 0,
                Or => 1,
                Adc => 2,
                Sbb => 3,
                And => 4,
                Sub => 5,
                Xor => 6,
                Cmp => 7,
                _ => 0,
            };
            if let I(imm) = b {
                let small =
                    op != Test && w != 8 && constant(imm).is_some_and(|n| (-128..128).contains(&n));
                if !small
                    && matches!(a, R(r) if r.class == RegisterClass::Gpr && r.number == 0 && !r.high_byte)
                {
                    // AL/AX/EAX/RAX have an immediate form without ModRM.
                    // Keep sign-extended imm8 group forms when those are shorter.
                    if w == 16 {
                        e.bytes.push(0x66);
                    }
                    if w == 64 {
                        e.bytes.push(0x48);
                    }
                    e.bytes.push(if op == Test {
                        if w == 8 { 0xa8 } else { 0xa9 }
                    } else {
                        group * 8 + 4 + u8::from(w != 8)
                    });
                } else {
                    e.rm(
                        0,
                        &[if op == Test {
                            if w == 8 { 0xf6 } else { 0xf7 }
                        } else if w == 8 {
                            0x80
                        } else if small {
                            0x83
                        } else {
                            0x81
                        }],
                        w,
                        group,
                        a,
                        None,
                    )?;
                }
                e.imm(
                    imm,
                    if small || w == 8 {
                        1
                    } else if w == 16 {
                        2
                    } else {
                        4
                    },
                    small || w == 64,
                )?;
            } else {
                let (r, m, load) = if gpr(b) {
                    (reg(b)?, a, false)
                } else {
                    (reg(a)?, b, true)
                };
                if op == Test && load {
                    return Err(err());
                }
                let byte = if op == Test { 0x84 } else { group * 8 }
                    + u8::from(w != 8)
                    + if load { 2 } else { 0 };
                e.rm(0, &[byte], w, r.number, m, Some(r))?;
            }
        }
        (Neg | Not | Mul | Imul | Div | Idiv | Inc | Dec, [a]) if gpr(a) || matches!(a, M(_)) => {
            let w = width(a);
            if !matches!(w, 8 | 16 | 32 | 64) {
                return Err(err());
            }
            let group = match op {
                Not => 2,
                Neg => 3,
                Mul => 4,
                Imul => 5,
                Div => 6,
                Idiv => 7,
                Inc => 0,
                _ => 1,
            };
            e.rm(
                0,
                &[if matches!(op, Inc | Dec) {
                    if w == 8 { 0xfe } else { 0xff }
                } else if w == 8 {
                    0xf6
                } else {
                    0xf7
                }],
                w,
                group,
                a,
                None,
            )?;
        }
        (Imul, [R(r), b]) if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) => {
            if let I(a) = b {
                let small = constant(a).is_some_and(|n| (-128..128).contains(&n));
                e.rm(
                    0,
                    &[if small { 0x6b } else { 0x69 }],
                    r.width,
                    r.number,
                    &o[0],
                    None,
                )?;
                e.imm(
                    a,
                    if small {
                        1
                    } else if r.width == 16 {
                        2
                    } else {
                        4
                    },
                    small || r.width == 64,
                )?;
            } else {
                check_width(b, r.width)?;
                e.rm(0, &[0x0f, 0xaf], r.width, r.number, b, None)?;
            }
        }
        (Imul, [R(r), b, I(a)])
            if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) =>
        {
            check_width(b, r.width)?;
            let small = constant(a).is_some_and(|n| (-128..128).contains(&n));
            e.rm(
                0,
                &[if small { 0x6b } else { 0x69 }],
                r.width,
                r.number,
                b,
                None,
            )?;
            e.imm(
                a,
                if small {
                    1
                } else if r.width == 16 {
                    2
                } else {
                    4
                },
                small || r.width == 64,
            )?;
        }
        (Shl | Shr | Sar | Rol | Ror, [a, b]) => {
            let w = width(a);
            if !matches!(w, 8 | 16 | 32 | 64) {
                return Err(err());
            }
            let group = match op {
                Rol => 0,
                Ror => 1,
                Shl => 4,
                Shr => 5,
                _ => 7,
            };
            let by_cl =
                matches!(b,R(r) if r.class==RegisterClass::Gpr && r.width==8 && r.number==1);
            let one = matches!(b,I(a) if constant(a)==Some(1));
            e.rm(
                0,
                &[if by_cl {
                    0xd2
                } else if one {
                    0xd0
                } else {
                    0xc0
                } + u8::from(w != 8)],
                w,
                group,
                a,
                None,
            )?;
            if !by_cl && !one {
                e.imm(immediate(b)?, 1, false)?;
            }
        }
        (Shld | Shrd, [a, R(r), b])
            if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) =>
        {
            check_width(a, r.width)?;
            let cl = matches!(b,R(r) if r.class==RegisterClass::Gpr && r.width==8 && r.number==1);
            e.rm(
                0,
                &[0x0f, if op == Shld { 0xa4 } else { 0xac } + u8::from(cl)],
                r.width,
                r.number,
                a,
                None,
            )?;
            if !cl {
                e.imm(immediate(b)?, 1, false)?;
            }
        }
        (Bsf | Bsr | Popcnt, [R(r), b])
            if r.class == RegisterClass::Gpr && matches!(r.width, 16 | 32 | 64) =>
        {
            check_width(b, r.width)?;
            e.rm(
                if op == Popcnt { 0xf3 } else { 0 },
                &[
                    0x0f,
                    match op {
                        Bsf => 0xbc,
                        Bsr => 0xbd,
                        _ => 0xb8,
                    },
                ],
                r.width,
                r.number,
                b,
                None,
            )?;
        }
        (Bswap, [R(r)]) if r.class == RegisterClass::Gpr && matches!(r.width, 32 | 64) => {
            if r.width == 64 || r.number >= 8 {
                e.bytes
                    .push(0x40 | if r.width == 64 { 8 } else { 0 } | (r.number >> 3))
            }
            e.bytes.extend([0x0f, 0xc8 + (r.number & 7)]);
        }
        (Bt | Bts | Btr | Btc, [a, b]) => {
            let w = infer_width(a, b)?;
            if !matches!(w, 16 | 32 | 64) {
                return Err(err());
            }
            let n = match op {
                Bt => 4,
                Bts => 5,
                Btr => 6,
                _ => 7,
            };
            if let I(imm) = b {
                e.rm(0, &[0x0f, 0xba], w, n, a, None)?;
                e.imm(imm, 1, false)?;
            } else {
                let r = reg(b)?;
                e.rm(0, &[0x0f, 0xa3 + (n - 4) * 8], w, r.number, a, None)?;
            }
        }
        (Xchg | Xadd | Cmpxchg, [a, b]) => {
            let w = infer_width(a, b)?;
            let r = reg(b)?;
            if r.class != RegisterClass::Gpr {
                return Err(err());
            }
            let bytes = if op == Xchg {
                vec![if w == 8 { 0x86 } else { 0x87 }]
            } else {
                vec![
                    0x0f,
                    if op == Xadd { 0xc0 } else { 0xb0 } + u8::from(w != 8),
                ]
            };
            e.rm(0, &bytes, w, r.number, a, Some(r))?;
        }
        _ => {
            if op.required_features().avx {
                vex::encode(&mut e, i)?;
            } else {
                encode_sse(&mut e, i).map_err(|_| err())?;
            }
        }
    }
    // PC-relative relocation addends are measured from the relocated field,
    // but the CPU uses the end of the entire instruction (including immediates).
    let len = e.bytes.len();
    for fix in &mut e.fixups {
        if matches!(fix.kind, 2 | 4 | 9 | 15 | 22) {
            fix.address.expression.constant -= (len - fix.offset) as i128;
        }
    }
    Ok(e)
}
fn check_width(o: &Operand, w: u16) -> Result<(), String> {
    if matches!(o,R(r) if r.class==RegisterClass::Gpr && r.width==w)
        || matches!(o,M(m) if m.width==0||m.width==w)
    {
        Ok(())
    } else {
        Err("operand width/class mismatch".into())
    }
}
fn infer_width(a: &Operand, b: &Operand) -> Result<u16, String> {
    let w = if width(a) != 0 { width(a) } else { width(b) };
    if !matches!(w, 8 | 16 | 32 | 64) {
        return Err("memory operand needs an explicit width".into());
    }
    check_width(a, w)?;
    if !matches!(b, I(_)) {
        check_width(b, w)?
    }
    Ok(w)
}

fn encode_sse(e: &mut Encoding, i: &Instruction) -> Result<(), String> {
    let o = &i.operands;
    let op = i.opcode;
    let err = || "unsupported SSE operands".to_string();
    let [a, b, ..] = o.as_slice() else {
        return Err(err());
    };
    let (prefix, byte, mem_width, store) = match op {
        Movss => (0xf3, 0x10, 32, true),
        Movsd => (0xf2, 0x10, 64, true),
        Movaps => (0, 0x28, 128, true),
        Movups => (0, 0x10, 128, true),
        Movapd => (0x66, 0x28, 128, true),
        Movupd => (0x66, 0x10, 128, true),
        Addss => (0xf3, 0x58, 32, false),
        Addsd => (0xf2, 0x58, 64, false),
        Subss => (0xf3, 0x5c, 32, false),
        Subsd => (0xf2, 0x5c, 64, false),
        Mulss => (0xf3, 0x59, 32, false),
        Mulsd => (0xf2, 0x59, 64, false),
        Divss => (0xf3, 0x5e, 32, false),
        Divsd => (0xf2, 0x5e, 64, false),
        Sqrtss => (0xf3, 0x51, 32, false),
        Sqrtsd => (0xf2, 0x51, 64, false),
        Minss => (0xf3, 0x5d, 32, false),
        Minsd => (0xf2, 0x5d, 64, false),
        Maxss => (0xf3, 0x5f, 32, false),
        Maxsd => (0xf2, 0x5f, 64, false),
        Ucomiss => (0, 0x2e, 32, false),
        Ucomisd => (0x66, 0x2e, 64, false),
        Andps => (0, 0x54, 128, false),
        Andpd => (0x66, 0x54, 128, false),
        Orps => (0, 0x56, 128, false),
        Orpd => (0x66, 0x56, 128, false),
        Xorps => (0, 0x57, 128, false),
        Xorpd => (0x66, 0x57, 128, false),
        Cvtss2sd => (0xf3, 0x5a, 32, false),
        Cvtsd2ss => (0xf2, 0x5a, 64, false),
        Roundss | Roundsd => {
            if o.len() != 3 || !xmm(a) {
                return Err(err());
            }
            check_xmm(b, if op == Roundss { 32 } else { 64 })?;
            e.rm(
                0x66,
                &[0x0f, 0x3a, if op == Roundss { 0x0a } else { 0x0b }],
                32,
                reg(a)?.number,
                b,
                None,
            )?;
            e.imm(immediate(&o[2])?, 1, false)?;
            return Ok(());
        }
        Movd | Movq => {
            if o.len() != 2 {
                return Err(err());
            }
            let w = if op == Movq { 64 } else { 32 };
            if xmm(a) && xmm(b) && op == Movq {
                e.rm(0xf3, &[0x0f, 0x7e], 32, reg(a)?.number, b, None)?;
                return Ok(());
            }
            let (r, m, load) = if xmm(a) {
                (reg(a)?, b, true)
            } else if xmm(b) {
                (reg(b)?, a, false)
            } else {
                return Err(err());
            };
            check_width(m, w)?;
            e.rm(
                0x66,
                &[0x0f, if load { 0x6e } else { 0x7e }],
                w,
                r.number,
                m,
                None,
            )?;
            return Ok(());
        }
        Cvtsi2ss | Cvtsi2sd => {
            if o.len() != 2 || !xmm(a) || !matches!(width(b), 32 | 64) {
                return Err(err());
            }
            check_width(b, width(b))?;
            e.rm(
                if op == Cvtsi2ss { 0xf3 } else { 0xf2 },
                &[0x0f, 0x2a],
                width(b),
                reg(a)?.number,
                b,
                None,
            )?;
            return Ok(());
        }
        Cvttss2si | Cvttsd2si => {
            if o.len() != 2 || !gpr(a) || !matches!(width(a), 32 | 64) {
                return Err(err());
            }
            check_xmm(b, if op == Cvttss2si { 32 } else { 64 })?;
            e.rm(
                if op == Cvttss2si { 0xf3 } else { 0xf2 },
                &[0x0f, 0x2c],
                width(a),
                reg(a)?.number,
                b,
                None,
            )?;
            return Ok(());
        }
        _ => return Err(err()),
    };
    if o.len() != 2 {
        return Err(err());
    }
    let (r, m, load) = if xmm(a) {
        (reg(a)?, b, true)
    } else if store && xmm(b) && matches!(a, M(_)) {
        (reg(b)?, a, false)
    } else {
        return Err(err());
    };
    check_xmm(m, mem_width)?;
    e.rm(
        prefix,
        &[0x0f, byte + u8::from(!load)],
        32,
        r.number,
        m,
        None,
    )?;
    Ok(())
}
fn check_xmm(o: &Operand, w: u16) -> Result<(), String> {
    if xmm(o) || matches!(o,M(m) if m.width==0||m.width==w) {
        Ok(())
    } else {
        Err("expected XMM register or matching memory width".into())
    }
}

fn validate(i: &Instruction) -> Result<(), String> {
    fn register(r: Register) -> bool {
        r.number < 16
            && match r.class {
                RegisterClass::Gpr => {
                    matches!(r.width, 8 | 16 | 32 | 64)
                        && (!r.high_byte || r.width == 8 && (4..8).contains(&r.number))
                }
                RegisterClass::Xmm => r.width == 128 && !r.high_byte,
                RegisterClass::Rip => r.number == 0 && r.width == 64 && !r.high_byte,
            }
    }
    for o in &i.operands {
        match o {
            R(r) if !register(*r) || r.class == RegisterClass::Rip => {
                return Err("invalid operand register".into());
            }
            M(m) => {
                if !matches!(m.width, 0 | 8 | 16 | 32 | 64 | 128)
                    || !matches!(m.scale, 1 | 2 | 4 | 8)
                    || m.segment.is_some_and(|s| !matches!(s, 0x64 | 0x65))
                {
                    return Err("invalid memory width/scale/segment".into());
                }
                for r in [m.base, m.index].into_iter().flatten() {
                    if !register(r) || r.width != 64 || r.class == RegisterClass::Xmm {
                        return Err("invalid address register".into());
                    }
                }
                if m.index
                    .is_some_and(|r| r.class == RegisterClass::Rip || r.number == 4)
                    || (m.base.is_some_and(|r| r.class == RegisterClass::Rip) && m.index.is_some())
                {
                    return Err("invalid address index".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
