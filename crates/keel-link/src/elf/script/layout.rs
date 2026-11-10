//! Script layout and emission. Input identities survive aggregation so local
//! symbols and relocation places retain their original offsets.
use super::{Assignment, Command, Expr, Output, Phdr, Script};
use crate::{
    Result,
    elf::{
        Executable, Options, PAGE, Section, Symbol, Unit, end, put16, put32, put64, reloc::x86_64,
        shdr, string, symbol_and_section_tables, symbol_record,
    },
};
use object::SymbolSection;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug)]
struct Value {
    n: u64,
    section: Option<usize>,
    absolute: bool,
}
impl Value {
    fn number(n: u64) -> Self {
        Self {
            n,
            section: None,
            absolute: false,
        }
    }
    fn absolute(n: u64) -> Self {
        Self {
            n,
            section: None,
            absolute: true,
        }
    }
}
#[derive(Clone)]
enum Definition {
    Expression {
        expr: Expr,
        dot: Value,
        context: Option<usize>,
        provide: bool,
    },
    Captured {
        value: Value,
        provide: bool,
    },
}
impl Definition {
    fn provide(&self) -> bool {
        match self {
            Self::Expression { provide, .. } | Self::Captured { provide, .. } => *provide,
        }
    }
}
#[derive(Clone, Copy)]
struct Placement {
    output: usize,
    offset: u64,
}
struct OutputSection {
    name: String,
    vma: u64,
    lma: u64,
    offset: u64,
    size: u64,
    align: u64,
    flags: u64,
    kind: u32,
    data: Vec<u8>,
    phdrs: Vec<usize>,
    holes: Vec<(u64, u64)>,
}
struct Layout<'a> {
    inputs: &'a [Section],
    units: &'a [Unit<'a>],
    globals: &'a crate::elf::Globals<'a>,
    placements: Vec<Option<Placement>>,
    used: Vec<bool>,
    outputs: Vec<OutputSection>,
    definitions: BTreeMap<String, Definition>,
    resolved: RefCell<BTreeMap<String, Value>>,
    used_provides: RefCell<BTreeSet<String>>,
    dot: Value,
    assertions: Vec<(Expr, Value, Option<usize>)>,
}

fn address_align(n: u64, alignment: u64) -> Result<u64> {
    if !alignment.is_power_of_two() {
        return Err(format!("invalid script alignment {alignment}").into());
    }
    n.checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
        .ok_or("script address alignment overflow".into())
}
fn wildcard(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.as_bytes(), name.as_bytes());
    let (mut pi, mut ni, mut star, mut retry) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            pi += 1;
            retry = ni;
        } else if let Some(s) = star {
            retry += 1;
            ni = retry;
            pi = s + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}
impl Layout<'_> {
    fn symbol(&self, ui: usize, symbol: &Symbol, depth: usize) -> Result<Value> {
        if depth > 128 {
            return Err(format!(
                "cyclic script symbol expression involving `{}`",
                symbol.name
            )
            .into());
        }
        if symbol.global() {
            if self.definitions.contains_key(symbol.name) {
                return self.named(symbol.name, depth + 1);
            }
            if let Some(&(u, s)) = self.globals.get(&symbol.name) {
                return self.raw_symbol(u, &self.units[u].symbols[&s]);
            }
        }
        self.raw_symbol(ui, symbol)
    }
    fn raw_symbol(&self, ui: usize, symbol: &Symbol) -> Result<Value> {
        match symbol.section {
            SymbolSection::Absolute => Ok(Value::absolute(symbol.value)),
            SymbolSection::Section(index) => {
                let input = self.units[ui]
                    .sections
                    .get(&index.0)
                    .ok_or_else(|| format!("reference to omitted section for `{}`", symbol.name))?;
                let place = self.placements[*input].ok_or_else(|| {
                    format!(
                        "reference to unplaced or discarded section for `{}`",
                        symbol.name
                    )
                })?;
                Ok(Value {
                    n: self.outputs[place.output]
                        .vma
                        .wrapping_add(place.offset)
                        .wrapping_add(symbol.value),
                    section: Some(place.output),
                    absolute: false,
                })
            }
            SymbolSection::Undefined if symbol.weak() => Ok(Value::absolute(0)),
            _ => Err(format!("unresolved script symbol `{}`", symbol.name).into()),
        }
    }
    fn named(&self, name: &str, depth: usize) -> Result<Value> {
        if depth > 128 {
            return Err(format!("cyclic script symbol expression involving `{name}`").into());
        }
        if self.definitions.get(name).is_some_and(Definition::provide) {
            self.used_provides.borrow_mut().insert(name.to_owned());
        }
        if let Some(value) = self.resolved.borrow().get(name).copied() {
            return Ok(value);
        }
        let value = if let Some(def) = self.definitions.get(name) {
            match def {
                Definition::Captured { value, .. } => *value,
                Definition::Expression {
                    expr, dot, context, ..
                } => {
                    let mut value = self.eval(expr, *dot, depth + 1)?;
                    if !value.absolute && value.section.is_none() {
                        if let Some(index) = *context {
                            value.n = self.outputs[index].vma.wrapping_add(value.n);
                            value.section = Some(index);
                        } else {
                            value.absolute = true;
                        }
                    }
                    value
                }
            }
        } else {
            let &(ui, si) = self
                .globals
                .get(name)
                .ok_or_else(|| format!("unresolved script symbol `{name}`"))?;
            self.raw_symbol(ui, &self.units[ui].symbols[&si])?
        };
        self.resolved.borrow_mut().insert(name.to_owned(), value);
        Ok(value)
    }
    fn eval(&self, expr: &Expr, dot: Value, depth: usize) -> Result<Value> {
        if depth > 256 {
            return Err("script expression recursion limit exceeded".into());
        }
        match expr {
            Expr::Number(n) => Ok(Value::number(*n)),
            Expr::Name(name) if name == "." => Ok(dot),
            Expr::Name(name) => self.named(name, depth + 1),
            Expr::Unary(op, arg) => {
                let mut v = self.eval(arg, dot, depth + 1)?;
                v.n = match op.as_str() {
                    "~" => !v.n,
                    "-" => v.n.wrapping_neg(),
                    "+" => v.n,
                    "!" => u64::from(v.n == 0),
                    _ => unreachable!(),
                };
                Ok(v)
            }
            Expr::Binary(op, left, right) => {
                let a = self.eval(left, dot, depth + 1)?;
                if op == "||" && a.n != 0 {
                    return Ok(Value::number(1));
                }
                if op == "&&" && a.n == 0 {
                    return Ok(Value::number(0));
                }
                let b = self.eval(right, dot, depth + 1)?;
                let n = match op.as_str() {
                    "+" => a.n.wrapping_add(b.n),
                    "-" => a.n.wrapping_sub(b.n),
                    "*" => a.n.wrapping_mul(b.n),
                    "/" => a.n.checked_div(b.n).ok_or("script division by zero")?,
                    "%" => a.n.checked_rem(b.n).ok_or("script division by zero")?,
                    "<<" => {
                        a.n.checked_shl(b.n.try_into().unwrap_or(u32::MAX))
                            .ok_or("script shift out of range")?
                    }
                    ">>" => {
                        a.n.checked_shr(b.n.try_into().unwrap_or(u32::MAX))
                            .ok_or("script shift out of range")?
                    }
                    "&" => a.n & b.n,
                    "|" => a.n | b.n,
                    "^" => a.n ^ b.n,
                    "==" => u64::from(a.n == b.n),
                    "!=" => u64::from(a.n != b.n),
                    "<" => u64::from(a.n < b.n),
                    ">" => u64::from(a.n > b.n),
                    "<=" => u64::from(a.n <= b.n),
                    ">=" => u64::from(a.n >= b.n),
                    "||" => u64::from(a.n != 0 || b.n != 0),
                    "&&" => u64::from(a.n != 0 && b.n != 0),
                    _ => unreachable!(),
                };
                let comparison = matches!(
                    op.as_str(),
                    "==" | "!=" | "<" | ">" | "<=" | ">=" | "||" | "&&"
                );
                let section = if comparison || a.section.is_some() && a.section == b.section {
                    None
                } else {
                    a.section.or(b.section)
                };
                Ok(Value {
                    n,
                    section,
                    absolute: !comparison && section.is_none() && (a.absolute || b.absolute),
                })
            }
            Expr::Call(name, args) if matches!(name.as_str(), "ADDR" | "LOADADDR" | "SIZEOF") => {
                let Expr::Name(section) = &args[0] else {
                    return Err(format!("{name} requires an output section name").into());
                };
                let out = self
                    .outputs
                    .iter()
                    .position(|s| s.name == *section)
                    .ok_or_else(|| format!("{name}: unknown output section {section}"))?;
                let s = &self.outputs[out];
                Ok(match name.as_str() {
                    "ADDR" => Value {
                        n: s.vma,
                        section: Some(out),
                        absolute: false,
                    },
                    "LOADADDR" => Value::absolute(s.lma),
                    _ => Value::number(s.size),
                })
            }
            Expr::Call(name, args) => {
                let a = self.eval(&args[0], dot, depth + 1)?;
                match name.as_str() {
                    "ABSOLUTE" => Ok(Value::absolute(a.n)),
                    "ALIGN" => {
                        let (value, alignment) = if args.len() == 1 {
                            (dot, a.n)
                        } else {
                            (a, self.eval(&args[1], dot, depth + 1)?.n)
                        };
                        Ok(Value {
                            n: address_align(value.n, alignment)?,
                            ..value
                        })
                    }
                    "MAX" | "MIN" => {
                        let b = self.eval(&args[1], dot, depth + 1)?;
                        Ok(if (name == "MAX") == (a.n >= b.n) {
                            a
                        } else {
                            b
                        })
                    }
                    _ => unreachable!(),
                }
            }
            Expr::Assert(condition, message) => {
                if self.eval(condition, dot, depth + 1)?.n == 0 {
                    Err(format!("linker script assertion failed: {message}").into())
                } else {
                    Ok(Value::number(1))
                }
            }
        }
    }
    fn assign(&mut self, a: &Assignment, context: Option<usize>) -> Result<()> {
        self.resolved.get_mut().clear();
        if a.name == "." {
            // ASSERT evaluates lazily and returns one when used as an expression.
            if matches!(a.expr, Expr::Assert(..)) {
                if context.is_some() {
                    return Err(
                        "ASSERT location assignments inside output sections are unsupported".into(),
                    );
                }
                self.assertions.push((a.expr.clone(), self.dot, context));
                self.dot = Value::absolute(1);
                return Ok(());
            }
            let mut value = self.eval(&a.expr, self.dot, 0)?;
            if let Some(out) = context {
                if value.section.is_none() && !value.absolute {
                    value.n = self.outputs[out]
                        .vma
                        .checked_add(value.n)
                        .ok_or("script address overflow")?;
                }
                if value.n < self.dot.n {
                    return Err(
                        "script location counter moves backwards inside output section".into(),
                    );
                }
                let size = value
                    .n
                    .checked_sub(self.outputs[out].vma)
                    .ok_or("invalid output section size")?;
                end(0, size)?;
                let o = &mut self.outputs[out];
                o.holes.push((o.size, size));
                o.size = size;
                value.section = Some(out);
            }
            self.dot = value;
        } else if !a.provide
            || !self.globals.contains_key(a.name.as_str())
                && !self.definitions.contains_key(&a.name)
        {
            // Freeze dot at the assignment; defer forward aliases until addresses exist.
            let definition = if self.definitions.contains_key(&a.name) && a.expr.references(&a.name)
            {
                Definition::Captured {
                    value: self.eval(&a.expr, self.dot, 0)?,
                    provide: a.provide,
                }
            } else {
                Definition::Expression {
                    expr: a.expr.clone(),
                    dot: self.dot,
                    context,
                    provide: a.provide,
                }
            };
            self.definitions.insert(a.name.clone(), definition);
        }
        self.resolved.get_mut().clear();
        Ok(())
    }
    fn input(&mut self, patterns: &[(String, bool)], output: Option<usize>) -> Result<()> {
        self.resolved.get_mut().clear();
        let mut matches: Vec<_> = self
            .inputs
            .iter()
            .enumerate()
            .filter(|(i, s)| {
                !self.used[*i]
                    && patterns.iter().any(|(pattern, _)| {
                        wildcard(
                            if pattern == "COMMON" {
                                ".bss.common"
                            } else {
                                pattern
                            },
                            &s.name,
                        )
                    })
            })
            .map(|(i, _)| i)
            .collect();
        if patterns.first().is_some_and(|(_, sort)| *sort) {
            matches.sort_by(|&a, &b| self.inputs[a].name.cmp(&self.inputs[b].name));
        }
        for index in matches {
            self.used[index] = true;
            let Some(out) = output else { continue };
            let input = &self.inputs[index];
            let address = address_align(self.dot.n, input.align)?;
            let offset = address
                .checked_sub(self.outputs[out].vma)
                .ok_or("script section offset underflow")?;
            let size = end(offset, input.size)?;
            let o = &mut self.outputs[out];
            o.holes.push((o.size, offset));
            o.size = size;
            o.align = o.align.max(input.align);
            if self.placements.iter().flatten().any(|p| p.output == out)
                && (o.flags ^ input.flags) & 0x400 != 0
            {
                return Err("TLS and non-TLS inputs cannot share a script output".into());
            }
            if o.flags & 0x400 != 0 && (o.kind == 8) != input.bss {
                return Err(
                    "initialized and zero-filled TLS require separate script outputs".into(),
                );
            }
            o.flags |= input.flags;
            if !input.bss {
                o.kind = input.kind;
            }
            self.placements[index] = Some(Placement {
                output: out,
                offset,
            });
            self.dot = Value {
                n: address
                    .checked_add(input.size)
                    .ok_or("script address overflow")?,
                section: Some(out),
                absolute: false,
            };
        }
        Ok(())
    }
    fn output(
        &mut self,
        command: &Output,
        phdrs: &[Phdr],
        inherited: &mut Vec<usize>,
    ) -> Result<()> {
        self.resolved.get_mut().clear();
        if command.name == "/DISCARD/" {
            for cmd in &command.body {
                let Command::Input(patterns) = cmd else {
                    return Err("only input selectors are supported in /DISCARD/".into());
                };
                self.input(patterns, None)?;
            }
            return Ok(());
        }
        if self.outputs.iter().any(|o| o.name == command.name) {
            return Err(format!("duplicate output section {}", command.name).into());
        }
        let used = &self.used;
        let alignment = command
            .body
            .iter()
            .filter_map(|c| {
                if let Command::Input(patterns) = c {
                    Some(patterns)
                } else {
                    None
                }
            })
            .flatten()
            .flat_map(|(p, _)| {
                self.inputs
                    .iter()
                    .enumerate()
                    .filter(move |(i, s)| {
                        !used[*i]
                            && wildcard(if p == "COMMON" { ".bss.common" } else { p }, &s.name)
                    })
                    .map(|(_, s)| s.align)
            })
            .max()
            .unwrap_or(1);
        let vma = if let Some(expr) = &command.address {
            self.eval(expr, self.dot, 0)?.n
        } else {
            address_align(self.dot.n, alignment)?
        };
        let out = self.outputs.len();
        if !command.phdrs.is_empty() {
            inherited.clear();
            for name in &command.phdrs {
                if name == "NONE" {
                    continue;
                }
                inherited.push(
                    phdrs
                        .iter()
                        .position(|p| p.name == *name)
                        .ok_or_else(|| format!("unknown PHDR `{name}`"))?,
                );
            }
        }
        if inherited.iter().filter(|&&i| phdrs[i].kind == 1).count() > 1 {
            return Err("an output section in multiple LOAD segments is unsupported".into());
        }
        let fill = if let Some(expr) = &command.fill {
            u32::try_from(self.eval(expr, self.dot, 0)?.n)
                .map_err(|_| "script fill exceeds 32 bits")?
                .to_be_bytes()
        } else {
            [0; 4]
        };
        let previous_delta = self
            .outputs
            .iter()
            .rev()
            .find(|o| o.flags & 2 != 0)
            .map_or(0, |o| o.vma.wrapping_sub(o.lma));
        self.outputs.push(OutputSection {
            name: command.name.clone(),
            vma,
            lma: vma.wrapping_sub(previous_delta),
            offset: 0,
            size: 0,
            align: alignment,
            flags: 0,
            kind: 8,
            data: Vec::new(),
            phdrs: inherited.clone(),
            holes: Vec::new(),
        });
        if let Some(expr) = &command.load {
            self.outputs[out].lma = self.eval(expr, self.dot, 0)?.n;
        } else if command.address.is_some() || command.info {
            self.outputs[out].lma = vma;
        }
        self.dot = Value {
            n: vma,
            section: Some(out),
            absolute: false,
        };
        for c in &command.body {
            match c {
                Command::Assign(a) => self.assign(a, Some(out))?,
                Command::Assert(e) => self.assertions.push((e.clone(), self.dot, Some(out))),
                Command::Input(patterns) => self.input(patterns, Some(out))?,
                Command::Output(_) => return Err("nested output sections are unsupported".into()),
            }
        }
        let o = &mut self.outputs[out];
        if o.size != 0 && o.flags == 0 {
            o.flags = 3;
        }
        if command.info {
            o.flags &= !2;
        }
        // A section containing only location-counter reservations is NOBITS.
        // A section with any file-backed input materializes all its holes.
        if o.kind != 8 {
            o.data.resize(o.size as usize, 0);
            for &(start, end) in &o.holes {
                for (i, byte) in o.data[start as usize..end as usize].iter_mut().enumerate() {
                    *byte = fill[i % 4];
                }
            }
        }
        self.dot = Value::absolute(vma.checked_add(o.size).ok_or("script address overflow")?);
        self.resolved.get_mut().clear();
        Ok(())
    }
}

pub(in crate::elf) fn link(
    mut script: Script,
    mut sections: Vec<Section>,
    mut units: Vec<Unit>,
    globals: crate::elf::Globals<'_>,
    options: &Options,
    warnings: Vec<String>,
) -> Result<Executable> {
    let page = options.max_page_size.unwrap_or(PAGE);
    let entry_name = &options.entry;
    // Allocate common definitions before script matching, preserving input symbol IDs.
    let common_index = sections.len();
    let mut common = Section {
        merge: None,
        name: ".bss.common".into(),
        flags: 3,
        kind: 8,
        align: 1,
        size: 0,
        bss: true,
        data: Vec::new(),
        offset: 0,
    };
    let mut common_offsets = BTreeMap::new();
    for unit in &units {
        for symbol in unit
            .symbols
            .values()
            .filter(|s| s.section == SymbolSection::Common)
        {
            let selected = globals
                .get(&symbol.name)
                .is_some_and(|&(ui, si)| units[ui].symbols[&si].section == SymbolSection::Common);
            if !selected {
                continue;
            }
            let entry = common_offsets
                .entry(symbol.name)
                .or_insert((0u64, 1u64, 0u64));
            entry.0 = entry.0.max(symbol.size);
            entry.1 = entry.1.max(symbol.value.max(1));
        }
    }
    for (size, alignment, offset) in common_offsets.values_mut() {
        *offset = address_align(common.size, *alignment)?;
        common.size = end(*offset, *size)?;
        common.align = common.align.max(*alignment);
    }
    for unit in &mut units {
        let synthetic = unit
            .symbols
            .values()
            .filter_map(|s| s.section.index().map(|i| i.0))
            .max()
            .unwrap_or(0)
            .max(unit.sections.last_key().unwrap_or(0))
            + 1;
        unit.sections.insert(synthetic, common_index);
        for symbol in unit
            .symbols
            .values_mut()
            .filter(|s| s.section == SymbolSection::Common)
        {
            let Some(&(size, _, offset)) = common_offsets.get(&symbol.name) else {
                continue;
            };
            symbol.section = SymbolSection::Section(object::SectionIndex(synthetic));
            symbol.value = offset;
            symbol.size = size;
        }
    }
    sections.push(common);
    let imports = BTreeMap::new();
    let selected = crate::elf::selected::Index::new(&units, &globals, &imports);
    let tls_plan = crate::elf::tls::Plan::prepare(&mut sections, &units, &selected, false)?;
    // Bound orphan placement to the two compiler-generated classes present in
    // the pinned aggregate. Other orphans remain errors rather than guesses.
    for (name, anchors) in [
        (".eh_frame", &[".rodata", ".text"][..]),
        (".data.rel.ro", &[".data"][..]),
        (".got.tls", &[".data", ".tdata"][..]),
        (".text.tls", &[".text"][..]),
    ] {
        let selected = script.commands.iter().any(|c| match c {
            Command::Output(o) => o.body.iter().any(
                |c| matches!(c, Command::Input(ps) if ps.iter().any(|(p, _)| wildcard(p, name))),
            ),
            _ => false,
        });
        if !selected && sections.iter().any(|s| s.name == name && s.size != 0) {
            let anchor = anchors.iter().find_map(|anchor| script.commands.iter().position(|c| matches!(c, Command::Output(o) if o.name == *anchor)))
                .ok_or_else(|| format!("orphan {name} requires an explicit output section or supported placement anchor"))?;
            script.commands.insert(
                anchor + 1,
                Command::Output(Output {
                    name: name.into(),
                    address: None,
                    load: None,
                    body: vec![Command::Input(vec![(name.into(), false)])],
                    phdrs: Vec::new(),
                    fill: None,
                    info: false,
                }),
            );
        }
    }
    let mut layout = Layout {
        inputs: &sections,
        units: &units,
        globals: &globals,
        placements: vec![None; sections.len()],
        used: vec![false; sections.len()],
        outputs: Vec::new(),
        definitions: BTreeMap::new(),
        resolved: RefCell::new(BTreeMap::new()),
        used_provides: RefCell::new(BTreeSet::new()),
        dot: Value::absolute(0),
        assertions: Vec::new(),
    };
    let mut inherited = Vec::new();
    for command in &script.commands {
        match command {
            Command::Assign(a) => layout.assign(a, None)?,
            Command::Assert(e) => layout.assertions.push((e.clone(), layout.dot, None)),
            Command::Output(o) => layout.output(o, &script.phdrs, &mut inherited)?,
            Command::Input(_) => return Err("input selector outside output section".into()),
        }
    }
    for (i, input) in sections.iter().enumerate() {
        if !layout.used[i] && input.size != 0 {
            return Err(format!(
                "script leaves orphan input section `{}`; explicit placement is required",
                input.name
            )
            .into());
        }
    }
    for (expr, dot, _) in &layout.assertions {
        layout.eval(expr, *dot, 0)?;
    }
    // Resolve every ordinary script definition, including unused forward aliases.
    // PROVIDE is lazy, as GNU ld requires; unresolved unused provides are harmless.
    let mut script_values = BTreeMap::new();
    for (name, def) in &layout.definitions {
        if !def.provide() {
            script_values.insert(name.clone(), layout.named(name, 0)?);
        }
    }
    // Resolve input references first, including dependencies on other PROVIDEs.
    // A second pass emits every PROVIDE reached by input or script expressions.
    for (name, def) in &layout.definitions {
        if def.provide()
            && units
                .iter()
                .any(|u| u.symbols.values().any(|s| s.global() && s.name == *name))
        {
            layout.named(name, 0)?;
        }
    }
    for (name, def) in &layout.definitions {
        let used = layout.used_provides.borrow().contains(name);
        if def.provide() && used {
            script_values.insert(name.clone(), layout.named(name, 0)?);
        }
    }
    let entry = layout.named(entry_name, 0)?.n;
    // An absolute physical entry is valid when its LMA lies in executable code.
    if !layout.outputs.iter().any(|o| {
        o.flags & 4 != 0
            && (entry.wrapping_sub(o.vma) < o.size || entry.wrapping_sub(o.lma) < o.size)
    }) {
        return Err("script entry must lie in executable VMA or LMA".into());
    }
    emit(
        &mut layout,
        &tls_plan,
        &script.phdrs,
        &script_values,
        entry,
        warnings,
        page,
    )
}

fn emit(
    layout: &mut Layout<'_>,
    tls_plan: &crate::elf::tls::Plan,
    phdrs: &[Phdr],
    script_values: &BTreeMap<String, Value>,
    entry: u64,
    warnings: Vec<String>,
    page: u64,
) -> Result<Executable> {
    // With explicit PHDRS only those headers are emitted. Without PHDRS use
    // one LOAD per allocated output, avoiding assumptions about permission order.
    let automatic: Vec<_> = if phdrs.is_empty() {
        layout
            .outputs
            .iter()
            .enumerate()
            .filter(|(_, o)| o.size != 0 && o.flags & 2 != 0)
            .map(|(i, o)| (i, o.flags))
            .collect()
    } else {
        Vec::new()
    };
    let has_tls = layout.outputs.iter().any(|o| o.flags & 0x400 != 0);
    let automatic_tls = has_tls && phdrs.is_empty();
    if has_tls && !phdrs.is_empty() && phdrs.iter().filter(|p| p.kind == 7).count() != 1 {
        return Err("TLS script outputs require one PT_TLS header".into());
    }
    let phnum = phdrs.len().max(automatic.len()) + usize::from(automatic_tls);
    if 64 + phnum * 56 > page as usize {
        return Err("script program headers exceed first page".into());
    }
    let mut segment_alignments = BTreeMap::new();
    for (index, o) in layout
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| o.size != 0 && o.flags & 2 != 0)
    {
        let loads: Vec<_> = if phdrs.is_empty() {
            vec![index]
        } else {
            o.phdrs
                .iter()
                .copied()
                .filter(|&i| phdrs[i].kind == 1)
                .collect()
        };
        for load in loads {
            let a = segment_alignments.entry(load).or_insert(page);
            *a = (*a).max(o.align);
        }
    }
    let mut cursor = page;
    let mut segment_origins: BTreeMap<usize, (u64, u64, u64)> = BTreeMap::new();
    let mut tls_origin = None;
    for (index, o) in layout.outputs.iter_mut().enumerate() {
        if o.size == 0 {
            continue;
        }
        let load = if phdrs.is_empty() {
            Some(index)
        } else {
            o.phdrs.iter().copied().find(|&i| phdrs[i].kind == 1)
        };
        if o.flags & 2 != 0 {
            if let Some(load) = load {
                if let Some(&(vma, lma, offset)) = segment_origins.get(&load) {
                    let delta = o
                        .vma
                        .checked_sub(vma)
                        .ok_or("script segment VMA moves backwards")?;
                    if o.lma.checked_sub(lma) != Some(delta) {
                        return Err("script segment has inconsistent VMA/LMA delta".into());
                    }
                    o.offset = end(offset, delta)?;
                    if o.kind != 8 && o.offset < cursor {
                        return Err(
                            "script file-backed sections overlap or follow NOBITS within a segment"
                                .into(),
                        );
                    }
                } else {
                    let alignment = segment_alignments[&load];
                    o.offset = end(
                        address_align(cursor.saturating_sub(o.vma % alignment), alignment)?,
                        o.vma % alignment,
                    )?;
                    if o.offset < cursor {
                        o.offset = end(o.offset, alignment)?;
                    }
                    segment_origins.insert(load, (o.vma, o.lma, o.offset));
                }
            } else {
                o.offset = address_align(cursor, o.align)?;
            }
        } else {
            o.offset = address_align(cursor, o.align)?;
        }
        if o.flags & 0x400 != 0 {
            if let Some((vma, offset)) = tls_origin {
                o.offset = end(
                    offset,
                    o.vma
                        .checked_sub(vma)
                        .ok_or("TLS script VMA moves backwards")?,
                )?;
                if o.kind != 8 && o.offset < cursor {
                    return Err("TLS script file data overlaps".into());
                }
            } else {
                tls_origin = Some((o.vma, o.offset));
            }
        }
        if o.kind != 8 {
            cursor = end(o.offset, o.size)?;
        }
    }
    if layout
        .outputs
        .iter()
        .any(|o| o.size != 0 && o.flags & 0x400 == 0 && o.phdrs.iter().any(|&i| phdrs[i].kind == 7))
    {
        return Err("non-TLS output assigned to PT_TLS".into());
    }
    let tls_outputs: Vec<_> = layout
        .outputs
        .iter()
        .filter(|o| o.flags & 0x400 != 0)
        .collect();
    let tls = if let Some(first) = tls_outputs.first() {
        let alignment = tls_outputs.iter().map(|o| o.align).max().unwrap_or(1);
        if first.vma % alignment != 0 {
            return Err("script TLS base must satisfy template alignment".into());
        }
        let end = tls_outputs
            .iter()
            .map(|o| o.vma + o.size)
            .max()
            .unwrap_or(first.vma);
        let mut zero = false;
        for o in &tls_outputs {
            if zero && o.kind != 8 {
                return Err("initialized TLS follows zero-filled TLS".into());
            }
            zero |= o.kind == 8;
            if !phdrs.is_empty() && !o.phdrs.iter().any(|&p| phdrs[p].kind == 7) {
                return Err("TLS output is missing its PT_TLS assignment".into());
            }
        }
        if layout.outputs.iter().any(|o| {
            o.flags & 2 != 0
                && o.flags & 0x400 == 0
                && o.size != 0
                && o.vma < end
                && o.vma + o.size > first.vma
        }) {
            return Err("non-TLS output overlaps TLS template".into());
        }
        Some((first.vma, first.offset, end - first.vma, alignment))
    } else {
        None
    };
    let mut bytes = vec![0; cursor as usize];
    for o in &layout.outputs {
        if o.kind != 8 && o.size != 0 {
            bytes[o.offset as usize..(o.offset + o.size) as usize].copy_from_slice(&o.data);
        }
    }
    for (index, input) in layout.inputs.iter().enumerate() {
        if let Some(place) = layout.placements[index] {
            let o = &layout.outputs[place.output];
            if !input.bss && !input.data.is_empty() {
                let at = (o.offset + place.offset) as usize;
                bytes[at..at + input.data.len()].copy_from_slice(&input.data);
            }
        }
    }
    let address = |index: usize| -> Result<(u64, u64)> {
        let p = layout.placements[index].ok_or("TLS GOT input was discarded")?;
        let out = &layout.outputs[p.output];
        Ok((out.vma + p.offset, out.offset + p.offset))
    };
    if let Some((base, _, size, alignment)) = tls {
        tls_plan.emit_script(
            &mut bytes,
            address,
            |(u, i)| {
                layout
                    .raw_symbol(u, &layout.units[u].symbols[&i])?
                    .n
                    .checked_sub(base)
                    .ok_or("TLS symbol outside script template".into())
            },
            size,
            alignment,
        )?;
    }
    for (ui, unit) in layout.units.iter().enumerate() {
        for r in &unit.relocs {
            let Some(place) = layout.placements[r.section] else {
                continue;
            };
            let out = &layout.outputs[place.output];
            let symbol = unit
                .symbols
                .get(&r.symbol)
                .ok_or("relocation references invalid symbol index")?;
            let mut s = layout.symbol(ui, symbol, 0)?.n;
            let definition = if symbol.global() {
                layout
                    .globals
                    .get(&symbol.name)
                    .map(|&(u, i)| &layout.units[u].symbols[&i])
                    .unwrap_or(symbol)
            } else {
                symbol
            };
            if let Some(model) = crate::elf::tls::model(r.kind, false) {
                let key = if symbol.global() {
                    layout
                        .globals
                        .get(&symbol.name)
                        .copied()
                        .unwrap_or((ui, r.symbol))
                } else {
                    (ui, r.symbol)
                };
                s = tls_plan.script_address(key, model, address)?;
                if r.kind == 35 {
                    let at = (out.offset + place.offset + r.offset) as usize;
                    if bytes.get(at..at + 2) != Some(&[0xff, 0x10]) {
                        return Err("TLSDESC_CALL requires call *(%rax)".into());
                    }
                }
            } else if matches!(r.kind, 17 | 18 | 21 | 23) {
                if definition.info & 15 != object::elf::STT_TLS {
                    return Err("TLS relocation requires a TLS symbol".into());
                }
                let (base, _, size, alignment) = tls.ok_or("TLS relocation without template")?;
                s = s.checked_sub(base).ok_or("TLS symbol outside template")?;
                if matches!(r.kind, 18 | 23) {
                    s = s.wrapping_sub(address_align(size, alignment)?);
                }
            } else if definition.info & 15 == object::elf::STT_TLS {
                return Err("non-TLS script relocation against TLS symbol".into());
            }
            let p = out
                .vma
                .checked_add(place.offset)
                .and_then(|v| v.checked_add(r.offset))
                .ok_or("relocation place overflow")?;
            if x86_64::uses_got(r.kind) && r.kind != 22 {
                return Err(format!(
                    "{}: relocation {} requires unsupported script GOT/dynamic layout",
                    unit.name, r.kind
                )
                .into());
            }
            let field = x86_64::encode(r.kind, i128::from(s) + i128::from(r.addend), p).map_err(
                |e| match e {
                    x86_64::Error::Unsupported => {
                        format!("{}: unsupported x86-64 relocation {}", unit.name, r.kind)
                    }
                    x86_64::Error::Overflow => format!(
                        "{}: relocation {} overflow for `{}`",
                        unit.name, r.kind, symbol.name
                    ),
                },
            )?;
            let at = (out.offset + place.offset + r.offset) as usize;
            bytes[at..at + field.width].copy_from_slice(&field.bytes[..field.width]);
        }
    }
    if layout.outputs.iter().filter(|o| o.size != 0).count() + 4 >= 0xff00 {
        return Err("too many script output sections".into());
    }
    let mut names = vec![0];
    let mut headers = vec![[0; 64]];
    let mut section_indices = vec![0; layout.outputs.len()];
    for (index, o) in layout.outputs.iter().enumerate() {
        if o.size == 0 {
            continue;
        }
        section_indices[index] = headers.len() as u16;
        headers.push(shdr(
            string(&mut names, &o.name),
            o.kind,
            o.flags,
            o.vma,
            o.offset,
            o.size,
            0,
            0,
            o.align,
            if matches!(o.kind, 14..=16) { 8 } else { 0 },
        ));
    }
    let mut strings = vec![0];
    let mut symbols = vec![0; 24];
    let mut record = |name: &str, value: Value, size: u64, info: u8, other: u8| -> Result<()> {
        let index = value.section.map_or(0xfff1, |i| section_indices[i]);
        // Empty outputs have no section header; boundary symbols remain absolute.
        let index = if index == 0 { 0xfff1 } else { index };
        symbols.extend_from_slice(&symbol_record(
            string(&mut strings, name),
            info,
            other,
            index,
            if info & 15 == object::elf::STT_TLS {
                value.n - tls.ok_or("TLS symbol without template")?.0
            } else {
                value.n
            },
            size,
        ));
        Ok(())
    };
    let mut locals = 1;
    for (ui, unit) in layout.units.iter().enumerate() {
        for symbol in unit
            .symbols
            .values()
            .filter(|s| !s.global() && !s.name.is_empty())
        {
            if let Ok(value) = layout.raw_symbol(ui, symbol) {
                record(symbol.name, value, symbol.size, symbol.info, symbol.other)?;
                locals += 1;
            }
        }
    }
    for (name, (ui, si)) in crate::elf::ordered(layout.globals) {
        if script_values.contains_key(name) {
            continue;
        }
        let symbol = &layout.units[ui].symbols[&si];
        if let Ok(value) = layout.raw_symbol(ui, symbol) {
            record(name, value, symbol.size, symbol.info, symbol.other)?;
        }
    }
    for (name, value) in script_values {
        record(name, *value, 0, 0x10, 0)?;
    }
    let shoff =
        symbol_and_section_tables(&mut bytes, &mut headers, names, &symbols, &strings, locals)?;
    end(0, bytes.len() as u64)?;
    bytes[..16].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 62);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, entry);
    put64(&mut bytes, 32, 64);
    put64(&mut bytes, 40, shoff);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(&mut bytes, 56, phnum as u16);
    put16(&mut bytes, 58, 64);
    put16(&mut bytes, 60, headers.len() as u16);
    put16(&mut bytes, 62, (headers.len() - 1) as u16);
    for i in 0..phnum {
        let members: Vec<_> = layout
            .outputs
            .iter()
            .enumerate()
            .filter(|(index, o)| {
                o.size != 0
                    && o.flags & 2 != 0
                    && if phdrs.is_empty() {
                        if automatic_tls && i == automatic.len() {
                            o.flags & 0x400 != 0
                        } else {
                            *index == automatic[i].0
                        }
                    } else {
                        o.phdrs.contains(&i)
                    }
            })
            .map(|(_, o)| o)
            .collect();
        let (kind, flags) = if phdrs.is_empty() {
            let flags = if automatic_tls && i == automatic.len() {
                0
            } else {
                automatic[i].1
            };
            (
                if automatic_tls && i == automatic.len() {
                    7
                } else {
                    1
                },
                4 | if flags & 1 != 0 { 2 } else { 0 } | if flags & 4 != 0 { 1 } else { 0 },
            )
        } else {
            (phdrs[i].kind, phdrs[i].flags as u64)
        };
        let at = 64 + 56 * i;
        put32(&mut bytes, at, kind);
        put32(&mut bytes, at + 4, flags as u32);
        if let Some(first) = members.first() {
            let file_end = members
                .iter()
                .filter(|o| o.kind != 8)
                .map(|o| o.offset + o.size)
                .max()
                .unwrap_or(first.offset);
            let memory_end = members.iter().map(|o| o.vma + o.size).max().unwrap();
            put64(&mut bytes, at + 8, first.offset);
            put64(&mut bytes, at + 16, first.vma);
            put64(&mut bytes, at + 24, first.lma);
            put64(&mut bytes, at + 32, file_end - first.offset);
            put64(&mut bytes, at + 40, memory_end - first.vma);
            put64(
                &mut bytes,
                at + 48,
                if kind == 1 {
                    members.iter().map(|o| o.align).max().unwrap_or(1).max(page)
                } else {
                    members.iter().map(|o| o.align).max().unwrap_or(1)
                },
            );
        }
    }
    Ok(Executable { bytes, warnings })
}
