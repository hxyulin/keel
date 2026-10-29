//! Ordinary application core forms absent from the original assembler inventory.
use super::*;
use std::fmt::{self, Write};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoopKind {
    Loop,
    Equal,
    NotEqual,
    Zero,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StringKind {
    Movs,
    Stos,
    Lods,
    Cmps,
    Scas,
    Ins,
    Outs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Repeat {
    None,
    Rep,
    Repne,
}
include!("isa/core_opcodes.rs");
include!("isa/core_mnemonics.rs");
impl CoreOpcode {
    pub fn parse(s: &str) -> Option<Self> {
        if let Some((kind, width)) = string_kind(s) {
            return Some(Self::String {
                kind,
                width,
                repeat: Repeat::None,
                address32: false,
                segment: None,
            });
        }
        Some(match s {
            "xlat" | "xlatb" => Self::Xlat {
                address32: false,
                segment: None,
            },
            "loop" | "loopq" => Self::Loop {
                kind: LoopKind::Loop,
                address32: false,
            },
            "loope" | "loopz" | "loopeq" | "loopzq" => Self::Loop {
                kind: LoopKind::Equal,
                address32: false,
            },
            "loopne" | "loopnz" | "loopneq" | "loopnzq" => Self::Loop {
                kind: LoopKind::NotEqual,
                address32: false,
            },
            "loopl" => Self::Loop {
                kind: LoopKind::Loop,
                address32: true,
            },
            "loopel" | "loopzl" => Self::Loop {
                kind: LoopKind::Equal,
                address32: true,
            },
            "loopnel" | "loopnzl" => Self::Loop {
                kind: LoopKind::NotEqual,
                address32: true,
            },
            "jrcxz" => Self::Loop {
                kind: LoopKind::Zero,
                address32: false,
            },
            "jecxz" => Self::Loop {
                kind: LoopKind::Zero,
                address32: true,
            },
            "enter" | "enterq" => Self::Enter { operand16: false },
            "enterw" => Self::Enter { operand16: true },
            "retf" | "retfl" | "lret" | "lretl" => Self::RetFar { width: 32 },
            "retfq" | "lretq" => Self::RetFar { width: 64 },
            "retfw" | "lretw" => Self::RetFar { width: 16 },
            _ => return Self::parse_static(s),
        })
    }
    pub fn mnemonic(self) -> &'static str {
        core_mnemonic!(self, {
            Self::String { kind, width, .. } => match (kind, width) {
                (StringKind::Movs, 8) => "movsb",
                (StringKind::Movs, 16) => "movsw",
                (StringKind::Movs, 32) => "movsd",
                (StringKind::Movs, 64) => "movsq",
                (StringKind::Stos, 8) => "stosb",
                (StringKind::Stos, 16) => "stosw",
                (StringKind::Stos, 32) => "stosd",
                (StringKind::Stos, 64) => "stosq",
                (StringKind::Lods, 8) => "lodsb",
                (StringKind::Lods, 16) => "lodsw",
                (StringKind::Lods, 32) => "lodsd",
                (StringKind::Lods, 64) => "lodsq",
                (StringKind::Cmps, 8) => "cmpsb",
                (StringKind::Cmps, 16) => "cmpsw",
                (StringKind::Cmps, 32) => "cmpsd",
                (StringKind::Cmps, 64) => "cmpsq",
                (StringKind::Scas, 8) => "scasb",
                (StringKind::Scas, 16) => "scasw",
                (StringKind::Scas, 32) => "scasd",
                (StringKind::Scas, 64) => "scasq",
                (StringKind::Ins, 8) => "insb",
                (StringKind::Ins, 16) => "insw",
                (StringKind::Ins, 32) => "insd",
                (StringKind::Outs, 8) => "outsb",
                (StringKind::Outs, 16) => "outsw",
                (StringKind::Outs, 32) => "outsd",
                _ => "invalid-string-width",
            },
            Self::Xlat { .. } => "xlatb",
            Self::Loop {
                kind: LoopKind::Loop,
                ..
            } => "loop",
            Self::Loop {
                kind: LoopKind::Equal,
                ..
            } => "loope",
            Self::Loop {
                kind: LoopKind::NotEqual,
                ..
            } => "loopne",
            Self::Loop {
                kind: LoopKind::Zero,
                address32: false,
            } => "jrcxz",
            Self::Loop {
                kind: LoopKind::Zero,
                address32: true,
            } => "jecxz",
            Self::Enter { .. } => "enter",
            Self::RetFar { width: 16 } => "retfw",
            Self::RetFar { width: 32 } => "retf",
            Self::RetFar { width: 64 } => "retfq",
            Self::RetFar { .. } => "invalid-retf-width",
        })
    }
    pub(super) fn write_to<W: Write + ?Sized>(self, w: &mut W) -> fmt::Result {
        if let Self::String {
            kind,
            width,
            repeat,
            address32,
            segment,
        } = self
        {
            if !matches!(width, 8 | 16 | 32 | 64)
                || matches!(kind, StringKind::Ins | StringKind::Outs) && width == 64
            {
                return Err(fmt::Error);
            }
            if address32 {
                w.write_str("addr32 ")?;
            }
            if let Some(s) = segment {
                w.write_str(match s {
                    0x64 => "fs ",
                    0x65 => "gs ",
                    _ => return Err(fmt::Error),
                })?;
            }
            w.write_str(match repeat {
                Repeat::None => "",
                Repeat::Rep if matches!(kind, StringKind::Cmps | StringKind::Scas) => "repe ",
                Repeat::Rep => "rep ",
                Repeat::Repne => "repne ",
            })?;
        }
        if let Self::Xlat { address32, segment } = self {
            if address32 {
                w.write_str("addr32 ")?;
            }
            if let Some(s) = segment {
                w.write_str(match s {
                    0x64 => "fs ",
                    0x65 => "gs ",
                    _ => return Err(fmt::Error),
                })?;
            }
        }
        if matches!(
            self,
            Self::Loop {
                address32: true,
                kind: LoopKind::Loop | LoopKind::Equal | LoopKind::NotEqual
            }
        ) {
            w.write_str("addr32 ")?;
        }
        if matches!(self, Self::Enter { operand16: true }) {
            w.write_str("data16 ")?;
        }
        w.write_str(self.mnemonic())
    }
}
impl Opcode {
    pub fn is_direct_branch(self) -> bool {
        matches!(
            self,
            Self::Call | Self::Jmp | Self::Jcc(_) | Self::Core(CoreOpcode::Loop { .. })
        )
    }
}
impl Instruction {
    pub(super) fn core_prefix(mut self, prefix: &str) -> Result<Self, String> {
        self.opcode = match (prefix, self.opcode) {
            ("addr32", Opcode::Evex(mut op)) if !op.options.address32 => {
                op.options.address32 = true;
                Opcode::Evex(op)
            }
            (
                "addr32",
                Opcode::Core(CoreOpcode::String {
                    kind,
                    width,
                    repeat,
                    address32: false,
                    segment,
                }),
            ) => Opcode::Core(CoreOpcode::String {
                kind,
                width,
                repeat,
                address32: true,
                segment,
            }),
            (
                "addr32",
                Opcode::Core(CoreOpcode::Xlat {
                    address32: false,
                    segment,
                }),
            ) => Opcode::Core(CoreOpcode::Xlat {
                address32: true,
                segment,
            }),
            (
                "rep" | "repe" | "repz" | "repne" | "repnz",
                Opcode::Core(CoreOpcode::String {
                    kind,
                    width,
                    repeat: Repeat::None,
                    address32,
                    segment,
                }),
            ) => Opcode::Core(CoreOpcode::String {
                kind,
                width,
                repeat: if matches!(prefix, "repne" | "repnz") {
                    Repeat::Repne
                } else {
                    Repeat::Rep
                },
                address32,
                segment,
            }),
            (
                "fs" | "gs",
                Opcode::Core(CoreOpcode::String {
                    kind:
                        kind @ (StringKind::Movs
                        | StringKind::Lods
                        | StringKind::Cmps
                        | StringKind::Outs),
                    width,
                    repeat,
                    address32,
                    segment: None,
                }),
            ) => Opcode::Core(CoreOpcode::String {
                kind,
                width,
                repeat,
                address32,
                segment: Some(if prefix == "fs" { 0x64 } else { 0x65 }),
            }),
            (
                "fs" | "gs",
                Opcode::Core(CoreOpcode::Xlat {
                    address32,
                    segment: None,
                }),
            ) => Opcode::Core(CoreOpcode::Xlat {
                address32,
                segment: Some(if prefix == "fs" { 0x64 } else { 0x65 }),
            }),
            (
                "addr32",
                Opcode::Core(CoreOpcode::Loop {
                    kind,
                    address32: false,
                }),
            ) => Opcode::Core(CoreOpcode::Loop {
                kind,
                address32: true,
            }),
            ("data16", Opcode::Core(CoreOpcode::Enter { operand16: false })) => {
                Opcode::Core(CoreOpcode::Enter { operand16: true })
            }
            ("data16", Opcode::Core(CoreOpcode::RetFar { width: 32 })) => {
                Opcode::Core(CoreOpcode::RetFar { width: 16 })
            }
            _ => {
                return Err(format!(
                    "{prefix} is not applicable to this core instruction"
                ));
            }
        };
        Ok(self)
    }
}

fn string_kind(s: &str) -> Option<(StringKind, u16)> {
    let (base, bits) = match s.as_bytes().last()? {
        b'b' => (&s[..s.len() - 1], 8),
        b'w' => (&s[..s.len() - 1], 16),
        b'd' | b'l' => (&s[..s.len() - 1], 32),
        b'q' => (&s[..s.len() - 1], 64),
        _ => (s, 0),
    };
    Some((
        match base {
            "movs" => StringKind::Movs,
            "stos" => StringKind::Stos,
            "lods" => StringKind::Lods,
            "cmps" => StringKind::Cmps,
            "scas" => StringKind::Scas,
            "ins" => StringKind::Ins,
            "outs" => StringKind::Outs,
            _ => return None,
        },
        bits,
    ))
}
/// Explicit string operands denote fixed implicit addresses, never relocatable
/// arbitrary memory. Parse them separately so address32 aliases do not widen
/// the general ModRM register/address model owned by the V3 work.
pub(super) fn parse_string(
    s: &str,
    att: bool,
    prefix: bool,
) -> Option<Result<Instruction, String>> {
    let (name, args) = s
        .trim()
        .split_once(char::is_whitespace)
        .unwrap_or((s.trim(), ""));
    let name = lowercase(name);
    let args = args.trim();
    let (kind, mut bits) = if let Some(v) = string_kind(&name) {
        (Some(v.0), v.1)
    } else if matches!(name.as_ref(), "xlat" | "xlatb") {
        (None, 8)
    } else {
        return None;
    };
    // MOVSD/CMPSD with vector operands are the distinct existing SSE forms.
    if matches!(name.as_ref(), "movsd" | "cmpsd") && args.to_ascii_lowercase().contains("xmm") {
        return None;
    }
    Some((|| {
        let mut address32 = false;
        let mut source_segment = None;
        if !args.is_empty() {
            let mut operands = args.split(',').map(str::trim).collect::<Vec<_>>();
            if att && matches!(kind, Some(StringKind::Movs | StringKind::Cmps)) {
                operands.reverse();
            }
            let expected = match kind {
                Some(StringKind::Movs) => vec![("di", false), ("si", true)],
                Some(StringKind::Ins) => vec![("di", false)],
                Some(StringKind::Outs) => vec![("si", true)],
                Some(StringKind::Cmps) => vec![("si", true), ("di", false)],
                Some(StringKind::Lods) => vec![("si", true)],
                Some(StringKind::Stos | StringKind::Scas) => vec![("di", false)],
                None => vec![("bx", true)],
            };
            if operands.len() != expected.len() {
                return Err(
                    "string instruction has the wrong number of implicit-address operands".into(),
                );
            }
            let mut common = None;
            for (text, (reg, source)) in operands.into_iter().zip(expected) {
                let (w, a32, seg) = implicit_memory(text, att, prefix, reg, source)?;
                if w != 0 {
                    if bits == 0 {
                        bits = w;
                    } else if bits != w {
                        return Err("string operand width mismatch".into());
                    }
                }
                if common.is_some_and(|v| v != a32) {
                    return Err("string address widths must match".into());
                }
                common = Some(a32);
                if source {
                    source_segment = seg;
                }
            }
            address32 = common.unwrap_or(false);
        }
        if !matches!(bits, 8 | 16 | 32 | 64)
            || matches!(kind, Some(StringKind::Ins | StringKind::Outs)) && bits == 64
        {
            return Err(
                "string instruction needs a byte/word/dword/qword suffix or explicit width".into(),
            );
        }
        let opcode = if let Some(kind) = kind {
            CoreOpcode::String {
                kind,
                width: bits,
                repeat: Repeat::None,
                address32,
                segment: source_segment,
            }
        } else {
            CoreOpcode::Xlat {
                address32,
                segment: source_segment,
            }
        };
        Ok(Instruction {
            opcode: Opcode::Core(opcode),
            operands: vec![],
            lock: false,
        })
    })())
}
fn implicit_memory(
    s: &str,
    att: bool,
    register_prefix: bool,
    reg: &str,
    source: bool,
) -> Result<(u16, bool, Option<u8>), String> {
    let lowered = s.to_ascii_lowercase();
    let mut s = lowered.trim();
    let mut bits = 0;
    if !att {
        for (prefix, w) in [
            ("byte ptr ", 8),
            ("word ptr ", 16),
            ("dword ptr ", 32),
            ("qword ptr ", 64),
        ] {
            if let Some(rest) = s.strip_prefix(prefix) {
                bits = w;
                s = rest.trim();
                break;
            }
        }
    }
    let mut segment = None;
    for (name, b) in [
        ("fs", Some(0x64)),
        ("gs", Some(0x65)),
        ("ds", None),
        ("es", None),
    ] {
        let prefix = if register_prefix {
            format!("%{name}:")
        } else {
            format!("{name}:")
        };
        if let Some(rest) = s.strip_prefix(&prefix) {
            if !source && name != "es" || source && name == "es" {
                return Err("invalid segment for the fixed string operand".into());
            }
            segment = b;
            s = rest.trim();
            break;
        }
    }
    let base = if att {
        s.strip_prefix("0(")
            .or_else(|| s.strip_prefix('('))
            .and_then(|v| v.strip_suffix(')'))
    } else {
        s.strip_prefix('[')
            .and_then(|v| v.strip_suffix(']'))
            .map(str::trim)
    }
    .ok_or("string memory must name its fixed SI/DI/BX base without displacement or index")?;
    let base = if register_prefix {
        base.strip_prefix('%')
            .ok_or("fixed string register requires `%`")?
    } else {
        base
    };
    let a32 = if base == format!("r{reg}") {
        false
    } else if base == format!("e{reg}") {
        true
    } else {
        return Err("invalid fixed string address register".into());
    };
    Ok((bits, a32, segment))
}

impl Instruction {
    /// Preserve lexical width suffixes even when the opcode identity is unsized.
    /// Shared by both dialect parsers so aliases never silently change width.
    pub(super) fn core_suffix(mut self, name: &str) -> Result<Self, String> {
        let far_bits = match (name, self.opcode) {
            ("lcallw" | "ljmpw", Opcode::Core(CoreOpcode::CallFar | CoreOpcode::JmpFar)) => {
                Some(32)
            }
            ("lcalll" | "ljmpl", Opcode::Core(CoreOpcode::CallFar | CoreOpcode::JmpFar)) => {
                Some(48)
            }
            ("lcallq" | "ljmpq", Opcode::Core(CoreOpcode::CallFar | CoreOpcode::JmpFar)) => {
                Some(80)
            }
            _ => None,
        };
        if let Some(bits) = far_bits {
            let [Operand::Memory(memory)] = self.operands.as_mut_slice() else {
                return Err("far transfer suffix requires memory".into());
            };
            if memory.width != 0 && memory.width != bits {
                return Err("far pointer width disagrees with suffix".into());
            }
            memory.width = bits;
            return Ok(self);
        }
        let (bits, index, selector) = match (name, self.opcode) {
            ("inb", Opcode::Core(CoreOpcode::In)) => (8, 0, false),
            ("inw", Opcode::Core(CoreOpcode::In)) => (16, 0, false),
            ("inl", Opcode::Core(CoreOpcode::In)) => (32, 0, false),
            ("outb", Opcode::Core(CoreOpcode::Out)) => (8, 1, false),
            ("outw", Opcode::Core(CoreOpcode::Out)) => (16, 1, false),
            ("outl", Opcode::Core(CoreOpcode::Out)) => (32, 1, false),
            (
                "lfsw" | "lgsw" | "lssw",
                Opcode::Core(CoreOpcode::Lfs | CoreOpcode::Lgs | CoreOpcode::Lss),
            ) => (16, 0, true),
            (
                "lfsl" | "lgsl" | "lssl",
                Opcode::Core(CoreOpcode::Lfs | CoreOpcode::Lgs | CoreOpcode::Lss),
            ) => (32, 0, true),
            (
                "lfsq" | "lgsq" | "lssq",
                Opcode::Core(CoreOpcode::Lfs | CoreOpcode::Lgs | CoreOpcode::Lss),
            ) => (64, 0, true),
            _ => return Ok(self),
        };
        if !matches!(self.operands.get(index),Some(Operand::Register(r)) if r.class==RegisterClass::Gpr && r.width==bits)
        {
            return Err(format!(
                "{name} suffix requires a {bits}-bit general register"
            ));
        }
        if selector {
            let [_, Operand::Memory(memory)] = self.operands.as_mut_slice() else {
                return Err("far selector load requires memory".into());
            };
            if memory.width != 0 && memory.width != bits + 16 {
                return Err("selector pointer width disagrees with suffix".into());
            }
            memory.width = bits + 16;
        }
        Ok(self)
    }
}
