//! Terminal long-mode interrupt return from an explicitly owned GPR image.
//!
//! Link [`program`] once and import [`declaration`]. Call it using the ordinary
//! SysV AMD64 C convention (RDI) with a pointer to [`Image`], then terminate the IR block with
//! `trap`. It never returns through the compiled caller's epilogue. Do not mark
//! the call pure, readonly, `cc(fast)`, or returns-twice.
//!
//! The caller must already execute at CPL0 in 64-bit mode, with NT clear and
//! CET shadow stacks disabled. The target must be valid 64-bit code at the
//! same or a less privileged level. Selectors, mappings, canonical RIP/RSP,
//! legal flags and all other architectural IRETQ preconditions are the caller's
//! responsibility. No frame validation or flags sanitization occurs.
//!
//! The image remains exclusively owned and mapped through the transfer. The
//! final three instructions install RSP at its RIP word, restore R10, and IRETQ.
//! The adapter reads its 160 bytes and emits no stores. Event/fault delivery
//! can nevertheless use this stack: the caller must provide writable backing,
//! headroom and valid handlers, or independent IST stacks. Disable maskable
//! interrupts before calling; NMI, debug and fault policy remains caller-owned.
//! IRETQ also has its architectural NMI-unblocking effect, including on faults.
//!
//! This restores GPRs and the architectural return frame only. It does not
//! restore SIMD/x87 state, FS/GS bases, mappings, debug registers or arbitrary
//! segment caches. It provides no entry shim, exception recovery or unwind ABI.

use super::{Directive, Expression, Opcode, Output, Sink, asm_arg, memory, r64, sized};
use keel_asm_ir::Program;
use keel_ir::{FuncDecl, FunctionAttrs, Signature, Type};
use std::fmt::Write;

pub const SYMBOL: &str = "__keel_x86_iretq";
/// Byte offset at which the processor's five-word IRETQ frame begins.
pub const FRAME_OFFSET: usize = 120;
pub const IMAGE_SIZE: usize = 160;

/// Twenty little-endian machine words, aligned to eight bytes.
///
/// GPR words occupy offsets 0..120 in the order shown. The final five words
/// are RIP, CS, RFLAGS, RSP, SS, including for a same-CPL 64-bit return. Selector
/// words must contain valid zero-extended selectors; unused high bits are zero.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Image {
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rbx: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

const _: () = assert!(std::mem::size_of::<Image>() == IMAGE_SIZE);
const _: () = assert!(std::mem::offset_of!(Image, rip) == FRAME_OFFSET);

/// SysV AMD64 C transport taking one image pointer and returning no values.
pub fn signature() -> Signature {
    Signature::new(&[Type::Ptr], &[])
}

/// Import contract. A call must still end its IR path explicitly, e.g. `trap`.
pub fn declaration() -> FuncDecl {
    FuncDecl {
        name: SYMBOL.into(),
        sig: signature(),
        attrs: FunctionAttrs {
            noreturn: true,
            ..FunctionAttrs::default()
        },
    }
}

pub fn program() -> Program {
    match emit(true) {
        Output::Structured(program) => program,
        _ => unreachable!(),
    }
}

/// GNU-compatible Intel syntax for exactly the same transport.
pub fn assembly() -> String {
    match emit(false) {
        Output::Text(text, _) => text.into_string(),
        _ => unreachable!(),
    }
}

fn emit(structured: bool) -> Output {
    let mut out = Output::new(structured);
    if !structured {
        out.write_str(".intel_syntax noprefix\n").unwrap();
    }
    out.directive(Directive::ElfSection {
        name: ".text".into(),
        flags: 6,
        nobits: false,
    });
    out.align(4);
    out.directive(Directive::Global(SYMBOL.into()));
    out.directive(Directive::ElfHidden(SYMBOL.into()));
    out.directive(Directive::ElfType {
        name: SYMBOL.into(),
        function: true,
    });
    out.label(SYMBOL.into());
    out.instruction(Opcode::Mov, [asm_arg(r64(10)), asm_arg(r64(7))], false);
    for (word, register) in [0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]
        .into_iter()
        .enumerate()
    {
        if register != 10 {
            out.instruction(
                Opcode::Mov,
                [
                    asm_arg(r64(register)),
                    asm_arg(sized(memory(r64(10), word as i128 * 8), 64)),
                ],
                false,
            );
        }
    }
    out.instruction(
        Opcode::Lea,
        [
            asm_arg(r64(4)),
            asm_arg(memory(r64(10), FRAME_OFFSET as i128)),
        ],
        false,
    );
    out.instruction(
        Opcode::Mov,
        [asm_arg(r64(10)), asm_arg(sized(memory(r64(10), 72), 64))],
        false,
    );
    out.instruction(Opcode::parse("iretq").unwrap(), [], false);
    out.directive(Directive::Size {
        name: SYMBOL.into(),
        expression: Expression::difference(".", SYMBOL),
    });
    out.directive(Directive::ElfSection {
        name: ".note.GNU-stack".into(),
        flags: 0,
        nobits: false,
    });
    out
}
