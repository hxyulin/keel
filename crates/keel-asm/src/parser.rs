mod arithmetic;
mod control;
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

pub(super) fn assemble(source: &str) -> Result<Assembly, Error> {
    let mut result = Assembly {
        text: Vec::new(),
        text_alignment: 2,
        symbols: Vec::new(),
        relocations: Vec::new(),
    };
    let mut labels = BTreeMap::new();
    let mut globals = BTreeMap::new();
    let mut branches = Vec::new();
    let mut addresses = Vec::new();
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
                .insert(name.to_string(), result.text.len() as u64)
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
            relocation_lines.insert(relocation.offset, line);
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
        match op.as_str() {
            ".text" => count(&args, 0, 0).map_err(err)?,
            ".section" => {
                if args != ["__TEXT", "__text"]
                    && args != ["__TEXT", "__text", "regular", "pure_instructions"]
                {
                    return Err(err(
                        "only the __TEXT,__text instruction section is supported".into(),
                    ));
                }
            }
            ".globl" | ".global" | ".extern" => {
                count(&args, 1, usize::MAX).map_err(err)?;
                for name in args {
                    symbol(name).map_err(err)?;
                    globals.insert(name.to_string(), line);
                }
            }
            ".p2align" => {
                count(&args, 1, 1).map_err(err)?;
                let power = unsigned(args[0], 16).map_err(err)?;
                result.text_alignment = result.text_alignment.max(power);
                let alignment = 1usize << power;
                let padding = (alignment - result.text.len() % alignment) % alignment;
                let nop = encode("NOP_HI_hints", &[]).map_err(err)?.to_le_bytes();
                for _ in 0..padding / 4 {
                    result.text.extend_from_slice(&nop);
                }
            }
            _ if op.starts_with('.') => return Err(err(format!("unsupported directive `{op}`"))),
            _ => result
                .text
                .extend_from_slice(&instruction(&op, &args).map_err(err)?.to_le_bytes()),
        }
    }
    for mut branch in branches {
        let err = |message| Error::new(branch.line, message);
        let delta = if is_integer(&branch.spec.target) {
            integer(&branch.spec.target).map_err(err)?
        } else {
            let expr = relocate::expression(&branch.spec.target).map_err(err)?;
            let target = labels.get(&expr.name);
            let local = expr.name.starts_with('L') && !globals.contains_key(&expr.name);
            if branch.spec.width == 26 && target.is_none() && !local {
                if expr.addend % 4 != 0 {
                    return Err(err("branch addend must be 4-byte aligned".into()));
                }
                relocation_lines.insert(branch.offset as u32, branch.line);
                result.relocations.push(crate::Relocation {
                    offset: branch
                        .offset
                        .try_into()
                        .map_err(|_| err("text offset exceeds 32 bits".into()))?,
                    symbol: expr.name,
                    kind: crate::RelocationKind::Branch26,
                    addend: expr.addend,
                });
                0
            } else {
                let target = target.ok_or_else(|| err(format!("undefined local branch target `{}`; conditional branches require definitions in this file", expr.name)))?;
                *target as i64 + expr.addend - branch.offset as i64
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
        let word = relocate::address_word(
            "adr",
            &destination,
            *target as i64 + expr.addend - offset as i64,
        )
        .map_err(|e| Error::new(line, e))?;
        result.text[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }
    let mut names: BTreeSet<_> = labels.keys().chain(globals.keys()).cloned().collect();
    for relocation in &result.relocations {
        if relocation.symbol.starts_with('L')
            && !labels.contains_key(&relocation.symbol)
            && !globals.contains_key(&relocation.symbol)
        {
            return Err(Error::new(
                relocation_lines[&relocation.offset],
                format!("undefined local symbol `{}`", relocation.symbol),
            ));
        }
        names.insert(relocation.symbol.clone());
    }
    result.symbols = names
        .into_iter()
        .map(|name| {
            let offset = labels.get(&name).copied();
            Symbol {
                global: offset.is_none() || globals.contains_key(&name),
                name,
                offset,
            }
        })
        .collect();
    Ok(result)
}
