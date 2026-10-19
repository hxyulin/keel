//! x86 identities are independent of the pinned AArch64 encoding IDs.
use crate::Expression;
use smallvec::SmallVec;
use std::borrow::Cow;

fn lowercase(s: &str) -> Cow<'_, str> {
    if s.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(s.to_ascii_lowercase())
    } else {
        Cow::Borrowed(s)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opcode {
    Mov,
    Movabs,
    Lea,
    Movsx,
    Movzx,
    Movsxd,
    Push,
    Pop,
    Ret,
    Leave,
    Nop,
    Ud2,
    Add,
    Or,
    Adc,
    Sbb,
    And,
    Sub,
    Xor,
    Cmp,
    Test,
    Neg,
    Not,
    Inc,
    Dec,
    Mul,
    Imul,
    Div,
    Idiv,
    Cqo,
    Cdq,
    Cbw,
    Cwde,
    Cdqe,
    Shl,
    Shr,
    Sar,
    Rol,
    Ror,
    Shld,
    Shrd,
    Bsf,
    Bsr,
    Bswap,
    Popcnt,
    Andn,
    Bextr,
    Blsi,
    Blsmsk,
    Blsr,
    Tzcnt,
    Shlx,
    Shrx,
    Sarx,
    Rorx,
    Bzhi,
    Pext,
    Pdep,
    Mulx,
    Bt,
    Bts,
    Btr,
    Btc,
    Xchg,
    Xadd,
    Cmpxchg,
    Mfence,
    Lfence,
    Sfence,
    Pause,
    Syscall,
    Int3,
    Call,
    Jmp,
    Jcc(u8),
    Setcc(u8),
    Cmovcc(u8),
    Vaddps,
    Vaddpd,
    Vsubps,
    Vsubpd,
    Vmulps,
    Vmulpd,
    Vdivps,
    Vdivpd,
    Vaddss,
    Vaddsd,
    Vsubss,
    Vsubsd,
    Vmulss,
    Vmulsd,
    Vdivss,
    Vdivsd,
    Vsqrtss,
    Vsqrtsd,
    Vminss,
    Vminsd,
    Vmaxss,
    Vmaxsd,
    Vucomiss,
    Vucomisd,
    Vmovss,
    Vmovsd,
    Vmovaps,
    Vmovups,
    Vmovapd,
    Vmovupd,
    Vandps,
    Vandpd,
    Vorps,
    Vorpd,
    Vxorps,
    Vxorpd,
    Vroundss,
    Vroundsd,
    Vzeroupper,
    Movss,
    Movsd,
    Movaps,
    Movups,
    Movhlps,
    Paddb,
    Paddw,
    Paddd,
    Paddq,
    Psubb,
    Psubw,
    Psubd,
    Psubq,
    Pmullw,
    Addps,
    Addpd,
    Subps,
    Subpd,
    Mulps,
    Mulpd,
    Divps,
    Divpd,
    Sqrtps,
    Sqrtpd,

    Movapd,
    Movupd,
    Movd,
    Movq,
    Addss,
    Addsd,
    Subss,
    Subsd,
    Mulss,
    Mulsd,
    Divss,
    Divsd,
    Sqrtss,
    Sqrtsd,
    Minss,
    Minsd,
    Maxss,
    Maxsd,
    Ucomiss,
    Ucomisd,
    Andps,
    Andpd,
    Orps,
    Orpd,
    Xorps,
    Xorpd,
    Roundss,
    Roundsd,
    Cvtsi2ss,
    Cvtsi2sd,
    Cvttss2si,
    Cvttsd2si,
    Cvtss2sd,
    Cvtsd2ss,
    // Legacy SSE/SSE2: part of the x86-64 baseline, independent of AVX.
    Movdqa,
    Movdqu,
    Pxor,
    Pand,
    Pandn,
    Por,
    Pcmpeqb,
    Pcmpeqw,
    Pcmpeqd,
    Pcmpgtb,
    Pcmpgtw,
    Pcmpgtd,
    Pshufd,
    Pshuflw,
    Pshufhw,
    Punpcklbw,
    Punpcklwd,
    Punpckldq,
    Punpcklqdq,
    Punpckhbw,
    Punpckhwd,
    Punpckhdq,
    Punpckhqdq,
    Unpcklps,
    Unpckhps,
    Unpcklpd,
    Unpckhpd,
    Shufps,
    Shufpd,
    Andnps,
    Andnpd,
    Minps,
    Minpd,
    Maxps,
    Maxpd,
    Cmpps,
    Cmppd,
    Cmpss,
    Cmpsd,
    Cmpltss,
    Cmpnltps,
    Pmovmskb,
    Movmskps,
    Movmskpd,
    Pmaddwd,
    Pmulhuw,
    Pmulhw,
    Pmuludq,
    Psllw,
    Pslld,
    Psllq,
    Psrlw,
    Psrld,
    Psrlq,
    Psraw,
    Psrad,
    Pslldq,
    Psrldq,
    Cvtdq2ps,
    Cvtps2dq,
    Cvttps2dq,
    Cvtps2pd,
    Cvtpd2ps,
    Cvtdq2pd,
    Cvtpd2dq,
    Cvttpd2dq,
}
impl Opcode {
    pub fn parse(s: &str) -> Option<Self> {
        use Opcode::*;
        Some(match s {
            "mov" => Mov,
            "movabs" => Movabs,
            "lea" => Lea,
            "movsx" => Movsx,
            "movzx" => Movzx,
            "movsxd" => Movsxd,
            "push" => Push,
            "pop" => Pop,
            "ret" => Ret,
            "leave" => Leave,
            "nop" => Nop,
            "ud2" => Ud2,
            "add" => Add,
            "or" => Or,
            "adc" => Adc,
            "sbb" => Sbb,
            "and" => And,
            "sub" => Sub,
            "xor" => Xor,
            "cmp" => Cmp,
            "test" => Test,
            "neg" => Neg,
            "not" => Not,
            "inc" => Inc,
            "dec" => Dec,
            "mul" => Mul,
            "imul" => Imul,
            "div" => Div,
            "idiv" => Idiv,
            "cqo" => Cqo,
            "cdq" => Cdq,
            "cbw" => Cbw,
            "cwde" => Cwde,
            "cdqe" => Cdqe,
            "shl" => Shl,
            "shr" => Shr,
            "sar" => Sar,
            "rol" => Rol,
            "ror" => Ror,
            "shld" => Shld,
            "shrd" => Shrd,
            "bsf" => Bsf,
            "bsr" => Bsr,
            "bswap" => Bswap,
            "popcnt" => Popcnt,
            "andn" => Andn,
            "bextr" => Bextr,
            "blsi" => Blsi,
            "blsmsk" => Blsmsk,
            "blsr" => Blsr,
            "tzcnt" => Tzcnt,
            "shlx" => Shlx,
            "shrx" => Shrx,
            "sarx" => Sarx,
            "rorx" => Rorx,
            "bzhi" => Bzhi,
            "pext" => Pext,
            "pdep" => Pdep,
            "mulx" => Mulx,
            "bt" => Bt,
            "bts" => Bts,
            "btr" => Btr,
            "btc" => Btc,
            "xchg" => Xchg,
            "xadd" => Xadd,
            "cmpxchg" => Cmpxchg,
            "mfence" => Mfence,
            "lfence" => Lfence,
            "sfence" => Sfence,
            "pause" => Pause,
            "syscall" => Syscall,
            "int3" => Int3,
            "call" => Call,
            "jmp" => Jmp,
            "vaddps" => Vaddps,
            "vaddpd" => Vaddpd,
            "vsubps" => Vsubps,
            "vsubpd" => Vsubpd,
            "vmulps" => Vmulps,
            "vmulpd" => Vmulpd,
            "vdivps" => Vdivps,
            "vdivpd" => Vdivpd,
            "vaddss" => Vaddss,
            "vaddsd" => Vaddsd,
            "vsubss" => Vsubss,
            "vsubsd" => Vsubsd,
            "vmulss" => Vmulss,
            "vmulsd" => Vmulsd,
            "vdivss" => Vdivss,
            "vdivsd" => Vdivsd,
            "vsqrtss" => Vsqrtss,
            "vsqrtsd" => Vsqrtsd,
            "vminss" => Vminss,
            "vminsd" => Vminsd,
            "vmaxss" => Vmaxss,
            "vmaxsd" => Vmaxsd,
            "vucomiss" => Vucomiss,
            "vucomisd" => Vucomisd,
            "vmovss" => Vmovss,
            "vmovsd" => Vmovsd,
            "vmovaps" => Vmovaps,
            "vmovups" => Vmovups,
            "vmovapd" => Vmovapd,
            "vmovupd" => Vmovupd,
            "vandps" => Vandps,
            "vandpd" => Vandpd,
            "vorps" => Vorps,
            "vorpd" => Vorpd,
            "vxorps" => Vxorps,
            "vxorpd" => Vxorpd,
            "vroundss" => Vroundss,
            "vroundsd" => Vroundsd,
            "vzeroupper" => Vzeroupper,
            "movss" => Movss,
            "movsd" => Movsd,
            "movaps" => Movaps,
            "movups" => Movups,
            "movhlps" => Movhlps,
            "paddb" => Paddb,
            "paddw" => Paddw,
            "paddd" => Paddd,
            "paddq" => Paddq,
            "psubb" => Psubb,
            "psubw" => Psubw,
            "psubd" => Psubd,
            "psubq" => Psubq,
            "pmullw" => Pmullw,
            "addps" => Addps,
            "addpd" => Addpd,
            "subps" => Subps,
            "subpd" => Subpd,
            "mulps" => Mulps,
            "mulpd" => Mulpd,
            "divps" => Divps,
            "divpd" => Divpd,
            "sqrtps" => Sqrtps,
            "sqrtpd" => Sqrtpd,

            "movapd" => Movapd,
            "movupd" => Movupd,
            "movd" => Movd,
            "movq" => Movq,
            "addss" => Addss,
            "addsd" => Addsd,
            "subss" => Subss,
            "subsd" => Subsd,
            "mulss" => Mulss,
            "mulsd" => Mulsd,
            "divss" => Divss,
            "divsd" => Divsd,
            "sqrtss" => Sqrtss,
            "sqrtsd" => Sqrtsd,
            "minss" => Minss,
            "minsd" => Minsd,
            "maxss" => Maxss,
            "maxsd" => Maxsd,
            "ucomiss" => Ucomiss,
            "ucomisd" => Ucomisd,
            "andps" => Andps,
            "andpd" => Andpd,
            "orps" => Orps,
            "orpd" => Orpd,
            "xorps" => Xorps,
            "xorpd" => Xorpd,
            "roundss" => Roundss,
            "roundsd" => Roundsd,
            "cvtsi2ss" => Cvtsi2ss,
            "cvtsi2sd" => Cvtsi2sd,
            "cvttss2si" => Cvttss2si,
            "cvttsd2si" => Cvttsd2si,
            "cvtss2sd" => Cvtss2sd,
            "cvtsd2ss" => Cvtsd2ss,
            "movdqa" => Movdqa,
            "movdqu" => Movdqu,
            "pxor" => Pxor,
            "pand" => Pand,
            "pandn" => Pandn,
            "por" => Por,
            "pcmpeqb" => Pcmpeqb,
            "pcmpeqw" => Pcmpeqw,
            "pcmpeqd" => Pcmpeqd,
            "pcmpgtb" => Pcmpgtb,
            "pcmpgtw" => Pcmpgtw,
            "pcmpgtd" => Pcmpgtd,
            "pshufd" => Pshufd,
            "pshuflw" => Pshuflw,
            "pshufhw" => Pshufhw,
            "punpcklbw" => Punpcklbw,
            "punpcklwd" => Punpcklwd,
            "punpckldq" => Punpckldq,
            "punpcklqdq" => Punpcklqdq,
            "punpckhbw" => Punpckhbw,
            "punpckhwd" => Punpckhwd,
            "punpckhdq" => Punpckhdq,
            "punpckhqdq" => Punpckhqdq,
            "unpcklps" => Unpcklps,
            "unpckhps" => Unpckhps,
            "unpcklpd" => Unpcklpd,
            "unpckhpd" => Unpckhpd,
            "shufps" => Shufps,
            "shufpd" => Shufpd,
            "andnps" => Andnps,
            "andnpd" => Andnpd,
            "minps" => Minps,
            "minpd" => Minpd,
            "maxps" => Maxps,
            "maxpd" => Maxpd,
            "cmpps" => Cmpps,
            "cmppd" => Cmppd,
            "cmpss" => Cmpss,
            "cmpsd" => Cmpsd,
            "cmpltss" => Cmpltss,
            "cmpnltps" => Cmpnltps,
            "pmovmskb" => Pmovmskb,
            "movmskps" => Movmskps,
            "movmskpd" => Movmskpd,
            "pmaddwd" => Pmaddwd,
            "pmulhuw" => Pmulhuw,
            "pmulhw" => Pmulhw,
            "pmuludq" => Pmuludq,
            "psllw" => Psllw,
            "pslld" => Pslld,
            "psllq" => Psllq,
            "psrlw" => Psrlw,
            "psrld" => Psrld,
            "psrlq" => Psrlq,
            "psraw" => Psraw,
            "psrad" => Psrad,
            "pslldq" => Pslldq,
            "psrldq" => Psrldq,
            "cvtdq2ps" => Cvtdq2ps,
            "cvtps2dq" => Cvtps2dq,
            "cvttps2dq" => Cvttps2dq,
            "cvtps2pd" => Cvtps2pd,
            "cvtpd2ps" => Cvtpd2ps,
            "cvtdq2pd" => Cvtdq2pd,
            "cvtpd2dq" => Cvtpd2dq,
            "cvttpd2dq" => Cvttpd2dq,
            "sal" => Shl,
            _ => {
                let (kind, cc) = if let Some(cc) = s.strip_prefix("cmov") {
                    (2, cc)
                } else if let Some(cc) = s.strip_prefix("set") {
                    (1, cc)
                } else {
                    (0, s.strip_prefix('j')?)
                };
                let cc = match cc {
                    "o" => 0,
                    "no" => 1,
                    "b" | "c" | "nae" => 2,
                    "ae" | "nb" | "nc" => 3,
                    "e" | "z" => 4,
                    "ne" | "nz" => 5,
                    "be" | "na" => 6,
                    "a" | "nbe" => 7,
                    "s" => 8,
                    "ns" => 9,
                    "p" | "pe" => 10,
                    "np" | "po" => 11,
                    "l" | "nge" => 12,
                    "ge" | "nl" => 13,
                    "le" | "ng" => 14,
                    "g" | "nle" => 15,
                    _ => return None,
                };
                match kind {
                    0 => Jcc(cc),
                    1 => Setcc(cc),
                    _ => Cmovcc(cc),
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterClass {
    Gpr,
    Xmm,
    Rip,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Register {
    pub class: RegisterClass,
    pub number: u8,
    pub width: u16,
    pub high_byte: bool,
}
impl Register {
    pub fn parse(s: &str) -> Option<Self> {
        let s = lowercase(s);
        let s = s.as_ref();
        if s == "rip" {
            return Some(Self {
                class: RegisterClass::Rip,
                number: 0,
                width: 64,
                high_byte: false,
            });
        }
        if let Some(n) = s
            .strip_prefix("xmm")
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| *n < 16)
        {
            return Some(Self {
                class: RegisterClass::Xmm,
                number: n,
                width: 128,
                high_byte: false,
            });
        }
        let named = match s {
            "rax" => Some((0, 64, false)),
            "rcx" => Some((1, 64, false)),
            "rdx" => Some((2, 64, false)),
            "rbx" => Some((3, 64, false)),
            "rsp" => Some((4, 64, false)),
            "rbp" => Some((5, 64, false)),
            "rsi" => Some((6, 64, false)),
            "rdi" => Some((7, 64, false)),
            "eax" => Some((0, 32, false)),
            "ecx" => Some((1, 32, false)),
            "edx" => Some((2, 32, false)),
            "ebx" => Some((3, 32, false)),
            "esp" => Some((4, 32, false)),
            "ebp" => Some((5, 32, false)),
            "esi" => Some((6, 32, false)),
            "edi" => Some((7, 32, false)),
            "ax" => Some((0, 16, false)),
            "cx" => Some((1, 16, false)),
            "dx" => Some((2, 16, false)),
            "bx" => Some((3, 16, false)),
            "sp" => Some((4, 16, false)),
            "bp" => Some((5, 16, false)),
            "si" => Some((6, 16, false)),
            "di" => Some((7, 16, false)),
            "al" => Some((0, 8, false)),
            "cl" => Some((1, 8, false)),
            "dl" => Some((2, 8, false)),
            "bl" => Some((3, 8, false)),
            "spl" => Some((4, 8, false)),
            "bpl" => Some((5, 8, false)),
            "sil" => Some((6, 8, false)),
            "dil" => Some((7, 8, false)),
            "ah" => Some((4, 8, true)),
            "ch" => Some((5, 8, true)),
            "dh" => Some((6, 8, true)),
            "bh" => Some((7, 8, true)),
            _ => None,
        };
        if let Some((number, width, high_byte)) = named {
            return Some(Self {
                class: RegisterClass::Gpr,
                number,
                width,
                high_byte,
            });
        }
        if let Some(rest) = s.strip_prefix('r') {
            let (number, width) = match rest.as_bytes().last()? {
                b'b' => (&rest[..rest.len() - 1], 8),
                b'w' => (&rest[..rest.len() - 1], 16),
                b'd' => (&rest[..rest.len() - 1], 32),
                _ => (rest, 64),
            };
            if let Some(n) = number.parse::<u8>().ok().filter(|n| (8..16).contains(n)) {
                return Some(Self {
                    class: RegisterClass::Gpr,
                    number: n,
                    width,
                    high_byte: false,
                });
            }
        }
        None
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modifier {
    #[default]
    None,
    Plt,
    GotPcRel,
    TpOff,
    GotTpOff,
    DtpOff,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Address {
    pub expression: Expression,
    pub modifier: Modifier,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memory {
    /// Zero means infer from the other operand.
    pub width: u16,
    pub base: Option<Register>,
    pub index: Option<Register>,
    pub scale: u8,
    /// Optional FS (0x64) or GS (0x65) override.
    pub segment: Option<u8>,
    pub displacement: Address,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Register(Register),
    Immediate(Address),
    Memory(Memory),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: Opcode,
    pub operands: Vec<Operand>,
    pub lock: bool,
}

/// Split at delimiters outside strings and address brackets.
pub fn split(s: &str, delimiter: char) -> Vec<&str> {
    split_inline(s, delimiter).into_vec()
}
/// Inline storage for the short operand/directive lists in ordinary assembly.
pub fn split_inline(s: &str, delimiter: char) -> SmallVec<[&str; 4]> {
    if !s.contains(delimiter) {
        return smallvec::smallvec![s.trim()];
    }
    let (mut quoted, mut escaped, mut depth, mut start) = (false, false, 0, 0);
    let mut parts = SmallVec::new();
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quoted = !quoted
        }
        if !quoted {
            if c == '[' {
                depth += 1
            }
            if c == ']' {
                depth -= 1
            }
            if c == delimiter && depth == 0 {
                parts.push(s[start..i].trim());
                start = i + c.len_utf8();
            }
        }
    }
    parts.push(s[start..].trim());
    parts
}
pub fn integer(s: &str) -> Result<i128, String> {
    let s = s.trim();
    let (sign, s) = if let Some(s) = s.strip_prefix('-') {
        (-1, s)
    } else {
        (1, s.strip_prefix('+').unwrap_or(s))
    };
    let (radix, s) = if let Some(s) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (16, s)
    } else if let Some(s) = s.strip_prefix("0b") {
        (2, s)
    } else if s.len() > 1 && s.starts_with('0') {
        // GAS and LLVM MC read a leading zero as octal, so `010` is 8 and `08` is invalid.
        (8, &s[1..])
    } else {
        (10, s)
    };
    // from_str_radix accepts its own sign; only the leading one parsed above is valid.
    if s.starts_with(['+', '-']) {
        return Err(format!("invalid integer `{s}`"));
    }
    let n = i128::from_str_radix(s, radix).map_err(|_| format!("invalid integer `{s}`"))?;
    // LLVM MC rejects integer literals wider than 64 bits in every expression.
    if n > i128::from(u64::MAX) {
        return Err(format!("integer literal `{s}` exceeds 64 bits"));
    }
    Ok(sign * n)
}
pub fn unquote(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if !s.starts_with('"') || !s.ends_with('"') || s.len() < 2 {
        return Err(format!("expected quoted string: {s}"));
    }
    let mut out = Vec::new();
    let mut bytes = s.as_bytes()[1..s.len() - 1].iter().copied().peekable();
    while let Some(b) = bytes.next() {
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let b = bytes.next().ok_or("unfinished string escape")?;
        out.push(match b {
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'\\' | b'"' => b,
            b'0'..=b'7' => {
                let mut value = u16::from(b - b'0');
                for _ in 0..2 {
                    if let Some(b'0'..=b'7') = bytes.peek() {
                        value = value * 8 + u16::from(bytes.next().unwrap() - b'0');
                    } else {
                        break;
                    }
                }
                u8::try_from(value).map_err(|_| "octal escape exceeds a byte")?
            }
            _ => return Err("unsupported string escape".into()),
        });
    }
    Ok(out)
}
pub fn symbol(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.starts_with('"') {
        return String::from_utf8(unquote(s)?).map_err(|_| "symbol is not UTF-8".into());
    }
    if s.is_empty()
        || s.as_bytes().first().is_some_and(u8::is_ascii_digit)
        || !s
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.$".contains(&b))
    {
        return Err(format!("invalid symbol `{s}`"));
    }
    Ok(s.into())
}
/// Parse the linear symbol expressions emitted by the compiler.
pub fn address(s: &str) -> Result<Address, String> {
    let mut out = Address::default();
    let (mut quote, mut escaped, mut start, mut sign) = (false, false, 0, 1i128);
    let mut terms = SmallVec::<[_; 4]>::new();
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quote = !quote
        }
        if !quote && (c == '+' || c == '-') {
            if i > start && !s[start..i].trim().is_empty() {
                terms.push((sign, &s[start..i]));
            }
            sign = if c == '-' { -1 } else { 1 };
            start = i + 1;
        }
    }
    if quote {
        return Err("unterminated symbol quote".into());
    }
    terms.push((sign, &s[start..]));
    for (sign, term) in terms {
        let term = term.trim();
        // Symbol names cannot start with a digit. Avoid formatting an integer
        // parse error for every ordinary symbolic address term.
        let number = if term.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            integer(term).ok()
        } else {
            None
        };
        if let Some(n) = number {
            out.expression.constant = out
                .expression
                .constant
                .checked_add(sign * n)
                .ok_or("expression overflow")?;
            continue;
        }
        let parts = split_inline(term, '@');
        let name = symbol(parts[0])?;
        if parts.len() > 2 {
            return Err("multiple relocation modifiers".into());
        }
        if let Some(modifier) = parts.get(1) {
            if out.modifier != Modifier::None {
                return Err("multiple relocation modifiers".into());
            }
            out.modifier = match lowercase(modifier).as_ref() {
                "plt" => Modifier::Plt,
                "gotpcrel" => Modifier::GotPcRel,
                "tpoff" => Modifier::TpOff,
                "gottpoff" => Modifier::GotTpOff,
                "dtpoff" => Modifier::DtpOff,
                _ => return Err(format!("unsupported relocation modifier `{modifier}`")),
            };
        }
        let slot = if sign == 1 {
            &mut out.expression.add
        } else {
            &mut out.expression.subtract
        };
        if slot.replace(name).is_some() {
            return Err("expression has multiple symbols with the same sign".into());
        }
    }
    Ok(out)
}
fn operand(s: &str) -> Result<Operand, String> {
    if let Some(reg) = Register::parse(s) {
        return Ok(Operand::Register(reg));
    }
    let mut s = s.trim();
    let mut width = 0;
    for (prefix, bits) in [
        ("byte ptr ", 8),
        ("word ptr ", 16),
        ("dword ptr ", 32),
        ("qword ptr ", 64),
        ("xmmword ptr ", 128),
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            width = bits;
            s = rest.trim();
            break;
        }
    }
    let mut segment = None;
    for (prefix, byte) in [("fs:", 0x64), ("gs:", 0x65)] {
        if let Some(rest) = s.strip_prefix(prefix) {
            segment = Some(byte);
            s = rest.trim();
            break;
        }
    }
    if s.starts_with('[') || segment.is_some() || width != 0 {
        let body = if s.starts_with('[') {
            s.strip_prefix('[')
                .unwrap()
                .strip_suffix(']')
                .ok_or("unclosed memory operand")?
        } else {
            s
        };
        let mut mem = Memory {
            width,
            base: None,
            index: None,
            scale: 1,
            segment,
            displacement: Address::default(),
        };
        // Split signed address terms outside quotes; registers cannot be subtracted.
        let (mut quote, mut escaped, mut start, mut sign) = (false, false, 0, 1);
        let mut terms = SmallVec::<[_; 4]>::new();
        for (i, c) in body.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if quote && c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                quote = !quote
            }
            if !quote && (c == '+' || c == '-') {
                if !body[start..i].trim().is_empty() {
                    terms.push((sign, body[start..i].trim()));
                }
                sign = if c == '-' { -1 } else { 1 };
                start = i + 1;
            }
        }
        terms.push((sign, body[start..].trim()));
        let mut remaining = String::new();
        for (sign, term) in terms {
            let factors = split_inline(term, '*');
            let scaled = if factors.len() == 2 {
                if let Some(r) = Register::parse(factors[0]) {
                    Some((r, integer(factors[1])?))
                } else if let Some(r) = Register::parse(factors[1]) {
                    Some((r, integer(factors[0])?))
                } else {
                    return Err("memory scale requires a register".into());
                }
            } else if factors.len() > 2 {
                return Err("invalid address product".into());
            } else {
                None
            };
            if let Some((r, scale)) = scaled {
                if sign < 0 || ![1, 2, 4, 8].contains(&scale) || mem.index.is_some() {
                    return Err("invalid memory index/scale".into());
                }
                mem.index = Some(r);
                mem.scale = scale as u8;
            } else if let Some(r) = Register::parse(term) {
                if sign < 0 {
                    return Err("cannot subtract an address register".into());
                }
                if mem.base.is_none() {
                    mem.base = Some(r)
                } else if mem.index.is_none() {
                    mem.index = Some(r)
                } else {
                    return Err("too many address registers".into());
                }
            } else {
                if sign < 0 {
                    remaining.push('-')
                } else if !remaining.is_empty() {
                    remaining.push('+')
                }
                remaining.push_str(term);
            }
        }
        if !remaining.is_empty() {
            mem.displacement = address(&remaining)?
        }
        for r in [mem.base, mem.index].into_iter().flatten() {
            if r.width != 64 || r.class == RegisterClass::Xmm {
                return Err("addresses require 64-bit general registers".into());
            }
        }
        if mem
            .index
            .is_some_and(|r| r.class == RegisterClass::Rip || r.number == 4)
            || (mem.base.is_some_and(|r| r.class == RegisterClass::Rip) && mem.index.is_some())
        {
            return Err("invalid address index".into());
        }
        return Ok(Operand::Memory(mem));
    }
    Ok(Operand::Immediate(address(
        s.strip_prefix("offset ").unwrap_or(s),
    )?))
}
impl Instruction {
    pub fn parse_intel(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (mnemonic, operands) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let lock = mnemonic.eq_ignore_ascii_case("lock");
        let (mnemonic, operands) = if lock {
            let rest = operands.trim();
            if rest.is_empty() {
                return Err("lock requires an instruction".into());
            }
            rest.split_once(char::is_whitespace).unwrap_or((rest, ""))
        } else {
            (mnemonic, operands)
        };
        let opcode = Opcode::parse(&lowercase(mnemonic))
            .ok_or_else(|| format!("unsupported x86 instruction `{mnemonic}`"))?;
        let operands = if operands.trim().is_empty() {
            Vec::new()
        } else {
            split_inline(operands, ',')
                .into_iter()
                .map(operand)
                .collect::<Result<_, _>>()?
        };
        Ok(Self {
            opcode,
            operands,
            lock,
        })
    }
}

/// Parse Intel source into shared program items. Directives retain their ELF
/// spelling; instructions have architecture-specific identities and operands.
pub fn parse_program(source: &str) -> Result<crate::Program, String> {
    let mut p = crate::Program::new();
    for (line, source) in source.lines().enumerate() {
        let mut quote = false;
        let mut escaped = false;
        let mut end = source.len();
        for (i, c) in source.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            if quote && c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                quote = !quote
            }
            if !quote && (c == '#' || source[i..].starts_with("//")) {
                end = i;
                break;
            }
        }
        for statement in split_inline(&source[..end], ';') {
            let mut s = statement.trim();
            if s.is_empty() {
                continue;
            }
            // A colon in fs:/gs: is part of an instruction, not a label.
            let labels = split_inline(s, ':');
            if labels.len() > 1
                && (labels[0].starts_with('"') || !labels[0].contains(char::is_whitespace))
            {
                let name = symbol(labels[0]).map_err(|e| format!("line {}: {e}", line + 1))?;
                p.label(name);
                let mut quote = false;
                let mut escaped = false;
                let colon = s
                    .char_indices()
                    .find_map(|(i, c)| {
                        if escaped {
                            escaped = false;
                            return None;
                        }
                        if quote && c == '\\' {
                            escaped = true;
                            return None;
                        }
                        if c == '"' {
                            quote = !quote
                        }
                        if c == ':' && !quote { Some(i) } else { None }
                    })
                    .unwrap();
                s = s[colon + 1..].trim();
            }
            if s.is_empty() {
                continue;
            }
            if s.starts_with('.') || split_inline(s, '=').len() == 2 {
                p.text(s);
                p.text("\n")
            } else {
                p.x86_instruction(
                    Instruction::parse_intel(s).map_err(|e| format!("line {}: {e}", line + 1))?,
                )
            }
        }
    }
    Ok(p)
}

mod format;

mod features;
pub use features::Features;
