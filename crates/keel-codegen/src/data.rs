//! Data objects as assembler directives, shared by the targets.

use crate::Format;
use crate::metadata::{Directive, Expression, SectionKind, Sink};
use keel_ir::{DataInit, DataItem, DataObject};
use std::fmt::Write;

/// `s` as a quoted assembler string.
pub(crate) fn asm_string(s: &str) -> String {
    let mut quoted = String::from("\"");
    for b in s.bytes() {
        match b {
            b'"' | b'\\' => write!(quoted, "\\{}", b as char).unwrap(),
            b' '..=b'~' => quoted.push(b as char),
            _ => write!(quoted, "\\{b:03o}").unwrap(),
        }
    }
    quoted.push('"');
    quoted
}

/// Emits a data object into the section its contents call for:
///
/// - mutable and all zero: zero-fill (`__bss`), which takes no file space;
/// - mutable: `__DATA,__data`;
/// - constant with pointers: `__DATA,__const`, since the loader writes the
///   pointers at startup and then makes the pages read-only;
/// - constant without pointers: `__TEXT,__const`, read-only from the start.
///
/// Objects are exported (`.globl`) unless local, like functions.
pub(crate) fn emit_macho(out: &mut impl Sink, data: &DataObject, init: &DataInit) {
    let name = Format::MachO.symbol(&data.name);
    let alignment = init.align.trailing_zeros();
    if data.thread_local {
        let initial = format!("{name}$tlv$init");
        if init.is_zero() {
            out.directive(Directive::ZeroFill {
                symbol: initial.clone(),
                size: init.size(),
                alignment: alignment as u8,
                thread_local: true,
            });
        } else {
            out.section("__DATA", "__thread_data", SectionKind::ThreadData);
            out.align(alignment);
            out.label(initial.clone());
            emit_macho_items(out, init);
        }
        out.section("__DATA", "__thread_vars", SectionKind::ThreadVariables);
        if !data.local {
            out.directive(Directive::Global(name.clone()));
        }
        out.align(3);
        out.label(name);
        out.integer(8, Expression::symbol("__tlv_bootstrap"));
        out.number(8, 0);
        out.integer(8, Expression::symbol(initial));
        return;
    }
    if !data.local {
        out.directive(Directive::Global(name.clone()));
    }
    if data.hidden {
        out.directive(Directive::PrivateExtern(name.clone()));
    }
    if data.weak_odr {
        out.directive(Directive::WeakDefinition(name.clone()));
    }
    if !data.weak_odr && !init.constant && init.is_zero() {
        out.directive(Directive::ZeroFill {
            symbol: name,
            size: init.size(),
            alignment: alignment as u8,
            thread_local: false,
        });
        return;
    }
    let (segment, section) = match (init.constant, init.has_relocations()) {
        (false, _) => ("__DATA", "__data"),
        (true, true) => ("__DATA", "__const"),
        (true, false) => ("__TEXT", "__const"),
    };
    out.section(segment, section, SectionKind::Data);
    out.align(alignment);
    out.label(name);
    emit_macho_items(out, init);
}
fn emit_macho_items(out: &mut impl Sink, init: &DataInit) {
    for item in &init.items {
        match item {
            DataItem::Bytes(bytes) => out.bytes(bytes),
            DataItem::Int { ty, value } => {
                let width = ty.bytes().unwrap_or(8);
                let bits = 8 * width;
                out.number(width as u8, (*value as u64) & (u64::MAX >> (64 - bits)));
            }
            DataItem::Zero(0) => {}
            DataItem::Zero(n) => out.directive(Directive::Space(u64::from(*n))),
            DataItem::Ptr { symbol, addend } => out.integer(
                8,
                Expression::symbol(Format::MachO.symbol(symbol)).with_addend(i128::from(*addend)),
            ),
            DataItem::RttiName { symbol } => out.integer(
                8,
                // Signed spelling encodes exactly the same modulo-2^64 addition.
                Expression::symbol(Format::MachO.symbol(symbol)).with_addend(i128::from(i64::MIN)),
            ),
        }
    }
}

/// ELF puts a data object in the section its contents call for, as on
/// Mach-O: `.bss` (zero-fill), `.data`, `.data.rel.ro` (constant, but
/// with pointers the loader writes before making it read-only) or
/// `.rodata`. A thread-local goes in `.tbss` or `.tdata`, the image each
/// thread's copy is made from; the code finds its copy from the thread
/// pointer. Each symbol gets its type and size, which debuggers and
/// `nm` show.
pub(crate) fn emit_elf(out: &mut impl Sink, data: &DataObject, init: &DataInit) {
    let name = data.name.clone();
    let (section, flags, nobits) = match (data.thread_local, init.constant, init.is_zero()) {
        (true, _, true) => (".tbss", 0x403, true),
        (true, _, false) => (".tdata", 0x403, false),
        (false, false, true) => (".bss", 3, true),
        (false, false, false) => (".data", 3, false),
        (false, true, _) if init.has_relocations() => (".data.rel.ro", 3, false),
        _ => (".rodata", 2, false),
    };
    out.directive(Directive::ElfSection {
        name: section.into(),
        flags,
        nobits,
    });
    if !data.local {
        out.directive(Directive::Global(name.clone()));
    }
    out.directive(Directive::ElfType {
        name: name.clone(),
        function: false,
    });
    out.directive(Directive::Size {
        name: name.clone(),
        expression: Expression::constant(init.size().into()),
    });
    out.align(init.align.trailing_zeros());
    out.label(name);
    if nobits {
        out.directive(Directive::Space(init.size()));
    } else {
        emit_elf_items(out, init);
    }
}

fn emit_elf_items(out: &mut impl Sink, init: &DataInit) {
    for item in &init.items {
        match item {
            DataItem::Bytes(b) => out.bytes(b),
            DataItem::Int { ty, value } => {
                let width = ty.bytes().unwrap_or(8);
                let bits = 8 * width;
                out.number(width as u8, (*value as u64) & (u64::MAX >> (64 - bits)));
            }
            DataItem::Zero(n) => out.directive(Directive::Space(u64::from(*n))),
            DataItem::Ptr { symbol, addend } => out.integer(
                8,
                Expression::symbol(symbol).with_addend(i128::from(*addend)),
            ),
            DataItem::RttiName { .. } => unreachable!("RTTI tagging is rejected for ELF"),
        }
    }
}
