//! AVX-512F assembly forms and EVEX decorators. Compiler semantics are separate.
use super::*;
use std::fmt::{self, Write};
include!("isa/evex_opcodes.rs");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rounding {
    Nearest,
    Down,
    Up,
    Zero,
    Sae,
}
impl Rounding {
    pub fn bits(self) -> u8 {
        match self {
            Self::Nearest | Self::Sae => 0,
            Self::Down => 1,
            Self::Up => 2,
            Self::Zero => 3,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Nearest => "rn-sae",
            Self::Down => "rd-sae",
            Self::Up => "ru-sae",
            Self::Zero => "rz-sae",
            Self::Sae => "sae",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EvexOptions {
    pub mask: Option<u8>,
    /// Explicit address-size override, including base-free VSIB addressing.
    pub address32: bool,
    pub zero: bool,
    /// Zero means no embedded broadcast; otherwise the count in {1toN}.
    pub broadcast: u8,
    pub rounding: Option<Rounding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EvexOpcode {
    pub kind: EvexMnemonic,
    pub options: EvexOptions,
}
impl EvexOpcode {
    pub fn parse(name: &str) -> Option<Self> {
        Some(Self {
            kind: EvexMnemonic::parse(name)?,
            options: EvexOptions::default(),
        })
    }
    pub const fn mnemonic(self) -> &'static str {
        self.kind.mnemonic()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct EvexForm {
    pub form: isa::IsaForm,
    pub functionality: &'static str,
    pub tuple: &'static str,
    pub element: u16,
    pub mask: &'static str,
    pub zero: bool,
    pub required: Features,
    /// -1 means either value; otherwise the imported EVEX.b constraint.
    pub evex_b: i8,
    pub ll_mask: u8,
}
impl EvexForm {
    pub fn requirements(self) -> Features {
        self.required
    }
    pub fn displacement_scale(self, broadcast: bool) -> u16 {
        let vector = self.form.vector / 8;
        let element = self.element / 8;
        if broadcast {
            return element;
        }
        match self.tuple {
            "fv" | "fvm" => vector,
            "hv" | "hvm" => vector / 2,
            "qvm" => vector / 4,
            "ovm" => vector / 8,
            "t1s" | "t1f" | "gscat" => element,
            "t2" => element * 2,
            "t4" => element * 4,
            "t8" => element * 8,
            "m128" => 16,
            "dup" => {
                if self.form.vector == 128 {
                    8
                } else {
                    vector
                }
            }
            _ => 1,
        }
    }
    pub fn memory_bytes(self, spec: isa::IsaOperand, broadcast: bool) -> u16 {
        if broadcast {
            self.element / 8
        } else if spec.bytes(32) != 0 {
            spec.bytes(32)
        } else {
            self.displacement_scale(false)
        }
    }
    pub fn matches(self, i: &Instruction) -> bool {
        let Opcode::Evex(op) = i.opcode else {
            return false;
        };
        let options = op.options;
        let f = self.form;
        if op.mnemonic() != f.mnemonic
            || i.lock
            || i.operands.len() != f.operands.len()
            || options.mask.is_some_and(|n| n == 0 || n > 7)
            || self.mask == "forbidden" && options.mask.is_some()
            || self.mask == "required" && options.mask.is_none()
            || options.zero && (!self.zero || options.mask.is_none())
        {
            return false;
        }
        let b = options.broadcast != 0 || options.rounding.is_some();
        if self.evex_b >= 0 && (self.evex_b == 1) != b {
            return false;
        }
        if options.rounding.is_some_and(|r| match r {
            Rounding::Sae => self.functionality != "sae",
            _ => self.functionality != "rc",
        }) || options.broadcast != 0 && self.functionality != "bc"
        {
            return false;
        }
        if f.vex
            && (EvexOptions {
                address32: false,
                ..options
            }) != EvexOptions::default()
        {
            return false;
        }
        for (spec, operand) in f.operands.iter().zip(&i.operands) {
            match operand {
                Operand::Register(r) => {
                    if !r.is_valid() || r.high_byte {
                        return false;
                    }
                    let class = match spec.kind {
                        "xmm" => RegisterClass::Xmm,
                        "ymm" => RegisterClass::Ymm,
                        "zmm" => RegisterClass::Zmm,
                        "mask" => RegisterClass::Mask,
                        "gpr32" | "gpr64" => RegisterClass::Gpr,
                        _ => return false,
                    };
                    let width = match spec.kind {
                        "gpr32" => 32,
                        "gpr64" => 64,
                        "xmm" => 128,
                        "ymm" => 256,
                        "zmm" => 512,
                        "mask" => 64,
                        _ => return false,
                    };
                    if r.class != class || r.width != width {
                        return false;
                    }
                }
                Operand::Immediate(a) => {
                    if spec.kind != "imm"
                        || a.modifier != Modifier::None
                        || a.expression.add.is_some()
                        || a.expression.subtract.is_some()
                        || !(0..=255).contains(&a.expression.constant)
                    {
                        return false;
                    }
                }
                Operand::Memory(m) => {
                    if options.address32
                        && m.base
                            .into_iter()
                            .chain(m.index)
                            .any(|r| r.class == RegisterClass::Gpr && r.width != 32)
                    {
                        return false;
                    }
                    if m.displacement.modifier == Modifier::GotTpOff {
                        return false;
                    }
                    if !matches!(spec.kind, "mem" | "mem_vsibx" | "mem_vsiby" | "mem_vsibz")
                        || !matches!(m.scale, 1 | 2 | 4 | 8)
                        || m.segment.is_some_and(|s| !matches!(s, 0x64 | 0x65))
                    {
                        return false;
                    }
                    let bytes = self.memory_bytes(*spec, options.broadcast != 0);
                    if m.width != 0 && m.width != bytes * 8 {
                        return false;
                    }
                    if options.broadcast != 0
                        && (self.element == 0
                            || u16::from(options.broadcast)
                                != self.memory_bytes(*spec, false) * 8 / self.element)
                    {
                        return false;
                    }
                    if let Some(base) = m.base
                        && (!base.is_valid()
                            || !matches!(base.class, RegisterClass::Gpr | RegisterClass::Rip)
                            || !matches!(base.width, 32 | 64))
                    {
                        return false;
                    }
                    let vsib = match spec.kind {
                        "mem_vsibx" => Some(RegisterClass::Xmm),
                        "mem_vsiby" => Some(RegisterClass::Ymm),
                        "mem_vsibz" => Some(RegisterClass::Zmm),
                        _ => None,
                    };
                    match (vsib, m.index) {
                        (Some(class), Some(index)) if index.class == class && index.is_valid() => {
                            if m.base.is_some_and(|r| r.class == RegisterClass::Rip) {
                                return false;
                            }
                            if f.gather && i.operands.iter().any(|o| matches!(o, Operand::Register(r) if matches!(r.class, RegisterClass::Xmm | RegisterClass::Ymm | RegisterClass::Zmm) && r.number == index.number)) { return false; }
                        }
                        (None, None) => {}
                        (None, Some(index))
                            if index.class == RegisterClass::Gpr
                                && index.is_valid()
                                && matches!(index.width, 32 | 64)
                                && index.number != 4
                                && m.base.is_none_or(|base| {
                                    base.class != RegisterClass::Rip && base.width == index.width
                                }) => {}
                        _ => return false,
                    }
                }
            }
        }
        true
    }
}
pub fn form(i: &Instruction) -> Option<&'static EvexForm> {
    let Opcode::Evex(op) = i.opcode else {
        return None;
    };
    let forms = isa::EVEX_FORMS;
    let start = forms.partition_point(|f| f.form.mnemonic < op.mnemonic());
    let mut candidates = forms[start..]
        .iter()
        .take_while(|f| f.form.mnemonic == op.mnemonic())
        .filter(|f| f.matches(i));
    let first = candidates.next()?;
    // An XMM result can have different active lengths for different memory
    // inputs. Do not let source ordering silently choose that operation.
    for other in candidates {
        for (index, operand) in i.operands.iter().enumerate() {
            if matches!(operand, Operand::Memory(m) if m.width == 0)
                && first.memory_bytes(first.form.operands[index], op.options.broadcast != 0)
                    != other.memory_bytes(other.form.operands[index], op.options.broadcast != 0)
            {
                return None;
            }
        }
    }
    Some(first)
}
/// Select an AVX-512 adapter when operands require its register file.
pub fn promote(opcode: Opcode, operands: &[Operand]) -> Opcode {
    if matches!(opcode, Opcode::Evex(_)) {
        return opcode;
    }
    let wide = |r: &Register| {
        r.class == RegisterClass::Zmm
            || r.class == RegisterClass::Mask
            || matches!(r.class, RegisterClass::Xmm | RegisterClass::Ymm) && r.number >= 16
    };
    if operands.iter().any(|o| match o {
        Operand::Register(r) => wide(r),
        Operand::Memory(m) => m.width == 512 || m.index.as_ref().is_some_and(wide),
        _ => false,
    }) && let Some(op) = EvexOpcode::parse(&opcode.to_string())
    {
        return Opcode::Evex(op);
    }
    opcode
}

pub(super) fn write<W: Write + ?Sized>(
    w: &mut W,
    opcode: EvexOpcode,
    operands: &[Operand],
    lock: bool,
) -> fmt::Result {
    if lock {
        w.write_str("lock ")?;
    }
    if opcode.options.address32 {
        w.write_str("addr32 ")?;
    }
    // Preserve explicit EVEX selection even for undecorated low XMM/YMM registers.
    if !isa::EVEX_FORMS
        .iter()
        .any(|f| f.form.mnemonic == opcode.mnemonic() && f.form.vex)
    {
        w.write_str("{evex} ")?;
    }
    w.write_str(opcode.mnemonic())?;
    let round_at = if matches!(operands.last(), Some(Operand::Immediate(_)))
        || matches!(operands.last(), Some(Operand::Register(r)) if r.class == RegisterClass::Gpr)
    {
        operands.len().saturating_sub(1)
    } else {
        operands.len()
    };
    for (n, operand) in operands.iter().enumerate() {
        w.write_str(if n == 0 { "\t" } else { ", " })?;
        if n == round_at
            && let Some(round) = opcode.options.rounding
        {
            write!(w, "{{{}}}, ", round.name())?;
        }
        operand.write_to(w)?;
        if n == 0 {
            if let Some(mask) = opcode.options.mask {
                write!(w, "{{k{mask}}}")?;
            }
            if opcode.options.zero {
                w.write_str("{z}")?;
            }
        }
        if matches!(operand, Operand::Memory(_)) && opcode.options.broadcast != 0 {
            write!(w, "{{1to{}}}", opcode.options.broadcast)?;
        }
    }
    if round_at == operands.len()
        && let Some(round) = opcode.options.rounding
    {
        write!(w, ", {{{}}}", round.name())?;
    }
    Ok(())
}

/// Remove EVEX decorators before the ordinary dialect parser, retaining their
/// positions for validation. Quoted symbols are never interpreted as decorators.
pub(super) fn decorated(s: &str, att: bool, prefix: bool) -> Result<Option<Instruction>, String> {
    if !s.contains('{') {
        return Ok(None);
    }
    if s.split_whitespace()
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case("addr32"))
    {
        return Ok(None);
    }
    let mut text = s.trim();
    let force = text
        .get(..6)
        .is_some_and(|s| s.eq_ignore_ascii_case("{evex}"));
    if force {
        text = text[6..].trim_start();
    }
    let (name, tail) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    let mut pieces = Vec::new();
    let (mut quote, mut depth, mut start) = (false, 0, 0);
    let mut escape = false;
    for (n, c) in tail.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if quote && c == '\\' {
            escape = true;
            continue;
        }
        if c == '"' {
            quote = !quote;
        }
        if !quote {
            if c == '[' || c == '(' {
                depth += 1;
            }
            if c == ']' || c == ')' {
                depth -= 1;
            }
            if c == ',' && depth == 0 {
                pieces.push(&tail[start..n]);
                start = n + 1;
            }
        }
    }
    if !tail.trim().is_empty() {
        pieces.push(&tail[start..]);
    }
    let mut options = EvexOptions::default();
    let mut clean = Vec::new();
    let mut masked = None;
    let mut broadcast = None;
    let mut found = force;
    for piece in pieces {
        let mut body = String::new();
        let mut chars = piece.chars().peekable();
        let (mut quote, mut escape) = (false, false);
        while let Some(c) = chars.next() {
            if escape {
                body.push(c);
                escape = false;
                continue;
            }
            if quote && c == '\\' {
                body.push(c);
                escape = true;
                continue;
            }
            if c == '"' {
                quote = !quote;
            }
            if c != '{' || quote {
                body.push(c);
                continue;
            }
            found = true;
            let mut tag = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(c) => tag.push(c),
                    None => return Err("unterminated EVEX decorator".into()),
                }
            }
            let tag = tag.trim().to_ascii_lowercase();
            if tag == "z" {
                if options.zero || options.mask.is_none() || masked != Some(clean.len()) {
                    return Err("zeroing must follow a destination mask".into());
                }
                options.zero = true;
            } else if let Some(n) = tag.strip_prefix("1to") {
                if broadcast.is_some() || body.trim().is_empty() {
                    return Err("broadcast must decorate one memory operand".into());
                }
                options.broadcast = n.parse().map_err(|_| "invalid broadcast count")?;
                if options.broadcast == 0 {
                    return Err("invalid broadcast count".into());
                }
                broadcast = Some(clean.len());
            } else if let Some(round) = match tag.as_str() {
                "rn-sae" => Some(Rounding::Nearest),
                "rd-sae" => Some(Rounding::Down),
                "ru-sae" => Some(Rounding::Up),
                "rz-sae" => Some(Rounding::Zero),
                "sae" => Some(Rounding::Sae),
                _ => None,
            } {
                if options.rounding.is_some() || !body.trim().is_empty() {
                    return Err("rounding requires a standalone decorator".into());
                }
                options.rounding = Some(round);
            } else {
                let r = intel_register(&tag, prefix)
                    .filter(|r| r.class == RegisterClass::Mask)
                    .ok_or("unknown EVEX decorator")?;
                if masked.is_some() || body.trim().is_empty() {
                    return Err("duplicate or misplaced mask decorator".into());
                }
                options.mask = Some(r.number);
                masked = Some(clean.len());
            }
        }
        if !body.trim().is_empty() {
            clean.push(body.trim().to_owned());
        }
    }
    if !found {
        return Ok(None);
    }
    let mut instruction = if att {
        super::att::parse(&format!("{name} {}", clean.join(", ")), prefix)?
    } else {
        Instruction::parse_intel_prefix(&format!("{name} {}", clean.join(", ")), prefix)?
    };
    let destination = if att {
        clean.len().saturating_sub(1)
    } else {
        0
    };
    if masked.is_some_and(|n| n != destination) {
        return Err("mask must decorate the destination".into());
    }
    if let Some(n) = broadcast {
        let n = if att { clean.len() - 1 - n } else { n };
        if !matches!(instruction.operands.get(n), Some(Operand::Memory(_))) {
            return Err("broadcast requires memory".into());
        }
    }
    let mut opcode = EvexOpcode::parse(&instruction.opcode.to_string())
        .ok_or("instruction has no AVX-512F adapter")?;
    if force
        && !isa::EVEX_FORMS
            .iter()
            .any(|f| f.form.mnemonic == opcode.mnemonic() && !f.form.vex)
    {
        return Err("instruction has no EVEX encoding".into());
    }
    opcode.options = options;
    instruction.opcode = Opcode::Evex(opcode);
    Ok(Some(instruction))
}
