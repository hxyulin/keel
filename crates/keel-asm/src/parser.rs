//! Phase-scoped assembler allocation experiment.
//!
//! One local `Bump` owns scratch for each layout pass, with no global allocator,
//! thread-local state, unsafe lifetime extension, or storage shared across calls.
//! Source names borrow the caller's UTF-8 source; synthesized names and rewritten
//! GOT expressions live in the pass arena. Labels, globals, assignments, atom
//! ownership and expressions share these references instead of cloning names.
//! Branch/data/address fixup queues, label order, LEB checks, sorted/deduplicated
//! symbol-name lists, debug rows/frames and CFI rules/events/bytes use bumpalo
//! collections. Ordered maps retain their ordering and diagnostics.
//!
//! The arena is declared before its users. Collections stay ordinary stack-owned
//! values: bumpalo Vec and its consuming iterator drop their elements, including
//! nested collections or heap-owning modifier Strings, even on early returns.
//! We never place a heap-owning value directly in `Bump::alloc`, whose reset/drop
//! would not run that value's destructor. Scratch is destroyed at each pass end;
//! the owned previous-layout map survives separately for symbolic LEB convergence.
//!
//! This is deliberately a substantial slice, not an all-arena allocator. Remaining
//! transient system allocations include BTreeMap nodes (stable std BTreeMap has no
//! stable custom-allocator interface), the previous-layout map and name copies,
//! operand overflow/normalization scratch, parsed string and scalar-data byte
//! buffers, modifier/addend formatting, debug file tables/directive operands,
//! compact-unwind/CIE bookkeeping, and Mach-O writer scratch. Vector growth may
//! retain superseded arena capacity until the pass ends, increasing peak memory.
//!
//! Assembly text/sections/relocations/symbols and Error messages remain normal
//! owned public values. Names are promoted to Strings when they enter output;
//! section buffers are built directly in their escaping Vecs to avoid copying
//! an entire arena-backed assembly. Nonfinal LEB passes still build and discard
//! owned Assemblies. `assemble_object` still writes via Assembly and returns an
//! independently owned Vec, paying that existing intermediate-output cost.
use self::text::Trim;
use crate::FieldId;
mod arithmetic;
mod atomic;
mod atoms;
mod compact;
mod control;
mod data;
mod debug;
mod float;
mod memory;
mod relocate;
mod scanning;
mod storage;
mod text;
mod scalable;

mod vector;

use super::{Assembly, Error, InstructionLocation, Symbol, encode};
use bumpalo::{Bump, collections::Vec as BumpVec};
use std::collections::BTreeMap;
use storage::{Operands, encode_named, lower, upper};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    General,
    Sp,
    Zero,
}
#[derive(Clone, Copy)]
struct Reg {
    number: u32,
    bits: u32,
    kind: Kind,
}

fn reg(s: &str, sp: bool, zero: bool) -> Result<Reg, String> {
    let s = lower(s);
    let r = match s.as_ref() {
        "sp" | "wsp" => Reg {
            number: 31,
            bits: if s == "sp" { 64 } else { 32 },
            kind: Kind::Sp,
        },
        "xzr" | "wzr" => Reg {
            number: 31,
            bits: if s == "xzr" { 64 } else { 32 },
            kind: Kind::Zero,
        },
        "fp" => Reg {
            number: 29,
            bits: 64,
            kind: Kind::General,
        },
        "lr" => Reg {
            number: 30,
            bits: 64,
            kind: Kind::General,
        },
        _ => {
            let bits = match s.as_bytes().first() {
                Some(b'x') => 64,
                Some(b'w') => 32,
                _ => return Err(format!("expected register, got `{s}`")),
            };
            let number = s[1..]
                .parse::<u32>()
                .map_err(|_| format!("invalid register `{s}`"))?;
            if number > 30 {
                return Err(format!("invalid register `{s}`; use sp or zr explicitly"));
            }
            Reg {
                number,
                bits,
                kind: Kind::General,
            }
        }
    };
    if (r.kind == Kind::Sp && !sp) || (r.kind == Kind::Zero && !zero) {
        return Err(format!("register `{s}` is not allowed in this operand"));
    }
    Ok(r)
}
fn same_width(a: Reg, b: Reg) -> Result<(), String> {
    if a.bits != b.bits {
        Err("register widths must match".into())
    } else {
        Ok(())
    }
}
fn integer(s: &str) -> Result<i64, String> {
    let s = s.trim_fast().strip_prefix('#').unwrap_or(s.trim_fast());
    let (negative, digits) = if let Some(s) = s.strip_prefix('-') {
        (true, s)
    } else {
        (false, s.strip_prefix('+').unwrap_or(s))
    };
    if digits.starts_with(['-', '+']) {
        return Err(format!("invalid integer `{s}`"));
    }
    let value = if let Some(s) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(s, 16)
    } else {
        digits.parse()
    }
    .map_err(|_| format!("invalid integer `{s}`"))?;
    Ok(if negative { -value } else { value })
}
fn unsigned(s: &str, max: i64) -> Result<u32, String> {
    let n = integer(s)?;
    if !(0..=max).contains(&n) {
        return Err(format!("immediate must be in 0..={max}"));
    }
    Ok(n as u32)
}
fn shift(s: &str) -> Result<(&str, u32), String> {
    let mut parts = s.split_whitespace();
    let kind = parts.next().ok_or("expected shift")?;
    let amount = unsigned(parts.next().ok_or("shift needs an amount")?, 63)?;
    if parts.next().is_some() {
        return Err("unexpected shift operand".into());
    }
    Ok((kind, amount))
}
fn count(args: &[&str], min: usize, max: usize) -> Result<(), String> {
    if args.len() < min || args.len() > max || args.iter().any(|a| a.is_empty()) {
        Err(format!(
            "expected {} operand(s)",
            if min == max {
                min.to_string()
            } else {
                format!("{min}..={max}")
            }
        ))
    } else {
        Ok(())
    }
}
fn is_integer(s: &str) -> bool {
    s.starts_with(['#', '-', '+']) || s.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

fn instruction(op: &str, a: &[&str]) -> Result<u32, String> {
    if let Some(word) = vector::extra(op, a)? {
        return Ok(word);
    }
    if let Some(word) = atomic::instruction(op, a)? {
        return Ok(word);
    }
    match op {
        "ldr" | "str" | "ldrb" | "strb" | "ldrh" | "strh" | "ldrsb" | "ldrsh" | "ldrsw"
        | "ldur" | "stur" | "ldurb" | "sturb" | "ldurh" | "sturh" | "ldursb" | "ldursh"
        | "ldursw" | "ldp" | "stp" | "ldar" | "ldarb" | "ldarh" | "stlr" | "stlrb" | "stlrh" => {
            memory::instruction(op, a)
        }
        "movz" | "movk" | "movn" => {
            count(a, 2, 3)?;
            let d = reg(a[0], false, true)?;
            let imm = unsigned(a[1], 65535)?;
            let (kind, amount) = if a.len() == 3 {
                shift(a[2])?
            } else {
                ("lsl", 0)
            };
            if kind != "lsl" || amount % 16 != 0 || amount >= d.bits {
                return Err(
                    "move-wide shift must be lsl by 0, 16, 32, or 48 within the register width"
                        .into(),
                );
            }
            encode_named(
                format_args!("{}_{}_movewide", upper(op), d.bits),
                &[
                    (FieldId::Rd, d.number),
                    (FieldId::imm16, imm),
                    (FieldId::hw, amount / 16),
                ],
            )
        }
        "add" | "sub" | "adds" | "subs" => {
            count(a, 3, 4)?;
            let flags = op.ends_with('s');
            let immediate = is_integer(a[2]);
            if !immediate
                && (a[..2]
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case("sp") || s.eq_ignore_ascii_case("wsp"))
                    || a.get(3).is_some_and(|s| {
                        let s = lower(s);
                        s.starts_with("uxt") || s.starts_with("sxt")
                    }))
            {
                return arithmetic::extended(op, a);
            }
            let d = reg(a[0], immediate && !flags, !immediate || flags)?;
            let n = reg(a[1], immediate, !immediate)?;
            same_width(d, n)?;
            if immediate {
                let imm = unsigned(a[2], 4095)?;
                let (kind, amount) = if a.len() == 4 {
                    shift(a[3])?
                } else {
                    ("lsl", 0)
                };
                if kind != "lsl" || ![0, 12].contains(&amount) {
                    return Err("add/sub immediate shift must be lsl #0 or lsl #12".into());
                }
                encode_named(
                    format_args!(
                        "{}_{}{}_addsub_imm",
                        upper(op),
                        d.bits,
                        if flags { "S" } else { "" }
                    ),
                    &[
                        (FieldId::Rd, d.number),
                        (FieldId::Rn, n.number),
                        (FieldId::imm12, imm),
                        (FieldId::sh, amount / 12),
                    ],
                )
            } else {
                let m = reg(a[2], false, true)?;
                same_width(d, m)?;
                let (kind, amount) = if a.len() == 4 {
                    shift(a[3])?
                } else {
                    ("lsl", 0)
                };
                let shift = match kind {
                    "lsl" => 0,
                    "lsr" => 1,
                    "asr" => 2,
                    _ => return Err("expected lsl, lsr, or asr".into()),
                };
                if amount >= d.bits {
                    return Err("shift exceeds register width".into());
                }
                encode_named(
                    format_args!("{}_{}_addsub_shift", upper(op), d.bits),
                    &[
                        (FieldId::Rd, d.number),
                        (FieldId::Rn, n.number),
                        (FieldId::Rm, m.number),
                        (FieldId::shift, shift),
                        (FieldId::imm6, amount),
                    ],
                )
            }
        }
        "cmp" | "cmn" => {
            count(a, 2, 3)?;
            let n = reg(a[0], true, true)?;
            let mut args = Operands::new();
            args.push(if n.bits == 64 { "xzr" } else { "wzr" });
            for arg in a {
                args.push(arg);
            }
            instruction(if op == "cmp" { "subs" } else { "adds" }, &args)
        }
        "mov" => {
            count(a, 2, 2)?;
            if is_integer(a[1]) {
                return arithmetic::move_immediate(a);
            }
            let d = reg(a[0], true, true)?;
            let n = reg(a[1], true, true)?;
            same_width(d, n)?;
            if d.kind == Kind::Sp || n.kind == Kind::Sp {
                instruction("add", &[a[0], a[1], "#0"])
            } else {
                encode_named(
                    format_args!("ORR_{}_log_shift", d.bits),
                    &[
                        (FieldId::Rd, d.number),
                        (FieldId::Rn, 31),
                        (FieldId::Rm, n.number),
                        (FieldId::shift, 0),
                        (FieldId::imm6, 0),
                    ],
                )
            }
        }
        "ret" => {
            count(a, 0, 1)?;
            let n = reg(a.first().copied().unwrap_or("x30"), false, true)?;
            if n.bits != 64 {
                return Err("ret requires a 64-bit register".into());
            }
            encode("RET_64R_branch_reg", &[(FieldId::Rn, n.number)])
        }
        "nop" => {
            count(a, 0, 0)?;
            encode("NOP_HI_hints", &[])
        }
        "movi" => vector::move_immediate(a),
        "br" | "blr" | "brk" | "svc" => control::instruction(op, a),
        "fmov" | "fabs" | "fneg" | "fsqrt" | "frintn" | "frintp" | "frintm" | "frintz" | "fadd"
        | "fsub" | "fmul" | "fdiv" | "fmin" | "fmax" | "fcsel" | "fmadd" | "fmsub" | "fnmadd"
        | "fnmsub" | "fcmp" | "fcmpe" | "fcvt" | "scvtf" | "ucvtf" | "fcvtzs" | "fcvtzu" => {
            float::instruction(op, a)
        }
        _ => arithmetic::instruction(op, a),
    }
}

fn symbol(s: &str) -> Result<(), String> {
    let mut chars = s.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || "_.$".contains(c))
        || !chars.all(|c| c.is_ascii_alphanumeric() || "_.$".contains(c))
    {
        return Err(format!("invalid symbol `{s}`"));
    }
    Ok(())
}
struct Branch<'a> {
    line: usize,
    offset: usize,
    spec: control::Branch<'a>,
}

struct DataFixup<'a> {
    line: usize,
    location: data::Location,
    width: u8,
    expression: data::Expression<'a>,
    got: bool,
}

pub(super) fn assemble(
    source: &str,
    metrics: bool,
    features: crate::TargetFeatures,
) -> Result<(Assembly, Vec<InstructionLocation>), Error> {
    let mut previous = None;
    for _ in 0..32 {
        let (assembly, symbolic_leb, locations) =
            assemble_pass(source, previous.as_ref(), metrics, features)?;

        if !symbolic_leb {
            return Ok((assembly, locations));
        }
        let symbol_locations: BTreeMap<_, _> = assembly
            .symbols
            .iter()
            .filter_map(|s| {
                Some((
                    s.name.clone(),
                    data::Location {
                        section: s.section?,
                        offset: s.offset?,
                    },
                ))
            })
            .collect();
        if previous.as_ref() == Some(&symbol_locations) {
            return Ok((assembly, locations));
        }
        previous = Some(symbol_locations);
    }
    Err(Error::new(
        0,
        "symbolic LEB128 layout did not converge after 32 passes",
    ))
}
fn assemble_pass(
    source: &str,
    previous: Option<&BTreeMap<String, data::Location>>,
    metrics: bool,
    features: crate::TargetFeatures,
) -> Result<(Assembly, bool, Vec<InstructionLocation>), Error> {
    let mut locations = Vec::new();

    // Declare scratch first so every collection (including nested owners) drops
    // before its backing arena, on success and on every early error return.
    // Each symbolic LEB pass gets a fresh arena; `previous` is independently owned.
    let arena = Bump::new();
    let mut symbolic_leb = false;
    let mut leb_differences = BumpVec::new_in(&arena);
    let mut label_order = BumpVec::new_in(&arena);
    let mut result = Assembly {
        subsections_via_symbols: false,
        text: Vec::new(),
        text_alignment: 2,
        symbols: Vec::new(),
        relocations: Vec::new(),
        sections: Vec::new(),
    };
    let mut debug = debug::Debug::new(&arena);
    let mut prefix = String::from("Lkeel_metadata_");
    while source.contains(&prefix) {
        prefix.push('_');
    }
    let mut aliases = BTreeMap::new();
    let mut current = 0;
    let mut labels: BTreeMap<&str, data::Location> = BTreeMap::new();
    let mut globals = BTreeMap::new();
    let mut branches = BumpVec::new_in(&arena);
    let mut addresses = BumpVec::new_in(&arena);
    let mut data_fixups = BumpVec::new_in(&arena);
    let mut relocation_lines = BTreeMap::new();
    for (index, raw) in scanning::Lines::new(source).enumerate() {
        let line = index + 1;
        let err = |message| Error::new(line, message);
        let mut s = raw.trim_fast();
        while let Some((name, rest)) = s
            .split_once(':')
            .filter(|(name, _)| !name.trim_fast().contains(['"', ' ', '\t']))
        {
            let name = name.trim_fast();
            symbol(name).map_err(err)?;
            if labels
                .insert(
                    name,
                    data::Location {
                        section: current,
                        offset: data::position(&result, current),
                    },
                )
                .is_some()
            {
                return Err(err(format!("duplicate label `{name}`")));
            }
            label_order.push((name, labels[name]));
            s = rest.trim_fast();
        }
        if s.is_empty() {
            continue;
        }
        if let Some((name, expression)) = s
            .split_once('=')
            .filter(|(name, _)| symbol(name.trim_fast()).is_ok())
        {
            let name = name.trim_fast();
            symbol(name).map_err(err)?;
            if aliases
                .insert(name, (line, data::expression(expression).map_err(err)?))
                .is_some()
            {
                return Err(err("duplicate symbol assignment".into()));
            }
            continue;
        }
        let (op, operands) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let operands = operands.trim_fast();
        let op = lower(op);
        if debug
            .directive(
                &op,
                operands,
                current,
                data::position(&result, current),
                line,
            )
            .map_err(err)?
        {
            continue;
        }
        if op == ".ascii" || op == ".asciz" {
            let mut bytes = debug::string(operands).map_err(err)?;
            if op == ".asciz" {
                bytes.push(0);
            }
            data::append(&mut result, current, &bytes).map_err(err)?;
            continue;
        }
        let args = memory::operands(operands).map_err(err)?;
        if op == ".uleb128" || op == ".sleb128" {
            count(&args, 1, usize::MAX).map_err(err)?;
            let mut bytes = Vec::new();
            for arg in args.iter().copied() {
                let expr = data::expression(arg).map_err(err)?;
                let value = if let Some(add) = &expr.add {
                    symbolic_leb = true;
                    if let Some(sub) = &expr.subtract {
                        leb_differences.push((line, *add, *sub));
                    }
                    if let Some(previous) = previous {
                        let sub = expr.subtract.as_ref().ok_or_else(|| {
                            err("LEB128 requires a same-section symbol difference".into())
                        })?;
                        let a = previous
                            .get(*add)
                            .ok_or_else(|| err(format!("undefined LEB128 symbol `{add}`")))?;
                        let b = previous
                            .get(*sub)
                            .ok_or_else(|| err(format!("undefined LEB128 symbol `{sub}`")))?;
                        if a.section != b.section {
                            return Err(err(
                                "LEB128 requires a same-section symbol difference".into()
                            ));
                        }
                        expr.constant
                            .checked_add(i128::from(a.offset) - i128::from(b.offset))
                            .ok_or_else(|| err("LEB128 expression overflow".into()))?
                    } else {
                        0
                    }
                } else {
                    expr.constant
                };
                if op == ".uleb128" {
                    debug::uleb(
                        u64::try_from(value)
                            .map_err(|_| err("unsigned LEB128 value out of range".into()))?,
                        &mut bytes,
                    );
                } else {
                    debug::sleb(
                        i64::try_from(value)
                            .map_err(|_| err("signed LEB128 value out of range".into()))?,
                        &mut bytes,
                    );
                }
            }
            data::append(&mut result, current, &bytes).map_err(err)?;
            continue;
        }
        if op == ".set" {
            count(&args, 2, 2).map_err(err)?;
            symbol(args[0]).map_err(err)?;
            if aliases
                .insert(args[0], (line, data::expression(args[1]).map_err(err)?))
                .is_some()
            {
                return Err(err("duplicate symbol assignment".into()));
            }
            continue;
        }
        if let Some(width) = data::width(&op) {
            count(&args, 1, usize::MAX).map_err(err)?;
            for argument in args.iter().copied() {
                let got = argument.contains("@GOT");
                if let Some((_, suffix)) = argument.split_once("@GOT")
                    && suffix
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_whitespace() && c != '+' && c != '-')
                {
                    return Err(err("data GOT pointers require the @GOT modifier".into()));
                }
                let expression: &str = if got {
                    let (head, tail) = argument.split_once("@GOT").unwrap();
                    let mut text = bumpalo::collections::String::with_capacity_in(
                        head.len() + tail.len(),
                        &arena,
                    );
                    text.push_str(head);
                    text.push_str(tail);
                    text.into_bump_str()
                } else {
                    argument
                };
                let expr = data::expression(expression).map_err(err)?;
                if expr.add.is_none() {
                    data::append(
                        &mut result,
                        current,
                        &data::value(expr.constant, width).map_err(err)?,
                    )
                    .map_err(err)?;
                } else {
                    data::bytes_mut(&mut result, current).map_err(err)?;
                    let location = data::Location {
                        section: current,
                        offset: data::position(&result, current),
                    };
                    data_fixups.push(DataFixup {
                        line,
                        location,
                        width,
                        expression: expr,
                        got,
                    });
                    data::reserve(&mut result, current, u64::from(width), 0).map_err(err)?;
                }
            }
            continue;
        }
        match op.as_ref() {
            ".subsections_via_symbols" => {
                count(&args, 0, 0).map_err(err)?;
                result.subsections_via_symbols = true;
                continue;
            }
            ".text" | ".data" | ".const" | ".const_data" | ".bss" => {
                count(&args, 0, 0).map_err(err)?;
                current = match op.as_ref() {
                    ".text" => 0,
                    ".const" => {
                        data::select(&mut result, "__TEXT", "__const", crate::SectionKind::Data)
                            .map_err(err)?
                    }
                    ".const_data" => {
                        data::select(&mut result, "__DATA", "__const", crate::SectionKind::Data)
                            .map_err(err)?
                    }
                    ".bss" => {
                        data::select(&mut result, "__DATA", "__bss", crate::SectionKind::ZeroFill)
                            .map_err(err)?
                    }
                    _ => data::select(&mut result, "__DATA", "__data", crate::SectionKind::Data)
                        .map_err(err)?,
                };
                continue;
            }
            ".section" => {
                current = data::section(&mut result, &args).map_err(err)?;
                continue;
            }
            ".globl" | ".global" | ".extern" => {
                count(&args, 1, usize::MAX).map_err(err)?;
                for name in args.iter().copied() {
                    symbol(name).map_err(err)?;
                    globals.insert(name, line);
                }
                continue;
            }
            ".p2align" => {
                count(&args, 1, 1).map_err(err)?;
                let power = unsigned(args[0], 16).map_err(err)?;
                data::align(&mut result, current, power).map_err(err)?;
                continue;
            }
            ".space" | ".zero" => {
                count(&args, 1, 2).map_err(err)?;
                let amount = u64::from(unsigned(args[0], u32::MAX as i64).map_err(err)?);
                let fill = args
                    .get(1)
                    .map(|s| unsigned(s, 255))
                    .transpose()
                    .map_err(err)?
                    .unwrap_or(0) as u8;
                data::reserve(&mut result, current, amount, fill).map_err(err)?;
                continue;
            }
            ".zerofill" | ".tbss" => {
                let tls = op == ".tbss";
                let (name, size, alignment) = data::allocation(&args, tls).map_err(err)?;
                if labels.contains_key(name) {
                    return Err(err(format!("duplicate label `{name}`")));
                }
                let section = data::select(
                    &mut result,
                    "__DATA",
                    if tls { "__thread_bss" } else { "__bss" },
                    if tls {
                        crate::SectionKind::ThreadZeroFill
                    } else {
                        crate::SectionKind::ZeroFill
                    },
                )
                .map_err(err)?;
                data::align(&mut result, section, alignment).map_err(err)?;
                labels.insert(
                    name,
                    data::Location {
                        section,
                        offset: data::position(&result, section),
                    },
                );
                label_order.push((name, labels[name]));
                data::reserve(&mut result, section, size, 0).map_err(err)?;
                continue;
            }
            _ if op.starts_with('.') => return Err(err(format!("unsupported directive `{op}`"))),
            _ => {}
        }
        if current != 0 || !result.text.len().is_multiple_of(4) {
            return Err(err(
                "instructions require four-byte-aligned __TEXT,__text locations".into(),
            ));
        }
        if metrics {
            locations.push(InstructionLocation {
                line,
                offset: result.text.len(),
            });
        }
        if let Some(spec) = control::branch(&op, &args, &arena).map_err(err)? {
            if !is_integer(spec.target) {
                let expression = relocate::expression(spec.target).map_err(err)?;
                if expression.modifier.is_some() {
                    return Err(err("branch targets do not accept symbol modifiers".into()));
                }
            }
            branches.push(Branch {
                line,
                offset: result.text.len(),
                spec,
            });
            result.text.extend_from_slice(&[0; 4]);
            continue;
        }
        if let Some((word, mut relocation)) = relocate::instruction(&op, &args).map_err(err)? {
            relocation.offset = result
                .text
                .len()
                .try_into()
                .map_err(|_| err("text offset exceeds 32 bits".into()))?;
            relocation_lines.insert((0, relocation.offset), line);
            result.relocations.push(relocation);
            result.text.extend_from_slice(&word.to_le_bytes());
            continue;
        }
        if op == "adr" || op == "adrp" {
            count(&args, 2, 2).map_err(err)?;
            let word = if is_integer(args[1]) {
                relocate::address_word(&op, args[0], integer(args[1]).map_err(err)?).map_err(err)?
            } else {
                let expr = relocate::expression(args[1]).map_err(err)?;
                if expr.modifier.is_some() {
                    return Err(err("adr does not accept symbol modifiers".into()));
                }
                addresses.push((line, result.text.len(), args[0], expr));
                relocate::address_word("adr", args[0], 0).map_err(err)?
            };
            result.text.extend_from_slice(&word.to_le_bytes());
            continue;
        }
        let word = match scalable::instruction(&op, &args, features).map_err(err)? {
            Some(word) => word,
            None => instruction(&op, &args).map_err(err)?,
        };
        result.text.extend_from_slice(&word.to_le_bytes());
    }
    let mut atoms = atoms::Atoms::new(
        &arena,
        &label_order,
        &globals,
        &mut labels,
        &prefix,
        result.subsections_via_symbols,
    );
    if result.subsections_via_symbols {
        for (line, add, sub) in leb_differences {
            if let (Some(a), Some(b)) = (labels.get(&add), labels.get(&sub))
                && !atoms.same((add, *a), (sub, *b))
            {
                return Err(Error::new(
                    line,
                    "LEB128 symbol differences must stay within one linker atom",
                ));
            }
        }
    }
    for mut branch in branches {
        let err = |message| Error::new(branch.line, message);
        let kind = if branch.spec.encoding.ends_with("_loadlit") {
            "literal load"
        } else {
            "branch"
        };
        let delta = if is_integer(branch.spec.target) {
            integer(branch.spec.target).map_err(err)?
        } else {
            let expr = relocate::expression(branch.spec.target).map_err(err)?;
            let target = labels.get(expr.name);
            let local = expr.name.starts_with('L') && !globals.contains_key(expr.name);
            if branch.spec.width == 26
                && (target.is_some_and(|t| {
                    t.section != 0
                        || (result.subsections_via_symbols
                            && !atoms.contains(
                                data::Location {
                                    section: 0,
                                    offset: branch.offset as u64,
                                },
                                (expr.name, *t),
                            ))
                }) || (target.is_none() && !local))
            {
                if expr.addend % 4 != 0 {
                    return Err(err("branch addend must be 4-byte aligned".into()));
                }
                let offset = branch
                    .offset
                    .try_into()
                    .map_err(|_| err("text offset exceeds 32 bits".into()))?;
                relocation_lines.insert((0, offset), branch.line);
                result.relocations.push(crate::Relocation {
                    section: 0,
                    size: 4,
                    subtract_symbol: None,
                    offset,
                    symbol: expr.name.to_string(),
                    kind: crate::RelocationKind::Branch26,
                    addend: expr.addend,
                });
                0
            } else {
                let target = target
                    .ok_or_else(|| err(format!("undefined local {kind} target `{}`", expr.name)))?;
                if target.section != 0 {
                    return Err(err(format!("{kind} targets must be in __text")));
                }
                if result.subsections_via_symbols
                    && !atoms.contains(
                        data::Location {
                            section: 0,
                            offset: branch.offset as u64,
                        },
                        (expr.name, *target),
                    )
                {
                    return Err(err(format!(
                        "{kind} requires a target in the same linker atom"
                    )));
                }
                target.offset as i64 + expr.addend - branch.offset as i64
            }
        };
        let bound = 1i64 << (branch.spec.width + 1);
        if delta % 4 != 0 || !(-bound..bound).contains(&delta) {
            return Err(err(format!(
                "{kind} displacement must be 4-byte aligned and in {}..={} bytes",
                -bound,
                bound - 4
            )));
        }
        branch.spec.fields.push((
            branch.spec.field,
            ((delta / 4) as u32) & ((1 << branch.spec.width) - 1),
        ));
        let word = encode(branch.spec.encoding, &branch.spec.fields).map_err(err)?;
        result.text[branch.offset..branch.offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    for (line, offset, destination, expr) in addresses {
        let target = labels.get(expr.name).ok_or_else(|| {
            Error::new(
                line,
                format!("adr requires a defined text symbol `{}`", expr.name),
            )
        })?;
        if target.section != 0 {
            return Err(Error::new(
                line,
                "adr requires a target in the same text section",
            ));
        }
        if result.subsections_via_symbols
            && !atoms.contains(
                data::Location {
                    section: 0,
                    offset: offset as u64,
                },
                (expr.name, *target),
            )
        {
            return Err(Error::new(
                line,
                "adr requires a target in the same linker atom; use adrp/add across atoms",
            ));
        }
        let word = relocate::address_word(
            "adr",
            destination,
            target.offset as i64 + expr.addend - offset as i64,
        )
        .map_err(|e| Error::new(line, e))?;
        result.text[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    debug.finish(&mut result, &mut labels, &prefix)?;
    let mut constants = BTreeMap::new();
    for (name, (line, expr)) in aliases {
        if labels.contains_key(&name) {
            return Err(Error::new(line, "assignment conflicts with label"));
        }
        let value = if let (Some(add), Some(sub)) = (&expr.add, &expr.subtract) {
            let a = labels
                .get(add)
                .ok_or_else(|| Error::new(line, "undefined assignment symbol"))?;
            let b = labels
                .get(sub)
                .ok_or_else(|| Error::new(line, "undefined assignment symbol"))?;
            if a.section != b.section {
                return Err(Error::new(
                    line,
                    "assignments require a same-section difference",
                ));
            }
            expr.constant
                .checked_add(i128::from(a.offset) - i128::from(b.offset))
                .ok_or_else(|| Error::new(line, "assignment overflow"))?
        } else if expr.add.is_none() {
            expr.constant
        } else {
            return Err(Error::new(
                line,
                "assignments require constants or same-section differences",
            ));
        };
        constants.insert(name, value);
    }
    for fixup in data_fixups {
        let mut expr = fixup.expression;
        if fixup.got {
            let target = expr
                .add
                .take()
                .ok_or_else(|| Error::new(fixup.line, "GOT pointer requires a symbol"))?;
            let sub = expr
                .subtract
                .as_ref()
                .and_then(|s| labels.get(s))
                .ok_or_else(|| {
                    Error::new(
                        fixup.line,
                        "GOT pointer requires a defined subtraction label",
                    )
                })?;
            if fixup.width != 4 || sub.section != fixup.location.section {
                return Err(Error::new(
                    fixup.line,
                    "GOT pointer requires .long symbol@GOT-label in the same section",
                ));
            }
            let addend = expr
                .constant
                .checked_add(i128::from(fixup.location.offset) - i128::from(sub.offset))
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| {
                    Error::new(fixup.line, "GOT pointer addend exceeds signed 32 bits")
                })?;
            if addend != 0 {
                return Err(Error::new(
                    fixup.line,
                    "GOT pointers require zero addend and subtraction of their own address",
                ));
            }
            let offset = u32::try_from(fixup.location.offset)
                .map_err(|_| Error::new(fixup.line, "GOT relocation offset exceeds 32 bits"))?;
            relocation_lines.insert((fixup.location.section, offset), fixup.line);
            result.relocations.push(crate::Relocation {
                section: fixup.location.section,
                size: 4,
                subtract_symbol: None,
                offset,
                symbol: target.to_string(),
                kind: crate::RelocationKind::PointerToGot,
                addend: i64::from(addend),
            });
            continue;
        }
        if let Some(value) = expr.add.as_ref().and_then(|s| constants.get(s)) {
            if expr.subtract.is_some() {
                return Err(Error::new(
                    fixup.line,
                    "constant assignment cannot be combined with a symbol difference",
                ));
            }
            let value = expr
                .constant
                .checked_add(*value)
                .ok_or_else(|| Error::new(fixup.line, "data expression overflow"))?;
            let bytes = data::value(value, fixup.width).map_err(|e| Error::new(fixup.line, e))?;
            let out = data::bytes_mut(&mut result, fixup.location.section)
                .map_err(|e| Error::new(fixup.line, e))?;
            let offset = fixup.location.offset as usize;
            out[offset..offset + usize::from(fixup.width)].copy_from_slice(&bytes);
            continue;
        }
        let expr = &mut expr;
        let add = expr.add.take().unwrap();
        let mut constant = expr.constant;
        let resolved = if expr.subtract == Some(add) {
            true
        } else if let Some(subtract) = &expr.subtract {
            match (labels.get(&add), labels.get(subtract)) {
                (Some(a), Some(b))
                    if a.section == b.section
                        && (!result.subsections_via_symbols
                            || atoms.same((add, *a), (subtract, *b))) =>
                {
                    constant = constant
                        .checked_add(i128::from(a.offset) - i128::from(b.offset))
                        .ok_or_else(|| Error::new(fixup.line, "data expression overflow"))?;
                    true
                }
                _ => false,
            }
        } else {
            false
        };
        if resolved {
            let bytes =
                data::value(constant, fixup.width).map_err(|e| Error::new(fixup.line, e))?;
            let out = data::bytes_mut(&mut result, fixup.location.section)
                .map_err(|e| Error::new(fixup.line, e))?;
            let offset = fixup.location.offset as usize;
            out[offset..offset + usize::from(fixup.width)].copy_from_slice(&bytes);
        } else {
            if fixup.width < 4 || (fixup.width != 8 && expr.subtract.is_none()) {
                return Err(Error::new(
                    fixup.line,
                    "symbolic data requires .quad pointers or .long/.quad differences",
                ));
            }
            data::value(constant, fixup.width).map_err(|e| Error::new(fixup.line, e))?;
            let offset =
                fixup.location.offset.try_into().map_err(|_| {
                    Error::new(fixup.line, "data relocation offset exceeds 32 bits")
                })?;
            relocation_lines.insert((fixup.location.section, offset), fixup.line);
            result.relocations.push(crate::Relocation {
                section: fixup.location.section,
                size: fixup.width,
                subtract_symbol: expr.subtract.take().map(str::to_string),
                offset,
                symbol: add.to_string(),
                kind: crate::RelocationKind::Unsigned,
                addend: constant as i64,
            });
        }
    }
    if result.subsections_via_symbols {
        atoms.rebase(&mut result, &mut labels, &globals, &prefix)?;
    }
    let mut names = BumpVec::from_iter_in(labels.keys().chain(globals.keys()).copied(), &arena);
    for relocation in &result.relocations {
        for name in std::iter::once(&relocation.symbol).chain(relocation.subtract_symbol.iter()) {
            if name.starts_with('L')
                && !labels.contains_key(name.as_str())
                && !globals.contains_key(name.as_str())
            {
                return Err(Error::new(
                    relocation_lines
                        .get(&(relocation.section, relocation.offset))
                        .copied()
                        .unwrap_or(0),
                    format!("undefined local symbol `{name}`"),
                ));
            }
            names.push(name.as_str());
        }
    }
    names.sort_unstable();
    names.dedup();
    result.symbols = names
        .into_iter()
        .map(|name| {
            let location = labels.get(&name);
            Symbol {
                global: location.is_none() || globals.contains_key(&name),
                offset: location.map(|l| l.offset),
                section: location.map(|l| l.section),
                name: name.to_string(),
            }
        })
        .collect();
    Ok((result, symbolic_leb, locations))
}
