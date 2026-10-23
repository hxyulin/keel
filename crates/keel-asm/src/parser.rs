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
pub(crate) mod elf;
mod float;
mod memory;
mod preprocess;
mod program;
pub(crate) use program::Compact;
mod relocate;
mod scalable;
mod scanning;
mod storage;
mod system;
mod text;
mod vector;

use super::{Assembly, Error, InstructionLocation, Symbol, encode};
use crate::fxhash::{FxHashMap, FxHashSet};
use bumpalo::{Bump, collections::Vec as BumpVec};
use keel_asm_ir::Opcode;
use std::collections::BTreeMap;

/// Label locations by name. Only looked up: wherever their order matters,
/// names are collected and sorted.
type Labels<'a> = FxHashMap<&'a str, data::Location>;
use storage::{Operands, encode_opcode, lower};

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
        "xzr" | "wzr" | "x31" | "w31" => Reg {
            number: 31,
            bits: if s.starts_with('x') { 64 } else { 32 },
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
    let (digits, radix) = text::integer_radix(digits);
    if digits.starts_with(['-', '+']) {
        return Err(format!("invalid integer `{s}`"));
    }
    let value = i64::from_str_radix(digits, radix).map_err(|_| format!("invalid integer `{s}`"))?;
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
    let kind = match lower(kind).as_ref() {
        "lsl" => "lsl",
        "lsr" => "lsr",
        "asr" => "asr",
        "ror" => "ror",
        _ => kind,
    };
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

fn move_wide_opcode(op: &str, bits: u32) -> Opcode {
    use Opcode::*;
    match (op, bits) {
        ("movz", 32) => MOVZ_32_movewide,
        ("movz", _) => MOVZ_64_movewide,
        ("movn", 32) => MOVN_32_movewide,
        ("movn", _) => MOVN_64_movewide,
        ("movk", 32) => MOVK_32_movewide,
        ("movk", _) => MOVK_64_movewide,
        _ => unreachable!("move-wide dispatch"),
    }
}

fn instruction(op: &str, a: &[&str]) -> Result<u32, String> {
    instruction_with_features(op, a, crate::TargetFeatures::default())
}
fn instruction_with_features(
    op: &str,
    a: &[&str],
    features: crate::TargetFeatures,
) -> Result<u32, String> {
    if op.starts_with("aes")
        || op.starts_with("sha")
        || matches!(op, "pmull" | "pmull2")
            && a.first()
                .is_some_and(|s| s.ends_with(".1q") || s.ends_with(".1Q"))
    {
        if let Some(word) = system::instruction(op, a, features)? {
            return Ok(word);
        }
    }
    if let Some(word) = vector::extra(op, a)? {
        return Ok(word);
    }
    if let Some(word) = atomic::instruction(op, a)? {
        return Ok(word);
    }
    match op {
        "ldr" | "str" | "ldrb" | "strb" | "ldrh" | "strh" | "ldrsb" | "ldrsh" | "ldrsw"
        | "ldur" | "stur" | "ldurb" | "sturb" | "ldurh" | "sturh" | "ldursb" | "ldursh"
        | "ldursw" | "ldp" | "stp" | "ldpsw" | "ldar" | "ldarb" | "ldarh" | "stlr" | "stlrb"
        | "stlrh" => memory::instruction(op, a),
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
            encode_opcode(
                move_wide_opcode(op, d.bits),
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
            let (word32, word64) = {
                use Opcode::*;
                match (op, immediate) {
                    ("add", true) => (ADD_32_addsub_imm, ADD_64_addsub_imm),
                    ("adds", true) => (ADDS_32S_addsub_imm, ADDS_64S_addsub_imm),
                    ("sub", true) => (SUB_32_addsub_imm, SUB_64_addsub_imm),
                    ("subs", true) => (SUBS_32S_addsub_imm, SUBS_64S_addsub_imm),
                    ("add", false) => (ADD_32_addsub_shift, ADD_64_addsub_shift),
                    ("adds", false) => (ADDS_32_addsub_shift, ADDS_64_addsub_shift),
                    ("sub", false) => (SUB_32_addsub_shift, SUB_64_addsub_shift),
                    ("subs", false) => (SUBS_32_addsub_shift, SUBS_64_addsub_shift),
                    _ => unreachable!("add/sub dispatch"),
                }
            };
            let opcode = if d.bits == 32 { word32 } else { word64 };
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
                encode_opcode(
                    opcode,
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
                encode_opcode(
                    opcode,
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
                encode_opcode(
                    if d.bits == 32 {
                        Opcode::ORR_32_log_shift
                    } else {
                        Opcode::ORR_64_log_shift
                    },
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
        | "fnmsub" | "fcmp" | "fcmpe" | "fccmp" | "fccmpe" | "fcvt" | "scvtf" | "ucvtf"
        | "fcvtzs" | "fcvtzu" | "fcvtms" | "fcvtmu" | "fcvtns" | "fcvtnu" | "fcvtps" | "fcvtpu"
        | "fcvtas" | "fcvtau" | "frinta" | "frintx" | "fmaxnm" | "fminnm" => {
            float::instruction(op, a)
        }
        _ => match system::instruction(op, a, features)? {
            Some(word) => Ok(word),
            None => arithmetic::instruction(op, a),
        },
    }
}

fn symbol(s: &str) -> Result<(), String> {
    if is_symbol(s) {
        Ok(())
    } else {
        Err(format!("invalid symbol `{s}`"))
    }
}
fn is_symbol(s: &str) -> bool {
    // Bytes of non-ASCII characters are rejected like the characters. Every
    // label and branch target is checked: classify bytes without branches,
    // so that whole names are checked a vector at a time.
    let valid = |b: u8| {
        (b | 0x20).wrapping_sub(b'a') < 26
            || b.wrapping_sub(b'0') < 10
            || b == b'_'
            || b == b'.'
            || b == b'$'
    };
    match s.as_bytes() {
        [first, rest @ ..] => {
            valid(*first)
                && !first.is_ascii_digit()
                && rest.iter().fold(true, |all, &b| all & valid(b))
        }
        [] => false,
    }
}
struct Branch<'a> {
    section: usize,
    line: usize,
    offset: usize,
    /// A structured target, already checked to be a plain symbol.
    symbol: bool,
    spec: control::Branch<'a>,
}

/// A branch's description in diagnostics, named only when one is reported.
struct BranchKind(keel_asm_ir::Opcode);
impl std::fmt::Display for BranchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.name().ends_with("_loadlit") {
            "literal load"
        } else {
            "branch"
        })
    }
}

struct DataFixup<'a> {
    line: usize,
    location: data::Location,
    width: u8,
    expression: data::Expression<'a>,
    got: bool,
    right_shift: u32,
}

pub(super) fn assemble(
    source: &str,
    metrics: bool,
    features: crate::TargetFeatures,
) -> Result<(Assembly, Vec<InstructionLocation>), Error> {
    match assemble_plain(program::Input::Text(source), metrics, features) {
        Err(error) if preprocess::required(&error) => {
            let expanded = preprocess::expand(source)?;
            let (assembly, mut locations) =
                assemble_plain(program::Input::Text(&expanded.source), metrics, features)
                    .map_err(|error| expanded.remap(error))?;
            for location in &mut locations {
                location.line = expanded.original_line(location.line);
            }
            Ok((assembly, locations))
        }
        result => result,
    }
}

pub(super) fn assemble_program(
    source: &keel_asm_ir::Program,
    features: crate::TargetFeatures,
) -> Result<Assembly, Error> {
    assemble_structured(program::Input::Program(source), features)
}

pub(super) fn assemble_compact(source: &program::Compact) -> Result<Assembly, Error> {
    assemble_structured(program::Input::Compact(source), source.features())
}

fn assemble_structured(
    source: program::Input<'_>,
    features: crate::TargetFeatures,
) -> Result<Assembly, Error> {
    match assemble_plain(source, false, features) {
        Err(e) if preprocess::required(&e) => {
            // Rare inline-assembly macro fallback, explicit and fully validated.
            let text = program::materialize(source, features)?;
            assemble(&text, false, features).map(|(assembly, _)| assembly)
        }
        result => result.map(|(assembly, _)| assembly),
    }
}

fn assemble_plain(
    source: program::Input<'_>,
    metrics: bool,
    features: crate::TargetFeatures,
) -> Result<(Assembly, Vec<InstructionLocation>), Error> {
    let mut previous = None;
    // Populate only on a second pass: ordinary single-pass input retains no
    // extra representation. Each entry packs a statement ordinal and a fixed
    // instruction word into eight bytes. Symbolic instructions are excluded.
    let mut fixed_words = Vec::new();
    for _ in 0..32 {
        let (assembly, symbolic_leb, locations) = assemble_pass(
            source,
            previous.as_ref(),
            metrics,
            features,
            &mut fixed_words,
        )?;
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
    source: program::Input<'_>,
    previous: Option<&BTreeMap<String, data::Location>>,
    metrics: bool,
    features: crate::TargetFeatures,
    fixed_words: &mut Vec<u64>,
) -> Result<(Assembly, bool, Vec<InstructionLocation>), Error> {
    let record_words = previous.is_some() && fixed_words.is_empty();
    let mut next_word = 0;
    let mut locations = Vec::new();
    // Declare scratch first so every collection (including nested owners) drops
    // before its backing arena, on success and on every early error return.
    // Each symbolic LEB pass gets a fresh arena; `previous` is independently owned.
    let arena = Bump::new();
    let mut symbolic_leb = false;
    let mut has_build_version = false;
    let mut leb_differences = BumpVec::new_in(&arena);
    let reservations = source.reservations();
    let (label_count, branch_count) = (reservations.labels, reservations.branches);
    let mut label_order = BumpVec::with_capacity_in(label_count, &arena);
    let mut result = Assembly {
        build_version: crate::BuildVersion::default(),
        linker_optimization_hints: Vec::new(),
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
    let mut labels: Labels = FxHashMap::default();
    // Unwind tables add a label for each procedure after layout.
    labels.reserve(label_count + reservations.frames);
    let mut globals: FxHashMap<&str, usize> = FxHashMap::default();
    let mut private_externs = FxHashMap::default();
    let mut weak_references = FxHashMap::default();
    let mut weak_definitions = FxHashMap::default();
    let mut no_dead_strip = FxHashMap::default();
    let mut weak_hidden = FxHashMap::default();
    let mut common = BTreeMap::new();
    let mut branches = BumpVec::with_capacity_in(branch_count, &arena);
    let mut loh = BumpVec::new_in(&arena);
    let mut addresses = BumpVec::new_in(&arena);
    let mut data_fixups = BumpVec::new_in(&arena);
    let mut relocation_lines = FxHashMap::default();
    for (statement, raw) in source.lines(&arena).enumerate() {
        let (line, raw) = raw?;
        let err = |message| Error::new(line, message);
        let raw = match raw {
            program::Entry::Directive(d) => {
                use keel_asm_ir::Directive as D;
                d.validate().map_err(err)?;
                match d {
                    D::ElfLifecycleArray { .. }
                    | D::ElfSection { .. }
                    | D::ElfSectionMetadata { .. }
                    | D::ElfType { .. }
                    | D::ElfWeak(_)
                    | D::ElfHidden(_)
                    | D::Size { .. }
                    | D::X86Integer { .. } => {
                        return Err(err("ELF directive in AArch64 Mach-O program".into()));
                    }
                    D::Section {
                        segment,
                        name,
                        kind,
                    } => current = data::select(&mut result, segment, name, *kind).map_err(err)?,
                    D::Global(name) => {
                        globals.insert(name.as_str(), line);
                    }
                    D::WeakReference(name) => {
                        weak_references.insert(name.as_str(), line);
                    }
                    D::NoDeadStrip(name) => {
                        no_dead_strip.insert(name.as_str(), line);
                    }
                    D::WeakDefinition(name) => {
                        symbol(name).map_err(err)?;
                        if name.starts_with('L') {
                            return Err(err("weak definition requires a non-local symbol".into()));
                        }
                        weak_definitions.insert(name.as_str(), line);
                    }
                    D::PrivateExtern(name) => {
                        if name.starts_with('L') {
                            return Err(err("private extern requires a non-local symbol".into()));
                        }
                        globals.insert(name.as_str(), line);
                        private_externs.insert(name.as_str(), line);
                    }
                    D::SubsectionsViaSymbols => result.subsections_via_symbols = true,
                    D::Align(power) => {
                        data::align(&mut result, current, u32::from(*power)).map_err(err)?
                    }
                    D::CString(bytes) => {
                        data::append(&mut result, current, bytes).map_err(err)?;
                        data::append(&mut result, current, &[0]).map_err(err)?;
                    }
                    D::Bytes(bytes) => data::append(&mut result, current, bytes).map_err(err)?,
                    D::Space(size) => data::reserve(&mut result, current, *size, 0).map_err(err)?,
                    D::Set {
                        name,
                        expression: e,
                    } => {
                        if aliases
                            .insert(name.as_str(), (line, native_expression(e)))
                            .is_some()
                        {
                            return Err(err("duplicate symbol assignment".into()));
                        }
                    }
                    D::Integer {
                        width,
                        expression: e,
                        got,
                    } => {
                        let expr = native_expression(e);
                        if expr.add.is_none() {
                            if *got {
                                return Err(err("GOT pointer requires a symbol".into()));
                            }
                            data::append(
                                &mut result,
                                current,
                                &data::value(expr.constant, *width).map_err(err)?,
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
                                width: *width,
                                expression: expr,
                                right_shift: 0,
                                got: *got,
                            });
                            data::reserve(&mut result, current, u64::from(*width), 0)
                                .map_err(err)?;
                        }
                    }
                    D::Leb {
                        signed,
                        expression: e,
                    } => {
                        let expr = native_expression(e);
                        let value = if let Some(add) = expr.add {
                            symbolic_leb = true;
                            if let Some(sub) = expr.subtract
                                && !data::is_debug(&result, current)
                            {
                                leb_differences.push((line, add, sub));
                            }
                            if let Some(previous) = previous {
                                let sub = expr.subtract.ok_or_else(|| {
                                    err("LEB128 requires a same-section symbol difference".into())
                                })?;
                                let a = previous.get(add).ok_or_else(|| {
                                    err(format!("undefined LEB128 symbol `{add}`"))
                                })?;
                                let b = previous.get(sub).ok_or_else(|| {
                                    err(format!("undefined LEB128 symbol `{sub}`"))
                                })?;
                                if a.section != b.section {
                                    return Err(err(
                                        "LEB128 requires a same-section symbol difference".into(),
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
                        let mut bytes = Vec::new();
                        if *signed {
                            debug::sleb(
                                i64::try_from(value)
                                    .map_err(|_| err("signed LEB128 value out of range".into()))?,
                                &mut bytes,
                            )
                        } else {
                            debug::uleb(
                                u64::try_from(value).map_err(|_| {
                                    err("unsigned LEB128 value out of range".into())
                                })?,
                                &mut bytes,
                            )
                        }
                        data::append(&mut result, current, &bytes).map_err(err)?;
                    }
                    D::ZeroFill {
                        symbol: name,
                        size,
                        alignment,
                        thread_local,
                    } => {
                        if labels.contains_key(name.as_str()) {
                            return Err(err(format!("duplicate label `{name}`")));
                        }
                        let section = data::select(
                            &mut result,
                            "__DATA",
                            if *thread_local {
                                "__thread_bss"
                            } else {
                                "__bss"
                            },
                            if *thread_local {
                                crate::SectionKind::ThreadZeroFill
                            } else {
                                crate::SectionKind::ZeroFill
                            },
                        )
                        .map_err(err)?;
                        data::align(&mut result, section, u32::from(*alignment)).map_err(err)?;
                        let location = data::Location {
                            section,
                            offset: data::position(&result, section),
                        };
                        labels.insert(name.as_str(), location);
                        label_order.push((name.as_str(), location));
                        data::reserve(&mut result, section, *size, 0).map_err(err)?;
                    }
                    D::File { index, path } => debug.file(*index, path.clone()).map_err(err)?,
                    D::Loc {
                        file,
                        line: row,
                        column,
                        prologue_end,
                    } => {
                        if !data::is_code(&result, current) {
                            return Err(err(".loc requires an executable section".into()));
                        }
                        debug
                            .loc(
                                *file,
                                *row,
                                *column,
                                *prologue_end,
                                current,
                                data::position(&result, current),
                            )
                            .map_err(err)?;
                    }
                    D::Cfi(cfi) => {
                        if !data::is_code(&result, current) {
                            return Err(err("CFI requires an executable section".into()));
                        }
                        debug
                            .cfi(cfi, current, data::position(&result, current), line)
                            .map_err(err)?;
                    }
                }
                continue;
            }
            program::Entry::Symbolic(spec) => {
                use keel_asm_ir::{PageKind as P, SymbolicInstruction as S};
                spec.validate().map_err(err)?;
                symbol(spec.target()).map_err(err)?;
                if !data::is_code(&result, current)
                    || !data::position(&result, current).is_multiple_of(4)
                {
                    return Err(err(
                        "instructions require a four-byte-aligned executable section".into(),
                    ));
                }
                let instruction = spec.zero_instruction();
                let word = crate::encode_instruction(&instruction, features).map_err(err)?;
                let offset = data::position(&result, current) as usize;
                if metrics {
                    locations.push(InstructionLocation {
                        line,
                        section: current,
                        offset,
                    });
                }
                match spec {
                    S::Conditional { .. } | S::Compare { .. } => {
                        let mut fields = BumpVec::new_in(&arena);
                        fields.extend(
                            instruction
                                .fields
                                .iter()
                                .copied()
                                .filter(|(f, _)| *f != FieldId::imm19),
                        );
                        branches.push(Branch {
                            section: current,
                            line,
                            offset,
                            symbol: true,
                            spec: control::Branch {
                                encoding: instruction.opcode,
                                field: FieldId::imm19,
                                width: 19,
                                fields,
                                target: spec.target(),
                            },
                        });
                    }
                    S::Address { register, .. } => addresses.push((
                        current,
                        line,
                        offset,
                        u32::from(*register),
                        relocate::Expression {
                            name: spec.target(),
                            modifier: None,
                            addend: 0,
                        },
                    )),
                    S::PageAddress { kind, .. } | S::PageOffset { kind, .. } => {
                        let page = matches!(spec, S::PageAddress { .. });
                        let kind = match (kind, page) {
                            (P::Direct, true) => crate::RelocationKind::Page21,
                            (P::Direct, false) => crate::RelocationKind::PageOff12,
                            (P::Got, true) => crate::RelocationKind::GotPage21,
                            (P::Got, false) => crate::RelocationKind::GotPageOff12,
                            (P::ThreadLocal, true) => crate::RelocationKind::TlvpPage21,
                            (P::ThreadLocal, false) => crate::RelocationKind::TlvpPageOff12,
                        };
                        let offset = u32::try_from(offset)
                            .map_err(|_| err("relocation offset exceeds 32 bits".into()))?;
                        relocation_lines.insert((current, offset), line);
                        result.relocations.push(crate::Relocation {
                            section: current,
                            size: 4,
                            subtract_symbol: None,
                            offset,
                            symbol: spec.target().into(),
                            kind,
                            addend: 0,
                        });
                    }
                }
                data::instruction(&mut result, current, &word.to_le_bytes()).map_err(err)?;
                continue;
            }
            program::Entry::Branch(link, target) => {
                symbol(target).map_err(err)?;
                if !data::is_code(&result, current)
                    || !data::position(&result, current).is_multiple_of(4)
                {
                    return Err(err(
                        "instructions require a four-byte-aligned executable section".into(),
                    ));
                }
                if metrics {
                    locations.push(InstructionLocation {
                        section: current,
                        line,
                        offset: data::position(&result, current) as usize,
                    });
                }
                branches.push(Branch {
                    section: current,
                    line,
                    offset: data::position(&result, current) as usize,
                    symbol: true,
                    spec: control::Branch {
                        encoding: if link {
                            keel_asm_ir::Opcode::BL_only_branch_imm
                        } else {
                            keel_asm_ir::Opcode::B_only_branch_imm
                        },
                        field: FieldId::imm26,
                        width: 26,
                        fields: BumpVec::new_in(&arena),
                        target,
                    },
                });
                data::instruction(&mut result, current, &[0; 4]).map_err(err)?;
                continue;
            }
            program::Entry::Instruction(instruction) => {
                if !data::is_code(&result, current)
                    || !data::position(&result, current).is_multiple_of(4)
                {
                    return Err(err(
                        "instructions require a four-byte-aligned executable section".into(),
                    ));
                }
                let word = crate::encode_instruction(instruction, features).map_err(err)?;
                if metrics {
                    locations.push(InstructionLocation {
                        section: current,
                        line,
                        offset: data::position(&result, current) as usize,
                    });
                }
                data::instruction(&mut result, current, &word.to_le_bytes()).map_err(err)?;
                continue;
            }
            program::Entry::Words(words) => {
                // One statement per word, from `line`. Each keeps the
                // section and its alignment, so only the first can fail.
                if !data::is_code(&result, current)
                    || !data::position(&result, current).is_multiple_of(4)
                {
                    return Err(err(
                        "instructions require a four-byte-aligned executable section".into(),
                    ));
                }
                if metrics {
                    let start = data::position(&result, current) as usize;
                    locations.extend((0..words.len()).map(|i| InstructionLocation {
                        section: current,
                        line: line + i,
                        offset: start + 4 * i,
                    }));
                }
                data::instructions(&mut result, current, words, line)?;
                continue;
            }
            program::Entry::Label(name) => {
                symbol(name).map_err(err)?;
                let location = data::Location {
                    section: current,
                    offset: data::position(&result, current),
                };
                if labels.insert(name, location).is_some() {
                    return Err(err(format!("duplicate label `{name}`")));
                }
                label_order.push((name, location));
                continue;
            }
            program::Entry::Text(raw) => raw,
        };
        let mut s = raw.trim_fast();
        while let Some((name, rest)) = s
            .split_once(':')
            .filter(|(name, _)| !name.trim_fast().contains(['"', ' ', '\t']))
        {
            let name = name.trim_fast();
            symbol(name).map_err(err)?;
            let location = data::Location {
                section: current,
                offset: data::position(&result, current),
            };
            if labels.insert(name, location).is_some() {
                return Err(err(format!("duplicate label `{name}`")));
            }
            label_order.push((name, location));
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
        if !record_words
            && let Some(&cached) = fixed_words.get(next_word)
            && cached >> 32 == statement as u64
        {
            next_word += 1;
            // Layout may have moved since encoding. Keep the same executable
            // section/alignment checks and final metric positions on replay.
            if !data::is_code(&result, current)
                || !data::position(&result, current).is_multiple_of(4)
            {
                return Err(err(
                    "instructions require a four-byte-aligned executable section".into(),
                ));
            }
            if metrics {
                locations.push(InstructionLocation {
                    section: current,
                    line,
                    offset: data::position(&result, current) as usize,
                });
            }
            data::instruction(&mut result, current, &(cached as u32).to_le_bytes()).map_err(err)?;
            continue;
        }
        let (op, operands) = s.split_once(char::is_whitespace).unwrap_or((s, ""));
        let operands = operands.trim_fast();
        let op = lower(op);
        if (op == ".loc" || op.starts_with(".cfi_")) && !data::is_code(&result, current) {
            return Err(err(if op == ".loc" {
                ".loc requires __text or another executable section".into()
            } else {
                "CFI directives require __text or another executable section".into()
            }));
        }
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
        if op == ".loh" {
            let (kind, operands) = operands
                .split_once(char::is_whitespace)
                .ok_or_else(|| err("expected LOH kind and symbols".into()))?;
            let kind = match kind {
                "AdrpAdrp" | "1" => 1,
                "AdrpLdr" | "2" => 2,
                "AdrpAddLdr" | "3" => 3,
                "AdrpLdrGotLdr" | "4" => 4,
                "AdrpAddStr" | "5" => 5,
                "AdrpLdrGotStr" | "6" => 6,
                "AdrpAdd" | "7" => 7,
                "AdrpLdrGot" | "8" => 8,
                _ => return Err(err("invalid LOH kind".into())),
            };
            let args = memory::operands(operands.trim_fast()).map_err(err)?;
            let arity = if matches!(kind, 1 | 2 | 7 | 8) { 2 } else { 3 };
            count(&args, arity, arity).map_err(err)?;
            let mut names = [""; 3];
            for (out, name) in names.iter_mut().zip(args.iter()) {
                symbol(name).map_err(err)?;
                *out = name;
            }
            loh.push((line, kind, names, arity));
            continue;
        }
        if op == ".build_version" {
            if has_build_version {
                return Err(err("duplicate .build_version".into()));
            }
            result.build_version = data::build_version(operands).map_err(err)?;
            has_build_version = true;
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
                    if let Some(sub) = &expr.subtract
                        && !data::is_debug(&result, current)
                    {
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
                let modifier = argument.split_once("@GOT");
                let got = modifier.is_some();
                if let Some((_, suffix)) = modifier
                    && suffix
                        .chars()
                        .next()
                        .is_some_and(|c| !c.is_whitespace() && c != '+' && c != '-')
                {
                    return Err(err("data GOT pointers require the @GOT modifier".into()));
                }
                let expression: &str = if let Some((head, tail)) = modifier {
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
                let (expression, right_shift) =
                    if let Some((inner, shift)) = expression.split_once(">>") {
                        let inner = inner
                            .trim_fast()
                            .strip_prefix('(')
                            .and_then(|s| s.strip_suffix(')'))
                            .ok_or_else(|| {
                                err("shifted data requires a parenthesized expression".into())
                            })?;
                        (inner, unsigned(shift.trim_fast(), 63).map_err(err)?)
                    } else {
                        (expression, 0)
                    };
                let expr = data::expression(expression).map_err(err)?;
                if expr.add.is_none() {
                    data::append(
                        &mut result,
                        current,
                        &data::value(
                            data::shift_value(expr.constant, right_shift).map_err(err)?,
                            width,
                        )
                        .map_err(err)?,
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
                        right_shift,
                        got,
                    });
                    data::reserve(&mut result, current, u64::from(width), 0).map_err(err)?;
                }
            }
            continue;
        }
        match op.as_ref() {
            ".comm" => {
                count(&args, 2, 3).map_err(err)?;
                symbol(args[0]).map_err(err)?;
                let size = u64::try_from(integer(args[1]).map_err(err)?)
                    .map_err(|_| err("invalid common size".into()))?;
                let alignment = args
                    .get(2)
                    .map_or(Ok(0), |s| unsigned(s, 15))
                    .map_err(err)? as u8;
                if size == 0 || common.insert(args[0], (size, alignment)).is_some() {
                    return Err(err("empty or duplicate common definition".into()));
                }
                globals.insert(args[0], line);
                continue;
            }
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
            ".no_dead_strip" => {
                count(&args, 1, usize::MAX).map_err(err)?;
                for name in args.iter().copied() {
                    symbol(name).map_err(err)?;
                    no_dead_strip.insert(name, line);
                }
                continue;
            }
            ".weak_reference" => {
                count(&args, 1, usize::MAX).map_err(err)?;
                for name in args.iter().copied() {
                    symbol(name).map_err(err)?;
                    if name.starts_with('L') {
                        return Err(err("weak reference requires a non-local symbol".into()));
                    }
                    weak_references.insert(name, line);
                }
                continue;
            }
            ".weak_definition" | ".weak_def_can_be_hidden" => {
                count(&args, 1, usize::MAX).map_err(err)?;
                for name in args.iter().copied() {
                    symbol(name).map_err(err)?;
                    if name.starts_with('L') {
                        return Err(err("weak definition requires a non-local symbol".into()));
                    }
                    weak_definitions.insert(name, line);
                    if op == ".weak_def_can_be_hidden" {
                        weak_hidden.insert(name, line);
                    }
                }
                continue;
            }
            ".globl" | ".global" | ".extern" | ".private_extern" => {
                count(&args, 1, usize::MAX).map_err(err)?;
                for name in args.iter().copied() {
                    symbol(name).map_err(err)?;
                    if op == ".private_extern" {
                        if name.starts_with('L') {
                            return Err(err("private extern requires a non-local symbol".into()));
                        }
                        private_externs.insert(name, line);
                    }
                    // LLVM MC ignores .extern: undefined references are already
                    // external, and a later definition stays local unless .globl.
                    if op != ".extern" {
                        globals.insert(name, line);
                    }
                }
                continue;
            }
            ".p2align" => {
                if args.is_empty() || args.len() > 3 {
                    return Err(err("expected 1..=3 alignment operands".into()));
                }
                let power = unsigned(args[0], 16).map_err(err)?;
                let fill = args
                    .get(1)
                    .filter(|s| !s.is_empty())
                    .map(|s| unsigned(s, 255))
                    .transpose()
                    .map_err(err)?
                    .map(|n| n as u8);
                let maximum = args
                    .get(2)
                    .map(|s| unsigned(s, u32::MAX as i64))
                    .transpose()
                    .map_err(err)?
                    .map(u64::from);
                if maximum == Some(0) {
                    return Err(err("alignment maximum byte count must be positive".into()));
                }
                data::align_with(&mut result, current, power, fill, maximum).map_err(err)?;
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
                let (section_name, name, size, alignment) =
                    data::allocation(&args, tls).map_err(err)?;
                if labels.contains_key(name) {
                    return Err(err(format!("duplicate label `{name}`")));
                }
                let section = data::select(
                    &mut result,
                    "__DATA",
                    section_name,
                    if tls {
                        crate::SectionKind::ThreadZeroFill
                    } else {
                        crate::SectionKind::ZeroFill
                    },
                )
                .map_err(err)?;
                data::align(&mut result, section, alignment).map_err(err)?;
                let location = data::Location {
                    section,
                    offset: data::position(&result, section),
                };
                labels.insert(name, location);
                label_order.push((name, location));
                data::reserve(&mut result, section, size, 0).map_err(err)?;
                continue;
            }
            _ if op.starts_with('.') => return Err(err(format!("unsupported directive `{op}`"))),
            _ => {}
        }
        if !data::is_code(&result, current) || !data::position(&result, current).is_multiple_of(4) {
            return Err(err(
                "instructions require a four-byte-aligned executable section".into(),
            ));
        }
        let (op, args) = if op.contains('.')
            || op == "mov"
                && args.first().is_some_and(|s| s.starts_with(['s', 'd']))
                && args
                    .get(1)
                    .is_some_and(|s| s.contains('[') && !s.contains('.'))
        {
            vector::apple_syntax(&op, args, &arena).map_err(err)?
        } else {
            (op.as_ref(), args)
        };
        if metrics {
            locations.push(InstructionLocation {
                section: current,
                line,
                offset: data::position(&result, current) as usize,
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
                section: current,
                line,
                offset: data::position(&result, current) as usize,
                symbol: false,
                spec,
            });
            data::instruction(&mut result, current, &[0; 4]).map_err(err)?;
            continue;
        }
        if let Some((word, mut relocation)) = relocate::instruction(&op, &args).map_err(err)? {
            relocation.section = current;
            relocation.offset = data::position(&result, current)
                .try_into()
                .map_err(|_| err("text offset exceeds 32 bits".into()))?;
            relocation_lines.insert((current, relocation.offset), line);
            result.relocations.push(relocation);
            data::instruction(&mut result, current, &word.to_le_bytes()).map_err(err)?;
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
                addresses.push((
                    current,
                    line,
                    data::position(&result, current) as usize,
                    reg(args[0], false, true).map_err(err)?.number,
                    expr,
                ));
                relocate::address_word("adr", args[0], 0).map_err(err)?
            };
            data::instruction(&mut result, current, &word.to_le_bytes()).map_err(err)?;
            continue;
        }
        let word = match scalable::instruction(&op, &args, features).map_err(err)? {
            Some(word) => word,
            None => match vector::numeric(&op, &args, features).map_err(err)? {
                Some(word) => word,
                None => instruction_with_features(op, &args, features).map_err(err)?,
            },
        };
        if record_words && let Ok(statement) = u32::try_from(statement) {
            fixed_words.push((u64::from(statement) << 32) | u64::from(word));
        }
        data::instruction(&mut result, current, &word.to_le_bytes()).map_err(err)?;
    }
    // Resolve assignments before instruction fixups so function aliases (including
    // Clang's TLS initialization wrapper aliases) retain their defining section.
    // Iteration admits forward/chained assignments and diagnoses unresolved cycles.
    let mut constants = BTreeMap::new();
    let mut alias_definitions = BTreeMap::new();
    while !aliases.is_empty() {
        let mut resolved = Vec::new();
        for (&name, (line, expr)) in &aliases {
            let err = |message| Error::new(*line, message);
            if labels.contains_key(name) {
                return Err(err("assignment conflicts with label"));
            }
            if expr.subtract.is_none()
                && let Some(add) = expr.add
                && let Some(target) = labels.get(add).copied()
            {
                if expr.constant != 0 {
                    return Err(err(
                        "section-symbol aliases with nonzero addends require unsupported Mach-O alternate-entry semantics",
                    ));
                }
                labels.insert(name, target);
                // Exact aliases take the defining symbol's weak attributes,
                // replacing annotations on the alias itself. Keep this identity
                // separate from locations, visibility and linker-atom ownership.
                alias_definitions.insert(name, alias_definitions.get(add).copied().unwrap_or(add));
                // Source order is significant for temporary labels at atom ends.
                // Keep aliases beside their target, rather than assigning them
                // to whichever function happened to be parsed last.
                let position = label_order
                    .iter()
                    .position(|(label, _)| *label == add)
                    .ok_or_else(|| err("symbol alias has no source definition"))?;
                label_order.insert(position + 1, (name, target));
                resolved.push(name);
                continue;
            }
            let value = match (expr.add, expr.subtract) {
                (Some(add), Some(sub)) => {
                    let (Some(a), Some(b)) = (labels.get(add), labels.get(sub)) else {
                        continue;
                    };
                    if a.section != b.section {
                        return Err(err("assignments require a same-section difference"));
                    }
                    expr.constant
                        .checked_add(i128::from(a.offset) - i128::from(b.offset))
                        .ok_or_else(|| err("assignment overflow"))?
                }
                (None, None) => expr.constant,
                (Some(add), None) => {
                    let Some(value) = constants.get(add).copied() else {
                        continue;
                    };
                    expr.constant
                        .checked_add(value)
                        .ok_or_else(|| err("assignment overflow"))?
                }
                _ => {
                    return Err(err(
                        "assignments require constants, symbol aliases or same-section differences",
                    ));
                }
            };
            if globals.contains_key(name) || weak_definitions.contains_key(name) {
                return Err(err(
                    "exported absolute symbol assignments require unsupported Mach-O N_ABS semantics",
                ));
            }
            constants.insert(name, value);
            resolved.push(name);
        }
        if resolved.is_empty() {
            let (_, (line, _)) = aliases.first_key_value().unwrap();
            return Err(Error::new(*line, "undefined or cyclic symbol assignment"));
        }
        for name in resolved {
            aliases.remove(name);
        }
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
    // Temporary names are rarely global: skip their lookups when none is.
    let temporary_globals = globals.keys().any(|name| name.starts_with('L'));
    for mut branch in branches {
        let err = |message| Error::new(branch.line, message);
        let kind = BranchKind(branch.spec.encoding);
        let delta = if !branch.symbol && is_integer(branch.spec.target) {
            integer(branch.spec.target).map_err(err)?
        } else {
            let expr = if branch.symbol {
                relocate::Expression {
                    name: branch.spec.target,
                    modifier: None,
                    addend: 0,
                }
            } else {
                relocate::expression(branch.spec.target).map_err(err)?
            };
            let target = labels.get(expr.name);
            let local = expr.name.starts_with('L')
                && !(temporary_globals && globals.contains_key(expr.name));
            if branch.spec.width == 26
                && (target.is_some_and(|t| {
                    t.section != branch.section
                        || (result.subsections_via_symbols
                            && !atoms.contains(
                                data::Location {
                                    section: branch.section,
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
                relocation_lines.insert((branch.section, offset), branch.line);
                result.relocations.push(crate::Relocation {
                    section: branch.section,
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
                if target.section != branch.section {
                    return Err(err(format!(
                        "{kind} targets must be in the same executable section"
                    )));
                }
                if result.subsections_via_symbols
                    && !atoms.contains(
                        data::Location {
                            section: branch.section,
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
        let instruction =
            keel_asm_ir::Instruction::new(branch.spec.encoding, branch.spec.fields.iter().copied());
        let word = crate::encode_instruction(&instruction, features).map_err(err)?;
        data::bytes_mut(&mut result, branch.section).map_err(err)?
            [branch.offset..branch.offset + 4]
            .copy_from_slice(&word.to_le_bytes());
    }
    for (section, line, offset, destination, expr) in addresses {
        let target = labels.get(expr.name).ok_or_else(|| {
            Error::new(
                line,
                format!("adr requires a defined text symbol `{}`", expr.name),
            )
        })?;
        if target.section != section {
            return Err(Error::new(
                line,
                "adr requires a target in the same text section",
            ));
        }
        if result.subsections_via_symbols
            && !atoms.contains(
                data::Location {
                    section,
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
        let word = relocate::address_word_register(
            "adr",
            destination,
            target.offset as i64 + expr.addend - offset as i64,
        )
        .map_err(|e| Error::new(line, e))?;
        data::bytes_mut(&mut result, section).map_err(|e| Error::new(line, e))?[offset..offset + 4]
            .copy_from_slice(&word.to_le_bytes());
    }
    debug.finish(&mut result, &mut labels, &prefix)?;
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
            let bytes = data::value(
                data::shift_value(constant, fixup.right_shift)
                    .map_err(|e| Error::new(fixup.line, e))?,
                fixup.width,
            )
            .map_err(|e| Error::new(fixup.line, e))?;
            let out = data::bytes_mut(&mut result, fixup.location.section)
                .map_err(|e| Error::new(fixup.line, e))?;
            let offset = fixup.location.offset as usize;
            out[offset..offset + usize::from(fixup.width)].copy_from_slice(&bytes);
        } else {
            if fixup.right_shift != 0
                || fixup.width < 4
                || (fixup.width != 8 && expr.subtract.is_none())
            {
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
    for (line, kind, names, arity) in loh {
        let mut offsets = [0; 3];
        let mut first = None;
        let mut hint_section = None;
        for (offset, name) in offsets.iter_mut().zip(&names[..arity]) {
            let location = *labels
                .get(name)
                .ok_or_else(|| Error::new(line, format!("undefined LOH symbol `{name}`")))?;
            if !data::is_code(&result, location.section)
                || hint_section.is_some_and(|s| s != location.section)
                || location.offset % 4 != 0
                || location.offset >= data::position(&result, location.section)
            {
                return Err(Error::new(
                    line,
                    "LOH symbols must reference text instructions",
                ));
            }
            if let Some((first_name, first_location)) = first {
                if result.subsections_via_symbols
                    && !atoms.same((first_name, first_location), (name, location))
                {
                    return Err(Error::new(
                        line,
                        "LOH symbols must belong to one linker atom",
                    ));
                }
            } else {
                first = Some((*name, location));
            }
            hint_section = Some(location.section);
            *offset = location.offset;
        }
        result
            .linker_optimization_hints
            .push(crate::LinkerOptimizationHint {
                section: hint_section.unwrap(),
                kind,
                offsets,
            });
    }
    if result.subsections_via_symbols {
        atoms.rebase(&mut result, &mut labels, &globals, &prefix)?;
    }
    // A compact program's assembly goes straight to the Mach-O writer, which
    // drops temporary `L` labels that are neither global nor a relocation's
    // symbol, then orders the remaining (far fewer) symbols itself. Omit
    // those labels and skip sorting; other assemblies keep every symbol in
    // name order, as does a pass whose symbol locations seed another pass.
    let compact = matches!(source, program::Input::Compact(_)) && !symbolic_leb;
    // Label names are unique; dedupe the other (mostly repeated) references
    // here so that only distinct names are sorted.
    let mut names = if compact {
        // Look up only the global and relocated names, not every label.
        let mut names = BumpVec::from_iter_in(
            labels.keys().copied().filter(|n| !n.starts_with('L')),
            &arena,
        );
        let mut kept = FxHashSet::default();
        let referenced = result
            .relocations
            .iter()
            .flat_map(|r| std::iter::once(r.symbol.as_str()).chain(r.subtract_symbol.as_deref()));
        for name in globals.keys().copied().chain(referenced) {
            if name.starts_with('L')
                && let Some((&label, _)) = labels.get_key_value(name)
                && kept.insert(label)
            {
                names.push(label);
            }
        }
        names
    } else {
        BumpVec::from_iter_in(labels.keys().copied(), &arena)
    };
    let mut others = FxHashSet::default();
    let mut add = |name| {
        if !labels.contains_key(name) && others.insert(name) {
            names.push(name);
        }
    };
    for &name in globals
        .keys()
        .chain(weak_definitions.keys())
        .chain(weak_references.keys())
        .chain(no_dead_strip.keys())
    {
        add(name);
    }
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
            add(name.as_str());
        }
    }
    if !compact {
        names.sort_unstable_by(|a, b| name_order(a, b));
    }
    for name in common.keys() {
        if labels.contains_key(name) {
            return Err(Error::new(
                globals[name],
                "common symbol also has a section definition",
            ));
        }
    }
    result.symbols = names
        .into_iter()
        .map(|name| {
            let location = labels.get(&name);
            let definition = alias_definitions.get(name).copied().unwrap_or(name);
            Symbol {
                common: common.get(name).copied(),
                global: location.is_none() || globals.contains_key(&name),
                private_extern: private_externs.contains_key(name),
                weak_reference: location.is_none() && weak_references.contains_key(name),
                weak_definition: weak_definitions.contains_key(definition),
                no_dead_strip: no_dead_strip.contains_key(name),
                weak_can_be_hidden: weak_hidden.contains_key(definition),
                offset: location.map(|l| l.offset),
                section: location.map(|l| l.section),
                name: name.to_string(),
            }
        })
        .collect();
    Ok((result, symbolic_leb, locations))
}

/// `str` order, comparing eight bytes at a time inline: labels share long
/// prefixes (`Lfunction_block12`), which a `memcmp` call per comparison
/// handles slowly.
fn name_order(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.as_bytes(), b.as_bytes());
    while let (Some((x, rest_a)), Some((y, rest_b))) =
        (a.split_first_chunk::<8>(), b.split_first_chunk::<8>())
    {
        if x != y {
            return u64::from_be_bytes(*x).cmp(&u64::from_be_bytes(*y));
        }
        (a, b) = (rest_a, rest_b);
    }
    for (x, y) in a.iter().zip(b) {
        if x != y {
            return x.cmp(y);
        }
    }
    a.len().cmp(&b.len())
}

fn native_expression(e: &keel_asm_ir::Expression) -> data::Expression<'_> {
    data::Expression {
        add: e.add.as_deref(),
        subtract: e.subtract.as_deref(),
        constant: e.constant,
    }
}

#[cfg(test)]
mod layout_replay_tests {
    use super::*;

    #[test]
    fn fixed_words_match_fresh_encoding_on_every_symbolic_layout_pass() {
        let source = ".text\n.globl _f\n_f: mov x0, #7\nLstart: add x0, x0, #1\n\
            b Lend\nadr x1, Lend\nadrp x2, _external@PAGE\n\
            ldr x2, [x2, _external@PAGEOFF]\nLend: ret\n\
            .section __TEXT,__cold,regular,pure_instructions\n\
            /* across\nlines */ Lcold: fadd d0, d0, d1\nret\n\
            .section __DWARF,__debug_info,regular,debug\n\
            Lbase: .uleb128 Lfinish-Lbase\n.space 127\n\
            .uleb128 Lfinish-Lbase\nLfinish: .sleb128 Lbase-Lfinish\n\
            .quad Lend-Lstart\n.text\n.p2align 2\nnop\n";
        let input = program::Input::Text(source);
        let mut previous = None;
        let mut words = Vec::new();
        let mut passes = 0;
        for _ in 0..32 {
            let cached = assemble_pass(
                input,
                previous.as_ref(),
                true,
                crate::TargetFeatures::default(),
                &mut words,
            )
            .unwrap();
            let fresh = assemble_pass(
                input,
                previous.as_ref(),
                true,
                crate::TargetFeatures::default(),
                &mut Vec::new(),
            )
            .unwrap();
            assert_eq!(cached, fresh);
            passes += 1;
            let symbols: BTreeMap<_, _> = cached
                .0
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
            if previous.as_ref() == Some(&symbols) {
                break;
            }
            previous = Some(symbols);
        }
        assert!(passes >= 3);
        assert_eq!(words.len(), 6); // branches, ADR and symbol relocations excluded
    }
}

#[cfg(test)]
mod name_order_tests {
    #[test]
    fn matches_str_order() {
        let names = [
            "",
            "L",
            "La",
            "Lsqlite3VdbeExec_block447",
            "Lsqlite3VdbeExec_block45",
            "Lsqlite3VdbeExec_block4",
            "Lsqlite3VdbeExec",
            "Lsqlite3Vdbe",
            "_sqlite3",
            "_sqlite3_",
            "ltmp_section1",
            "Z",
            "a$b",
            "a.b",
            "abcdefgh",
            "abcdefghi",
            "abcdefgg",
            "abcdefgh\u{e9}",
            "abcdefgh\u{7f}",
        ];
        for a in names {
            for b in names {
                assert_eq!(super::name_order(a, b), a.cmp(b), "{a:?} {b:?}");
            }
        }
    }
}
