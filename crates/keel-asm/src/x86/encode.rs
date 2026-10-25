mod tables;

use Operand::{Immediate as I, Memory as M, Register as R};
use keel_asm_ir::x86::{
    Address, Instruction, Memory, Modifier, Opcode::*, Operand, Register, RegisterClass,
};

/// A field for the operand address it borrows, which keeps its own
/// constant: `adjust` is added to it (PC-relative fields count from the
/// end of the whole instruction), so no symbol name is copied.
#[derive(Debug)]
pub(super) struct Fixup<'a> {
    pub offset: usize,
    pub width: u8,
    pub address: &'a Address,
    pub adjust: i128,
    pub kind: u32,
}
#[derive(Default)]
pub(super) struct Encoding<'a> {
    // x86 instructions are at most 15 bytes: never allocate.
    pub bytes: smallvec::SmallVec<[u8; 16]>,
    // Rarely more than one symbolic operand.
    pub fixups: smallvec::SmallVec<[Fixup<'a>; 1]>,
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
fn constant(a: &Address) -> Option<i128> {
    if a.expression.add.is_none() && a.expression.subtract.is_none() && a.modifier == Modifier::None
    {
        Some(a.expression.constant)
    } else {
        None
    }
}
impl<'a> Encoding<'a> {
    fn number(&mut self, n: i128, bytes: u8) {
        self.bytes
            .extend_from_slice(&n.to_le_bytes()[..usize::from(bytes)]);
    }
    fn imm(&mut self, a: &'a Address, bytes: u8, signed: bool) -> Result<(), String> {
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
                address: a,
                adjust: 0,
                kind,
            });
            self.number(0, bytes);
        }
        Ok(())
    }
    fn branch(&mut self, a: &'a Address, plt: bool, width: u8) -> Result<(), String> {
        if !matches!(a.modifier, Modifier::None | Modifier::Plt) {
            return Err("invalid branch relocation modifier".into());
        }
        self.fixups.push(Fixup {
            offset: self.bytes.len(),
            width,
            address: a,
            adjust: 0,
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
        rm: &'a Operand,
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
        if matches!(rm,M(m) if m.base.into_iter().chain(m.index).any(|r| r.width==32)) {
            self.bytes.push(0x67);
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
    fn memory(&mut self, field: u8, m: &'a Memory) -> Result<(), String> {
        let address32 = m.base.into_iter().chain(m.index).any(|r| r.width == 32);
        let a = &m.displacement;
        let symbolic = a.expression.add.is_some()
            || a.expression.subtract.is_some()
            || a.modifier != Modifier::None;
        let raw_displacement = a.expression.constant;
        // Address arithmetic wraps at 32 bits. Canonicalize its signed view
        // before choosing disp8, while preserving symbolic relocation addends.
        let d = if address32 && !symbolic && (0..=u32::MAX as i128).contains(&raw_displacement) {
            raw_displacement as u32 as i32 as i128
        } else {
            raw_displacement
        };
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
                    address: a,
                    adjust: 0,
                    kind,
                });
                self.number(0, 4);
            } else {
                // Numeric IP-relative displacements use the selected address size.
                self.imm(a, 4, !address32)?;
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
            self.imm(a, 4, !address32)?
        }
        Ok(())
    }
}

impl<'a> Encoding<'a> {
    pub(super) fn vex_rm_bits(
        &mut self,
        map: u8,
        pp: u8,
        byte: u8,
        field: u8,
        source: Option<u8>,
        rm: &'a Operand,
        l: bool,
        w: bool,
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
        let v = source.map_or(0x78, |r| ((!r) & 15) << 3) | (u8::from(l) << 2) | (u8::from(w) << 7);
        let r = ((!field >> 3) & 1) << 7;
        if map == 1 && b == 0 && x == 0 && !w {
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

pub(super) fn encode(i: &Instruction, short: bool) -> Result<Encoding<'_>, String> {
    validate(i)?;
    tables::encode(i, short)
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
                RegisterClass::Mmx => r.number < 8 && r.width == 64 && !r.high_byte,
                RegisterClass::Ymm => r.width == 256 && !r.high_byte,
                RegisterClass::X87 => r.number < 8 && r.width == 80 && !r.high_byte,
                RegisterClass::Segment => r.number < 6 && r.width == 16 && !r.high_byte,
                RegisterClass::Rip => r.number == 0 && matches!(r.width, 32 | 64) && !r.high_byte,
            }
    }
    let vsib = if i.opcode.required_features().avx {
        keel_asm_ir::x86::V3_FORMS
            .iter()
            .find(|f| f.matches(i))
            .map_or(0, |f| f.vsib)
    } else {
        0
    };
    for o in &i.operands {
        match o {
            R(r) if !register(*r) || r.class == RegisterClass::Rip => {
                return Err("invalid operand register".into());
            }
            M(m) => {
                // GOTTPOFF is an instruction-sensitive TLS relaxation, not a
                // generic SIMD load/store relocation. GNU rejects these forms.
                if m.displacement.modifier == Modifier::GotTpOff
                    && (tables::vector_or_state(i)
                        || i.operands
                            .iter()
                            .any(|o| xmm(o) || matches!(o, R(r) if matches!(r.class, RegisterClass::Mmx | RegisterClass::Ymm)))
                        || matches!(i.opcode, Ldmxcsr | Stmxcsr | V3(_)))
                {
                    return Err("GOTTPOFF is not supported on SIMD/state or v3 operands".into());
                }
                if !(matches!(m.width, 0 | 8 | 16 | 32 | 64 | 128 | 256)
                    || matches!(i.opcode, Fld | Fstp) && m.width == 80
                    || matches!(
                        i.opcode,
                        Core(
                            keel_asm_ir::x86::CoreOpcode::Lfs
                                | keel_asm_ir::x86::CoreOpcode::Lgs
                                | keel_asm_ir::x86::CoreOpcode::Lss
                        )
                    ) && matches!(m.width, 48 | 80)
                    || matches!(
                        i.opcode,
                        Core(
                            keel_asm_ir::x86::CoreOpcode::Sgdt
                                | keel_asm_ir::x86::CoreOpcode::Sidt
                                | keel_asm_ir::x86::CoreOpcode::Lgdt
                                | keel_asm_ir::x86::CoreOpcode::Lidt
                        )
                    ) && m.width == 80
                    || matches!(
                        i.opcode,
                        Core(
                            keel_asm_ir::x86::CoreOpcode::CallFar
                                | keel_asm_ir::x86::CoreOpcode::JmpFar
                        )
                    ) && matches!(m.width, 48 | 80)
                    || matches!(
                        i.opcode,
                        X87(keel_asm_ir::x86::X87Opcode::Fbld | keel_asm_ir::x86::X87Opcode::Fbstp)
                    ) && m.width == 80)
                    || !matches!(m.scale, 1 | 2 | 4 | 8)
                    || m.segment.is_some_and(|s| !matches!(s, 0x64 | 0x65))
                {
                    return Err("invalid memory width/scale/segment".into());
                }
                let address32 = m.base.into_iter().chain(m.index).any(|r| r.width == 32);
                let features = i.opcode.required_features();
                // VEX/VSIB encoders have their own prefix paths. Keep their
                // existing address-size contract until those paths support 67.
                if address32 && m.displacement.modifier == Modifier::GotTpOff {
                    return Err("GOTTPOFF relaxation requires a 64-bit address".into());
                }
                if address32
                    && (features.avx || features.bmi1 || features.bmi2 || matches!(i.opcode, V3(_)))
                {
                    return Err("32-bit addresses are not supported on VEX/v3 forms".into());
                }
                if let (Some(base), Some(index)) = (m.base, m.index) {
                    if index.class == RegisterClass::Gpr && base.width != index.width {
                        return Err("mixed address register widths".into());
                    }
                }
                for (is_index, r) in [(false, m.base), (true, m.index)]
                    .into_iter()
                    .filter_map(|(index, r)| r.map(|r| (index, r)))
                {
                    if is_index
                        && vsib != 0
                        && register(r)
                        && r.width == vsib
                        && matches!(r.class, RegisterClass::Xmm | RegisterClass::Ymm)
                    {
                        continue;
                    }
                    if !register(r)
                        || !matches!(r.width, 32 | 64)
                        || !matches!(r.class, RegisterClass::Gpr | RegisterClass::Rip)
                    {
                        return Err("invalid address register".into());
                    }
                }
                if m.index.is_some_and(|r| {
                    r.class == RegisterClass::Rip || r.class == RegisterClass::Gpr && r.number == 4
                }) || (m.base.is_some_and(|r| r.class == RegisterClass::Rip)
                    && m.index.is_some())
                {
                    return Err("invalid address index".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
