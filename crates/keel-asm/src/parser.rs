mod arithmetic;
mod control;
mod data;
mod float;
mod memory;
mod relocate;
mod vector;

use super::{Assembly, Error, Symbol, encode};
use std::collections::{BTreeMap, BTreeSet};

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
    let s = s.to_ascii_lowercase();
    let r = match s.as_str() {
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
    let s = s.trim().strip_prefix('#').unwrap_or(s.trim());
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
            encode(
                &format!("{}_{}_movewide", op.to_ascii_uppercase(), d.bits),
                &[("Rd", d.number), ("imm16", imm), ("hw", amount / 16)],
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
                        let s = s.to_ascii_lowercase();
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
                encode(
                    &format!(
                        "{}_{}{}_addsub_imm",
                        op.to_ascii_uppercase(),
                        d.bits,
                        if flags { "S" } else { "" }
                    ),
                    &[
                        ("Rd", d.number),
                        ("Rn", n.number),
                        ("imm12", imm),
                        ("sh", amount / 12),
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
                encode(
                    &format!("{}_{}_addsub_shift", op.to_ascii_uppercase(), d.bits),
                    &[
                        ("Rd", d.number),
                        ("Rn", n.number),
                        ("Rm", m.number),
                        ("shift", shift),
                        ("imm6", amount),
                    ],
                )
            }
        }
        "cmp" | "cmn" => {
            count(a, 2, 3)?;
            let n = reg(a[0], true, true)?;
            let mut args = vec![if n.bits == 64 { "xzr" } else { "wzr" }];
            args.extend_from_slice(a);
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
                encode(
                    &format!("ORR_{}_log_shift", d.bits),
                    &[
                        ("Rd", d.number),
                        ("Rn", 31),
                        ("Rm", n.number),
                        ("shift", 0),
                        ("imm6", 0),
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
            encode("RET_64R_branch_reg", &[("Rn", n.number)])
        }
        "nop" => {
            count(a, 0, 0)?;
            encode("NOP_HI_hints", &[])
        }
        "cnt" | "addv" => vector::instruction(op, a),
        "br" | "blr" | "brk" => control::instruction(op, a),
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
struct Branch {
    line: usize,
    offset: usize,
    spec: control::Branch,
}

struct DataFixup {
    line: usize,
    location: data::Location,
    width: u8,
    expression: data::Expression,
}

pub(super) fn assemble(source: &str) -> Result<Assembly, Error> {
    let mut result = Assembly {
        text: Vec::new(),
        text_alignment: 2,
        symbols: Vec::new(),
        relocations: Vec::new(),
        sections: Vec::new(),
    };
    let mut current = 0;
    let mut labels: BTreeMap<String, data::Location> = BTreeMap::new();
    let mut globals = BTreeMap::new();
    let mut branches = Vec::new();
    let mut addresses = Vec::new();
    let mut data_fixups = Vec::new();
    let mut relocation_lines = BTreeMap::new();
    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        let err = |message| Error::new(line, message);
        let mut s = raw
            .split(';')
            .next()
            .unwrap()
            .split("//")
            .next()
            .unwrap()
            .trim();
        while let Some((name, rest)) = s.split_once(':') {
            let name = name.trim();
            symbol(name).map_err(err)?;
            if labels
                .insert(
                    name.to_string(),
                    data::Location {
                        section: current,
                        offset: data::position(&result, current),
                    },
                )
                .is_some()
            {
                return Err(err(format!("duplicate label `{name}`")));
            }
            s = rest.trim();
        }
        if s.is_empty() {
            continue;
        }
        let (op, operands) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let operands = operands.trim();
        let op = op.to_ascii_lowercase();
        let args = memory::operands(operands).map_err(err)?;
        if let Some(width) = data::width(&op) {
            count(&args, 1, usize::MAX).map_err(err)?;
            for argument in args {
                let expr = data::expression(argument).map_err(err)?;
                if expr.add.is_none() {
                    data::append(
                        &mut result,
                        current,
                        &data::value(expr.constant, width).map_err(err)?,
                    )
                    .map_err(err)?;
                } else {
                    if width < 4 || (width != 8 && expr.subtract.is_none()) {
                        return Err(err(
                            "symbolic data requires .quad pointers or .long/.quad differences"
                                .into(),
                        ));
                    }
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
                    });
                    data::reserve(&mut result, current, u64::from(width), 0).map_err(err)?;
                }
            }
            continue;
        }
        match op.as_str() {
            ".text" | ".data" | ".const" | ".const_data" | ".bss" => {
                count(&args, 0, 0).map_err(err)?;
                current = match op.as_str() {
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
                for name in args {
                    symbol(name).map_err(err)?;
                    globals.insert(name.to_string(), line);
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
                    name.into(),
                    data::Location {
                        section,
                        offset: data::position(&result, section),
                    },
                );
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
        if let Some(spec) = control::branch(&op, &args).map_err(err)? {
            if !is_integer(&spec.target) {
                let expression = relocate::expression(&spec.target).map_err(err)?;
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
                addresses.push((line, result.text.len(), args[0].to_string(), expr));
                relocate::address_word("adr", args[0], 0).map_err(err)?
            };
            result.text.extend_from_slice(&word.to_le_bytes());
            continue;
        }
        result
            .text
            .extend_from_slice(&instruction(&op, &args).map_err(err)?.to_le_bytes());
    }
    for mut branch in branches {
        let err = |message| Error::new(branch.line, message);
        let delta = if is_integer(&branch.spec.target) {
            integer(&branch.spec.target).map_err(err)?
        } else {
            let expr = relocate::expression(&branch.spec.target).map_err(err)?;
            let target = labels.get(&expr.name);
            let local = expr.name.starts_with('L') && !globals.contains_key(&expr.name);
            if branch.spec.width == 26
                && (target.is_some_and(|t| t.section != 0) || (target.is_none() && !local))
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
                    symbol: expr.name,
                    kind: crate::RelocationKind::Branch26,
                    addend: expr.addend,
                });
                0
            } else {
                let target = target
                    .ok_or_else(|| err(format!("undefined local branch target `{}`", expr.name)))?;
                if target.section != 0 {
                    return Err(err("conditional branches require targets in __text".into()));
                }
                target.offset as i64 + expr.addend - branch.offset as i64
            }
        };
        let bound = 1i64 << (branch.spec.width + 1);
        if delta % 4 != 0 || !(-bound..bound).contains(&delta) {
            return Err(err(format!(
                "branch displacement must be 4-byte aligned and in {}..={} bytes",
                -bound,
                bound - 4
            )));
        }
        branch.spec.fields.push((
            branch.spec.field,
            ((delta / 4) as u32) & ((1 << branch.spec.width) - 1),
        ));
        let word = encode(&branch.spec.encoding, &branch.spec.fields).map_err(err)?;
        result.text[branch.offset..branch.offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    for (line, offset, destination, expr) in addresses {
        let target = labels.get(&expr.name).ok_or_else(|| {
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
        let word = relocate::address_word(
            "adr",
            &destination,
            target.offset as i64 + expr.addend - offset as i64,
        )
        .map_err(|e| Error::new(line, e))?;
        result.text[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    for fixup in data_fixups {
        let expr = fixup.expression;
        let add = expr.add.unwrap();
        let mut constant = expr.constant;
        let resolved = if expr.subtract.as_deref() == Some(add.as_str()) {
            true
        } else if let Some(subtract) = &expr.subtract {
            match (labels.get(&add), labels.get(subtract)) {
                (Some(a), Some(b)) if a.section == b.section => {
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
            data::value(constant, fixup.width).map_err(|e| Error::new(fixup.line, e))?;
            let offset =
                fixup.location.offset.try_into().map_err(|_| {
                    Error::new(fixup.line, "data relocation offset exceeds 32 bits")
                })?;
            relocation_lines.insert((fixup.location.section, offset), fixup.line);
            result.relocations.push(crate::Relocation {
                section: fixup.location.section,
                size: fixup.width,
                subtract_symbol: expr.subtract,
                offset,
                symbol: add,
                kind: crate::RelocationKind::Unsigned,
                addend: constant as i64,
            });
        }
    }
    let mut names: BTreeSet<_> = labels.keys().chain(globals.keys()).cloned().collect();
    for relocation in &result.relocations {
        for name in std::iter::once(&relocation.symbol).chain(relocation.subtract_symbol.iter()) {
            if name.starts_with('L') && !labels.contains_key(name) && !globals.contains_key(name) {
                return Err(Error::new(
                    relocation_lines[&(relocation.section, relocation.offset)],
                    format!("undefined local symbol `{name}`"),
                ));
            }
            names.insert(name.clone());
        }
    }
    result.symbols = names
        .into_iter()
        .map(|name| {
            let location = labels.get(&name);
            Symbol {
                global: location.is_none() || globals.contains_key(&name),
                offset: location.map(|l| l.offset),
                section: location.map(|l| l.section),
                name,
            }
        })
        .collect();
    Ok(result)
}
