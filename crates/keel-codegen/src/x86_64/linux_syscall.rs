//! Linux x86-64 raw syscall transport through an ordinary imported C function.
//!
//! Link this adapter once and declare [`SYMBOL`] with [`signature`]. Ordinary
//! calls retain their normal memory effects and caller-save clobbers; do not
//! mark the declaration pure, readonly, or `cc(fast)`. The return is the raw
//! kernel RAX value, including negative errno values; TLS errno is untouched.
//!
//! This is the native Linux 64-bit ABI, not x32 or another operating system.
//! A returning call must retain the original user stack and return normally.
//! Syscalls that replace control flow or create a child on a different stack
//! need a dedicated entry protocol instead of returning through this adapter.
//!
//! Register contract: Linux `arch/x86/entry/entry_64.S` and the x86-64 psABI
//! appendix A.2.1. RCX/R11 and flags are caller-clobbered; callee saves and
//! stack alignment remain unchanged. No x87/SIMD instructions are emitted.

use super::{Cfi, Directive, Expression, Opcode, Output, Sink, asm_arg, memory, r64, sized};
use keel_asm_ir::Program;
use keel_ir::{Signature, Type};
use std::fmt::Write;

/// C signature: `long __keel_linux_syscall6(long number, long a1, ..., long a6)`.
pub const SYMBOL: &str = "__keel_linux_syscall6";

/// An ordinary C call, with six machine-word arguments after the syscall number.
pub fn signature() -> Signature {
    Signature::new(&[Type::I64; 7], &[Type::I64])
}

/// A linkable ELF adapter expressed entirely as typed assembly.
pub fn program() -> Program {
    match emit(true) {
        Output::Structured(program) => program,
        _ => unreachable!(),
    }
}

/// The same adapter as GNU-compatible Intel assembly, including unwind metadata.
pub fn assembly() -> String {
    match emit(false) {
        Output::Text(text) => text.into_string(),
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
    out.directive(Directive::Cfi(Cfi::Start));
    out.directive(Directive::Cfi(Cfi::DefCfa {
        register: 7,
        offset: 8,
    }));
    out.directive(Directive::Cfi(Cfi::Offset {
        register: 16,
        offset: -8,
    }));
    // Copy left to right: every source is consumed before its register is
    // overwritten. C's seventh argument is immediately above the return PC.
    for (dst, src) in [(0, 7), (7, 6), (6, 2), (2, 1), (10, 8), (8, 9)] {
        out.instruction(Opcode::Mov, [asm_arg(r64(dst)), asm_arg(r64(src))], false);
    }
    out.instruction(
        Opcode::Mov,
        [asm_arg(r64(9)), asm_arg(sized(memory(r64(4), 8), 64))],
        false,
    );
    out.instruction(Opcode::Syscall, [], false);
    out.instruction(Opcode::Ret, [], false);
    out.directive(Directive::Cfi(Cfi::End));
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
