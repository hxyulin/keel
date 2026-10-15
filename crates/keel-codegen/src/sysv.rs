//! The System V x86-64 C calling convention (the psABI used by Linux):
//! where each argument and result goes. The code generator turns these
//! placements into moves.
//!
//! Scalars:
//!
//! - Integers and pointers go in rdi, rsi, rdx, rcx, r8, r9, floats in
//!   xmm0–xmm7, each class counted on its own. An `i128` takes two
//!   general registers, or 16 aligned bytes of stack. A 128-bit vector uses
//!   one XMM register or a 16-byte stack slot aligned to 16.
//! - Once a class runs out, its arguments go on the stack in order, each in
//!   its own 8-byte slot: unlike Apple arm64, the stack is never packed.
//! - Variadic arguments are placed exactly like named ones, in registers
//!   first. The caller passes an upper bound on the number of vector
//!   registers used in `al`, so the callee knows whether to save them.
//!
//! Structs (`byval`), by [`classify`]: a struct of up to 16 bytes is split
//! into 8-byte halves ("eightbytes"), each classed INTEGER if anything in it
//! is an integer and SSE if it holds only floats. Each half takes the next
//! register of its class: `{double, long}` goes in xmm0 and rdi. If the
//! registers left cannot take every half, the whole struct goes on the
//! stack, and later arguments may still use the registers. A bigger struct
//! (class MEMORY) is copied onto the stack, not passed by address as on
//! arm64.
//!
//! Struct results come back the same way, the halves in rax, rdx and
//! xmm0, xmm1; a MEMORY result is written through an address the caller
//! passes in rdi, which the callee also returns in rax.

use crate::regalloc::Loc;
use keel_ir::{Layout, Purpose, Type};

pub const RAX: u8 = 0;
pub const RCX: u8 = 1;
pub const RDX: u8 = 2;
pub const RSI: u8 = 6;
pub const RDI: u8 = 7;

/// The general-purpose argument registers, in order.
pub const INT_ARGS: [u8; 6] = [RDI, RSI, RDX, RCX, 8, 9];

/// The class of one eightbyte of a struct.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Class {
    Integer,
    Sse,
}

/// The classes of a struct's eightbytes, or `None` for MEMORY: bigger than
/// 16 bytes, or empty.
pub fn classify(layout: &Layout) -> Option<Vec<Class>> {
    let size = layout.size();
    if size == 0 || size > 16 {
        return None;
    }
    let mut classes = vec![Class::Sse; size.div_ceil(8) as usize];
    for (offset, ty) in layout.leaves() {
        // Fields are naturally aligned, so none straddles two halves.
        if !ty.is_float() {
            classes[(offset / 8) as usize] = Class::Integer;
        }
    }
    Some(classes)
}

/// Where one scalar is. An `i128` in registers is in this one (its low
/// half) and [`Slot::Regs`]'s second.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    /// One register, or two for an `i128`.
    Regs(Vec<Loc>),
    /// At this offset from the caller's `rsp` at the call.
    Stack(u32),
}

/// Where one argument goes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ArgLoc {
    /// A scalar.
    Value(Slot),
    /// A `byval` struct in registers, one per eightbyte.
    InRegs(Vec<Loc>),
    /// A `byval` struct copied onto the stack at this offset.
    OnStack(u32),
    /// An `sret` result that comes back in these registers; nothing is
    /// passed for it.
    SretRegs(Vec<Loc>),
    /// An `sret` result written to memory at the address passed in rdi.
    SretMemory,
}

/// An argument as the convention sees it.
#[derive(Copy, Clone, Debug)]
pub enum ArgKind<'a> {
    Value(Type),
    ByVal(&'a Layout),
    Sret(&'a Layout),
}

impl<'a> ArgKind<'a> {
    pub fn of(ty: Type, purpose: &'a Purpose) -> Self {
        match purpose {
            Purpose::Normal => ArgKind::Value(ty),
            Purpose::ByVal(layout) => ArgKind::ByVal(layout),
            Purpose::Sret(layout) => ArgKind::Sret(layout),
        }
    }
}

/// The registers a struct result of these classes comes back in: rax then
/// rdx for INTEGER halves, xmm0 then xmm1 for SSE ones.
fn result_regs(classes: &[Class]) -> Vec<Loc> {
    let (mut ints, mut floats) = ([RAX, RDX].into_iter(), 0..);
    classes
        .iter()
        .map(|class| match class {
            Class::Integer => Loc::Reg(ints.next().unwrap()),
            Class::Sse => Loc::FReg(floats.next().unwrap()),
        })
        .collect()
}

/// Registers and stack used so far while placing arguments.
#[derive(Default)]
struct Cursor {
    /// Index of the next free register in [`INT_ARGS`] and among
    /// xmm0–xmm7.
    ints: usize,
    floats: u8,
    /// Bytes of stack arguments so far.
    stack: u32,
}

impl Cursor {
    /// The next stack slot for `size` bytes, rounded up to 8.
    fn stack(&mut self, size: u64, align: u64) -> u32 {
        let offset = self.stack.next_multiple_of(align.max(8) as u32);
        self.stack = offset + (size.next_multiple_of(8) as u32);
        offset
    }

    /// Registers for these eightbyte classes, if all fit.
    fn regs(&mut self, classes: &[Class]) -> Option<Vec<Loc>> {
        let ints = classes.iter().filter(|&&c| c == Class::Integer).count();
        let floats = classes.len() - ints;
        if self.ints + ints > INT_ARGS.len() || usize::from(self.floats) + floats > 8 {
            return None;
        }
        Some(
            classes
                .iter()
                .map(|class| match class {
                    Class::Integer => {
                        self.ints += 1;
                        Loc::Reg(INT_ARGS[self.ints - 1])
                    }
                    Class::Sse => {
                        self.floats += 1;
                        Loc::FReg(self.floats - 1)
                    }
                })
                .collect(),
        )
    }

    fn scalar(&mut self, ty: Type) -> Slot {
        let classes: &[Class] = match ty {
            Type::I128 => &[Class::Integer, Class::Integer],
            _ if ty.is_float() || ty.is_vector() => &[Class::Sse],
            _ => &[Class::Integer],
        };
        match self.regs(classes) {
            Some(regs) => Slot::Regs(regs),
            None => {
                let size = u64::from(ty.bytes().unwrap_or(8));
                Slot::Stack(self.stack(size, size))
            }
        }
    }

    fn by_val(&mut self, layout: &Layout) -> ArgLoc {
        match classify(layout).and_then(|classes| self.regs(&classes)) {
            Some(regs) => ArgLoc::InRegs(regs),
            None => ArgLoc::OnStack(self.stack(layout.size(), layout.align())),
        }
    }
}

/// Where a call's arguments go.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Placement {
    /// Each argument's location.
    pub locs: Vec<ArgLoc>,
    /// Bytes of stack arguments.
    pub stack: u32,
    /// General-purpose and vector argument registers used: where a
    /// variadic function's `va_start` begins, and `al` for its callers.
    pub ints: u8,
    pub floats: u8,
}

/// Places arguments `args`.
pub fn arg_locs(args: &[ArgKind]) -> Placement {
    let mut cursor = Cursor::default();
    let locs = args
        .iter()
        .map(|arg| match *arg {
            ArgKind::Value(ty) => ArgLoc::Value(cursor.scalar(ty)),
            ArgKind::ByVal(layout) => cursor.by_val(layout),
            ArgKind::Sret(layout) => match classify(layout) {
                Some(classes) => ArgLoc::SretRegs(result_regs(&classes)),
                // The address is the first integer argument.
                None => {
                    cursor.ints += 1;
                    ArgLoc::SretMemory
                }
            },
        })
        .collect();
    Placement {
        locs,
        stack: cursor.stack,
        ints: cursor.ints as u8,
        floats: cursor.floats,
    }
}

/// Where a scalar result comes back: rax (and rdx for the high half of
/// an `i128`), or xmm0.
pub fn return_loc(ty: Type) -> Loc {
    if ty.is_float() || ty.is_vector() {
        Loc::FReg(0)
    } else {
        Loc::Reg(RAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Layout::Scalar as t;

    fn s(fields: Vec<Layout>) -> Layout {
        Layout::Struct(fields)
    }

    #[test]
    fn classes() {
        use Class::*;
        let dl = s(vec![t(Type::F64), t(Type::I64)]);
        assert_eq!(classify(&dl), Some(vec![Sse, Integer]));
        // Two floats share an eightbyte, which stays SSE.
        let ff = s(vec![t(Type::F32), t(Type::F32), t(Type::F64)]);
        assert_eq!(classify(&ff), Some(vec![Sse, Sse]));
        // One integer makes its eightbyte INTEGER.
        let fi = s(vec![t(Type::F32), t(Type::I32)]);
        assert_eq!(classify(&fi), Some(vec![Integer]));
        let big = s(vec![t(Type::I64), t(Type::I64), t(Type::I64)]);
        assert_eq!(classify(&big), None);
    }

    #[test]
    fn registers_then_stack() {
        let mut args: Vec<ArgKind> = (0..7).map(|_| ArgKind::Value(Type::I64)).collect();
        args.push(ArgKind::Value(Type::F64));
        args.push(ArgKind::Value(Type::I8));
        let Placement {
            locs,
            stack,
            floats,
            ..
        } = arg_locs(&args);
        assert_eq!(locs[0], ArgLoc::Value(Slot::Regs(vec![Loc::Reg(RDI)])));
        assert_eq!(locs[5], ArgLoc::Value(Slot::Regs(vec![Loc::Reg(9)])));
        assert_eq!(locs[6], ArgLoc::Value(Slot::Stack(0)));
        assert_eq!(locs[7], ArgLoc::Value(Slot::Regs(vec![Loc::FReg(0)])));
        // Every stack argument takes 8 bytes.
        assert_eq!(locs[8], ArgLoc::Value(Slot::Stack(8)));
        assert_eq!((stack, floats), (16, 1));
    }

    /// A struct whose halves do not all fit goes on the stack, and the
    /// registers stay open for the arguments after it.
    #[test]
    fn struct_that_does_not_fit() {
        let ll = s(vec![t(Type::I64), t(Type::I64)]);
        let mut args: Vec<ArgKind> = (0..5).map(|_| ArgKind::Value(Type::I64)).collect();
        args.push(ArgKind::ByVal(&ll));
        args.push(ArgKind::Value(Type::I32));
        let Placement { locs, stack, .. } = arg_locs(&args);
        assert_eq!(locs[5], ArgLoc::OnStack(0));
        assert_eq!(locs[6], ArgLoc::Value(Slot::Regs(vec![Loc::Reg(9)])));
        assert_eq!(stack, 16);
    }

    #[test]
    fn sret() {
        let big = s(vec![t(Type::I64), t(Type::I64), t(Type::I64)]);
        let dl = s(vec![t(Type::F64), t(Type::I64)]);
        let args = [ArgKind::Sret(&big), ArgKind::Value(Type::I64)];
        let locs = arg_locs(&args).locs;
        assert_eq!(
            locs,
            [
                ArgLoc::SretMemory,
                ArgLoc::Value(Slot::Regs(vec![Loc::Reg(RSI)]))
            ]
        );
        let locs = arg_locs(&[ArgKind::Sret(&dl)]).locs;
        assert_eq!(locs, [ArgLoc::SretRegs(vec![Loc::FReg(0), Loc::Reg(RAX)])]);
    }
}
