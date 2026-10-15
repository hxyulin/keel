//! Intel syntax serialization for the shared x86 instruction API.
use super::*;
use std::fmt;
const CC: [&str; 16] = [
    "o", "no", "b", "ae", "e", "ne", "be", "a", "s", "ns", "p", "np", "l", "ge", "le", "g",
];
impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Opcode::Mov => f.write_str("mov"),
            Opcode::Movabs => f.write_str("movabs"),
            Opcode::Lea => f.write_str("lea"),
            Opcode::Movsx => f.write_str("movsx"),
            Opcode::Movzx => f.write_str("movzx"),
            Opcode::Movsxd => f.write_str("movsxd"),
            Opcode::Push => f.write_str("push"),
            Opcode::Pop => f.write_str("pop"),
            Opcode::Ret => f.write_str("ret"),
            Opcode::Leave => f.write_str("leave"),
            Opcode::Nop => f.write_str("nop"),
            Opcode::Ud2 => f.write_str("ud2"),
            Opcode::Add => f.write_str("add"),
            Opcode::Or => f.write_str("or"),
            Opcode::Adc => f.write_str("adc"),
            Opcode::Sbb => f.write_str("sbb"),
            Opcode::And => f.write_str("and"),
            Opcode::Sub => f.write_str("sub"),
            Opcode::Xor => f.write_str("xor"),
            Opcode::Cmp => f.write_str("cmp"),
            Opcode::Test => f.write_str("test"),
            Opcode::Neg => f.write_str("neg"),
            Opcode::Not => f.write_str("not"),
            Opcode::Inc => f.write_str("inc"),
            Opcode::Dec => f.write_str("dec"),
            Opcode::Mul => f.write_str("mul"),
            Opcode::Imul => f.write_str("imul"),
            Opcode::Div => f.write_str("div"),
            Opcode::Idiv => f.write_str("idiv"),
            Opcode::Cqo => f.write_str("cqo"),
            Opcode::Cdq => f.write_str("cdq"),
            Opcode::Cbw => f.write_str("cbw"),
            Opcode::Cwde => f.write_str("cwde"),
            Opcode::Cdqe => f.write_str("cdqe"),
            Opcode::Shl => f.write_str("shl"),
            Opcode::Shr => f.write_str("shr"),
            Opcode::Sar => f.write_str("sar"),
            Opcode::Rol => f.write_str("rol"),
            Opcode::Ror => f.write_str("ror"),
            Opcode::Shld => f.write_str("shld"),
            Opcode::Shrd => f.write_str("shrd"),
            Opcode::Bsf => f.write_str("bsf"),
            Opcode::Bsr => f.write_str("bsr"),
            Opcode::Bswap => f.write_str("bswap"),
            Opcode::Popcnt => f.write_str("popcnt"),
            Opcode::Andn => f.write_str("andn"),
            Opcode::Bextr => f.write_str("bextr"),
            Opcode::Blsi => f.write_str("blsi"),
            Opcode::Blsmsk => f.write_str("blsmsk"),
            Opcode::Blsr => f.write_str("blsr"),
            Opcode::Tzcnt => f.write_str("tzcnt"),
            Opcode::Shlx => f.write_str("shlx"),
            Opcode::Shrx => f.write_str("shrx"),
            Opcode::Sarx => f.write_str("sarx"),
            Opcode::Rorx => f.write_str("rorx"),
            Opcode::Bzhi => f.write_str("bzhi"),
            Opcode::Pext => f.write_str("pext"),
            Opcode::Pdep => f.write_str("pdep"),
            Opcode::Mulx => f.write_str("mulx"),
            Opcode::Bt => f.write_str("bt"),
            Opcode::Bts => f.write_str("bts"),
            Opcode::Btr => f.write_str("btr"),
            Opcode::Btc => f.write_str("btc"),
            Opcode::Xchg => f.write_str("xchg"),
            Opcode::Xadd => f.write_str("xadd"),
            Opcode::Cmpxchg => f.write_str("cmpxchg"),
            Opcode::Mfence => f.write_str("mfence"),
            Opcode::Lfence => f.write_str("lfence"),
            Opcode::Sfence => f.write_str("sfence"),
            Opcode::Pause => f.write_str("pause"),
            Opcode::Syscall => f.write_str("syscall"),
            Opcode::Int3 => f.write_str("int3"),
            Opcode::Call => f.write_str("call"),
            Opcode::Jmp => f.write_str("jmp"),
            Opcode::Vaddss => f.write_str("vaddss"),
            Opcode::Vaddsd => f.write_str("vaddsd"),
            Opcode::Vsubss => f.write_str("vsubss"),
            Opcode::Vsubsd => f.write_str("vsubsd"),
            Opcode::Vmulss => f.write_str("vmulss"),
            Opcode::Vmulsd => f.write_str("vmulsd"),
            Opcode::Vdivss => f.write_str("vdivss"),
            Opcode::Vdivsd => f.write_str("vdivsd"),
            Opcode::Vsqrtss => f.write_str("vsqrtss"),
            Opcode::Vsqrtsd => f.write_str("vsqrtsd"),
            Opcode::Vminss => f.write_str("vminss"),
            Opcode::Vminsd => f.write_str("vminsd"),
            Opcode::Vmaxss => f.write_str("vmaxss"),
            Opcode::Vmaxsd => f.write_str("vmaxsd"),
            Opcode::Vucomiss => f.write_str("vucomiss"),
            Opcode::Vucomisd => f.write_str("vucomisd"),
            Opcode::Vmovss => f.write_str("vmovss"),
            Opcode::Vmovsd => f.write_str("vmovsd"),
            Opcode::Vmovaps => f.write_str("vmovaps"),
            Opcode::Vmovups => f.write_str("vmovups"),
            Opcode::Vmovapd => f.write_str("vmovapd"),
            Opcode::Vmovupd => f.write_str("vmovupd"),
            Opcode::Vandps => f.write_str("vandps"),
            Opcode::Vandpd => f.write_str("vandpd"),
            Opcode::Vorps => f.write_str("vorps"),
            Opcode::Vorpd => f.write_str("vorpd"),
            Opcode::Vxorps => f.write_str("vxorps"),
            Opcode::Vxorpd => f.write_str("vxorpd"),
            Opcode::Vroundss => f.write_str("vroundss"),
            Opcode::Vroundsd => f.write_str("vroundsd"),
            Opcode::Vzeroupper => f.write_str("vzeroupper"),
            Opcode::Movss => f.write_str("movss"),
            Opcode::Movsd => f.write_str("movsd"),
            Opcode::Movaps => f.write_str("movaps"),
            Opcode::Movups => f.write_str("movups"),
            Opcode::Paddb => f.write_str("paddb"),
            Opcode::Paddw => f.write_str("paddw"),
            Opcode::Paddd => f.write_str("paddd"),
            Opcode::Paddq => f.write_str("paddq"),
            Opcode::Psubb => f.write_str("psubb"),
            Opcode::Psubw => f.write_str("psubw"),
            Opcode::Psubd => f.write_str("psubd"),
            Opcode::Psubq => f.write_str("psubq"),
            Opcode::Pmullw => f.write_str("pmullw"),
            Opcode::Addps => f.write_str("addps"),
            Opcode::Addpd => f.write_str("addpd"),
            Opcode::Subps => f.write_str("subps"),
            Opcode::Subpd => f.write_str("subpd"),
            Opcode::Mulps => f.write_str("mulps"),
            Opcode::Mulpd => f.write_str("mulpd"),
            Opcode::Divps => f.write_str("divps"),
            Opcode::Divpd => f.write_str("divpd"),
            Opcode::Sqrtps => f.write_str("sqrtps"),
            Opcode::Sqrtpd => f.write_str("sqrtpd"),

            Opcode::Movapd => f.write_str("movapd"),
            Opcode::Movupd => f.write_str("movupd"),
            Opcode::Movd => f.write_str("movd"),
            Opcode::Movq => f.write_str("movq"),
            Opcode::Addss => f.write_str("addss"),
            Opcode::Addsd => f.write_str("addsd"),
            Opcode::Subss => f.write_str("subss"),
            Opcode::Subsd => f.write_str("subsd"),
            Opcode::Mulss => f.write_str("mulss"),
            Opcode::Mulsd => f.write_str("mulsd"),
            Opcode::Divss => f.write_str("divss"),
            Opcode::Divsd => f.write_str("divsd"),
            Opcode::Sqrtss => f.write_str("sqrtss"),
            Opcode::Sqrtsd => f.write_str("sqrtsd"),
            Opcode::Minss => f.write_str("minss"),
            Opcode::Minsd => f.write_str("minsd"),
            Opcode::Maxss => f.write_str("maxss"),
            Opcode::Maxsd => f.write_str("maxsd"),
            Opcode::Ucomiss => f.write_str("ucomiss"),
            Opcode::Ucomisd => f.write_str("ucomisd"),
            Opcode::Andps => f.write_str("andps"),
            Opcode::Andpd => f.write_str("andpd"),
            Opcode::Orps => f.write_str("orps"),
            Opcode::Orpd => f.write_str("orpd"),
            Opcode::Xorps => f.write_str("xorps"),
            Opcode::Xorpd => f.write_str("xorpd"),
            Opcode::Roundss => f.write_str("roundss"),
            Opcode::Roundsd => f.write_str("roundsd"),
            Opcode::Cvtsi2ss => f.write_str("cvtsi2ss"),
            Opcode::Cvtsi2sd => f.write_str("cvtsi2sd"),
            Opcode::Cvttss2si => f.write_str("cvttss2si"),
            Opcode::Cvttsd2si => f.write_str("cvttsd2si"),
            Opcode::Cvtss2sd => f.write_str("cvtss2sd"),
            Opcode::Cvtsd2ss => f.write_str("cvtsd2ss"),
            Opcode::Jcc(c) => write!(f, "j{}", CC.get(usize::from(*c)).ok_or(fmt::Error)?),
            Opcode::Setcc(c) => write!(f, "set{}", CC.get(usize::from(*c)).ok_or(fmt::Error)?),
            Opcode::Cmovcc(c) => write!(f, "cmov{}", CC.get(usize::from(*c)).ok_or(fmt::Error)?),
        }
    }
}
impl fmt::Display for Register {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.class {
            RegisterClass::Rip => f.write_str("rip"),
            RegisterClass::Xmm => write!(f, "xmm{}", self.number),
            RegisterClass::Gpr => {
                if self.number >= 16 || !matches!(self.width, 8 | 16 | 32 | 64) {
                    return Err(fmt::Error);
                }
                let n = usize::from(self.number);
                if self.high_byte {
                    return f.write_str(
                        ["ah", "ch", "dh", "bh"]
                            .get(n.wrapping_sub(4))
                            .ok_or(fmt::Error)?,
                    );
                }
                if n >= 8 {
                    return write!(
                        f,
                        "r{}{}",
                        n,
                        match self.width {
                            8 => "b",
                            16 => "w",
                            32 => "d",
                            _ => "",
                        }
                    );
                }
                f.write_str(match self.width {
                    8 => ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil"][n],
                    16 => ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"][n],
                    32 => ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"][n],
                    _ => ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"][n],
                })
            }
        }
    }
}
fn symbol(f: &mut fmt::Formatter<'_>, name: &str) -> fmt::Result {
    if name == "." {
        return f.write_str(".");
    }
    f.write_str("\"")?;
    for b in name.bytes() {
        match b {
            b'"' | b'\\' => write!(f, "\\{}", b as char)?,
            b' '..=b'~' => write!(f, "{}", b as char)?,
            _ => write!(f, "\\{b:03o}")?,
        }
    }
    f.write_str("\"")
}
impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let e = &self.expression;
        if let Some(name) = &e.add {
            symbol(f, name)?;
            f.write_str(match self.modifier {
                Modifier::None => "",
                Modifier::Plt => "@PLT",
                Modifier::GotPcRel => "@GOTPCREL",
                Modifier::TpOff => "@TPOFF",
                Modifier::GotTpOff => "@GOTTPOFF",
                Modifier::DtpOff => "@DTPOFF",
            })?;
        }
        if let Some(name) = &e.subtract {
            f.write_str("-")?;
            symbol(f, name)?;
        }
        if e.add.is_none() && e.subtract.is_none() {
            write!(f, "{}", e.constant)
        } else if e.constant != 0 {
            write!(f, "{:+}", e.constant)
        } else {
            Ok(())
        }
    }
}
impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Register(r) => r.fmt(f),
            Operand::Immediate(a) => a.fmt(f),
            Operand::Memory(m) => {
                if m.width != 0 {
                    f.write_str(match m.width {
                        8 => "byte ptr ",
                        16 => "word ptr ",
                        32 => "dword ptr ",
                        64 => "qword ptr ",
                        128 => "xmmword ptr ",
                        _ => return Err(fmt::Error),
                    })?;
                }
                if let Some(s) = m.segment {
                    f.write_str(match s {
                        0x64 => "fs:",
                        0x65 => "gs:",
                        _ => return Err(fmt::Error),
                    })?;
                }
                f.write_str("[")?;
                let mut any = false;
                if let Some(r) = m.base {
                    write!(f, "{r}")?;
                    any = true;
                }
                if let Some(r) = m.index {
                    if any {
                        f.write_str(" + ")?;
                    }
                    write!(f, "{r} * {}", m.scale)?;
                    any = true;
                }
                if m.displacement.expression.add.is_some()
                    || m.displacement.expression.subtract.is_some()
                    || m.displacement.expression.constant != 0
                    || !any
                {
                    if any {
                        f.write_str(" + ")?;
                    }
                    write!(f, "{}", m.displacement)?;
                }
                f.write_str("]")
            }
        }
    }
}
impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.lock {
            f.write_str("lock ")?;
        }
        write!(f, "{}", self.opcode)?;
        for (i, o) in self.operands.iter().enumerate() {
            f.write_str(if i == 0 { "\t" } else { ", " })?;
            if matches!(o,Operand::Immediate(a) if a.expression.add.is_some()||a.expression.subtract.is_some())
                && !matches!(self.opcode, Opcode::Call | Opcode::Jmp | Opcode::Jcc(_))
            {
                f.write_str("offset ")?;
            }
            write!(f, "{o}")?;
        }
        Ok(())
    }
}
