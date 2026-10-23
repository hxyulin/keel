//! Legacy post-SSE2 forms. Shared rm emission owns addressing and relocations;
//! no instruction substitution is permitted (FP exceptions/NaNs remain native).
use super::*;
use keel_asm_ir::x86::{V2Form, V2Opcode};

fn imm8(e: &mut Encoding<'_>, o: &Operand) -> Result<(), String> {
    let n = constant(immediate(o)?)
        .filter(|n| (0..=255).contains(n))
        .ok_or("v2 control requires a constant imm8 in 0..255")?;
    e.number(n, 1);
    Ok(())
}
fn memory(o: &Operand, bits: u16) -> bool {
    matches!(o, M(m) if m.width == 0 || m.width == bits)
}
fn integer(o: &Operand, bits: u16) -> bool {
    (gpr(o) && width(o) == bits) || memory(o, bits)
}

pub(super) fn encode<'a>(
    e: &mut Encoding<'a>,
    i: &'a Instruction,
    op: V2Opcode,
) -> Result<(), String> {
    let s = op.spec();
    let o = i.operands.as_slice();
    let invalid = || format!("unsupported operands for {}", s.mnemonic);
    let code = [0x0f, s.map, s.byte];
    let code = if s.map == 0x0f { &code[1..] } else { &code[..] };
    match s.form {
        V2Form::Nullary => {
            if !o.is_empty() {
                return Err(invalid());
            }
            e.bytes.push(s.byte);
        }
        V2Form::Cx16 => {
            let [a] = o else { return Err(invalid()) };
            if !memory(a, 128) {
                return Err(invalid());
            }
            // Implicit RDX:RAX compare and RCX:RBX replacement; REX.W is required.
            e.rm(0, code, 64, 1, a, None)?;
        }
        V2Form::Fisttp => {
            let [M(m)] = o else { return Err(invalid()) };
            let byte = match m.width {
                16 => 0xdf,
                32 => 0xdb,
                64 => 0xdd,
                _ => return Err("fisttp needs explicit 16/32/64-bit memory width".into()),
            };
            e.rm(0, &[byte], 32, 1, &o[0], None)?;
        }
        V2Form::Crc32 => {
            let [a, b] = o else { return Err(invalid()) };
            if !gpr(a) || !matches!(width(a), 32 | 64) {
                return Err(invalid());
            }
            let bits = width(b);
            if !matches!(bits, 8 | 16 | 32 | 64)
                || !integer(b, bits)
                || (width(a) == 64 && !matches!(bits, 8 | 64))
                || (width(a) == 32 && bits == 64)
            {
                return Err(invalid());
            }
            // Word source adds 66, byte/word/dword results never set REX.W;
            // the r64,r/m8 variant does set it. rm rejects AH+REX combinations.
            let encoding_bits = if bits == 16 { 16 } else { width(a) };
            let start = e.bytes.len();
            e.rm(
                0xf2,
                &[0x0f, 0x38, if bits == 8 { 0xf0 } else { 0xf1 }],
                encoding_bits,
                reg(a)?.number,
                b,
                None,
            )?;
            if bits == 16 {
                // LLVM/GAS canonicalize 66 before F2, after any segment prefix.
                let prefix = start + usize::from(matches!(b, M(m) if m.segment.is_some()));
                e.bytes.swap(prefix, prefix + 1);
            }
        }
        V2Form::Extract => {
            let [a, b, control] = o else {
                return Err(invalid());
            };
            let rb = if s.memory_bits <= 16 {
                32
            } else {
                s.memory_bits
            };
            if !xmm(b) || !((gpr(a) && width(a) == rb) || memory(a, s.memory_bits)) {
                return Err(invalid());
            }
            e.rm(
                s.prefix,
                code,
                if rb == 64 { 64 } else { 32 },
                reg(b)?.number,
                a,
                None,
            )?;
            imm8(e, control)?;
        }
        V2Form::Insert => {
            let [a, b, control] = o else {
                return Err(invalid());
            };
            let rb = if s.memory_bits <= 16 {
                32
            } else {
                s.memory_bits
            };
            if !xmm(a) || !((gpr(b) && width(b) == rb) || memory(b, s.memory_bits)) {
                return Err(invalid());
            }
            e.rm(
                s.prefix,
                code,
                if rb == 64 { 64 } else { 32 },
                reg(a)?.number,
                b,
                None,
            )?;
            imm8(e, control)?;
        }
        V2Form::Binary | V2Form::Control | V2Form::Control64 | V2Form::Memory | V2Form::Blend => {
            let [a, b, ..] = o else { return Err(invalid()) };
            let mmx = matches!(a, R(r) if r.class == RegisterClass::Mmx);
            if !xmm(a) && !(mmx && op.supports_mmx()) {
                return Err(invalid());
            }
            match s.form {
                V2Form::Control | V2Form::Control64 if o.len() == 3 => {}
                V2Form::Blend if o.len() == 3 => {
                    if !matches!(&o[2], R(r) if r.class == RegisterClass::Xmm && r.number == 0) {
                        return Err("legacy variable blend mask must be xmm0".into());
                    }
                }
                V2Form::Control | V2Form::Control64 => return Err(invalid()),
                _ if o.len() != 2 => return Err(invalid()),
                _ => {}
            }
            if s.form == V2Form::Memory && !memory(b, s.memory_bits) {
                return Err(invalid());
            }
            if mmx {
                if !matches!(b, R(r) if r.class == RegisterClass::Mmx) && !memory(b, 64) {
                    return Err(invalid());
                }
            } else {
                check_xmm(b, s.memory_bits)?;
            }
            e.rm(
                if mmx { 0 } else { s.prefix },
                code,
                if s.form == V2Form::Control64 { 64 } else { 32 },
                reg(a)?.number,
                b,
                None,
            )?;
            if matches!(s.form, V2Form::Control | V2Form::Control64) {
                imm8(e, &o[2])?;
            }
        }
    }
    Ok(())
}
