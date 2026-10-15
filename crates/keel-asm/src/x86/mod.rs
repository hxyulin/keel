//! Native x86-64 ELF assembler. Intel syntax is parsed once into `keel-asm-ir`.
//! The initial layout uses near branches, avoiding unstable short-branch layout.
mod encode;
mod metadata;
use crate::Error;
use keel_asm_ir::{
    Directive, Expression, Item, Program,
    x86::{self, Address, Modifier},
};
use object::write::{Object, Relocation, Symbol, SymbolSection};
use object::{
    Architecture, BinaryFormat, Endianness, RelocationFlags, SectionFlags, SectionKind,
    SymbolFlags, SymbolKind, SymbolScope,
};
use std::collections::BTreeMap;

struct Section {
    name: String,
    flags: u64,
    nobits: bool,
    align: u64,
    data: Vec<u8>,
    size: u64,
}
#[derive(Clone)]
struct Sym {
    section: Option<usize>,
    value: u64,
    global: bool,
    weak: bool,
    hidden: bool,
    kind: SymbolKind,
    size: Option<Expression>,
    alias: Option<Expression>,
    common: Option<(u64, u64)>,
}
impl Default for Sym {
    fn default() -> Self {
        Self {
            section: None,
            value: 0,
            global: false,
            weak: false,
            hidden: false,
            kind: SymbolKind::Unknown,
            size: None,
            alias: None,
            common: None,
        }
    }
}
struct Fix {
    section: usize,
    offset: u64,
    width: u8,
    address: Address,
    kind: u32,
    line: usize,
}
#[derive(Default)]
struct Assembler {
    sections: Vec<Section>,
    symbols: BTreeMap<String, Sym>,
    fixes: Vec<Fix>,
    current: usize,
    cfi: Option<metadata::Frame>,
    frames: Vec<metadata::Frame>,
    files: BTreeMap<u32, Vec<u8>>,
    locations: Vec<metadata::Location>,
}

/// Assemble standalone Intel syntax as a Linux relocatable ELF64 object.
pub fn assemble_object(source: &str) -> Result<Vec<u8>, Error> {
    let mut a = Assembler::new()?;
    a.source(source, 1)?;
    a.object()
}
/// Consume x86 instructions and shared directives. Diagnostic positions refer
/// to program items; positions inside explicit text refer to its source lines.
pub fn assemble_program_object(program: &Program) -> Result<Vec<u8>, Error> {
    let mut a = Assembler::new()?;
    for (index, item) in program.items.iter().enumerate() {
        let line = index + 1;
        if let Item::Text(source) = item {
            a.source(source, line)?
        } else {
            a.item(item, line).map_err(|e| Error::new(line, e))?
        }
    }
    a.object()
}
impl Assembler {
    fn new() -> Result<Self, Error> {
        let mut a = Self::default();
        a.section(".text", Some(6), false)
            .map_err(|e| Error::new(0, e))?;
        Ok(a)
    }
    fn source(&mut self, source: &str, first: usize) -> Result<(), Error> {
        for (index, source) in source.lines().enumerate() {
            let line = first + index;
            let p = x86::parse_program(source)
                .map_err(|e| Error::new(line, e.strip_prefix("line 1: ").unwrap_or(&e)))?;
            for item in &p.items {
                let result = if let Item::Text(t) = item {
                    t.lines()
                        .try_for_each(|directive| self.directive(directive, line))
                } else {
                    self.item(item, line)
                };
                result.map_err(|e| Error::new(line, e))?;
            }
        }
        Ok(())
    }
    fn item(&mut self, item: &Item, line: usize) -> Result<(), String> {
        match item {
            Item::X86Instruction(i) => self.instruction(i, line),
            Item::Label(n) => self.label(n),
            Item::Directive(d) => self.shared_directive(d, line),
            _ => Err("AArch64 instruction/branch in x86 program".into()),
        }
    }
    fn object(mut self) -> Result<Vec<u8>, Error> {
        if self.cfi.is_some() {
            return Err(Error::new(0, "unterminated .cfi_startproc"));
        }
        metadata::finish(&mut self).map_err(|e| Error::new(0, e))?;
        self.finish()
    }
    fn offset(&self) -> u64 {
        self.sections[self.current].size
    }
    fn section(&mut self, name: &str, flags: Option<u64>, nobits: bool) -> Result<(), String> {
        if let Some(i) = self.sections.iter().position(|s| s.name == name) {
            if flags.is_some_and(|f| f != self.sections[i].flags)
                || (nobits || matches!(name, ".bss" | ".tbss")) != self.sections[i].nobits
            {
                return Err(format!("conflicting section attributes for {name}"));
            }
            self.current = i;
            return Ok(());
        }
        let flags = flags.unwrap_or(match name {
            ".text" => 6,
            ".data" | ".bss" | ".data.rel.ro" => 3,
            ".rodata" => 2,
            ".tbss" | ".tdata" => 0x403,
            _ => 0,
        });
        self.current = self.sections.len();
        self.sections.push(Section {
            name: name.into(),
            flags,
            nobits: nobits || matches!(name, ".bss" | ".tbss"),
            align: 1,
            data: Vec::new(),
            size: 0,
        });
        Ok(())
    }
    fn bytes(&mut self, data: &[u8]) -> Result<(), String> {
        let s = &mut self.sections[self.current];
        if s.nobits && data.iter().any(|b| *b != 0) {
            return Err("nonzero initializer in NOBITS section".into());
        }
        s.size = s
            .size
            .checked_add(data.len() as u64)
            .ok_or("section size overflow")?;
        if !s.nobits {
            s.data.extend_from_slice(data)
        }
        Ok(())
    }
    fn zero(&mut self, n: u64, fill: u8) -> Result<(), String> {
        let s = &mut self.sections[self.current];
        if s.nobits && fill != 0 {
            return Err("nonzero fill in NOBITS section".into());
        }
        s.size = s.size.checked_add(n).ok_or("section size overflow")?;
        if !s.nobits {
            let len = usize::try_from(s.size).map_err(|_| "section too large")?;
            s.data
                .try_reserve(len - s.data.len())
                .map_err(|_| "section too large")?;
            s.data.resize(len, fill)
        }
        Ok(())
    }
    fn label(&mut self, name: &str) -> Result<(), String> {
        let value = self.offset();
        let sym = self.symbols.entry(name.into()).or_default();
        if sym.section.is_some() || sym.alias.is_some() || sym.common.is_some() {
            return Err(format!("duplicate symbol `{name}`"));
        }
        sym.section = Some(self.current);
        sym.value = value;
        Ok(())
    }
    fn instruction(&mut self, i: &x86::Instruction, line: usize) -> Result<(), String> {
        if self.sections[self.current].nobits {
            return Err("instruction in NOBITS section".into());
        }
        // Intel operands known to be absolute at this point are constants.
        // Forward references remain symbolic, including later absolute aliases.
        let mut folded: Option<x86::Instruction> = None;
        for (index, operand) in i.operands.iter().enumerate() {
            let address = match operand {
                x86::Operand::Immediate(address) => address,
                x86::Operand::Memory(memory) => &memory.displacement,
                _ => continue,
            };
            if address.modifier == Modifier::None
                && address.expression.add.is_some()
                && let Ok((None, value)) = self.eval(&address.expression, 0)
            {
                if matches!(
                    i.opcode,
                    x86::Opcode::Call | x86::Opcode::Jmp | x86::Opcode::Jcc(_)
                ) && matches!(operand, x86::Operand::Immediate(_))
                {
                    return Err("direct branch target cannot be absolute".into());
                }
                let operand = &mut folded.get_or_insert_with(|| i.clone()).operands[index];
                let address = match operand {
                    x86::Operand::Immediate(address) => address,
                    x86::Operand::Memory(memory) => &mut memory.displacement,
                    _ => unreachable!(),
                };
                address.expression = Expression::constant(value);
            }
        }
        let e = encode::encode(folded.as_ref().unwrap_or(i))?;
        let at = self.offset();
        for f in e.fixups {
            self.fixes.push(Fix {
                section: self.current,
                offset: at + f.offset as u64,
                width: f.width,
                address: f.address,
                kind: f.kind,
                line,
            })
        }
        self.bytes(&e.bytes)
    }
    fn capture_dot(&mut self, mut expression: Expression) -> Result<Expression, String> {
        if expression.add.as_deref() == Some(".") || expression.subtract.as_deref() == Some(".") {
            let name = format!(".Lkeel.dot.{}", self.symbols.len());
            self.label(&name)?;
            if expression.add.as_deref() == Some(".") {
                expression.add = Some(name.clone())
            }
            if expression.subtract.as_deref() == Some(".") {
                expression.subtract = Some(name)
            }
        }
        Ok(expression)
    }
    fn integer(&mut self, width: u8, mut address: Address, line: usize) -> Result<(), String> {
        address.expression = self.capture_dot(address.expression)?;
        if address.expression.add.is_none() && address.expression.subtract.is_none() {
            return self.bytes(&address.expression.constant.to_le_bytes()[..width as usize]);
        }
        if self.sections[self.current].nobits {
            return Err("symbolic initializer in NOBITS section".into());
        }
        let kind = match (address.modifier, width) {
            (Modifier::None, 8) => 1,
            (Modifier::None, 4) => 10,
            (Modifier::None, 2) => 12,
            (Modifier::None, 1) => 14,
            (Modifier::DtpOff, 8) => 17,
            (Modifier::TpOff, 4) => 23,
            _ => return Err("unsupported data relocation".into()),
        };
        self.fixes.push(Fix {
            section: self.current,
            offset: self.offset(),
            width,
            address,
            kind,
            line,
        });
        self.zero(u64::from(width), 0)
    }
    fn shared_directive(&mut self, d: &Directive, line: usize) -> Result<(), String> {
        match d {
            Directive::Section {
                segment,
                name,
                kind,
            } if segment.is_empty() => {
                use keel_asm_ir::SectionKind::*;
                let (flags, nobits) = match kind {
                    Debug => (0, false),
                    ThreadData => (0x403, false),
                    ThreadZeroFill => (0x403, true),
                    ZeroFill => (3, true),
                    EhFrame => (2, false),
                    Data => (
                        if name == ".text" {
                            6
                        } else if name == ".rodata" {
                            2
                        } else {
                            3
                        },
                        false,
                    ),
                    _ => return Err("unsupported shared ELF section kind".into()),
                };
                self.section(name, Some(flags), nobits)
            }
            Directive::File { index, path } => {
                if *index == 0 || self.files.insert(*index, path.clone()).is_some() {
                    return Err("invalid or duplicate file index".into());
                }
                Ok(())
            }
            Directive::Loc {
                file,
                line,
                column,
                prologue_end,
            } => {
                if !self.files.contains_key(file) {
                    return Err("location references undeclared file".into());
                }
                self.locations.push(metadata::Location {
                    section: self.current,
                    offset: self.offset(),
                    file: *file,
                    line: u64::from(*line),
                    column: u64::from(*column),
                    prologue_end: *prologue_end,
                });
                Ok(())
            }
            Directive::Cfi(cfi) => {
                use keel_asm_ir::Cfi;
                match cfi {
                    Cfi::Start => metadata::cfi(self, ".cfi_startproc", ""),
                    Cfi::End => metadata::cfi(self, ".cfi_endproc", ""),
                    Cfi::DefCfa { register, offset } => {
                        metadata::cfi(self, ".cfi_def_cfa", &format!("{register},{offset}"))
                    }
                    Cfi::Offset { register, offset } => {
                        metadata::cfi(self, ".cfi_offset", &format!("{register},{offset}"))
                    }
                    _ => Err("x86 exception personality/LSDA directives are unsupported".into()),
                }
            }
            Directive::ZeroFill {
                symbol,
                size,
                alignment,
                thread_local,
            } => {
                let old = self.current;
                self.section(if *thread_local { ".tbss" } else { ".bss" }, None, true)?;
                self.align(u32::from(*alignment), None)?;
                self.label(symbol)?;
                self.symbols.get_mut(symbol).unwrap().size =
                    Some(Expression::constant(i128::from(*size)));
                self.zero(*size, 0)?;
                self.current = old;
                Ok(())
            }
            Directive::Global(s) => {
                self.symbols.entry(s.clone()).or_default().global = true;
                Ok(())
            }
            Directive::Bytes(b) => self.bytes(b),
            Directive::CString(b) => {
                self.bytes(b)?;
                self.bytes(&[0])
            }
            Directive::Integer {
                width,
                expression,
                got: false,
            } if matches!(width, 1 | 2 | 4 | 8) => self.integer(
                *width,
                Address {
                    expression: expression.clone(),
                    modifier: Modifier::None,
                },
                line,
            ),
            Directive::Space(n) => self.zero(*n, 0),
            Directive::Align(n) => self.align(u32::from(*n), None),
            Directive::Set { name, expression } => {
                let e = self.capture_dot(expression.clone())?;
                self.set(name, e)
            }
            Directive::Leb { signed, expression }
                if expression.add.is_none() && expression.subtract.is_none() =>
            {
                self.leb(expression.constant, *signed)
            }
            _ => Err(format!(
                "unsupported shared directive for ELF: {d:?}; use ELF text spelling"
            )),
        }
    }
    fn align(&mut self, power: u32, max: Option<u64>) -> Result<(), String> {
        self.align_fill(power, max, None)
    }
    fn align_fill(&mut self, power: u32, max: Option<u64>, fill: Option<u8>) -> Result<(), String> {
        if power > 30 {
            return Err("alignment exceeds 2^30".into());
        }
        let align = 1u64 << power;
        let n = self.offset().wrapping_neg() & (align - 1);
        let section = &mut self.sections[self.current];
        section.align = section.align.max(align);
        let fill = fill.unwrap_or(if !section.nobits && section.flags & 4 != 0 {
            0x90
        } else {
            0
        });
        if max.is_none_or(|m| n <= m) {
            self.zero(n, fill)?
        }
        Ok(())
    }
    fn set(&mut self, name: &str, e: Expression) -> Result<(), String> {
        let sym = self.symbols.entry(name.into()).or_default();
        if sym.section.is_some() || sym.alias.is_some() {
            return Err(format!("duplicate symbol `{name}`"));
        }
        sym.alias = Some(e);
        Ok(())
    }
    fn leb(&mut self, n: i128, signed: bool) -> Result<(), String> {
        let mut b = Vec::new();
        metadata::leb(&mut b, n, signed)?;
        self.bytes(&b)
    }
    fn directive(&mut self, s: &str, line: usize) -> Result<(), String> {
        let assignment = x86::split(s, '=');
        if assignment.len() == 2 {
            let name = x86::symbol(assignment[0])?;
            let expression = self.capture_dot(x86::address(assignment[1])?.expression)?;
            return self.set(&name, expression);
        }
        let (op, args) = s
            .trim()
            .split_once(char::is_whitespace)
            .unwrap_or((s.trim(), ""));
        let args = args.trim();
        let parts = x86::split(args, ',');
        let num = |s: &str| -> Result<u64, String> {
            u64::try_from(x86::integer(s)?).map_err(|_| "expected nonnegative integer".into())
        };
        match op {
            "" => Ok(()),
            ".intel_syntax" if args == "noprefix" => Ok(()),
            ".text" | ".data" | ".bss" if args.is_empty() => self.section(op, None, op == ".bss"),
            ".section" => {
                let name = if parts[0].starts_with('"') {
                    x86::symbol(parts[0])?
                } else if !parts[0].is_empty()
                    && parts[0]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                {
                    parts[0].to_string()
                } else {
                    return Err("invalid ELF section name".into());
                };
                let mut flags = None;
                let mut nobits = name == ".bss" || name == ".tbss";
                if parts.len() > 3 {
                    return Err("section groups/entry sizes are unsupported".into());
                }
                if let Some(s) = parts.get(1) {
                    let mut f = 0;
                    for b in x86::unquote(s)? {
                        f |= match b {
                            b'a' => 2,
                            b'w' => 1,
                            b'x' => 4,
                            b'T' => 0x400,
                            _ => {
                                return Err(format!(
                                    "unsupported ELF section flag `{}`",
                                    b as char
                                ));
                            }
                        }
                    }
                    flags = Some(f)
                }
                if let Some(t) = parts.get(2) {
                    match *t {
                        "@progbits" | "%progbits" => {}
                        "@nobits" | "%nobits" => nobits = true,
                        _ => return Err(format!("unsupported section type `{t}`")),
                    }
                }
                self.section(&name, flags, nobits)
            }
            ".globl" | ".global" | ".local" | ".weak" | ".hidden" => {
                for name in parts {
                    let name = x86::symbol(name)?;
                    let s = self.symbols.entry(name).or_default();
                    match op {
                        ".local" => s.global = false,
                        ".hidden" => s.hidden = true,
                        ".weak" => {
                            s.weak = true;
                            s.global = true
                        }
                        _ => s.global = true,
                    }
                }
                Ok(())
            }
            ".type" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let kind = match parts[1] {
                    "@function" | "%function" => SymbolKind::Text,
                    "@object" | "%object" => SymbolKind::Data,
                    "@tls_object" => SymbolKind::Tls,
                    _ => return Err("unsupported ELF symbol type".into()),
                };
                self.symbols.entry(name).or_default().kind = kind;
                Ok(())
            }
            ".size" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let expr = self.capture_dot(x86::address(parts[1])?.expression)?;
                self.symbols.entry(name).or_default().size = Some(expr);
                Ok(())
            }
            ".set" | ".equ" if parts.len() == 2 => {
                let name = x86::symbol(parts[0])?;
                let expr = self.capture_dot(x86::address(parts[1])?.expression)?;
                self.set(&name, expr)
            }
            ".comm" | ".lcomm" if parts.len() == 2 || parts.len() == 3 => {
                let name = x86::symbol(parts[0])?;
                let size = num(parts[1])?;
                let align = if parts.len() == 3 {
                    num(parts[2])?
                } else {
                    size.checked_next_power_of_two().unwrap_or(16).min(16)
                };
                if !align.is_power_of_two() {
                    return Err("common alignment must be a power of two".into());
                }
                let sym = self.symbols.entry(name.clone()).or_default();
                if sym.section.is_some() || sym.common.is_some() {
                    return Err("duplicate common symbol".into());
                }
                sym.common = Some((size, align));
                sym.kind = SymbolKind::Data;
                sym.global = op == ".comm";
                if op == ".lcomm" {
                    let old = self.current;
                    self.section(".bss", None, true)?;
                    self.align(align.trailing_zeros(), None)?;
                    self.symbols.get_mut(&name).unwrap().common = None;
                    self.label(&name)?;
                    self.symbols.get_mut(&name).unwrap().size =
                        Some(Expression::constant(size.into()));
                    self.zero(size, 0)?;
                    self.current = old
                }
                Ok(())
            }
            ".p2align" | ".align" | ".balign" => {
                if parts.len() > 3 {
                    return Err("too many alignment operands".into());
                }
                if parts.get(1).is_some_and(|s| !s.is_empty() && *s != "0") {
                    return Err("nonzero alignment fill is unsupported".into());
                }
                let n = num(parts[0])?;
                let power = if op == ".p2align" {
                    u32::try_from(n).map_err(|_| "alignment too large")?
                } else {
                    if !n.is_power_of_two() {
                        return Err("alignment must be a power of two".into());
                    }
                    n.trailing_zeros()
                };
                let max = parts.get(2).map(|s| num(s)).transpose()?;
                let fill = parts.get(1).filter(|s| !s.is_empty()).map(|_| 0);
                self.align_fill(power, max, fill)
            }
            ".byte" | ".short" | ".word" | ".long" | ".int" | ".quad" => {
                let width = match op {
                    ".byte" => 1,
                    ".short" | ".word" => 2,
                    ".long" | ".int" => 4,
                    _ => 8,
                };
                for p in parts {
                    self.integer(width, x86::address(p)?, line)?
                }
                Ok(())
            }
            ".zero" | ".space" | ".skip" => {
                if parts.len() > 2 {
                    return Err("too many space operands".into());
                }
                let n = num(parts[0])?;
                let fill = parts.get(1).map(|s| num(s)).transpose()?.unwrap_or(0);
                self.zero(n, u8::try_from(fill).map_err(|_| "fill exceeds a byte")?)
            }
            ".ascii" | ".asciz" | ".string" => {
                for p in parts {
                    self.bytes(&x86::unquote(p)?)?;
                    if op != ".ascii" {
                        self.bytes(&[0])?
                    }
                }
                Ok(())
            }
            ".uleb128" | ".sleb128" => {
                for p in parts {
                    self.leb(x86::integer(p)?, op == ".sleb128")?
                }
                Ok(())
            }
            ".file" => {
                let (index, path) = args
                    .split_once(char::is_whitespace)
                    .ok_or(".file requires an index and quoted path")?;
                let index = u32::try_from(num(index)?).map_err(|_| "file index too large")?;
                if index == 0 || self.files.insert(index, x86::unquote(path)?).is_some() {
                    return Err("invalid or duplicate .file index".into());
                }
                Ok(())
            }
            ".loc" => {
                let p: Vec<_> = args.split_whitespace().collect();
                if p.len() < 3 || p.len() > 4 || p.get(3).is_some_and(|s| *s != "prologue_end") {
                    return Err("unsupported .loc operands".into());
                }
                let file = num(p[0])? as u32;
                if !self.files.contains_key(&file) {
                    return Err(".loc references undeclared file".into());
                }
                self.locations.push(metadata::Location {
                    section: self.current,
                    offset: self.offset(),
                    file,
                    line: num(p[1])?,
                    column: num(p[2])?,
                    prologue_end: p.len() == 4,
                });
                Ok(())
            }
            ".cfi_startproc"
            | ".cfi_endproc"
            | ".cfi_def_cfa_offset"
            | ".cfi_def_cfa_register"
            | ".cfi_def_cfa"
            | ".cfi_offset"
            | ".cfi_restore"
            | ".cfi_remember_state"
            | ".cfi_restore_state" => metadata::cfi(self, op, args),
            _ => Err(format!("unsupported ELF directive `{op}`")),
        }
    }
    fn eval_symbol(&self, name: &str, depth: usize) -> Result<(Option<usize>, i128), String> {
        if depth > 64 {
            return Err("cyclic or excessively nested symbol alias".into());
        }
        let s = self
            .symbols
            .get(name)
            .ok_or_else(|| format!("undefined symbol `{name}` in expression"))?;
        if let Some(e) = &s.alias {
            return self.eval(e, depth + 1);
        }
        if let Some(section) = s.section {
            Ok((Some(section), s.value.into()))
        } else {
            Err(format!("undefined symbol `{name}` in expression"))
        }
    }
    fn eval(&self, e: &Expression, depth: usize) -> Result<(Option<usize>, i128), String> {
        let (mut section, mut value) = (None, e.constant);
        if let Some(n) = &e.add {
            let (s, v) = self.eval_symbol(n, depth)?;
            section = s;
            value += v
        }
        if let Some(n) = &e.subtract {
            let (s, v) = self.eval_symbol(n, depth)?;
            if s != section {
                return Err("symbol difference requires the same section".into());
            }
            section = None;
            value -= v
        }
        Ok((section, value))
    }
    fn finish(mut self) -> Result<Vec<u8>, Error> {
        let err = |s: String| Error::new(0, s);
        // Ensure even undefined references receive symbol table entries.
        for f in &self.fixes {
            if let Some(n) = &f.address.expression.add {
                let sym = self.symbols.entry(n.clone()).or_default();
                if matches!(
                    f.address.modifier,
                    Modifier::TpOff | Modifier::GotTpOff | Modifier::DtpOff
                ) {
                    sym.kind = SymbolKind::Tls;
                }
            }
        }
        let mut object = Object::new(BinaryFormat::Elf, Architecture::X86_64, Endianness::Little);
        let mut sections = Vec::new();
        for s in &self.sections {
            let kind = if s.nobits {
                if s.flags & 0x400 != 0 {
                    SectionKind::UninitializedTls
                } else {
                    SectionKind::UninitializedData
                }
            } else if s.flags & 4 != 0 {
                SectionKind::Text
            } else if s.flags & 0x400 != 0 {
                SectionKind::Tls
            } else if s.flags & 1 != 0 {
                SectionKind::Data
            } else if s.flags & 2 != 0 {
                SectionKind::ReadOnlyData
            } else {
                SectionKind::Other
            };
            let id = object.add_section(Vec::new(), s.name.as_bytes().to_vec(), kind);
            object.section_mut(id).flags = SectionFlags::Elf { sh_flags: s.flags };
            if s.nobits {
                object.append_section_bss(id, s.size, s.align);
            } else {
                object.append_section_data(id, &s.data, s.align);
            }
            sections.push(id);
        }
        let mut symbols = BTreeMap::new();
        for (name, s) in &self.symbols {
            let (section, value) = if s.alias.is_some() {
                let (sec, value) = self.eval_symbol(name, 0).map_err(err)?;
                (
                    sec,
                    u64::try_from(value).map_err(|_| err("alias value out of range".into()))?,
                )
            } else {
                (s.section, s.value)
            };
            let size = if let Some(e) = &s.size {
                let (sec, n) = self.eval(e, 0).map_err(err)?;
                if sec.is_some() {
                    return Err(err("symbol size must be absolute".into()));
                }
                u64::try_from(n).map_err(|_| err("symbol size out of range".into()))?
            } else {
                0
            };
            let tls = s.kind == SymbolKind::Tls
                || section.is_some_and(|i| self.sections[i].flags & 0x400 != 0);
            let symbol = Symbol {
                name: name.as_bytes().to_vec(),
                value,
                size,
                kind: if tls { SymbolKind::Tls } else { s.kind },
                scope: if s.global || section.is_none() && s.alias.is_none() {
                    SymbolScope::Linkage
                } else {
                    SymbolScope::Compilation
                },
                weak: s.weak,
                section: match section {
                    Some(i) => SymbolSection::Section(sections[i]),
                    None if s.alias.is_some() => SymbolSection::Absolute,
                    None => SymbolSection::Undefined,
                },
                flags: SymbolFlags::Elf {
                    st_info: ((if s.weak {
                        2
                    } else if s.global || section.is_none() && s.alias.is_none() {
                        1
                    } else {
                        0
                    }) << 4)
                        | (if tls {
                            6
                        } else {
                            match s.kind {
                                SymbolKind::Text => 2,
                                SymbolKind::Data => 1,
                                _ => 0,
                            }
                        }),
                    st_other: if s.hidden { 2 } else { 0 },
                },
            };
            let id = if let Some((size, align)) = s.common {
                object.add_common_symbol(symbol, size, align)
            } else {
                object.add_symbol(symbol)
            };
            symbols.insert(name.clone(), id);
        }
        for f in &self.fixes {
            let expr = &f.address.expression;
            let mut kind = f.kind;
            let mut addend = expr.constant;
            let name = expr
                .add
                .as_ref()
                .ok_or_else(|| Error::new(f.line, "relocation requires a positive symbol"))?
                .clone();
            let defined = self.eval_symbol(&name, 0).ok();
            let absolute = if expr.subtract.is_some() {
                self.eval(expr, 0)
                    .ok()
                    .filter(|(s, _)| s.is_none())
                    .map(|(_, n)| n)
            } else if matches!(kind, 1 | 10 | 11 | 12 | 14)
                && defined.is_some_and(|(s, _)| s.is_none())
            {
                defined.map(|(_, n)| n + addend)
            } else if matches!(kind, 2 | 4)
                && defined.is_some_and(|(s, _)| s == Some(f.section))
                && !self.symbols[&name].global
            {
                defined.map(|(_, n)| n + addend - i128::from(f.offset))
            } else {
                None
            };
            if let Some(value) = absolute {
                let bits = u32::from(f.width) * 8;
                let signed = matches!(kind, 2 | 4 | 11 | 23);
                if value < -(1i128 << (bits - 1)) || value >= (1i128 << (bits - u32::from(signed)))
                {
                    return Err(Error::new(
                        f.line,
                        "resolved value does not fit relocation field",
                    ));
                }
                let offset = usize::try_from(f.offset)
                    .map_err(|_| Error::new(f.line, "offset too large"))?;
                self.sections[f.section].data[offset..offset + usize::from(f.width)]
                    .copy_from_slice(&value.to_le_bytes()[..usize::from(f.width)]);
                continue;
            }
            if let Some(sub) = &expr.subtract {
                let (sec, value) = self
                    .eval_symbol(sub, 0)
                    .map_err(|e| Error::new(f.line, e))?;
                if sec != Some(f.section) || f.width != 4 || kind != 10 {
                    return Err(Error::new(
                        f.line,
                        "unsupported cross-section symbol difference",
                    ));
                }
                kind = 2;
                addend += i128::from(f.offset) - value;
            }
            // Relocations against section-relative aliases use their underlying section symbol.
            let symbol = if self.symbols[&name].alias.is_some() {
                let (sec, value) = defined.ok_or_else(|| Error::new(f.line, "unresolved alias"))?;
                if let Some(sec) = sec {
                    addend += value;
                    object.section_symbol(sections[sec])
                } else {
                    // GOT/TLS modifiers still require a linker relocation
                    // even when the referenced symbol is absolute.
                    symbols[&name]
                }
            } else {
                symbols[&name]
            };
            object
                .add_relocation(
                    sections[f.section],
                    Relocation {
                        offset: f.offset,
                        symbol,
                        addend: i64::try_from(addend)
                            .map_err(|_| Error::new(f.line, "relocation addend exceeds i64"))?,
                        flags: RelocationFlags::Elf { r_type: kind },
                    },
                )
                .map_err(|e| Error::new(f.line, e.to_string()))?;
        }
        // Patches resolved above must replace the section data already copied to Object.
        for (i, s) in self.sections.iter().enumerate() {
            if !s.nobits {
                object
                    .section_mut(sections[i])
                    .data_mut()
                    .copy_from_slice(&s.data);
            }
        }
        object.write().map_err(|e| err(e.to_string()))
    }
}
