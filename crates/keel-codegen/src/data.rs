//! Data objects as assembler directives, shared by the targets.

use crate::Format;
use crate::metadata::{Directive, Expression, SectionKind, Sink};
use keel_ir::{DataInit, DataItem, DataObject};

/// Emits a data object into the section its contents call for:
///
/// - mutable and all zero: zero-fill (`__bss`), which takes no file space;
/// - mutable: `__DATA,__data`;
/// - constant with pointers: `__DATA,__const`, since the loader writes the
///   pointers at startup and then makes the pages read-only;
/// - constant without pointers: `__TEXT,__const`, read-only from the start.
///
/// Objects are exported (`.globl`) unless local, like functions; hidden ones
/// are also `.private_extern`.
pub(crate) fn emit_macho(
    out: &mut impl Sink,
    data: &DataObject,
    init: &DataInit,
    symbols: &crate::symbols::Symbols<'_>,
) {
    let name = Format::MachO.symbol(&data.name);
    let alignment = init.align.trailing_zeros();
    if data.retain {
        out.directive(Directive::NoDeadStrip(name.clone()));
    }
    if data.thread_local {
        // The template is private implementation storage. An asm-renamed user
        // definition or import may legitimately use its customary spelling.
        // Appended '$' cannot collide with another template's unsuffixed name,
        // which always ends in '$tlv$init'. Check imports as well as definitions.
        let mut initial = format!("{}$tlv$init", data.name);
        while symbols.contains(&initial) {
            initial.push('$');
        }
        let initial = Format::MachO.symbol(&initial);
        if data.retain {
            out.directive(Directive::NoDeadStrip(initial.clone()));
        }
        if let Some(section) = data.section {
            let (segment, section) = section.as_str().split_once(',').unwrap();
            let kind = if init.is_zero() {
                SectionKind::ThreadZeroFill
            } else {
                SectionKind::ThreadData
            };
            out.section(segment, section, kind);
            out.align(alignment);
            out.label(initial.clone());
            if init.is_zero() {
                out.directive(Directive::Space(init.size()));
            } else {
                emit_macho_items(out, init);
            }
        } else if init.is_zero() {
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
        if data.hidden {
            out.directive(Directive::PrivateExtern(name.clone()));
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
    if data.weak || data.weak_odr {
        out.directive(Directive::WeakDefinition(name.clone()));
    }
    if data.section.is_none() && !data.weak && !data.weak_odr && !init.constant && init.is_zero() {
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
    let (segment, section) = data
        .section
        .as_ref()
        .map(|s| s.as_str().split_once(',').unwrap())
        .unwrap_or((segment, section));
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
    let (section, mut flags, mut nobits) = match (data.thread_local, init.constant, init.is_zero())
    {
        (true, _, true) => (".tbss", 0x403, true),
        (true, _, false) => (".tdata", 0x403, false),
        (false, false, true) => (".bss", 3, true),
        (false, false, false) => (".data", 3, false),
        (false, true, _) if init.has_relocations() => (".data.rel.ro", 3, false),
        _ => (".rodata", 2, false),
    };
    if let Some(section) = data.section {
        use keel_ir::section::Placement;
        match section.initialized_storage(Placement::for_object(data.thread_local, init)) {
            Placement::Data => nobits = false,
            Placement::ReadOnly => {
                flags = 2;
                nobits = false;
            }
            _ => {}
        }
    }
    let custom = data.section.map(|s| s.as_str().to_owned());
    let section = custom.unwrap_or_else(|| {
        if data.retain {
            format!("{section}.keel.retain.{}", data.name)
        } else {
            section.into()
        }
    });
    let note = data.section.is_some_and(|section| {
        section.initialized_storage(keel_ir::section::Placement::for_object(
            data.thread_local,
            init,
        )) == keel_ir::section::Placement::Note
    });
    if note {
        out.directive(Directive::ElfNoteSection {
            name: section,
            retain: data.retain,
        });
        nobits = false;
    } else if data.weak_odr {
        out.directive(Directive::ElfSectionMetadata {
            name: format!("{section}.{}", data.name),
            flags: flags | 0x200,
            nobits,
            entry_size: 0,
            group: Some(data.name.clone()),
        });
    } else {
        out.directive(Directive::ElfSection {
            name: section,
            flags: flags | if data.retain { 0x200000 } else { 0 },
            nobits,
        });
    }
    if !data.local {
        out.directive(Directive::Global(name.clone()));
    }
    if data.hidden {
        out.directive(Directive::ElfHidden(name.clone()));
    }
    if data.weak || data.weak_odr {
        out.directive(Directive::ElfWeak(name.clone()));
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
