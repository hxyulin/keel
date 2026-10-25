//! Long-mode byte decoding from the same generated forms as the assembler.
use keel_asm_ir::x86::isa::{self, IsaForm, IsaOperand};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    Register(String),
    Immediate(i128),
    Target(u64),
    Memory {
        bits: u16,
        address_bits: u16,
        base: Option<String>,
        index: Option<String>,
        scale: u8,
        displacement: i64,
        segment: Option<&'static str>,
    },
}
#[derive(Clone, Debug)]
pub struct Instruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub form: &'static IsaForm,
    pub operand_size: u16,
    pub operands: Vec<Operand>,
    pub lock: bool,
    pub repeat: u8,
}
impl Instruction {
    pub fn assembly(&self) -> String {
        let mut text = String::new();
        if self.lock {
            text.push_str("lock ");
        }
        if self.repeat == 0xf3 {
            text.push_str("rep ");
        }
        if self.repeat == 0xf2 {
            text.push_str("repne ");
        }
        text.push_str(if self.form.far {
            match self.form.mnemonic {
                "call" => "callf",
                "jmp" => "jmpf",
                "ret" => match self.operand_size {
                    16 => "retfw",
                    64 => "retfq",
                    _ => "retf",
                },
                name => name,
            }
        } else {
            isa::fixed_width_mnemonic(self.form.mnemonic, self.operand_size)
                .unwrap_or(self.form.mnemonic)
        });
        let operands: Vec<_> = self
            .operands
            .iter()
            .map(|o| match o {
                Operand::Register(r) => r.clone(),
                Operand::Immediate(n) => format!("{n}"),
                Operand::Target(n) => format!("0x{n:x}"),
                Operand::Memory {
                    bits,
                    base,
                    index,
                    scale,
                    displacement,
                    segment,
                    ..
                } => {
                    let width = match bits {
                        8 => "byte",
                        16 => "word",
                        32 => "dword",
                        48 => "fword",
                        64 => "qword",
                        80 => "tbyte",
                        128 => "xmmword",
                        256 => "ymmword",
                        _ => "",
                    };
                    let mut address = base.clone().unwrap_or_default();
                    if let Some(index) = index {
                        if !address.is_empty() {
                            address.push_str(" + ");
                        }
                        address.push_str(index);
                        if *scale != 1 {
                            address.push_str(&format!("*{scale}"));
                        }
                    }
                    if *displacement != 0 || address.is_empty() {
                        if !address.is_empty() {
                            address.push_str(if *displacement < 0 { " - " } else { " + " });
                            address.push_str(&displacement.unsigned_abs().to_string());
                        } else {
                            address.push_str(&displacement.to_string());
                        }
                    }
                    let segment = segment.map_or(String::new(), |s| format!("{s}:"));
                    format!(
                        "{}{}{segment}[{address}]",
                        width,
                        if width.is_empty() { "" } else { " ptr " }
                    )
                }
            })
            .collect();
        if !operands.is_empty() {
            text.push(' ');
            text.push_str(&operands.join(", "));
        }
        text
    }
}

#[derive(Clone, Copy)]
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        if self.pos >= 15 {
            return None;
        }
        let value = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(value)
    }
    fn number(&mut self, n: usize, signed: bool) -> Option<i128> {
        if n == 0 || n > 8 {
            return None;
        }
        let mut value = 0u64;
        for shift in 0..n {
            value |= u64::from(self.byte()?) << (shift * 8);
        }
        Some(if signed {
            ((value as i64) << (64 - n * 8) >> (64 - n * 8)) as i128
        } else {
            value as i128
        })
    }
}
#[derive(Clone, Copy, Default)]
struct Prefix {
    p66: bool,
    address32: bool,
    rex: u8,
    rep: u8,
    lock: bool,
    segment: Option<&'static str>,
    vex: bool,
    map: u8,
    l: bool,
    source: u8,
}
impl Prefix {
    fn w(self) -> bool {
        self.rex & 8 != 0
    }
    fn b(self) -> u8 {
        self.rex & 1
    }
    fn x(self) -> u8 {
        (self.rex >> 1) & 1
    }
    fn r(self) -> u8 {
        (self.rex >> 2) & 1
    }
    fn address(self) -> u16 {
        if self.address32 { 32 } else { 64 }
    }
}

fn prefixes(r: &mut Reader<'_>) -> Option<Prefix> {
    let mut p = Prefix::default();
    loop {
        match *r.bytes.get(r.pos)? {
            0x66 => {
                p.p66 = true;
                p.rex = 0;
            }
            0x67 => {
                p.address32 = true;
                p.rex = 0;
            }
            0xf0 => {
                p.lock = true;
                p.rex = 0;
            }
            n @ (0xf2 | 0xf3) => {
                p.rep = n;
                p.rex = 0;
            }
            n @ (0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65) => {
                p.segment = Some(match n {
                    0x26 => "es",
                    0x2e => "cs",
                    0x36 => "ss",
                    0x3e => "ds",
                    0x64 => "fs",
                    _ => "gs",
                });
                p.rex = 0;
            }
            n @ 0x40..=0x4f => p.rex = n,
            _ => break,
        }
        r.byte()?;
    }
    let first = *r.bytes.get(r.pos)?;
    if first == 0xc4 || first == 0xc5 {
        if p.rex != 0 || p.p66 || p.rep != 0 || p.lock {
            return None;
        }
        r.byte()?;
        p.vex = true;
        let a = r.byte()?;
        let b = if first == 0xc5 {
            p.map = 1;
            p.rex = (!a >> 5) & 4;
            a
        } else {
            p.map = a & 31;
            p.rex = (!a >> 5) & 7;
            r.byte()?
        };
        p.rex |= (b >> 4) & 8;
        p.source = (!b >> 3) & 15;
        p.l = b & 4 != 0;
        match b & 3 {
            1 => p.p66 = true,
            2 => p.rep = 0xf3,
            3 => p.rep = 0xf2,
            _ => {}
        }
    }
    Some(p)
}
fn gpr(n: u8, bits: u16, rex: bool) -> String {
    // Encoded GPR fields have at most four bits in the current legacy/VEX import.
    keel_asm_ir::x86::Register {
        class: keel_asm_ir::x86::RegisterClass::Gpr,
        number: n,
        width: if matches!(bits, 8 | 16 | 32) {
            bits
        } else {
            64
        },
        high_byte: bits == 8 && !rex && (4..8).contains(&n),
    }
    .to_string()
}
fn register(spec: IsaOperand, n: u8, size: u16, p: Prefix) -> Option<String> {
    Some(match spec.kind {
        "gpr8" => gpr(n, 8, p.rex != 0 || p.vex),
        "gpr16" => gpr(n, 16, true),
        "gpr32" => gpr(n, 32, true),
        "gpr64" => gpr(n, 64, true),
        "gpr16_32_64" => gpr(n, size, true),
        "gpr32_32_64" => gpr(n, if size == 64 { 64 } else { 32 }, true),
        "gpr16_32_32" => gpr(n, if size == 16 { 16 } else { 32 }, true),
        "gpr_asz" => gpr(n, p.address(), true),
        "implicit_reg" => match spec.register {
            "osz_ax" => gpr(0, size, true),
            "asz_ax" => gpr(0, p.address(), true),
            other => other.into(),
        },
        kind => keel_asm_ir::x86::isa::register_name(kind, n)?,
    })
}
#[derive(Clone, Copy)]
struct Memory {
    base: Option<u8>,
    index: Option<u8>,
    scale: u8,
    disp: i64,
    ip: bool,
    sib: bool,
}
fn memory(r: &mut Reader<'_>, m: u8, p: Prefix) -> Option<Memory> {
    let mode = m >> 6;
    let rm = m & 7;
    let mut out = Memory {
        base: Some(rm + 8 * p.b()),
        index: None,
        scale: 1,
        disp: 0,
        ip: false,
        sib: false,
    };
    if rm == 4 {
        let sib = r.byte()?;
        out.sib = true;
        out.scale = 1 << (sib >> 6);
        out.index = Some(((sib >> 3) & 7) + 8 * p.x());
        out.base = Some((sib & 7) + 8 * p.b());
        if mode == 0 && sib & 7 == 5 {
            out.base = None;
            out.disp = r.number(4, true)? as i64;
        }
    } else if mode == 0 && rm == 5 {
        out.base = None;
        out.ip = true;
        out.disp = r.number(4, true)? as i64;
    }
    if mode == 1 {
        out.disp = r.number(1, true)? as i64;
    } else if mode == 2 {
        out.disp = r.number(4, true)? as i64;
    }
    Some(out)
}

fn decode_form(
    mut r: Reader<'_>,
    p: Prefix,
    byte: u8,
    f: &'static IsaForm,
    address: u64,
) -> Option<Instruction> {
    if f.vex != p.vex || f.map != p.map || f.byte != byte {
        return None;
    }
    if (f.w >= 0 && (f.w == 1) != p.w()) || (f.b >= 0 && f.b as u8 != p.b()) {
        return None;
    }
    if f.vector != 0 && (f.vector == 256) != p.l {
        return None;
    }
    if f.prefix == 0 && (p.rep != 0 || p.p66) {
        return None;
    }
    if f.prefix == 0x66 && !p.p66
        || f.prefix == 0xf2 && p.rep != 0xf2
        || f.prefix == 0xf3 && p.rep != 0xf3
        || f.prefix == -2 && p.rep == 0xf3
    {
        return None;
    }
    if p.lock && !f.lock {
        return None;
    }
    if f.group1 >= 0 && (f.group1 == 1) != (p.rep != 0) {
        return None;
    }
    let size = f.operand_size(p.p66, p.w());
    if !f.allows_size(size) || f.addresses & if p.address32 { 2 } else { 4 } == 0 {
        return None;
    }
    let m = if f.mod_mask != 0 {
        Some(r.byte()?)
    } else {
        None
    };
    if let Some(m) = m {
        if (!f.ignore_mod && f.mod_mask & (1 << (m >> 6)) == 0)
            || f.reg_mask & (1 << ((m >> 3) & 7)) == 0
            || f.rm_mask & (1 << (m & 7)) == 0
        {
            return None;
        }
    }
    let mem = if let Some(m) = m {
        if m >> 6 != 3 && !f.ignore_mod {
            Some(memory(&mut r, m, p)?)
        } else {
            None
        }
    } else {
        None
    };
    let mut operands = Vec::new();
    let mut targets = Vec::new();
    for spec in f.operands {
        let n = match spec.encoding {
            "modrm_reg" => ((m? >> 3) & 7) + p.r() * 8,
            "modrm_rm" => (m? & 7) + p.b() * 8,
            "opcode" => (byte & 7) + p.b() * 8,
            "ndsndd" => p.source,
            "is4" => r.byte()? >> 4,
            _ => 0,
        };
        let operand = match spec.kind {
            "mem" | "mem_vsibx" | "mem_vsiby" | "agen" | "agen_norel" => {
                let mem = mem?;
                let vsib = spec.kind.starts_with("mem_vsib");
                let index = mem.index.filter(|n| vsib || *n != 4).map(|n| {
                    if vsib {
                        format!(
                            "{}{n}",
                            if spec.kind == "mem_vsibx" {
                                "xmm"
                            } else {
                                "ymm"
                            }
                        )
                    } else {
                        gpr(n, p.address(), true)
                    }
                });
                Operand::Memory {
                    bits: spec.bytes(size) * 8,
                    address_bits: p.address(),
                    base: if mem.ip {
                        Some(if p.address32 { "eip" } else { "rip" }.into())
                    } else {
                        mem.base.map(|n| gpr(n, p.address(), true))
                    },
                    index,
                    scale: mem.scale,
                    displacement: mem.disp,
                    segment: p.segment,
                }
            }
            "moffs" => Operand::Memory {
                bits: spec.bytes(size) * 8,
                address_bits: p.address(),
                base: None,
                index: None,
                scale: 1,
                displacement: r.number((p.address() / 8) as usize, false)? as i64,
                segment: p.segment,
            },
            "imm" => Operand::Immediate(
                r.number(spec.bytes(size) as usize, spec.encoding.starts_with('s'))?,
            ),
            "implicit_imm1" => Operand::Immediate(1),
            "rel" => {
                targets.push(operands.len());
                Operand::Immediate(r.number(spec.bytes(size) as usize, true)?)
            }
            _ => Operand::Register(register(*spec, n, size, p)?),
        };
        operands.push(operand);
    }
    if !f.operands.iter().any(|o| o.encoding == "ndsndd") && p.vex && p.source != 0 {
        return None;
    }
    for index in targets {
        let Operand::Immediate(delta) = operands[index] else {
            unreachable!()
        };
        operands[index] = Operand::Target(
            address
                .wrapping_add(r.pos as u64)
                .wrapping_add_signed(delta as i64),
        );
    }
    Some(Instruction {
        address,
        bytes: r.bytes[..r.pos].to_vec(),
        form: f,
        operand_size: size,
        operands,
        lock: p.lock,
        repeat: if (p.rep == 0xf3 && f.rep) || (p.rep == 0xf2 && f.repne) {
            p.rep
        } else {
            0
        },
    })
}

/// Decode one instruction, consuming at most 15 bytes. `None` means invalid,
/// truncated or outside the imported forms; it does not imply a known length.
pub fn decode(bytes: &[u8], address: u64) -> Option<Instruction> {
    let mut r = Reader { bytes, pos: 0 };
    let mut p = prefixes(&mut r)?;
    let mut byte = r.byte()?;
    if !p.vex && byte == 0x0f {
        p.map = 1;
        byte = r.byte()?;
        if byte == 0x38 || byte == 0x3a {
            p.map = if byte == 0x38 { 2 } else { 3 };
            byte = r.byte()?;
        }
    }
    let mut best: Option<(u8, Instruction)> = None;
    for f in isa::decode_forms(p.vex, p.map, byte) {
        if !f.decode || f.vex != p.vex || f.map != p.map || f.byte != byte {
            continue;
        }
        if let Some(instruction) = decode_form(r, p, byte, f, address) {
            let score = u8::from(f.prefix > 0) * 2 + u8::from(f.prefix == 0);
            if best.as_ref().is_none_or(|(old, _)| score > *old) {
                best = Some((score, instruction));
            }
        }
    }
    best.map(|(_, instruction)| instruction)
}
