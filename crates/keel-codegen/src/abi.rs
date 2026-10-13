//! Apple arm64's C calling convention: where each argument and result goes.
//! The code generator turns these placements into moves.
//!
//! Scalars:
//!
//! - Named integers and pointers go in x0–x7, floats in v0–v7, each class
//!   counted on its own: `f(i64, f64, i64)` uses x0, d0 and x1.
//! - Once a class runs out, its arguments go on the stack in order, each at
//!   its natural size and alignment: Apple packs them, where standard AAPCS64
//!   gives each its own 8-byte slot.
//! - Variadic arguments always go on the stack, each in its own 8-byte slot
//!   (Apple's `va_list` is a pointer stepping 8 bytes at a time).
//!
//! Structs (`byval`), by [`classify`]:
//!
//! - A homogeneous float aggregate (one to four floats of one type, however
//!   nested) goes one member per FP register, if enough are left. Otherwise
//!   it goes on the stack at its natural size and alignment, and the FP
//!   registers are closed to later arguments.
//! - Any other struct of up to 16 bytes goes in one or two general-purpose
//!   registers, 8 bytes each, if enough are left; otherwise on the stack,
//!   rounded up to 8 bytes and 8-aligned, closing the general registers.
//! - A bigger struct is copied by the caller, and its address is passed
//!   like a pointer argument.
//!
//! Struct results (`sret`) come back the same way: an aggregate of floats
//! in v0–v3, a small struct in x0 and x1, and anything bigger through
//! memory whose address the caller passes in x8.

use crate::regalloc::Loc;
use keel_ir::{Layout, Purpose, Type};

/// How a struct crosses a call.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StructClass {
    /// `count` floats of type `ty`, one per FP register.
    Hfa { ty: Type, count: u32 },
    /// Up to 16 bytes in `count` general-purpose registers.
    Gprs { count: u32 },
    /// By address.
    Indirect,
}

pub fn classify(layout: &Layout) -> StructClass {
    let leaves = layout.leaves();
    if let Some(&(_, ty)) = leaves.first()
        && ty.is_float()
        && leaves.len() <= 4
        && leaves.iter().all(|&(_, t)| t == ty)
    {
        return StructClass::Hfa {
            ty,
            count: leaves.len() as u32,
        };
    }
    match layout.size() {
        size @ 1..=16 => StructClass::Gprs {
            count: size.div_ceil(8) as u32,
        },
        _ => StructClass::Indirect,
    }
}

/// Where one scalar, or one struct's address, is. An `i128` in registers
/// is in this one (its low half) and the next.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    /// x0–x7 or v0–v7.
    Reg(Loc),
    /// At this offset from the caller's `sp` at the call.
    Stack(u32),
}

/// Where one argument goes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ArgLoc {
    /// A scalar.
    Value(Slot),
    /// A `byval` struct in registers: general registers holding 8 bytes
    /// each (the last maybe fewer), or FP registers holding one member each.
    InRegs(Vec<Loc>),
    /// A `byval` struct copied onto the stack at this offset.
    OnStack(u32),
    /// A `byval` struct too big for registers: the address of a copy.
    Copy(Slot),
    /// An `sret` result that comes back in these registers; nothing is
    /// passed for it.
    SretRegs(Vec<Loc>),
    /// An `sret` result written to memory at the address passed in x8.
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

/// The registers a struct of class `class` occupies, starting at x0/v0:
/// where an `sret` result comes back.
fn result_regs(class: StructClass) -> Option<Vec<Loc>> {
    match class {
        StructClass::Hfa { count, .. } => Some((0..count as u8).map(Loc::FReg).collect()),
        StructClass::Gprs { count } => Some((0..count as u8).map(Loc::Reg).collect()),
        StructClass::Indirect => None,
    }
}

/// Registers and stack used so far while placing arguments.
#[derive(Default)]
struct Cursor {
    /// Next general-purpose and FP argument register; 8 means none left.
    ints: u8,
    floats: u8,
    /// Bytes of stack arguments so far.
    stack: u32,
}

impl Cursor {
    fn stack(&mut self, size: u32, align: u32) -> u32 {
        let offset = self.stack.next_multiple_of(align);
        self.stack = offset + size;
        offset
    }

    fn scalar(&mut self, ty: Type) -> Slot {
        // An i128 takes the next two general registers (Apple does not
        // round up to an even one, as AAPCS64 does), or 16 aligned bytes
        // of stack, closing the general registers.
        if ty == Type::I128 {
            return match Self::regs(&mut self.ints, 2, Loc::Reg) {
                Some(regs) => Slot::Reg(regs[0]),
                None => Slot::Stack(self.stack(16, 16)),
            };
        }
        let (next, class): (_, fn(u8) -> Loc) = if ty.is_float() {
            (&mut self.floats, Loc::FReg)
        } else {
            (&mut self.ints, Loc::Reg)
        };
        if *next < 8 {
            *next += 1;
            return Slot::Reg(class(*next - 1));
        }
        let size = ty.bytes().unwrap_or(8);
        Slot::Stack(self.stack(size, size))
    }

    /// `count` consecutive registers of a class starting at `*next`, or
    /// `None` (closing the class) if they do not all fit.
    fn regs(next: &mut u8, count: u32, class: fn(u8) -> Loc) -> Option<Vec<Loc>> {
        if u32::from(*next) + count > 8 {
            *next = 8;
            return None;
        }
        let regs = (*next..*next + count as u8).map(class).collect();
        *next += count as u8;
        Some(regs)
    }

    fn by_val(&mut self, layout: &Layout) -> ArgLoc {
        match classify(layout) {
            StructClass::Indirect => ArgLoc::Copy(self.scalar(Type::Ptr)),
            StructClass::Hfa { ty, count } => {
                match Self::regs(&mut self.floats, count, Loc::FReg) {
                    Some(regs) => ArgLoc::InRegs(regs),
                    None => {
                        let size = ty.bytes().unwrap();
                        ArgLoc::OnStack(self.stack(size * count, size))
                    }
                }
            }
            // Even a 16-byte aligned struct takes the next registers, with no
            // rounding to an even one: Apple differs from AAPCS64 here.
            StructClass::Gprs { count } => match Self::regs(&mut self.ints, count, Loc::Reg) {
                Some(regs) => ArgLoc::InRegs(regs),
                None => ArgLoc::OnStack(self.stack(8 * count, layout.align().max(8) as u32)),
            },
        }
    }
}

/// Places arguments `args`, the first `named` of them named and the rest
/// variadic. Returns each one's location and the bytes of stack they take.
pub fn arg_locs(args: &[ArgKind], named: usize) -> (Vec<ArgLoc>, u32) {
    let mut cursor = Cursor::default();
    let locs = args
        .iter()
        .enumerate()
        .map(|(i, arg)| match *arg {
            _ if i >= named => ArgLoc::Value(Slot::Stack(cursor.stack(8, 8))),
            ArgKind::Value(ty) => ArgLoc::Value(cursor.scalar(ty)),
            ArgKind::ByVal(layout) => cursor.by_val(layout),
            ArgKind::Sret(layout) => match result_regs(classify(layout)) {
                Some(regs) => ArgLoc::SretRegs(regs),
                None => ArgLoc::SretMemory,
            },
        })
        .collect();
    (locs, cursor.stack)
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
        let d2 = s(vec![t(Type::F64), t(Type::F64)]);
        assert_eq!(
            classify(&d2),
            StructClass::Hfa {
                ty: Type::F64,
                count: 2
            }
        );
        // Nesting and arrays count members, not fields.
        let nested = s(vec![t(Type::F32), Layout::Array(Box::new(t(Type::F32)), 3)]);
        assert_eq!(
            classify(&nested),
            StructClass::Hfa {
                ty: Type::F32,
                count: 4
            }
        );
        // Five floats are not an HFA, and 20 bytes is past 16.
        let five = Layout::Array(Box::new(t(Type::F32)), 5);
        assert_eq!(classify(&five), StructClass::Indirect);
        let mixed = s(vec![t(Type::F64), t(Type::I64)]);
        assert_eq!(classify(&mixed), StructClass::Gprs { count: 2 });
        let c3 = s(vec![t(Type::I8), t(Type::I8), t(Type::I8)]);
        assert_eq!(classify(&c3), StructClass::Gprs { count: 1 });
        let big = s(vec![t(Type::I64), t(Type::I64), t(Type::I64)]);
        assert_eq!(classify(&big), StructClass::Indirect);
    }

    /// The cases clang was checked against: a struct that no longer fits
    /// goes whole to the stack and closes its register class.
    #[test]
    fn struct_that_does_not_fit_closes_the_class() {
        let d2 = s(vec![t(Type::F64), t(Type::F64)]);
        let mut args: Vec<ArgKind> = (0..7).map(|_| ArgKind::Value(Type::F64)).collect();
        args.push(ArgKind::ByVal(&d2));
        args.push(ArgKind::Value(Type::F32));
        let (locs, stack) = arg_locs(&args, args.len());
        assert_eq!(locs[7], ArgLoc::OnStack(0));
        assert_eq!(locs[8], ArgLoc::Value(Slot::Stack(16)), "d7 is closed");
        assert_eq!(stack, 20);

        let f3 = s(vec![t(Type::F32), t(Type::F32), t(Type::F32)]);
        let mut args: Vec<ArgKind> = (0..8).map(|_| ArgKind::Value(Type::F64)).collect();
        args.extend([
            ArgKind::ByVal(&f3),
            ArgKind::Value(Type::F32),
            ArgKind::ByVal(&f3),
        ]);
        let (locs, _) = arg_locs(&args, args.len());
        // Apple packs a float aggregate on the stack at its own alignment.
        assert_eq!(
            locs[8..],
            [
                ArgLoc::OnStack(0),
                ArgLoc::Value(Slot::Stack(12)),
                ArgLoc::OnStack(16)
            ]
        );

        let s16 = s(vec![t(Type::I64), t(Type::I64)]);
        let c3 = s(vec![t(Type::I8), t(Type::I8), t(Type::I8)]);
        let mut args: Vec<ArgKind> = (0..7).map(|_| ArgKind::Value(Type::I64)).collect();
        args.extend([
            ArgKind::ByVal(&s16),
            ArgKind::ByVal(&c3),
            ArgKind::Value(Type::I8),
        ]);
        let (locs, _) = arg_locs(&args, args.len());
        assert_eq!(
            locs[7..],
            [
                ArgLoc::OnStack(0),
                ArgLoc::OnStack(16),
                ArgLoc::Value(Slot::Stack(24))
            ]
        );
    }

    #[test]
    fn indirect_and_sret() {
        let big = s(vec![t(Type::I64), t(Type::I64), t(Type::I64)]);
        let d2 = s(vec![t(Type::F64), t(Type::F64)]);
        let args = [
            ArgKind::Sret(&big),
            ArgKind::ByVal(&big),
            ArgKind::Value(Type::I64),
        ];
        let (locs, _) = arg_locs(&args, args.len());
        assert_eq!(
            locs,
            [
                ArgLoc::SretMemory,
                ArgLoc::Copy(Slot::Reg(Loc::Reg(0))),
                ArgLoc::Value(Slot::Reg(Loc::Reg(1))),
            ]
        );
        let (locs, _) = arg_locs(&[ArgKind::Sret(&d2)], 1);
        assert_eq!(locs, [ArgLoc::SretRegs(vec![Loc::FReg(0), Loc::FReg(1)])]);
    }
}
