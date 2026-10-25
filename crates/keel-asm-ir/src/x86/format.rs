//! Intel syntax serialization for the shared x86 instruction API.
//!
//! Every piece is written straight to the sink: a `String` sink compiles to
//! plain appends, without the general formatting machinery per operand.
use super::*;
use crate::directive::write_int;
use std::fmt::{self, Write};
include!("isa/baseline_mnemonics.rs");

impl Opcode {
    /// The mnemonic, split into a fixed part and a condition-code suffix.
    fn mnemonic(self) -> (&'static str, Option<u8>) {
        let name = baseline_mnemonic!(self, {
            Opcode::Core(op) => op.mnemonic(),
            Opcode::X87(op) => op.mnemonic(),
            Opcode::Fixed(op) => op.mnemonic(),
            Opcode::Extension(op) => op.mnemonic(),
            Opcode::V2(op) => op.spec().mnemonic,
            Opcode::V3(op) => op.mnemonic(),
            Opcode::Jcc(c) => return ("j", Some(c)),
            Opcode::Setcc(c) => return ("set", Some(c)),
            Opcode::Cmovcc(c) => return ("cmov", Some(c)),
        });
        (name, None)
    }
    fn write_to<W: Write + ?Sized>(self, w: &mut W) -> fmt::Result {
        if let Opcode::Core(op) = self {
            return op.write_to(w);
        }
        let (name, cc) = self.mnemonic();
        w.write_str(name)?;
        match cc {
            Some(c) => w.write_str(isa::condition_code_name(c).ok_or(fmt::Error)?),
            None => Ok(()),
        }
    }
}
impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_to(f)
    }
}
include!("isa/register_format.rs");
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
