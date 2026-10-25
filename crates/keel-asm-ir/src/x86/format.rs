//! Intel syntax serialization for the shared x86 instruction API.
//!
//! Every piece is written straight to the sink: a `String` sink compiles to
//! plain appends, without the general formatting machinery per operand.
use super::*;
use crate::directive::write_int;
use std::fmt::{self, Write};
const CC: [&str; 16] = [
    "o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g",
];
impl Opcode {
    /// The mnemonic, split into a fixed part and a condition-code suffix.
    fn mnemonic(self) -> (&'static str, Option<u8>) {
        let name = match self {
            Opcode::Core(op) => op.mnemonic(),
            Opcode::X87(op) => op.mnemonic(),
            Opcode::Fixed(op) => op.mnemonic(),
            Opcode::Extension(op) => op.mnemonic(),
            Opcode::Pclmulqdq => "pclmulqdq",
            Opcode::V2(op) => op.spec().mnemonic,
            Opcode::Movq6e => "movq.6e",
            Opcode::Movq7e => "movq.7e",
            Opcode::MovqD6 => "movq.d6",
            Opcode::Movq7f => "movq.7f",
            Opcode::MovssStore => "movss.store",
            Opcode::MovsdStore => "movsd.store",
            Opcode::MovapsStore => "movaps.store",
            Opcode::MovupsStore => "movups.store",
            Opcode::MovapdStore => "movapd.store",
            Opcode::MovupdStore => "movupd.store",
            Opcode::MovdqaStore => "movdqa.store",
            Opcode::MovdquStore => "movdqu.store",

            Opcode::Comiss => "comiss",
            Opcode::Comisd => "comisd",
            Opcode::Cvtss2si => "cvtss2si",
            Opcode::Cvtsd2si => "cvtsd2si",
            Opcode::Cvtpi2ps => "cvtpi2ps",
            Opcode::Cvtpi2pd => "cvtpi2pd",
            Opcode::Cvtps2pi => "cvtps2pi",
            Opcode::Cvtpd2pi => "cvtpd2pi",
            Opcode::Cvttps2pi => "cvttps2pi",
            Opcode::Cvttpd2pi => "cvttpd2pi",
            Opcode::Rcpps => "rcpps",
            Opcode::Rcpss => "rcpss",
            Opcode::Rsqrtps => "rsqrtps",
            Opcode::Rsqrtss => "rsqrtss",
            Opcode::Movhps => "movhps",
            Opcode::Movlps => "movlps",
            Opcode::Movhpd => "movhpd",
            Opcode::Movlpd => "movlpd",
            Opcode::Movlhps => "movlhps",
            Opcode::Movdq2q => "movdq2q",
            Opcode::Movq2dq => "movq2dq",
            Opcode::Movntq => "movntq",
            Opcode::Movntdq => "movntdq",
            Opcode::Movntps => "movntps",
            Opcode::Movntpd => "movntpd",
            Opcode::Movnti => "movnti",
            Opcode::Maskmovq => "maskmovq",
            Opcode::Maskmovdqu => "maskmovdqu",
            Opcode::Prefetchnta => "prefetchnta",
            Opcode::Prefetcht0 => "prefetcht0",
            Opcode::Prefetcht1 => "prefetcht1",
            Opcode::Prefetcht2 => "prefetcht2",
            Opcode::Fxsave => "fxsave",
            Opcode::Fxsave64 => "fxsave64",
            Opcode::Fxrstor => "fxrstor",
            Opcode::Fxrstor64 => "fxrstor64",
            Opcode::Cmpeqps => "cmpeqps",
            Opcode::Cmpltps => "cmpltps",
            Opcode::Cmpleps => "cmpleps",
            Opcode::Cmpunordps => "cmpunordps",
            Opcode::Cmpneqps => "cmpneqps",
            Opcode::Cmpnleps => "cmpnleps",
            Opcode::Cmpordps => "cmpordps",
            Opcode::Cmpeqpd => "cmpeqpd",
            Opcode::Cmpltpd => "cmpltpd",
            Opcode::Cmplepd => "cmplepd",
            Opcode::Cmpunordpd => "cmpunordpd",
            Opcode::Cmpneqpd => "cmpneqpd",
            Opcode::Cmpnltpd => "cmpnltpd",
            Opcode::Cmpnlepd => "cmpnlepd",
            Opcode::Cmpordpd => "cmpordpd",
            Opcode::Cmpeqss => "cmpeqss",
            Opcode::Cmpless => "cmpless",
            Opcode::Cmpunordss => "cmpunordss",
            Opcode::Cmpneqss => "cmpneqss",
            Opcode::Cmpnltss => "cmpnltss",
            Opcode::Cmpnless => "cmpnless",
            Opcode::Cmpordss => "cmpordss",
            Opcode::Cmpunordsd => "cmpunordsd",
            Opcode::Cmpnltsd => "cmpnltsd",
            Opcode::Cmpnlesd => "cmpnlesd",

            Opcode::Packssdw => "packssdw",
            Opcode::Packsswb => "packsswb",
            Opcode::Packuswb => "packuswb",
            Opcode::Paddsb => "paddsb",
            Opcode::Paddsw => "paddsw",
            Opcode::Paddusb => "paddusb",
            Opcode::Paddusw => "paddusw",
            Opcode::Psubsb => "psubsb",
            Opcode::Psubsw => "psubsw",
            Opcode::Psubusb => "psubusb",
            Opcode::Psubusw => "psubusw",
            Opcode::Pavgb => "pavgb",
            Opcode::Pavgw => "pavgw",
            Opcode::Pmaxsw => "pmaxsw",
            Opcode::Pmaxub => "pmaxub",
            Opcode::Pminsw => "pminsw",
            Opcode::Pminub => "pminub",
            Opcode::Psadbw => "psadbw",
            Opcode::Pshufw => "pshufw",
            Opcode::Pinsrw => "pinsrw",
            Opcode::Pextrw => "pextrw",

            Opcode::V3(op) => op.mnemonic(),
            Opcode::Mov => "mov",
            Opcode::Movabs => "movabs",
            Opcode::Lea => "lea",
            Opcode::Movsx => "movsx",
            Opcode::Movzx => "movzx",
            Opcode::Movsxd => "movsxd",
            Opcode::Push => "push",
            Opcode::Pop => "pop",
            Opcode::Ret => "ret",
            Opcode::Leave => "leave",
            Opcode::Nop => "nop",
            Opcode::Ud2 => "ud2",
            Opcode::Add => "add",
            Opcode::Or => "or",
            Opcode::Adc => "adc",
            Opcode::Sbb => "sbb",
            Opcode::And => "and",
            Opcode::Sub => "sub",
            Opcode::Xor => "xor",
            Opcode::Cmp => "cmp",
            Opcode::Test => "test",
            Opcode::Neg => "neg",
            Opcode::Not => "not",
            Opcode::Inc => "inc",
            Opcode::Dec => "dec",
            Opcode::Mul => "mul",
            Opcode::Imul => "imul",
            Opcode::Div => "div",
            Opcode::Idiv => "idiv",
            Opcode::Cqo => "cqo",
            Opcode::Cdq => "cdq",
            Opcode::Cbw => "cbw",
            Opcode::Cwde => "cwde",
            Opcode::Cdqe => "cdqe",
            Opcode::Shl => "shl",
            Opcode::Shr => "shr",
            Opcode::Sar => "sar",
            Opcode::Rol => "rol",
            Opcode::Ror => "ror",
            Opcode::Shld => "shld",
            Opcode::Shrd => "shrd",
            Opcode::Bsf => "bsf",
            Opcode::Bsr => "bsr",
            Opcode::Bswap => "bswap",
            Opcode::Popcnt => "popcnt",
            Opcode::Andn => "andn",
            Opcode::Bextr => "bextr",
            Opcode::Blsi => "blsi",
            Opcode::Blsmsk => "blsmsk",
            Opcode::Blsr => "blsr",
            Opcode::Tzcnt => "tzcnt",
            Opcode::Shlx => "shlx",
            Opcode::Shrx => "shrx",
            Opcode::Sarx => "sarx",
            Opcode::Rorx => "rorx",
            Opcode::Bzhi => "bzhi",
            Opcode::Pext => "pext",
            Opcode::Pdep => "pdep",
            Opcode::Mulx => "mulx",
            Opcode::Bt => "bt",
            Opcode::Bts => "bts",
            Opcode::Btr => "btr",
            Opcode::Btc => "btc",
            Opcode::Xchg => "xchg",
            Opcode::Xadd => "xadd",
            Opcode::Cmpxchg => "cmpxchg",
            Opcode::Mfence => "mfence",
            Opcode::Lfence => "lfence",
            Opcode::Sfence => "sfence",
            Opcode::Pause => "pause",
            Opcode::Syscall => "syscall",
            Opcode::Int3 => "int3",
            Opcode::Call => "call",
            Opcode::Jmp => "jmp",
            Opcode::Vaddps => "vaddps",
            Opcode::Vaddpd => "vaddpd",
            Opcode::Vsubps => "vsubps",
            Opcode::Vsubpd => "vsubpd",
            Opcode::Vmulps => "vmulps",
            Opcode::Vmulpd => "vmulpd",
            Opcode::Vdivps => "vdivps",
            Opcode::Vdivpd => "vdivpd",
            Opcode::Vaddss => "vaddss",
            Opcode::Vaddsd => "vaddsd",
            Opcode::Vsubss => "vsubss",
            Opcode::Vsubsd => "vsubsd",
            Opcode::Vmulss => "vmulss",
            Opcode::Vmulsd => "vmulsd",
            Opcode::Vdivss => "vdivss",
            Opcode::Vdivsd => "vdivsd",
            Opcode::Vsqrtss => "vsqrtss",
            Opcode::Vsqrtsd => "vsqrtsd",
            Opcode::Vminss => "vminss",
            Opcode::Vminsd => "vminsd",
            Opcode::Vmaxss => "vmaxss",
            Opcode::Vmaxsd => "vmaxsd",
            Opcode::Vucomiss => "vucomiss",
            Opcode::Vucomisd => "vucomisd",
            Opcode::Vmovss => "vmovss",
            Opcode::Vmovsd => "vmovsd",
            Opcode::Vmovaps => "vmovaps",
            Opcode::Vmovups => "vmovups",
            Opcode::Vmovapd => "vmovapd",
            Opcode::Vmovupd => "vmovupd",
            Opcode::Vandps => "vandps",
            Opcode::Vandpd => "vandpd",
            Opcode::Vorps => "vorps",
            Opcode::Vorpd => "vorpd",
            Opcode::Vxorps => "vxorps",
            Opcode::Vxorpd => "vxorpd",
            Opcode::Vroundss => "vroundss",
            Opcode::Vroundsd => "vroundsd",
            Opcode::Vzeroupper => "vzeroupper",
            Opcode::Movss => "movss",
            Opcode::Movsd => "movsd",
            Opcode::Movaps => "movaps",
            Opcode::Movhlps => "movhlps",
            Opcode::Movups => "movups",
            Opcode::Paddb => "paddb",
            Opcode::Paddw => "paddw",
            Opcode::Paddd => "paddd",
            Opcode::Paddq => "paddq",
            Opcode::Psubb => "psubb",
            Opcode::Psubw => "psubw",
            Opcode::Psubd => "psubd",
            Opcode::Psubq => "psubq",
            Opcode::Pmullw => "pmullw",
            Opcode::Addps => "addps",
            Opcode::Addpd => "addpd",
            Opcode::Subps => "subps",
            Opcode::Subpd => "subpd",
            Opcode::Mulps => "mulps",
            Opcode::Mulpd => "mulpd",
            Opcode::Divps => "divps",
            Opcode::Divpd => "divpd",
            Opcode::Sqrtps => "sqrtps",
            Opcode::Sqrtpd => "sqrtpd",
            Opcode::Movapd => "movapd",
            Opcode::Movupd => "movupd",
            Opcode::Movd => "movd",
            Opcode::Movq => "movq",
            Opcode::Addss => "addss",
            Opcode::Addsd => "addsd",
            Opcode::Subss => "subss",
            Opcode::Subsd => "subsd",
            Opcode::Mulss => "mulss",
            Opcode::Mulsd => "mulsd",
            Opcode::Divss => "divss",
            Opcode::Divsd => "divsd",
            Opcode::Sqrtss => "sqrtss",
            Opcode::Sqrtsd => "sqrtsd",
            Opcode::Minss => "minss",
            Opcode::Minsd => "minsd",
            Opcode::Maxss => "maxss",
            Opcode::Maxsd => "maxsd",
            Opcode::Ucomiss => "ucomiss",
            Opcode::Ucomisd => "ucomisd",
            Opcode::Andps => "andps",
            Opcode::Andpd => "andpd",
            Opcode::Orps => "orps",
            Opcode::Orpd => "orpd",
            Opcode::Xorps => "xorps",
            Opcode::Xorpd => "xorpd",
            Opcode::Roundss => "roundss",
            Opcode::Roundsd => "roundsd",
            Opcode::Cvtsi2ss => "cvtsi2ss",
            Opcode::Cvtsi2sd => "cvtsi2sd",
            Opcode::Cvttss2si => "cvttss2si",
            Opcode::Cvttsd2si => "cvttsd2si",
            Opcode::Cvtss2sd => "cvtss2sd",
            Opcode::Cvtsd2ss => "cvtsd2ss",
            Opcode::Movdqa => "movdqa",
            Opcode::Movdqu => "movdqu",
            Opcode::Pxor => "pxor",
            Opcode::Pand => "pand",
            Opcode::Pandn => "pandn",
            Opcode::Por => "por",
            Opcode::Pcmpeqb => "pcmpeqb",
            Opcode::Pcmpeqw => "pcmpeqw",
            Opcode::Pcmpeqd => "pcmpeqd",
            Opcode::Pcmpgtb => "pcmpgtb",
            Opcode::Pcmpgtw => "pcmpgtw",
            Opcode::Pcmpgtd => "pcmpgtd",
            Opcode::Pshufd => "pshufd",
            Opcode::Pshuflw => "pshuflw",
            Opcode::Pshufhw => "pshufhw",
            Opcode::Punpcklbw => "punpcklbw",
            Opcode::Punpcklwd => "punpcklwd",
            Opcode::Punpckldq => "punpckldq",
            Opcode::Punpcklqdq => "punpcklqdq",
            Opcode::Punpckhbw => "punpckhbw",
            Opcode::Punpckhwd => "punpckhwd",
            Opcode::Punpckhdq => "punpckhdq",
            Opcode::Punpckhqdq => "punpckhqdq",
            Opcode::Unpcklps => "unpcklps",
            Opcode::Unpckhps => "unpckhps",
            Opcode::Unpcklpd => "unpcklpd",
            Opcode::Unpckhpd => "unpckhpd",
            Opcode::Shufps => "shufps",
            Opcode::Shufpd => "shufpd",
            Opcode::Andnps => "andnps",
            Opcode::Andnpd => "andnpd",
            Opcode::Minps => "minps",
            Opcode::Minpd => "minpd",
            Opcode::Maxps => "maxps",
            Opcode::Maxpd => "maxpd",
            Opcode::Cmpps => "cmpps",
            Opcode::Cmppd => "cmppd",
            Opcode::Cmpss => "cmpss",
            Opcode::Cmpsd => "cmpsd",
            Opcode::Cmpltss => "cmpltss",
            Opcode::Cmpltsd => "cmpltsd",
            Opcode::Cmpeqsd => "cmpeqsd",
            Opcode::Cmplesd => "cmplesd",
            Opcode::Cmpneqsd => "cmpneqsd",
            Opcode::Cmpordsd => "cmpordsd",
            Opcode::Fld => "fld",
            Opcode::Fstp => "fstp",
            Opcode::Fxch => "fxch",
            Opcode::Fucomi => "fucomi",
            Opcode::Fucomip => "fucomip",
            Opcode::Fadd => "fadd",
            Opcode::Fmul => "fmul",
            Opcode::Fmulp => "fmulp",
            Opcode::Fsubp => "fsubp",
            Opcode::Fsubrp => "fsubrp",
            Opcode::Fcmovnb => "fcmovnb",
            Opcode::Fild => "fild",
            Opcode::Fistp => "fistp",
            Opcode::Fldcw => "fldcw",
            Opcode::Fnstcw => "fnstcw",
            Opcode::Fldz => "fldz",
            Opcode::Ldmxcsr => "ldmxcsr",
            Opcode::Stmxcsr => "stmxcsr",
            Opcode::Cmpnltps => "cmpnltps",
            Opcode::Pmovmskb => "pmovmskb",
            Opcode::Movmskps => "movmskps",
            Opcode::Movmskpd => "movmskpd",
            Opcode::Pmaddwd => "pmaddwd",
            Opcode::Pmulhuw => "pmulhuw",
            Opcode::Pmulhw => "pmulhw",
            Opcode::Pmuludq => "pmuludq",
            Opcode::Psllw => "psllw",
            Opcode::Pslld => "pslld",
            Opcode::Psllq => "psllq",
            Opcode::Psrlw => "psrlw",
            Opcode::Psrld => "psrld",
            Opcode::Psrlq => "psrlq",
            Opcode::Psraw => "psraw",
            Opcode::Psrad => "psrad",
            Opcode::Pslldq => "pslldq",
            Opcode::Psrldq => "psrldq",
            Opcode::Cvtdq2ps => "cvtdq2ps",
            Opcode::Cvtps2dq => "cvtps2dq",
            Opcode::Cvttps2dq => "cvttps2dq",
            Opcode::Cvtps2pd => "cvtps2pd",
            Opcode::Cvtpd2ps => "cvtpd2ps",
            Opcode::Cvtdq2pd => "cvtdq2pd",
            Opcode::Cvtpd2dq => "cvtpd2dq",
            Opcode::Cvttpd2dq => "cvttpd2dq",
            Opcode::Jcc(c) => return ("j", Some(c)),
            Opcode::Setcc(c) => return ("set", Some(c)),
            Opcode::Cmovcc(c) => return ("cmov", Some(c)),
        };
        (name, None)
    }
    fn write_to<W: Write + ?Sized>(self, w: &mut W) -> fmt::Result {
        if let Opcode::Core(op) = self {
            return op.write_to(w);
        }
        let (name, cc) = self.mnemonic();
        w.write_str(name)?;
        match cc {
            Some(c) => w.write_str(CC.get(usize::from(c)).ok_or(fmt::Error)?),
            None => Ok(()),
        }
    }
}
impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
impl Register {
    fn write_to<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        match self.class {
            RegisterClass::Segment => match self.number {
                0 => w.write_str("es"),
                1 => w.write_str("cs"),
                2 => w.write_str("ss"),
                3 => w.write_str("ds"),
                4 => w.write_str("fs"),
                5 => w.write_str("gs"),
                _ => Err(fmt::Error),
            },
            RegisterClass::Rip => w.write_str(if self.width == 32 { "eip" } else { "rip" }),
            RegisterClass::Ymm => {
                w.write_str("ymm")?;
                write_int(w, self.number.into(), false)
            }
            RegisterClass::Xmm => {
                w.write_str("xmm")?;
                write_int(w, self.number.into(), false)
            }
            RegisterClass::Mmx => {
                if self.number >= 8 || self.width != 64 || self.high_byte {
                    return Err(fmt::Error);
                }
                w.write_str("mm")?;
                write_int(w, self.number.into(), false)
            }
            RegisterClass::X87 => {
                if self.number >= 8 || self.width != 80 || self.high_byte {
                    return Err(fmt::Error);
                }
                w.write_str(
                    [
                        "st(0)", "st(1)", "st(2)", "st(3)", "st(4)", "st(5)", "st(6)", "st(7)",
                    ][usize::from(self.number)],
                )
            }
            RegisterClass::Gpr => {
                if self.number >= 16 || !matches!(self.width, 8 | 16 | 32 | 64) {
                    return Err(fmt::Error);
                }
                let n = usize::from(self.number);
                if self.high_byte {
                    return w.write_str(
                        ["ah", "ch", "dh", "bh"]
                            .get(n.wrapping_sub(4))
                            .ok_or(fmt::Error)?,
                    );
                }
                w.write_str(match self.width {
                    8 => [
                        "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b",
                        "r11b", "r12b", "r13b", "r14b", "r15b",
                    ][n],
                    16 => [
                        "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w",
                        "r11w", "r12w", "r13w", "r14w", "r15w",
                    ][n],
                    32 => [
                        "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d",
                        "r10d", "r11d", "r12d", "r13d", "r14d", "r15d",
                    ][n],
                    _ => [
                        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10",
                        "r11", "r12", "r13", "r14", "r15",
                    ][n],
                })
            }
        }
    }
}
impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
/// A quoted symbol, with runs of plain characters written whole.
fn symbol<W: Write + ?Sized>(w: &mut W, name: &str) -> fmt::Result {
    if name == "." {
        return w.write_str(".");
    }
    // Runs hold printable ASCII only, so they are always valid UTF-8, even
    // where an escaped byte splits a multibyte character.
    let run = |w: &mut W, run: &[u8]| match std::str::from_utf8(run) {
        Ok(s) => w.write_str(s),
        Err(_) => Err(fmt::Error),
    };
    w.write_char('"')?;
    let bytes = name.as_bytes();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(b, b' '..=b'~') && b != b'"' && b != b'\\' {
            continue;
        }
        run(w, &bytes[start..i])?;
        start = i + 1;
        if b == b'"' || b == b'\\' {
            w.write_char('\\')?;
            w.write_char(b as char)?;
        } else {
            let octal = [
                b'\\',
                b'0' + (b >> 6),
                b'0' + ((b >> 3) & 7),
                b'0' + (b & 7),
            ];
            w.write_str(std::str::from_utf8(&octal).map_err(|_| fmt::Error)?)?;
        }
    }
    run(w, &bytes[start..])?;
    w.write_char('"')
}
impl Address {
    pub fn write_to<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        write_expression(w, &self.expression, self.modifier)
    }
}
/// [`Address`]'s Intel spelling, for an expression not stored in one.
pub fn write_expression<W: Write + ?Sized>(
    w: &mut W,
    e: &Expression,
    modifier: Modifier,
) -> fmt::Result {
    if let Some(name) = &e.add {
        symbol(w, name)?;
        w.write_str(match modifier {
            Modifier::None => "",
            Modifier::Plt => "@PLT",
            Modifier::GotPcRel => "@GOTPCREL",
            Modifier::TpOff => "@TPOFF",
            Modifier::GotTpOff => "@GOTTPOFF",
            Modifier::DtpOff => "@DTPOFF",
        })?;
    }
    if let Some(name) = &e.subtract {
        w.write_str("-")?;
        symbol(w, name)?;
    }
    if e.add.is_none() && e.subtract.is_none() {
        write_int(w, e.constant, false)
    } else if e.constant != 0 {
        write_int(w, e.constant, true)
    } else {
        Ok(())
    }
}
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
impl Operand {
    pub fn write_to<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        match self {
            Operand::Register(r) => r.write_to(w),
            Operand::Immediate(a) => a.write_to(w),
            Operand::Memory(m) => {
                if m.width != 0 {
                    w.write_str(match m.width {
                        8 => "byte ptr ",
                        16 => "word ptr ",
                        32 => "dword ptr ",
                        64 => "qword ptr ",
                        48 => "fword ptr ",
                        80 => "tbyte ptr ",
                        128 => "xmmword ptr ",
                        256 => "ymmword ptr ",
                        _ => return Err(fmt::Error),
                    })?;
                }
                if let Some(s) = m.segment {
                    w.write_str(match s {
                        0x64 => "fs:",
                        0x65 => "gs:",
                        _ => return Err(fmt::Error),
                    })?;
                }
                w.write_str("[")?;
                let mut any = false;
                if let Some(r) = m.base {
                    r.write_to(w)?;
                    any = true;
                }
                if let Some(r) = m.index {
                    if any {
                        w.write_str(" + ")?;
                    }
                    r.write_to(w)?;
                    w.write_str(" * ")?;
                    write_int(w, m.scale.into(), false)?;
                    any = true;
                }
                if m.displacement.expression.add.is_some()
                    || m.displacement.expression.subtract.is_some()
                    || m.displacement.expression.constant != 0
                    || !any
                {
                    if any {
                        w.write_str(" + ")?;
                    }
                    m.displacement.write_to(w)?;
                }
                w.write_str("]")
            }
        }
    }
}
impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
/// An instruction's Intel spelling from its parts, without first assembling
/// an [`Instruction`]: the same text as its `Display`.
pub fn write_instruction<W: Write + ?Sized>(
    w: &mut W,
    opcode: Opcode,
    operands: &[Operand],
    lock: bool,
) -> fmt::Result {
    if lock {
        w.write_str("lock ")?;
    }
    let environment = matches!(opcode, Opcode::X87(op) if op.environment());
    if environment && matches!(operands, [Operand::Memory(m)] if m.width == 16) {
        w.write_str("data16 ")?;
    }
    opcode.write_to(w)?;
    for (i, o) in operands.iter().enumerate() {
        w.write_str(if i == 0 { "\t" } else { ", " })?;
        if matches!(o,Operand::Immediate(a) if a.expression.add.is_some()||a.expression.subtract.is_some())
            && !opcode.is_direct_branch()
        {
            w.write_str("offset ")?;
        }
        if environment && let Operand::Memory(m) = o {
            let mut m = m.clone();
            m.width = 0;
            Operand::Memory(m).write_to(w)?;
        } else {
            o.write_to(w)?;
        }
    }
    Ok(())
}
impl Instruction {
    pub fn write_to<W: Write + ?Sized>(&self, w: &mut W) -> fmt::Result {
        write_instruction(w, self.opcode, &self.operands, self.lock)
    }
}
impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_writes_match_formatting_machinery() {
        let symbol = |name: &str, constant| Address {
            expression: Expression {
                add: Some(name.into()),
                subtract: None,
                constant,
            },
            modifier: Modifier::Plt,
        };
        let a = symbol("a\"b\\c\u{e9}\u{1}d", -5);
        assert_eq!(a.to_string(), r#""a\"b\\c\303\251\001d"@PLT-5"#);
        assert_eq!(symbol("\u{e9}", 7).to_string(), r#""\303\251"@PLT+7"#);
        let constant = |n| Address {
            expression: Expression::constant(n),
            modifier: Modifier::None,
        };
        for n in [0, 9, -10, i128::from(u64::MAX), i128::MAX, i128::MIN] {
            assert_eq!(constant(n).to_string(), n.to_string());
        }
        let reg = |s| Register::parse(s).unwrap();
        let i =
            Instruction::parse_intel("lock add qword ptr fs:[r12 + r13 * 8 + \"x\"-\"y\"+3], r9")
                .unwrap();
        assert_eq!(
            i.to_string(),
            "lock add\tqword ptr fs:[r12 + r13 * 8 + \"x\"-\"y\"+3], r9"
        );
        for r in [
            "r8b", "r15w", "r10d", "r11", "sil", "ah", "xmm15", "st(3)", "rip",
        ] {
            assert_eq!(reg(r).to_string(), r);
        }
        let mut s = String::new();
        write_instruction(
            &mut s,
            Opcode::Jcc(14),
            &[Operand::Immediate(symbol("L", 0))],
            false,
        )
        .unwrap();
        assert_eq!(s, "jle\t\"L\"@PLT");
    }
}
