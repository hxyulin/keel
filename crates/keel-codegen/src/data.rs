//! Data objects as assembler directives, shared by the targets.

use crate::Format;
use keel_ir::{DataInit, DataItem, DataObject, Type};
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
pub(crate) fn emit_data(out: &mut String, format: Format, data: &DataObject, init: &DataInit) {
    if format == Format::Elf {
        emit_elf(out, data, init);
        return;
    }
    let name = &format.symbol(&data.name);
    let log2_align = init.align.trailing_zeros();
    if data.thread_local {
        emit_thread_local(out, format, data, init);
        return;
    }
    if !data.local {
        writeln!(out, "\t.globl\t{name}").unwrap();
    }
    if !init.constant && init.is_zero() {
        writeln!(
            out,
            "\t.zerofill\t__DATA,__bss,{name},{},{log2_align}",
            init.size()
        )
        .unwrap();
        return;
    }
    let section = match (init.constant, init.has_relocations()) {
        (false, _) => "__DATA,__data",
        (true, true) => "__DATA,__const",
        (true, false) => "__TEXT,__const",
    };
    writeln!(
        out,
        "\t.section\t{section}\n\t.p2align\t{log2_align}\n{name}:"
    )
    .unwrap();
    emit_items(out, format, init);
}

/// ELF puts a data object in the section its contents call for, as on
/// Mach-O: `.bss` (zero-fill), `.data`, `.data.rel.ro` (constant, but
/// with pointers the loader writes before making it read-only) or
/// `.rodata`. A thread-local goes in `.tbss` or `.tdata`, the image each
/// thread's copy is made from; the code finds its copy from the thread
/// pointer. Each symbol gets its type and size, which debuggers and
/// `nm` show.
fn emit_elf(out: &mut String, data: &DataObject, init: &DataInit) {
    let name = &data.name;
    let section = match (data.thread_local, init.constant, init.is_zero()) {
        (true, _, true) => ".tbss,\"awT\",@nobits",
        (true, _, false) => ".tdata,\"awT\",@progbits",
        (false, false, true) => ".bss",
        (false, false, false) => ".data",
        (false, true, _) if init.has_relocations() => ".data.rel.ro,\"aw\",@progbits",
        (false, true, _) => ".rodata",
    };
    writeln!(out, "\t.section\t{section}").unwrap();
    if !data.local {
        writeln!(out, "\t.globl\t{name}").unwrap();
    }
    let log2_align = init.align.trailing_zeros();
    writeln!(
        out,
        "\t.type\t{name},@object\n\t.size\t{name}, {}\n\t.p2align\t{log2_align}\n{name}:",
        init.size()
    )
    .unwrap();
    if section.contains("nobits") || section == ".bss" {
        writeln!(out, "\t.zero\t{}", init.size()).unwrap();
    } else {
        emit_items(out, Format::Elf, init);
    }
}

/// A thread-local variable is a descriptor, `_name` in `__thread_vars`:
/// the getter (`__tlv_bootstrap` until first use), a key the loader fills
/// in, and the address of the initial value, `_name$tlv$init`, in
/// `__thread_data` (or `__thread_bss` if zero). Each thread's copy is
/// made from that.
fn emit_thread_local(out: &mut String, format: Format, data: &DataObject, init: &DataInit) {
    let name = &format.symbol(&data.name);
    let log2_align = init.align.trailing_zeros();
    if init.is_zero() {
        writeln!(
            out,
            "\t.tbss\t{name}$tlv$init, {}, {log2_align}",
            init.size()
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "\t.section\t__DATA,__thread_data,thread_local_regular\n\t.p2align\t{log2_align}\n{name}$tlv$init:"
        )
        .unwrap();
        emit_items(out, format, init);
    }
    out.push_str("\t.section\t__DATA,__thread_vars,thread_local_variables\n");
    if !data.local {
        writeln!(out, "\t.globl\t{name}").unwrap();
    }
    writeln!(out, "\t.p2align\t3\n{name}:").unwrap();
    writeln!(
        out,
        "\t.quad\t__tlv_bootstrap\n\t.quad\t0\n\t.quad\t{name}$tlv$init"
    )
    .unwrap();
}

/// The bytes of a data object, as directives.
fn emit_items(out: &mut String, format: Format, init: &DataInit) {
    for item in &init.items {
        match item {
            DataItem::Bytes(bytes) if bytes.is_empty() => {}
            DataItem::Bytes(bytes) => {
                let list: Vec<String> = bytes.iter().map(|b| b.to_string()).collect();
                writeln!(out, "\t.byte\t{}", list.join(", ")).unwrap();
            }
            // The directives truncate to their width, and the verifier has
            // checked the value fits, so the bits are right either way.
            DataItem::Int { ty, value } => {
                let directive = match ty {
                    Type::I8 => ".byte",
                    Type::I16 => ".short",
                    Type::I32 => ".long",
                    _ => ".quad",
                };
                let bits = 8 * ty.bytes().unwrap_or(8);
                let unsigned = (*value as u64) & (u64::MAX >> (64 - bits));
                writeln!(out, "\t{directive}\t{unsigned}").unwrap();
            }
            DataItem::Zero(0) => {}
            DataItem::Zero(n) => writeln!(out, "\t.space\t{n}").unwrap(),
            DataItem::Ptr { symbol, addend: 0 } => {
                writeln!(out, "\t.quad\t{}", format.symbol(symbol)).unwrap()
            }
            DataItem::Ptr { symbol, addend } => {
                writeln!(out, "\t.quad\t{}+{addend}", format.symbol(symbol)).unwrap()
            }
        }
    }
}
