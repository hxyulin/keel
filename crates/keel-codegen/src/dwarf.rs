//! DWARF 4 debug info as assembler directives, shared by the targets: a
//! compile unit with the module's functions, variables and types. The
//! line table is the assembler's, from the `.loc` directives.

use crate::Format;
use crate::metadata::{Directive, Expression, Sink};
use keel_ir::{
    DebugScope, DebugType, DebugTypeData, Encoding, EntityRef, Function, Language, MemberLocation,
    Module,
};

/// What the debug info needs to know of a compiled function.
pub(crate) struct FrameDebug {
    pub frame_register: Option<u8>,
    /// Where each stack slot is, from the frame base register.
    pub slots: Vec<i64>,
    /// The labels where the code's lexical scope changes, and the new
    /// scope. The function starts in its own (`None`).
    pub scope_marks: Vec<(String, Option<DebugScope>)>,
}

/// Emits the DWARF 4 compile unit: one subprogram per function and the
/// line table the assembler builds from the `.loc` directives. Relative
/// file names are taken relative to the directory of the compilation,
/// which, as for C compilers, is the current directory.
///
/// The module's debug types, variables and globals become DIEs too.
/// `frames[i]` has the offsets of function `i`'s stack slots: each
/// subprogram's frame base is DWARF register `frame_reg`, and its variables are at fixed
/// offsets from it (`DW_OP_fbreg`).
pub(crate) fn emit_debug_info<T: Sink>(
    out: &mut T,
    format: Format,
    frame_reg: u8,
    module: &Module,
    frames: &[FrameDebug],
) {
    let l = format.local();
    let elf = format == Format::Elf;
    let split_code = module
        .functions
        .iter()
        .any(|f| f.attrs.section.is_some() || (elf && f.attrs.retain));

    out.debug_section(format, "abbrev");

    if elf {
        out.label(format!("{}debug_abbrev", l));
    }
    for (code, (tag, children, attrs)) in ABBREVS.iter().enumerate() {
        out.number(1, code + 1);
        out.number(1, tag);
        out.number(1, u8::from(*children));

        for (attr, form) in *attrs {
            out.number(1, attr);
            out.number(1, form);
        }

        out.number(1, 0);
        out.number(1, 0);
    }

    out.number(1, 0);

    let comp_dir = std::env::current_dir().map_or(String::new(), |d| d.display().to_string());
    let first = &module.functions[0].name;
    let last = &module.functions[module.functions.len() - 1].name;

    out.debug_section(format, "info");

    // Type references (`DW_FORM_ref4`) are offsets from here, the start
    // of the unit.

    out.label(format!("{}debug_cu", l));

    out.integer(
        4,
        Expression::difference(
            format!("{}debug_info_end", l),
            format!("{}debug_info_start", l),
        ),
    );

    out.label(format!("{}debug_info_start", l));

    // DWARF 4, the abbreviations' offset in their section, 8-byte
    // addresses; then the compile unit. Mach-O debug info stays in the
    // object files, where each section starts at 0. An ELF linker
    // concatenates each `.debug_*` section of every object, so offsets
    // into them are labels, which it relocates.

    out.number(2, 4);

    if elf {
        out.integer(4, Expression::symbol(format!("{}debug_abbrev", l)));
    } else {
        out.number(4, 0);
    }

    out.number(1, 8);

    let abbrev = match (module.debug_language, split_code) {
        (Some(_), true) => Abbrev::CompileUnitLanguageRanges,
        (None, true) => Abbrev::CompileUnitRanges,
        (Some(_), false) => Abbrev::CompileUnitLanguage,
        (None, false) => Abbrev::CompileUnit,
    };

    out.number(1, abbrev as u8);

    out.cstring("keel");

    if let Some(language) = module.debug_language {
        // DW_LANG_C89, DW_LANG_C99, DW_LANG_C11.
        let code = match language {
            Language::C89 => 0x01,
            Language::C99 => 0x0c,
            Language::C11 => 0x1d,
        };

        out.number(2, code);
    }

    out.cstring(&module.files[0]);

    out.cstring(&comp_dir);

    // The assembler's line table is all of `__debug_line`; on ELF it
    // starts at a label in `.debug_line`, added below.
    if elf {
        out.integer(4, Expression::symbol(format!("{}debug_line", l)));
    } else {
        out.number(4, 0);
    }
    if split_code {
        if elf {
            out.integer(4, Expression::symbol(format!("{l}debug_cu_ranges")));
        } else {
            out.integer(
                4,
                Expression::difference(format!("{l}debug_cu_ranges"), format!("{l}debug_ranges")),
            );
        }
    } else {
        // The unit's size spans several functions, which `.subsections_via_
        // symbols` makes separate atoms: written directly, the difference
        // becomes a relocation pair dsymutil does not support ("unsupported
        // relocation in __debug_info"). Through `.set` the assembler computes
        // it as a constant, as clang does.
        out.directive(Directive::Set {
            name: format!("{l}debug_unit_size"),
            expression: Expression::difference(
                format!("{l}{last}_end"),
                format!("{l}{first}_begin"),
            ),
        });
        out.integer(8, Expression::symbol(format!("{l}{first}_begin")));
        out.integer(4, Expression::symbol(format!("{l}debug_unit_size")));
    }
    let die = |out: &mut T, abbrev: Abbrev| {
        out.number(1, abbrev as u8);
    };
    let string = |out: &mut T, s: &str| {
        out.cstring(s);
    };
    let type_ref = |out: &mut T, ty: DebugType| {
        out.integer(
            4,
            Expression::difference(format!("{}dt{}", l, ty.index()), format!("{}debug_cu", l)),
        );
    };
    let udata = |out: &mut T, n: u64| {
        out.leb(false, Expression::constant((n) as i128));
    };
    // Range lists of scopes whose code is not contiguous, for
    // `__debug_ranges`.
    let mut range_lists: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for (func, frame) in module.functions.iter().zip(frames) {
        let name = &func.name;
        let abbrev = match (func.local, func.debug_return.is_some()) {
            (false, false) => Abbrev::Subprogram,
            (true, false) => Abbrev::LocalSubprogram,
            (false, true) => Abbrev::TypedSubprogram,
            (true, true) => Abbrev::LocalTypedSubprogram,
        };
        die(out, abbrev);
        string(out, name);
        if let Some(ty) = func.debug_return {
            type_ref(out, ty);
        }

        out.integer(8, Expression::symbol(format!("{}{}_begin", l, name)));
        out.integer(
            4,
            Expression::difference(format!("{}{}_end", l, name), format!("{}{}_begin", l, name)),
        );

        // The frame base: an expression of one byte, DW_OP_reg<n>.

        out.number(1, 1);
        out.number(1, 0x50 + frame.frame_register.unwrap_or(frame_reg));

        let ranges = scope_ranges(format, func, &frame.scope_marks);
        let var = |out: &mut T, var: &keel_ir::DebugVar| {
            die(
                out,
                if var.param {
                    Abbrev::Param
                } else {
                    Abbrev::Variable
                },
            );
            string(out, &var.name);
            type_ref(out, var.ty);
            // DW_OP_fbreg and the offset.
            let offset = sleb128(frame.slots[var.slot.index()]);
            let mut bytes = vec![(offset.len() + 1 + usize::from(var.indirect)) as u8, 0x91];
            bytes.extend(offset);
            if var.indirect {
                bytes.push(0x06);
            }
            out.bytes(&bytes);
        };
        // Parameters first, as debuggers list them in order; then the
        // scopes, each with its variables and the scopes in it.
        let in_scope = |scope: Option<DebugScope>| {
            let vars = func.debug_vars.iter().filter(move |v| v.scope == scope);
            vars.clone()
                .filter(|v| v.param)
                .chain(vars.filter(|v| !v.param))
        };
        for v in in_scope(None) {
            var(out, v);
        }
        // A stack of (scope, whether its children are next), depth first.
        let children = |parent: Option<DebugScope>| {
            (0..func.debug_scopes.len())
                .rev()
                .map(DebugScope::new)
                .filter(move |s| func.debug_scopes[s.index()] == parent)
        };
        let mut stack: Vec<(DebugScope, bool)> = children(None).map(|s| (s, false)).collect();
        while let Some((scope, opened)) = stack.pop() {
            if opened {
                out.number(1, 0);

                continue;
            }
            // A scope without code cannot be stopped in.
            let scope_ranges = &ranges[scope.index()];
            match scope_ranges.as_slice() {
                [] => continue,
                [(begin, end)] => {
                    die(out, Abbrev::LexicalBlock);

                    out.integer(8, Expression::symbol(format!("{}", begin)));
                    out.integer(
                        4,
                        Expression::difference(format!("{}", end), format!("{}", begin)),
                    );
                }
                _ => {
                    die(out, Abbrev::LexicalBlockRanges);
                    let list = format!("{l}debug_ranges{}", range_lists.len());
                    if elf {
                        out.integer(4, Expression::symbol(format!("{}", list)));
                    } else {
                        out.integer(
                            4,
                            Expression::difference(
                                format!("{}", list),
                                format!("{}debug_ranges", l),
                            ),
                        );
                    }
                    range_lists.push((format!("{l}{name}_begin"), scope_ranges.clone()));
                }
            }
            for v in in_scope(Some(scope)) {
                var(out, v);
            }
            stack.push((scope, true));
            stack.extend(children(Some(scope)).map(|s| (s, false)));
        }

        out.number(1, 0);
    }
    for global in &module.debug_globals {
        let Some(data) = module.data_object(&global.symbol) else {
            continue;
        };
        die(
            out,
            if data.local {
                Abbrev::LocalGlobal
            } else {
                Abbrev::Global
            },
        );
        string(out, &global.name);
        type_ref(out, global.ty);
        if data.thread_local && elf && frame_reg == 29 {
            // AAELF64 has no static DTPREL64 data relocation. Preserve the
            // DIE with an unavailable location, rather than emit a dynamic
            // loader relocation into .debug_info (GNU ld rejects it).
            out.number(1, 0);
        } else if data.thread_local && elf {
            // The variable's offset in its module's TLS block
            // (DW_OP_const8u), which DW_OP_GNU_push_tls_address turns into
            // this thread's copy, as GCC writes it.
            out.number(1, 10);
            out.number(1, 0x0e);
            out.directive(crate::metadata::Directive::X86Integer {
                width: 8,
                address: keel_asm_ir::x86::Address {
                    expression: Expression::symbol(&global.symbol),
                    modifier: keel_asm_ir::x86::Modifier::DtpOff,
                },
            });
            out.number(1, 0xe0);
        } else if data.thread_local {
            // The address of the thread-local's descriptor
            // (DW_OP_const8u), which DW_OP_form_tls_address turns into
            // this thread's copy, as clang writes it.

            out.number(1, 10);
            out.number(1, 0x0e);
            out.integer(
                8,
                Expression::symbol(format!("{}", format.symbol(&global.symbol))),
            );
            out.number(1, 0x9b);
        } else {
            // Nine bytes: DW_OP_addr and the address.

            out.number(1, 9);
            out.number(1, 0x03);
            out.integer(
                8,
                Expression::symbol(format!("{}", format.symbol(&global.symbol))),
            );
        }
    }
    for (i, ty) in module.debug_types.iter().enumerate() {
        out.label(format!("{}dt{}", l, i));

        match ty {
            DebugTypeData::Base {
                name,
                encoding,
                size,
            } => {
                die(out, Abbrev::BaseType);
                string(out, name);
                let ate = match encoding {
                    Encoding::Bool => 0x02,
                    Encoding::Float => 0x04,
                    Encoding::Signed => 0x05,
                    Encoding::SignedChar => 0x06,
                    Encoding::Unsigned => 0x07,
                    Encoding::UnsignedChar => 0x08,
                };

                out.number(1, ate);

                udata(out, u64::from(*size));
            }
            DebugTypeData::Pointer(Some(pointee)) => {
                die(out, Abbrev::Pointer);
                type_ref(out, *pointee);

                out.number(1, 8);
            }
            DebugTypeData::Pointer(None) => {
                die(out, Abbrev::VoidPointer);

                out.number(1, 8);
            }
            DebugTypeData::Struct {
                name,
                union,
                size: None,
                ..
            } => {
                die(
                    out,
                    if *union {
                        Abbrev::UnionDecl
                    } else {
                        Abbrev::StructDecl
                    },
                );
                string(out, name.as_deref().unwrap_or(""));
            }
            DebugTypeData::Struct {
                name,
                union,
                size: Some(size),
                members,
            } => {
                let abbrev = match (union, name.is_some()) {
                    (false, true) => Abbrev::Struct,
                    (false, false) => Abbrev::AnonStruct,
                    (true, true) => Abbrev::Union,
                    (true, false) => Abbrev::AnonUnion,
                };
                die(out, abbrev);
                if let Some(name) = name {
                    string(out, name);
                }
                udata(out, *size);
                for m in members {
                    match (&m.name, m.location) {
                        (Some(name), MemberLocation::Byte(offset)) => {
                            die(out, Abbrev::Member);
                            string(out, name);
                            type_ref(out, m.ty);
                            udata(out, offset);
                        }
                        (None, MemberLocation::Byte(offset)) => {
                            die(out, Abbrev::AnonMember);
                            type_ref(out, m.ty);
                            udata(out, offset);
                        }
                        (Some(name), MemberLocation::Bits { offset, size }) => {
                            die(out, Abbrev::BitField);
                            string(out, name);
                            type_ref(out, m.ty);
                            udata(out, u64::from(size));
                            udata(out, offset);
                        }
                        // An unnamed bit-field is padding.
                        (None, MemberLocation::Bits { .. }) => {}
                    }
                }

                out.number(1, 0);
            }
            DebugTypeData::Array { elem, count } => {
                die(out, Abbrev::Array);
                type_ref(out, *elem);
                match count {
                    Some(n) => {
                        die(out, Abbrev::Subrange);
                        udata(out, *n);
                    }
                    None => die(out, Abbrev::UnknownSubrange),
                }

                out.number(1, 0);
            }
            DebugTypeData::Typedef { name, ty } => {
                die(
                    out,
                    if ty.is_some() {
                        Abbrev::Typedef
                    } else {
                        Abbrev::VoidTypedef
                    },
                );
                string(out, name);
                if let Some(ty) = ty {
                    type_ref(out, *ty);
                }
            }
            DebugTypeData::Function => die(out, Abbrev::Function),
            DebugTypeData::Const(inner) | DebugTypeData::Volatile(inner) => {
                let volatile = matches!(ty, DebugTypeData::Volatile(_));
                let abbrev = match (volatile, inner) {
                    (false, Some(_)) => Abbrev::Const,
                    (false, None) => Abbrev::VoidConst,
                    (true, Some(_)) => Abbrev::Volatile,
                    (true, None) => Abbrev::VoidVolatile,
                };
                die(out, abbrev);
                if let Some(inner) = inner {
                    type_ref(out, *inner);
                }
            }
            DebugTypeData::Enum { name, ty, values } => {
                die(
                    out,
                    if name.is_some() {
                        Abbrev::Enum
                    } else {
                        Abbrev::AnonEnum
                    },
                );
                if let Some(name) = name {
                    string(out, name);
                }
                type_ref(out, *ty);
                let DebugTypeData::Base { size, .. } = module.debug_types[ty.index()] else {
                    unreachable!("the verifier checks an enum's type")
                };
                udata(out, u64::from(size));
                for (name, value) in values {
                    die(out, Abbrev::Enumerator);
                    string(out, name);

                    out.leb(true, Expression::constant(i128::from(*value)));
                }

                out.number(1, 0);
            }
        }
    }

    out.number(1, 0);

    out.label(format!("{}debug_info_end", l));

    if !range_lists.is_empty() || split_code {
        // Each list starts from its function (a base address entry, -1
        // and the address), so the entries are differences within one
        // function, which the assembler computes.

        out.debug_section(format, "ranges");
        out.label(format!("{}debug_ranges", l));

        if split_code {
            out.label(format!("{l}debug_cu_ranges"));
            // Each function may occupy a different input section. Base entries
            // avoid cross-section differences and describe only real code.
            for f in &module.functions {
                out.number(8, -1);
                out.integer(8, Expression::symbol(format!("{l}{}_begin", f.name)));
                out.number(8, 0);
                out.integer(
                    8,
                    Expression::difference(
                        format!("{l}{}_end", f.name),
                        format!("{l}{}_begin", f.name),
                    ),
                );
            }
            out.number(8, 0);
            out.number(8, 0);
        }
        for (i, (base, ranges)) in range_lists.iter().enumerate() {
            out.label(format!("{}debug_ranges{}", l, i));
            out.number(8, -1);
            out.integer(8, Expression::symbol(format!("{}", base)));

            for (begin, end) in ranges {
                out.integer(
                    8,
                    Expression::difference(format!("{}", begin), format!("{}", base)),
                );
                out.integer(
                    8,
                    Expression::difference(format!("{}", end), format!("{}", base)),
                );
            }

            out.number(8, 0);
            out.number(8, 0);
        }
    }
    if elf {
        out.debug_section(format, "line");
        out.label(format!("{}debug_line", l));
    }
}

/// The code of each lexical scope of `func`, as (begin, end) labels: the
/// code marked with it, and that of the scopes nested in it.
fn scope_ranges(
    format: Format,
    func: &Function,
    marks: &[(String, Option<DebugScope>)],
) -> Vec<Vec<(String, String)>> {
    let (l, name) = (format.local(), &func.name);
    let mut ranges: Vec<Vec<(String, String)>> = vec![Vec::new(); func.debug_scopes.len()];
    for (i, (begin, scope)) in marks.iter().enumerate() {
        let end = marks
            .get(i + 1)
            .map_or_else(|| format!("{l}{name}_end"), |(label, _)| label.clone());
        let mut scope = *scope;
        while let Some(s) = scope {
            let list = &mut ranges[s.index()];
            match list.last_mut() {
                // Continues the range: the scope was only nested deeper.
                Some(last) if last.1 == *begin => last.1 = end.clone(),
                _ => list.push((begin.clone(), end.clone())),
            }
            scope = func.debug_scopes[s.index()];
        }
    }
    ranges
}

/// The DIE kinds keel writes, numbered from 1 as in [`ABBREVS`].
#[derive(Copy, Clone)]
enum Abbrev {
    CompileUnit = 1,
    Subprogram,
    LocalSubprogram,
    TypedSubprogram,
    LocalTypedSubprogram,
    Param,
    Variable,
    Global,
    LocalGlobal,
    BaseType,
    Pointer,
    VoidPointer,
    Struct,
    AnonStruct,
    Union,
    AnonUnion,
    StructDecl,
    UnionDecl,
    Member,
    AnonMember,
    BitField,
    Array,
    Subrange,
    UnknownSubrange,
    Typedef,
    VoidTypedef,
    Function,
    CompileUnitLanguage,
    LexicalBlock,
    LexicalBlockRanges,
    Const,
    VoidConst,
    Volatile,
    VoidVolatile,
    Enum,
    AnonEnum,
    Enumerator,
    CompileUnitRanges,
    CompileUnitLanguageRanges,
}

// DWARF attributes and forms.
const AT_NAME: u8 = 0x03;
const AT_LOCATION: u8 = 0x02;
const AT_BYTE_SIZE: u8 = 0x0b;
const AT_STMT_LIST: u8 = 0x10;
const AT_LOW_PC: u8 = 0x11;
const AT_HIGH_PC: u8 = 0x12;
const AT_COMP_DIR: u8 = 0x1b;
const AT_LANGUAGE: u8 = 0x13;
const AT_CONST_VALUE: u8 = 0x1c;
const AT_RANGES: u8 = 0x55;
const FORM_DATA2: u8 = 0x05;
const FORM_SDATA: u8 = 0x0d;
const AT_PROTOTYPED: u8 = 0x27;
const AT_PRODUCER: u8 = 0x25;
const AT_COUNT: u8 = 0x37;
const AT_MEMBER_LOCATION: u8 = 0x38;
const AT_DECLARATION: u8 = 0x3c;
const AT_ENCODING: u8 = 0x3e;
const AT_EXTERNAL: u8 = 0x3f;
const AT_FRAME_BASE: u8 = 0x40;
const AT_TYPE: u8 = 0x49;
const AT_BIT_SIZE: u8 = 0x0d;
const AT_DATA_BIT_OFFSET: u8 = 0x6b;
const FORM_ADDR: u8 = 0x01;
const FORM_DATA1: u8 = 0x0b;
const FORM_DATA4: u8 = 0x06;
const FORM_STRING: u8 = 0x08;
const FORM_UDATA: u8 = 0x0f;
const FORM_REF4: u8 = 0x13;
const FORM_SEC_OFFSET: u8 = 0x17;
const FORM_EXPRLOC: u8 = 0x18;
const FORM_FLAG_PRESENT: u8 = 0x19;

type AbbrevDecl = (u8, bool, &'static [(u8, u8)]);

/// The abbreviation table, in [`Abbrev`] order: a DIE's tag, whether it
/// has children, and its attributes with their forms.
const ABBREVS: &[AbbrevDecl] = {
    const NAME: (u8, u8) = (AT_NAME, FORM_STRING);
    const TYPE: (u8, u8) = (AT_TYPE, FORM_REF4);
    const SIZE: (u8, u8) = (AT_BYTE_SIZE, FORM_UDATA);
    const PC: [(u8, u8); 2] = [(AT_LOW_PC, FORM_ADDR), (AT_HIGH_PC, FORM_DATA4)];
    const LOCATION: (u8, u8) = (AT_LOCATION, FORM_EXPRLOC);
    const EXTERNAL: (u8, u8) = (AT_EXTERNAL, FORM_FLAG_PRESENT);
    const DECLARATION: (u8, u8) = (AT_DECLARATION, FORM_FLAG_PRESENT);
    const MEMBER_LOCATION: (u8, u8) = (AT_MEMBER_LOCATION, FORM_UDATA);
    const FRAME_BASE: (u8, u8) = (AT_FRAME_BASE, FORM_EXPRLOC);
    const PROTOTYPED: (u8, u8) = (AT_PROTOTYPED, FORM_FLAG_PRESENT);
    &[
        (
            0x11, // DW_TAG_compile_unit
            true,
            &[
                (AT_PRODUCER, FORM_STRING),
                NAME,
                (AT_COMP_DIR, FORM_STRING),
                (AT_STMT_LIST, FORM_SEC_OFFSET),
                PC[0],
                PC[1],
            ],
        ),
        // DW_TAG_subprogram: exported or not, with a return type or not.
        // Prototyped, so a debugger calling one passes arguments as typed.
        (
            0x2e,
            true,
            &[NAME, PC[0], PC[1], FRAME_BASE, PROTOTYPED, EXTERNAL],
        ),
        (0x2e, true, &[NAME, PC[0], PC[1], FRAME_BASE, PROTOTYPED]),
        (
            0x2e,
            true,
            &[NAME, TYPE, PC[0], PC[1], FRAME_BASE, PROTOTYPED, EXTERNAL],
        ),
        (
            0x2e,
            true,
            &[NAME, TYPE, PC[0], PC[1], FRAME_BASE, PROTOTYPED],
        ),
        (0x05, false, &[NAME, TYPE, LOCATION]), // DW_TAG_formal_parameter
        (0x34, false, &[NAME, TYPE, LOCATION]), // DW_TAG_variable
        (0x34, false, &[NAME, TYPE, EXTERNAL, LOCATION]),
        (0x34, false, &[NAME, TYPE, LOCATION]),
        (0x24, false, &[NAME, (AT_ENCODING, FORM_DATA1), SIZE]), // DW_TAG_base_type
        (0x0f, false, &[TYPE, (AT_BYTE_SIZE, FORM_DATA1)]),      // DW_TAG_pointer_type
        (0x0f, false, &[(AT_BYTE_SIZE, FORM_DATA1)]),
        (0x13, true, &[NAME, SIZE]), // DW_TAG_structure_type
        (0x13, true, &[SIZE]),
        (0x17, true, &[NAME, SIZE]), // DW_TAG_union_type
        (0x17, true, &[SIZE]),
        (0x13, false, &[NAME, DECLARATION]),
        (0x17, false, &[NAME, DECLARATION]),
        (0x0d, false, &[NAME, TYPE, MEMBER_LOCATION]), // DW_TAG_member
        (0x0d, false, &[TYPE, MEMBER_LOCATION]),
        (
            0x0d,
            false,
            &[
                NAME,
                TYPE,
                (AT_BIT_SIZE, FORM_UDATA),
                (AT_DATA_BIT_OFFSET, FORM_UDATA),
            ],
        ),
        (0x01, true, &[TYPE]),                    // DW_TAG_array_type
        (0x21, false, &[(AT_COUNT, FORM_UDATA)]), // DW_TAG_subrange_type
        (0x21, false, &[]),
        (0x16, false, &[NAME, TYPE]), // DW_TAG_typedef
        (0x16, false, &[NAME]),
        (0x15, false, &[]), // DW_TAG_subroutine_type
        (
            0x11,
            true,
            &[
                (AT_PRODUCER, FORM_STRING),
                (AT_LANGUAGE, FORM_DATA2),
                NAME,
                (AT_COMP_DIR, FORM_STRING),
                (AT_STMT_LIST, FORM_SEC_OFFSET),
                PC[0],
                PC[1],
            ],
        ),
        (0x0b, true, &[PC[0], PC[1]]), // DW_TAG_lexical_block
        (0x0b, true, &[(AT_RANGES, FORM_SEC_OFFSET)]),
        (0x26, false, &[TYPE]), // DW_TAG_const_type
        (0x26, false, &[]),
        (0x35, false, &[TYPE]), // DW_TAG_volatile_type
        (0x35, false, &[]),
        (0x04, true, &[NAME, TYPE, SIZE]), // DW_TAG_enumeration_type
        (0x04, true, &[TYPE, SIZE]),
        (0x28, false, &[NAME, (AT_CONST_VALUE, FORM_SDATA)]), // DW_TAG_enumerator
        (
            0x11,
            true,
            &[
                (AT_PRODUCER, FORM_STRING),
                NAME,
                (AT_COMP_DIR, FORM_STRING),
                (AT_STMT_LIST, FORM_SEC_OFFSET),
                (AT_RANGES, FORM_SEC_OFFSET),
            ],
        ),
        (
            0x11,
            true,
            &[
                (AT_PRODUCER, FORM_STRING),
                (AT_LANGUAGE, FORM_DATA2),
                NAME,
                (AT_COMP_DIR, FORM_STRING),
                (AT_STMT_LIST, FORM_SEC_OFFSET),
                (AT_RANGES, FORM_SEC_OFFSET),
            ],
        ),
    ]
};

/// `n` in the signed LEB128 encoding: seven bits a byte, low first, the
/// top bit set on all but the last.
fn sleb128(mut n: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        let done = (n == 0 && byte & 0x40 == 0) || (n == -1 && byte & 0x40 != 0);
        bytes.push(if done { byte } else { byte | 0x80 });
        if done {
            return bytes;
        }
    }
}
