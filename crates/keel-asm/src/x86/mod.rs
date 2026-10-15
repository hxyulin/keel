//! Native x86-64 ELF assembler. Intel syntax is parsed once into `keel-asm-ir`.
//! Parsed items are laid out until optimistic short branches fit their final layout.
mod encode;
mod metadata;
mod sections;
use crate::Error;
pub use keel_asm_ir::x86::Features;
use keel_asm_ir::{
    Directive, Expression, Item, Program,
    x86::{self, Address, Modifier},
};
use object::write::{Comdat, Object, Relocation, Symbol, SymbolSection};
use object::{
    Architecture, BinaryFormat, ComdatKind, Endianness, RelocationFlags, SectionFlags, SectionKind,
    SymbolFlags, SymbolKind, SymbolScope,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

struct Section {
    name: String,
    flags: u64,
    nobits: bool,
    entry_size: u64,
    group: Option<String>,
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
    symbols: HashMap<String, Sym>,
    fixes: Vec<Fix>,
    current: usize,
    cfi: Option<metadata::Frame>,
    frames: Vec<metadata::Frame>,
    files: BTreeMap<u32, Vec<u8>>,
    locations: Vec<metadata::Location>,
}

/// Assemble standalone Intel syntax as a Linux relocatable ELF64 object.
pub fn assemble_object(source: &str) -> Result<Vec<u8>, Error> {
    assemble_object_with_features(source, x86::Features::default())
}
pub fn assemble_object_with_features(
    source: &str,
    features: x86::Features,
) -> Result<Vec<u8>, Error> {
    let mut items = Vec::new();
    parse_source(source, 1, &mut items)?;
    validate_features(&items, features)?;
    assemble_items(&items)
}
/// Consume x86 instructions and shared directives. Diagnostic positions refer
/// to program items; positions inside explicit text refer to its source lines.
pub fn assemble_program_object(program: &Program) -> Result<Vec<u8>, Error> {
    assemble_program_object_with_features(program, x86::Features::default())
}
pub fn assemble_program_object_with_features(
    program: &Program,
    features: x86::Features,
) -> Result<Vec<u8>, Error> {
    let mut items = Vec::with_capacity(program.items.len());
    for (index, item) in program.items.iter().enumerate() {
        if let Item::Text(source) = item {
            let mut parsed = Vec::new();
            parse_source(source, index + 1, &mut parsed)?;
            items.extend(
                parsed
                    .into_iter()
                    .map(|(item, line)| (std::borrow::Cow::Owned(item), line)),
            );
        } else {
            items.push((std::borrow::Cow::Borrowed(item), index + 1));
        }
    }
    validate_features(&items, features)?;
    assemble_items(&items)
}
fn validate_features<T: std::borrow::Borrow<Item>>(
    items: &[(T, usize)],
    features: x86::Features,
) -> Result<(), Error> {
    for (item, line) in items {
        if let Item::X86Instruction(i) = item.borrow() {
            let required = i.opcode.required_features();
            if !features.contains(required) {
                return Err(Error::new(
                    *line,
                    format!(
                        "{} requires x86 target features: {}",
                        i.opcode,
                        required.names().collect::<Vec<_>>().join(",")
                    ),
                ));
            }
        }
    }
    Ok(())
}
fn parse_source(source: &str, first: usize, items: &mut Vec<(Item, usize)>) -> Result<(), Error> {
    for (index, source) in source.lines().enumerate() {
        let line = first + index;
        let p = x86::parse_program(source)
            .map_err(|e| Error::new(line, e.strip_prefix("line 1: ").unwrap_or(&e)))?;
        items.extend(p.items.into_iter().map(|item| (item, line)));
    }
    Ok(())
}

/// In-process diagnostic timings. Normal assembly compiles out stage timing.
#[derive(Default, Debug)]
pub struct Profile {
    pub parse_ms: f64,
    pub layout_ms: Vec<f64>,
    pub branch_check_ms: Vec<f64>,
    pub discard_ms: Vec<f64>,
    pub widened: Vec<usize>,
    pub object_ms: f64,
    pub items: usize,
}
/// Assemble once and collect stage timings, excluding file I/O.
pub fn profile_object(source: &str) -> Result<(Vec<u8>, Profile), Error> {
    profile_object_with_features(source, Features::default())
}
pub fn profile_object_with_features(
    source: &str,
    features: Features,
) -> Result<(Vec<u8>, Profile), Error> {
    let mut profile = Profile::default();
    let start = std::time::Instant::now();
    let mut items = Vec::new();
    parse_source(source, 1, &mut items)?;
    validate_features(&items, features)?;
    profile.parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    profile.items = items.len();
    let object = assemble_items_inner::<true, _>(&items, &mut profile)?;
    Ok((object, profile))
}
fn assemble_items<T: std::borrow::Borrow<Item>>(items: &[(T, usize)]) -> Result<Vec<u8>, Error> {
    assemble_items_inner::<false, _>(items, &mut Profile::default())
}
fn assemble_items_inner<const PROFILE: bool, T: std::borrow::Borrow<Item>>(
    items: &[(T, usize)],
    profile: &mut Profile,
) -> Result<Vec<u8>, Error> {
    // Start optimistically short, then only widen. Alignment (especially max-skip)
    // is not monotonic: recheck *every* remaining short branch after each layout.
    // At least one branch widens per retry, so this terminates without cycles.
    let mut short: Vec<bool> = items
        .iter()
        .map(|(item, _)| {
            matches!(item.borrow(),
        Item::X86Instruction(i) if matches!(i.opcode, x86::Opcode::Jmp | x86::Opcode::Jcc(_))
            && matches!(i.operands.as_slice(), [x86::Operand::Immediate(address)]
                if address.modifier == Modifier::None && address.expression.add.is_some()
                    && address.expression.subtract.is_none()))
        })
        .collect();
    let mut encodings = vec![None; items.len()];
    let mut encoded_bytes = Vec::new();
    loop {
        let start = PROFILE.then(std::time::Instant::now);
        let mut a = Assembler::new()?;
        let mut branches = Vec::new();
        for (index, (item, line)) in items.iter().enumerate() {
            let item = item.borrow();
            let fix = a.fixes.len();
            let result = if let Item::Text(t) = item {
                t.lines()
                    .try_for_each(|directive| a.directive(directive, *line))
            } else if let Item::X86Instruction(i) = item {
                a.instruction_encoding(
                    i,
                    *line,
                    short[index],
                    &mut encodings[index],
                    &mut encoded_bytes,
                )
            } else {
                a.item(item, *line)
            };
            result.map_err(|e| Error::new(*line, e))?;
            if short[index] {
                branches.push((index, fix));
            }
        }
        if let Some(start) = start {
            profile
                .layout_ms
                .push(start.elapsed().as_secs_f64() * 1000.0);
        }
        let start = PROFILE.then(std::time::Instant::now);
        let mut changed = 0;
        for (index, fix) in branches {
            let f = &a.fixes[fix];
            let name = f.address.expression.add.as_deref().unwrap();
            let fits = a.local_symbol(name, 0)
                && a.eval(&f.address.expression, 0)
                    .is_ok_and(|(section, value)| {
                        section == Some(f.section)
                            && (-128..=127).contains(&(value - i128::from(f.offset)))
                    });
            if !fits {
                short[index] = false;
                changed += 1;
            }
        }
        if let Some(start) = start {
            profile
                .branch_check_ms
                .push(start.elapsed().as_secs_f64() * 1000.0);
            profile.widened.push(changed);
        }
        let start = PROFILE.then(std::time::Instant::now);
        if changed == 0 {
            let object = a.object()?;
            if let Some(start) = start {
                profile.object_ms = start.elapsed().as_secs_f64() * 1000.0;
            }
            return Ok(object);
        }
        drop(a);
        if let Some(start) = start {
            profile
                .discard_ms
                .push(start.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
impl Assembler {
    fn new() -> Result<Self, Error> {
        let mut a = Self::default();
        a.section(".text", Some(6), false)
            .map_err(|e| Error::new(0, e))?;
        Ok(a)
    }
    // Binding-sensitive expressions must reach the linker, including aliases
    // whose underlying definition is interposable.
    fn local_symbol(&self, name: &str, depth: usize) -> bool {
        if depth > 64 {
            return false;
        }
        self.symbols.get(name).is_some_and(|s| {
            !s.global
                && !s.weak
                && s.alias.as_ref().is_none_or(|e| {
                    e.add
                        .as_deref()
                        .is_none_or(|n| self.local_symbol(n, depth + 1))
                        && e.subtract
                            .as_deref()
                            .is_none_or(|n| self.local_symbol(n, depth + 1))
                })
        })
    }
    fn item(&mut self, item: &Item, line: usize) -> Result<(), String> {
        match item {
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
        self.section_metadata(name, flags, nobits, 0, None)
    }
    fn section_metadata(
        &mut self,
        name: &str,
        flags: Option<u64>,
        nobits: bool,
        entry_size: u64,
        group: Option<String>,
    ) -> Result<(), String> {
        let nobits = nobits || matches!(name, ".bss" | ".tbss");
        if let Some(i) = self
            .sections
            .iter()
            .position(|s| s.name == name && s.group == group)
        {
            let s = &self.sections[i];
            // A bare .section name switches back without redeclaring attributes.
            if flags
                .is_some_and(|f| f != s.flags || nobits != s.nobits || entry_size != s.entry_size)
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
        sections::validate(name, flags, nobits, entry_size, group.as_deref())?;
        self.current = self.sections.len();
        self.sections.push(Section {
            name: name.into(),
            flags,
            nobits,
            entry_size,
            group,
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
    fn instruction_encoding(
        &mut self,
        i: &x86::Instruction,
        line: usize,
        short: bool,
        cached: &mut Option<std::ops::Range<usize>>,
        encoded_bytes: &mut Vec<u8>,
    ) -> Result<(), String> {
        if self.sections[self.current].nobits {
            return Err("instruction in NOBITS section".into());
        }
        if let Some(range) = cached {
            return self.bytes(&encoded_bytes[range.clone()]);
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
        let e = encode::encode(folded.as_ref().unwrap_or(i), short)?;
        // Cache only symbol-free encodings, in one compact byte arena. Keeping
        // a heap allocation and symbolic fixups for every instruction costs
        // more memory/cache traffic than reencoding the symbolic minority.
        // Folded absolute expressions can depend on layout and are never cached.
        if folded.is_none() && e.fixups.is_empty() {
            let start = encoded_bytes.len();
            encoded_bytes.extend_from_slice(&e.bytes);
            *cached = Some(start..encoded_bytes.len());
        }
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
            Directive::X86Integer { width, address } if matches!(width, 1 | 2 | 4 | 8) => {
                self.integer(*width, address.clone(), line)
            }
            Directive::ElfSection {
                name,
                flags,
                nobits,
            } => self.section(name, Some(*flags), *nobits),
            Directive::ElfSectionMetadata {
                name,
                flags,
                nobits,
                entry_size,
                group,
            } => self.section_metadata(name, Some(*flags), *nobits, *entry_size, group.clone()),
            Directive::ElfType { name, function } => {
                self.symbols.entry(name.clone()).or_default().kind = if *function {
                    SymbolKind::Text
                } else {
                    SymbolKind::Data
                };
                Ok(())
            }
            Directive::Size { name, expression } => {
                let e = self.capture_dot(expression.clone())?;
                self.symbols.entry(name.clone()).or_default().size = Some(e);
                Ok(())
            }

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
            Directive::Cfi(cfi) => metadata::shared_cfi(self, cfi),
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
        let assignment = x86::split_inline(s, '=');
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
        let parts = x86::split_inline(args, ',');
        let num = |s: &str| -> Result<u64, String> {
            u64::try_from(x86::integer(s)?).map_err(|_| "expected nonnegative integer".into())
        };
        match op {
            "" => Ok(()),
            ".intel_syntax" if args == "noprefix" => Ok(()),
            ".text" | ".data" | ".bss" if args.is_empty() => self.section(op, None, op == ".bss"),
            ".section" => {
                let section = sections::parse(&parts)?;
                self.section_metadata(
                    &section.name,
                    section.flags,
                    section.nobits,
                    section.entry_size,
                    section.group,
                )
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
                let fill = parts
                    .get(1)
                    .filter(|s| !s.is_empty())
                    .map(|s| x86::integer(s).map(|n| n as u8))
                    .transpose()?;
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
        let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, section) in self.sections.iter().enumerate() {
            if let Some(signature) = &section.group {
                groups.entry(signature.clone()).or_default().push(i);
            }
        }
        let synthetic_signatures: BTreeSet<_> = groups
            .keys()
            .filter(|name| {
                self.symbols
                    .get(*name)
                    .is_none_or(|s| s.section.is_none() && s.alias.is_none() && s.common.is_none())
            })
            .cloned()
            .collect();
        for signature in groups.keys() {
            self.symbols.entry(signature.clone()).or_default();
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
        let identity: BTreeSet<_> = self
            .fixes
            .iter()
            .filter(|f| {
                // A 32-bit data difference against this section becomes PC32
                // below. An absolute positive symbol still needs its identity
                // in that relocation, just like an explicit PC-relative fixup.
                let pc_relative = matches!(f.kind, 2 | 4)
                    || f.kind == 10
                        && f.width == 4
                        && f.address
                            .expression
                            .subtract
                            .as_deref()
                            .is_some_and(|name| {
                                self.eval_symbol(name, 0)
                                    .is_ok_and(|(section, _)| section == Some(f.section))
                            });
                f.address.modifier != Modifier::None
                    || pc_relative
                        && f.address.expression.add.as_deref().is_some_and(|name| {
                            self.eval_symbol(name, 0)
                                .is_ok_and(|(section, _)| section.is_none())
                        })
            })
            .filter_map(|f| f.address.expression.add.as_deref())
            .collect();
        let mut omitted = BTreeSet::new();
        let mut symbols = BTreeMap::new();
        // Lookup order is irrelevant during layout; ELF symbol order is stable.
        let mut entries: Vec<_> = self.symbols.iter().collect();
        entries.sort_unstable_by_key(|(name, _)| *name);
        for (name, s) in entries {
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
            if name.starts_with(".L")
                && !s.global
                && !s.weak
                && !s.hidden
                && s.common.is_none()
                && !tls
                && !identity.contains(name.as_str())
                && !groups.contains_key(name)
                && (section.is_some() || s.alias.is_some())
                && self.local_symbol(name, 0)
            {
                omitted.insert(name.clone());
                continue;
            }
            let synthetic = synthetic_signatures.contains(name);
            let undefined = section.is_none() && s.alias.is_none() && !synthetic;
            let symbol = Symbol {
                name: name.as_bytes().to_vec(),
                value,
                size,
                kind: if tls { SymbolKind::Tls } else { s.kind },
                scope: if s.global || undefined {
                    SymbolScope::Linkage
                } else {
                    SymbolScope::Compilation
                },
                weak: s.weak,
                section: match section {
                    Some(i) => SymbolSection::Section(sections[i]),
                    // A temporary placeholder, patched to the generated .group
                    // section after serialization. Keep it out of layout/evaluation.
                    None if synthetic || s.alias.is_some() => SymbolSection::Absolute,
                    None => SymbolSection::Undefined,
                },
                flags: SymbolFlags::Elf {
                    st_info: ((if s.weak {
                        2
                    } else if s.global || undefined {
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
        for (signature, members) in &groups {
            object.add_comdat(Comdat {
                kind: ComdatKind::Any,
                symbol: symbols[signature],
                sections: members.iter().map(|&i| sections[i]).collect(),
            });
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
            } else if matches!(kind, 2 | 4 | 15)
                && defined.is_some_and(|(s, _)| s == Some(f.section))
                && self.local_symbol(&name, 0)
            {
                defined.map(|(_, n)| n + addend - i128::from(f.offset))
            } else {
                None
            };
            if let Some(value) = absolute {
                let bits = u32::from(f.width) * 8;
                let signed = matches!(kind, 2 | 4 | 15 | 11 | 23);
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
            let symbol = if (omitted.contains(&name) || self.symbols[&name].alias.is_some())
                && f.address.modifier == Modifier::None
                && self.local_symbol(&name, 0)
                && self.symbols[&name].kind != SymbolKind::Tls
                && !defined.is_some_and(|(section, _)| {
                    section.is_some_and(|sec| self.sections[sec].flags & 0x400 != 0)
                }) {
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
        let mut bytes = object.write().map_err(|e| err(e.to_string()))?;
        sections::finalize(&mut bytes, &self.sections, &groups, &synthetic_signatures)
            .map_err(err)?;
        Ok(bytes)
    }
}
