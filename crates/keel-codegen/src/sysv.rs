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
use keel_ir::{Layout, Purpose, Signature, Type, VectorType, X86VectorAbi};

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
    /// No occupied bytes: consumes no register and has no transported payload.
    #[expect(
        clippy::enum_variant_names,
        reason = "NO_CLASS is the SysV ABI name and part of this public enum"
    )]
    NoClass,
    Integer,
    Sse,
    X87,
    X87Up,
}

/// A recursively nested single SIMD field uses SSE followed by SSEUP:
/// one physical XMM/YMM register, not one register per eightbyte. At256 bits,
/// require a true single-member chain: Clang and GCC disagree about zero-size
/// sibling members even when the flattened leaves and layout are identical.
/// Other vector-containing aggregates supported here are MEMORY (>16bytes).
pub fn vector_wrapper(layout: &Layout) -> Option<Type> {
    fn single_member(layout: &Layout) -> bool {
        match layout {
            Layout::Scalar(t) => t.is_vector(),
            Layout::Struct(fields) => fields.len() == 1 && single_member(&fields[0]),
            Layout::Array(elem, 1) => single_member(elem),
            _ => false,
        }
    }
    let ty = register_vector(layout)?;
    if ty.bytes() == Some(32) && !single_member(layout) {
        return None;
    }
    Some(ty)
}

/// Physical SSE/SSEUP transfer width once placement has proved a single
/// vector register. Producer admission remains separate from this storage rule.
pub fn register_vector(layout: &Layout) -> Option<Type> {
    // MEMORY results and unnamed operands can reach this helper before their
    // placement is inspected. Bound expansion by physical register size first;
    // zero-sized members remain in the AST for the separate producer policy.
    let size = layout.checked_size()?;
    if !matches!(size, 8 | 16 | 32) || layout.align() != size {
        return None;
    }
    match layout.leaves().as_slice() {
        [(0, ty)] if ty.is_vector() && ty.bytes().is_some_and(|n| u64::from(n) == size) => {
            Some(*ty)
        }
        _ => None,
    }
}

/// GNU zero-size aggregate members are ABI-origin sensitive at256 bits:
/// GCC uses YMM while Clang uses MEMORY. The IR has no producer-ABI tag, so
/// this layout must fail closed instead of choosing either external convention.
pub fn ambiguous_vector_wrapper(layout: &Layout) -> bool {
    layout.size() == 32
        && layout.align() == 32
        && vector_wrapper(layout).is_none()
        && matches!(layout.leaves().as_slice(),[(0,t)] if t.is_vector() && t.bytes()==Some(32))
}

/// The bounded mixed-D8 stage: two immediate, naturally aligned eightbytes.
/// Keep this structural: flattening would also admit unproved nested or empty
/// sibling layouts. In particular f64x1 has a separate producer ABI policy.
#[cfg(test)]
fn mixed_d8_fixed(layout: &Layout) -> bool {
    fn d8(layout: &Layout) -> bool {
        matches!(
            layout,
            Layout::Scalar(Type::Vector(
                VectorType::I8x8
                    | VectorType::I16x4
                    | VectorType::I32x2
                    | VectorType::I64x1
                    | VectorType::F32x2
            ))
        )
    }
    fn scalar(layout: &Layout) -> bool {
        matches!(layout, Layout::Scalar(Type::I64 | Type::F64))
    }
    matches!(layout, Layout::Struct(fields) if matches!(fields.as_slice(), [a, b]
        if (d8(a) && scalar(b)) || (scalar(a) && d8(b))))
}

/// A vector occupying one independent SSE eightbyte under the common SysV
/// extension contract. f64x1 deliberately remains producer-specific.
fn common_d8(ty: Type) -> bool {
    matches!(
        ty,
        Type::Vector(
            VectorType::I8x8
                | VectorType::I16x4
                | VectorType::I32x2
                | VectorType::I64x1
                | VectorType::F32x2
        )
    )
}

/// Natural recursive aggregates made from ordinary scalars and common D8
/// leaves, with at least one D8 leaf. Layout supplies natural offsets/stride;
/// there are no unions, packed members or explicit alignment overrides here.
/// Zero-size extension members and other vector producer contracts are kept
/// separate. This predicate is structural, independent of nesting depth,
/// field count and array length; it is not a finite signature whitelist.
pub fn natural_d8_aggregate(layout: &Layout) -> bool {
    fn visit(layout: &Layout) -> Option<bool> {
        match layout {
            Layout::Scalar(ty) if common_d8(*ty) => Some(true),
            Layout::Scalar(ty) if !ty.is_vector() => Some(false),
            Layout::Scalar(_) => None,
            Layout::Struct(fields) if !fields.is_empty() => fields
                .iter()
                .try_fold(false, |found, field| Some(visit(field)? || found)),
            Layout::Array(elem, n) if *n > 0 => visit(elem),
            _ => None,
        }
    }
    !matches!(layout, Layout::Scalar(_)) && visit(layout) == Some(true)
}

/// The classes of a struct's eightbytes, or `None` for MEMORY: bigger than
/// 16 bytes. Empty objects have no classes and consume no argument storage.
pub fn classify(layout: &Layout) -> Option<Vec<Class>> {
    classify_eightbytes(layout, natural_d8_aggregate(layout))
}

fn classify_eightbytes(layout: &Layout, natural_d8: bool) -> Option<Vec<Class>> {
    let size = layout.size();
    if size > 16 || vector_wrapper(layout).is_some() {
        return None;
    }
    if layout.leaves().iter().any(|(_, t)| *t == Type::F80) {
        return (layout.leaves() == [(0, Type::F80)] && size == 16)
            .then(|| vec![Class::X87, Class::X87Up]);
    }
    let mut classes = vec![Class::NoClass; size.div_ceil(8) as usize];
    for (offset, ty) in layout.leaves() {
        // A naturally aligned leaf may occupy multiple eightbytes (i128).
        // D8 leaves occupy a whole naturally aligned eightbyte, independently
        // of nesting. Larger vectors still need the wrapper's SSEUP path.
        // Scalar integer/ptr leaves dominate any floats sharing their half.
        let leaf = if ty.is_float() || (natural_d8 && common_d8(ty)) {
            Class::Sse
        } else {
            Class::Integer
        };
        let end = offset + Layout::Scalar(ty).size();
        for class in &mut classes[(offset / 8) as usize..end.div_ceil(8) as usize] {
            if *class != Class::Integer {
                *class = leaf;
            }
        }
    }
    Some(classes)
}

/// Natural recursive producer policy established by the independent 110-layout
/// genuine GCC 16.2.1 / Clang 23.1.1 C and expected-register primary. It reuses
/// the common eightbyte classifier; placement and bank rollback stay in Cursor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ProducerAggregate {
    Memory,
    Vector(Type),
    Eightbytes(Vec<Class>),
}

/// Same signed displacement/frame bound as the x86 frame planner. Admission
/// checks the complete contract budget before any expanded leaf traversal.
pub const MAX_AGGREGATE_ABI_BYTES: u64 = i32::MAX as u64 / 2;

pub fn producer_aggregate(layout: &Layout, producer: X86VectorAbi) -> Option<ProducerAggregate> {
    fn declares_f64x1(l: &Layout) -> bool {
        match l {
            Layout::Scalar(t) => *t == Type::Vector(VectorType::F64x1),
            Layout::Struct(fields) => fields.iter().any(declares_f64x1),
            Layout::Array(elem, _) => declares_f64x1(elem),
        }
    }
    fn single(l: &Layout) -> bool {
        match l {
            Layout::Scalar(t) => t.is_vector(),
            Layout::Struct(fields) => fields.len() == 1 && single(&fields[0]),
            Layout::Array(elem, 1) => single(elem),
            _ => false,
        }
    }
    if matches!(layout, Layout::Scalar(_)) || !declares_f64x1(layout) {
        return None;
    }
    let size = layout.checked_size()?;
    if size > MAX_AGGREGATE_ABI_BYTES {
        return None;
    }
    fn occupied(l: &Layout) -> bool {
        match l {
            Layout::Scalar(t) => *t == Type::Vector(VectorType::F64x1),
            Layout::Struct(fields) => fields.iter().any(occupied),
            Layout::Array(elem, n) => *n != 0 && occupied(elem),
        }
    }
    if occupied(layout) {
        return Some(ProducerAggregate::Memory);
    }
    if size == 0 {
        return Some(ProducerAggregate::Eightbytes(vec![]));
    }
    // Every currently representable vector is at most 32 bytes. Larger
    // objects cannot be single-register/SSEUP wrappers.
    if size > 32 {
        return Some(ProducerAggregate::Memory);
    }
    let leaves = layout.leaves();
    // Zero-count arrays retain alignment but have no occupied eightbytes.
    // Producer-sensitive empty siblings can change a wide vector wrapper.
    if let [(0, ty)] = leaves.as_slice()
        && ty.is_vector()
        && ty
            .bytes()
            .is_some_and(|n| u64::from(n) == layout.size() && u64::from(n) == layout.align())
    {
        return Some(
            if ty.bytes() == Some(32) && producer == X86VectorAbi::Clang && !single(layout) {
                ProducerAggregate::Memory
            } else {
                ProducerAggregate::Vector(*ty)
            },
        );
    }
    Some(match classify_eightbytes(layout, true) {
        Some(classes) => ProducerAggregate::Eightbytes(classes),
        None => ProducerAggregate::Memory,
    })
}

/// Common planner policy entry point, independent of ISA features.
pub fn aggregate_policy(layout: &Layout, producer: Option<X86VectorAbi>) -> ProducerAggregate {
    if let Some(policy) = producer.and_then(|p| producer_aggregate(layout, p)) {
        return policy;
    }
    if let Some(ty) = vector_wrapper(layout) {
        return ProducerAggregate::Vector(ty);
    }
    classify(layout).map_or(ProducerAggregate::Memory, ProducerAggregate::Eightbytes)
}

/// Where one scalar is. An `i128` in registers is in this one (its low
/// half) and [`Slot::Regs`]'s second.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    /// One register, or two for an `i128`.
    Regs(Loc, Option<Loc>),
    /// At this offset from the caller's `rsp` at the call.
    Stack(u32),
}

/// A physical register and the byte offset of its payload in the object.
/// NO_CLASS eightbytes have no piece; offsets never depend on compacted index.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct RegPiece {
    pub offset: u64,
    pub loc: Loc,
}

/// Where one argument goes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ArgLoc {
    /// A scalar.
    Value(Slot),
    /// A `byval` struct in registers, with object offsets for occupied eightbytes.
    InRegs(Vec<RegPiece>),
    /// A `byval` struct copied onto the stack at this offset.
    OnStack(u32),
    /// An `sret` result that comes back in these registers; nothing is
    /// passed for it.
    SretRegs(Vec<RegPiece>),
    /// An `sret` result written to memory at the address passed in rdi.
    SretMemory,
    /// X87/X87UP aggregate result in ST0; no hidden pointer is passed.
    SretX87,
}

/// An argument as the convention sees it.
#[derive(Copy, Clone, Debug)]
pub enum ArgKind<'a> {
    Value(Type),
    /// Unnamed wide vectors use the overflow area even with free SIMD registers.
    StackValue(Type),
    ByVal(&'a Layout),
    ProducerByVal(&'a Layout, X86VectorAbi),
    ProducerSret(&'a Layout, X86VectorAbi),
    /// Audited producer wrapper passed entirely in memory.
    MemoryByVal(&'a Layout),
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
fn result_regs(classes: &[Class]) -> Vec<RegPiece> {
    let (mut ints, mut floats) = ([RAX, RDX].into_iter(), 0..);
    classes
        .iter()
        .enumerate()
        .filter_map(|(i, class)| {
            let loc = match class {
                Class::NoClass => return None,
                Class::Integer => Loc::Reg(ints.next().unwrap()),
                Class::Sse => Loc::FReg(floats.next().unwrap()),
                Class::X87 | Class::X87Up => unreachable!("x87 result is not a SIMD register"),
            };
            Some(RegPiece {
                offset: i as u64 * 8,
                loc,
            })
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
    stack_align: u32,
}

impl Cursor {
    /// The next stack slot for `size` bytes, rounded up to 8.
    fn stack(&mut self, size: u64, align: u64) -> u32 {
        self.stack_align = self.stack_align.max(align as u32);
        let offset = self.stack.next_multiple_of(align.max(8) as u32);
        self.stack = offset + (size.next_multiple_of(8) as u32);
        offset
    }

    /// Registers for these eightbyte classes, if all fit.
    fn regs(&mut self, classes: &[Class]) -> Option<Vec<RegPiece>> {
        if classes
            .iter()
            .any(|c| matches!(c, Class::X87 | Class::X87Up))
        {
            return None;
        }
        let ints = classes.iter().filter(|&&c| c == Class::Integer).count();
        let floats = classes.iter().filter(|&&c| c == Class::Sse).count();
        if self.ints + ints > INT_ARGS.len() || usize::from(self.floats) + floats > 8 {
            return None;
        }
        Some(
            classes
                .iter()
                .enumerate()
                .filter_map(|(i, class)| {
                    let loc = match class {
                        Class::NoClass => return None,
                        Class::Integer => {
                            self.ints += 1;
                            Loc::Reg(INT_ARGS[self.ints - 1])
                        }
                        Class::X87 | Class::X87Up => unreachable!("x87 arguments use MEMORY"),
                        Class::Sse => {
                            self.floats += 1;
                            Loc::FReg(self.floats - 1)
                        }
                    };
                    Some(RegPiece {
                        offset: i as u64 * 8,
                        loc,
                    })
                })
                .collect(),
        )
    }

    /// As [`Self::regs`] would place its eightbytes, without a list.
    fn scalar(&mut self, ty: Type) -> Slot {
        if ty == Type::F80 {
            return Slot::Stack(self.stack(16, 16));
        }
        if ty == Type::I128 {
            if self.ints + 2 <= INT_ARGS.len() {
                self.ints += 2;
                let (lo, hi) = (INT_ARGS[self.ints - 2], INT_ARGS[self.ints - 1]);
                return Slot::Regs(Loc::Reg(lo), Some(Loc::Reg(hi)));
            }
        } else if ty.is_float() || ty.is_vector() {
            if self.floats < 8 {
                self.floats += 1;
                return Slot::Regs(Loc::FReg(self.floats - 1), None);
            }
        } else if self.ints < INT_ARGS.len() {
            self.ints += 1;
            return Slot::Regs(Loc::Reg(INT_ARGS[self.ints - 1]), None);
        }
        let size = u64::from(ty.bytes().unwrap_or(8));
        Slot::Stack(self.stack(size, size))
    }

    fn by_val(&mut self, layout: &Layout, producer: Option<X86VectorAbi>) -> ArgLoc {
        match aggregate_policy(layout, producer) {
            ProducerAggregate::Vector(_) if self.floats < 8 => {
                self.floats += 1;
                ArgLoc::InRegs(vec![RegPiece {
                    offset: 0,
                    loc: Loc::FReg(self.floats - 1),
                }])
            }
            ProducerAggregate::Eightbytes(classes) => match self.regs(&classes) {
                Some(regs) => ArgLoc::InRegs(regs),
                None => ArgLoc::OnStack(self.stack(layout.size(), layout.align())),
            },
            _ => ArgLoc::OnStack(self.stack(layout.size(), layout.align())),
        }
    }

    fn sret(&mut self, layout: &Layout, producer: Option<X86VectorAbi>) -> ArgLoc {
        match aggregate_policy(layout, producer) {
            ProducerAggregate::Vector(_) => ArgLoc::SretRegs(vec![RegPiece {
                offset: 0,
                loc: Loc::FReg(0),
            }]),
            ProducerAggregate::Eightbytes(classes) if classes == [Class::X87, Class::X87Up] => {
                ArgLoc::SretX87
            }
            ProducerAggregate::Eightbytes(classes) => ArgLoc::SretRegs(result_regs(&classes)),
            ProducerAggregate::Memory => {
                self.ints += 1;
                ArgLoc::SretMemory
            }
        }
    }
}

/// Where a call's arguments go.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Placement {
    /// Each argument's location.
    pub locs: Vec<ArgLoc>,
    /// Bytes of stack arguments.
    pub stack: u32,
    /// Required alignment of the incoming stack argument area.
    pub stack_align: u32,
    /// General-purpose and vector argument registers used: where a
    /// variadic function's `va_start` begins, and `al` for its callers.
    pub ints: u8,
    pub floats: u8,
}

pub fn signature_param_kind<'a>(sig: &Signature, param: &'a keel_ir::AbiParam) -> ArgKind<'a> {
    if let Some(producer) = sig.x86_vector_abi {
        match &param.purpose {
            Purpose::Normal if param.ty == Type::Vector(VectorType::F64x1) => {
                return ArgKind::StackValue(param.ty);
            }
            Purpose::ByVal(layout) if producer_aggregate(layout, producer).is_some() => {
                return ArgKind::ProducerByVal(layout, producer);
            }
            Purpose::Sret(layout) if producer_aggregate(layout, producer).is_some() => {
                return ArgKind::ProducerSret(layout, producer);
            }
            _ => {}
        }
    }
    ArgKind::of(param.ty, &param.purpose)
}

/// GNU scalar f64x1 returns consume an implicit first GP argument.
pub fn scalar_memory_result(sig: &Signature) -> bool {
    sig.x86_vector_abi == Some(X86VectorAbi::Gnu)
        && sig
            .returns
            .first()
            .is_some_and(|p| p.ty == Type::Vector(VectorType::F64x1))
}

pub fn signature_arg_locs(sig: &Signature, args: &[ArgKind]) -> Placement {
    let mut placement = Placement::default();
    placement.fill_signature(sig, args.iter().copied());
    placement
}

#[cfg(test)]
pub fn arg_locs(args: &[ArgKind]) -> Placement {
    let mut placement = Placement::default();
    placement.fill(args.iter().copied());
    placement
}

impl Placement {
    /// Becomes [`arg_locs`] of `args`, reusing the list of locations.
    #[cfg(test)]
    pub fn fill<'a>(&mut self, args: impl IntoIterator<Item = ArgKind<'a>>) {
        self.fill_reserved(args, false);
    }

    pub fn fill_signature<'a>(
        &mut self,
        sig: &Signature,
        args: impl IntoIterator<Item = ArgKind<'a>>,
    ) {
        self.fill_reserved(args, scalar_memory_result(sig));
    }

    fn fill_reserved<'a>(&mut self, args: impl IntoIterator<Item = ArgKind<'a>>, hidden: bool) {
        let mut cursor = Cursor {
            ints: usize::from(hidden),
            ..Default::default()
        };
        self.locs.clear();
        self.locs.extend(args.into_iter().map(|arg| match arg {
            ArgKind::Value(ty) => ArgLoc::Value(cursor.scalar(ty)),
            ArgKind::StackValue(ty) => {
                let size = u64::from(ty.bytes().expect("sized stack value"));
                ArgLoc::Value(Slot::Stack(cursor.stack(size, size)))
            }
            ArgKind::ByVal(layout) => cursor.by_val(layout, None),
            ArgKind::ProducerByVal(layout, producer) => cursor.by_val(layout, Some(producer)),
            ArgKind::ProducerSret(layout, producer) => cursor.sret(layout, Some(producer)),
            ArgKind::MemoryByVal(layout) => {
                ArgLoc::OnStack(cursor.stack(layout.size(), layout.align()))
            }
            ArgKind::Sret(layout) => cursor.sret(layout, None),
        }));
        self.stack = cursor.stack;
        self.stack_align = cursor.stack_align.max(16);
        self.ints = cursor.ints as u8;
        self.floats = cursor.floats;
    }
}

/// Where a scalar result comes back: rax (and rdx for the high half of
/// an `i128`), or xmm0.
pub fn return_loc(ty: Type) -> Loc {
    assert_ne!(
        ty,
        Type::F80,
        "F80 returns in ST0, outside the SSA register allocator"
    );
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

    fn pieces(regs: Vec<Loc>) -> Vec<RegPiece> {
        regs.into_iter()
            .enumerate()
            .map(|(i, loc)| RegPiece {
                offset: i as u64 * 8,
                loc,
            })
            .collect()
    }

    #[test]
    fn no_class_padding_keeps_offsets_and_consumes_neither_bank() {
        for scalar in [Type::I8, Type::I64, Type::F64] {
            let occupied = if scalar.is_float() {
                Class::Sse
            } else {
                Class::Integer
            };
            let mut layout = s(vec![t(scalar), Layout::Array(Box::new(t(Type::I128)), 0)]);
            for _ in 0..4 {
                assert_eq!((layout.size(), layout.align()), (16, 16));
                assert_eq!(classify(&layout), Some(vec![occupied, Class::NoClass]));
                for gp in [0, 5, 6] {
                    for fp in [0, 7, 8] {
                        let mut cursor = Cursor {
                            ints: gp,
                            floats: fp,
                            ..Cursor::default()
                        };
                        let fits = if occupied == Class::Integer {
                            gp < 6
                        } else {
                            fp < 8
                        };
                        let loc = cursor.by_val(&layout, None);
                        assert_eq!(matches!(loc, ArgLoc::InRegs(_)), fits);
                        assert_eq!(
                            cursor.ints,
                            gp + usize::from(fits && occupied == Class::Integer)
                        );
                        assert_eq!(cursor.floats, fp + u8::from(fits && occupied == Class::Sse));
                    }
                }
                layout = s(vec![Layout::Array(Box::new(layout), 1)]);
            }
        }
        // Synthetic leading/all-empty classes ensure offsets are never compacted.
        let mut cursor = Cursor::default();
        assert_eq!(
            cursor.regs(&[Class::NoClass, Class::Integer]),
            Some(vec![RegPiece {
                offset: 8,
                loc: Loc::Reg(RDI)
            }])
        );
        assert_eq!(
            result_regs(&[Class::NoClass, Class::Sse]),
            vec![RegPiece {
                offset: 8,
                loc: Loc::FReg(0)
            }]
        );
        assert_eq!(cursor.regs(&[Class::NoClass, Class::NoClass]), Some(vec![]));
        assert_eq!((cursor.ints, cursor.floats), (1, 0));
        // Natural layouts cannot put a first nonempty leaf after leading padding.
        for layout in [
            s(vec![]),
            Layout::Array(Box::new(t(Type::I128)), 0),
            s(vec![s(vec![]), Layout::Array(Box::new(t(Type::I128)), 0)]),
        ] {
            assert_eq!(classify(&layout), Some(vec![]));
            assert_eq!(
                Cursor::default().by_val(&layout, None),
                ArgLoc::InRegs(vec![])
            );
        }
    }

    #[test]
    fn i128_leaf_marks_every_eightbyte_and_rolls_back_gp_only() {
        let mut layout = t(Type::I128);
        for _depth in 0..8 {
            assert_eq!(classify(&layout), Some(vec![Class::Integer; 2]));
            let result = arg_locs(&[ArgKind::Sret(&layout)]);
            assert_eq!(
                result.locs,
                [ArgLoc::SretRegs(pieces(vec![Loc::Reg(RAX), Loc::Reg(RDX)]))]
            );
            for gp in [0, 4, 5, 6] {
                for fp in [0, 7, 8] {
                    let mut args = vec![ArgKind::Value(Type::I64); gp];
                    args.extend(vec![ArgKind::Value(Type::F64); fp]);
                    args.extend([
                        ArgKind::ByVal(&layout),
                        ArgKind::Value(Type::I64),
                        ArgKind::Value(Type::F64),
                    ]);
                    let plan = arg_locs(&args);
                    let used = if gp + 2 <= 6 { gp + 2 } else { gp };
                    assert_eq!(
                        plan.locs[gp + fp],
                        if gp + 2 <= 6 {
                            ArgLoc::InRegs(pieces(vec![
                                Loc::Reg(INT_ARGS[gp]),
                                Loc::Reg(INT_ARGS[gp + 1]),
                            ]))
                        } else {
                            ArgLoc::OnStack(0)
                        }
                    );
                    assert_eq!(
                        matches!(plan.locs[gp + fp + 1], ArgLoc::Value(Slot::Regs(_, _))),
                        used < 6
                    );
                    assert_eq!(
                        plan.locs[gp + fp + 2],
                        if fp < 8 {
                            ArgLoc::Value(Slot::Regs(Loc::FReg(fp as u8), None))
                        } else {
                            ArgLoc::Value(Slot::Stack(if gp + 2 <= 6 {
                                if used < 6 { 0 } else { 8 }
                            } else if gp < 6 {
                                16
                            } else {
                                24
                            }))
                        }
                    );
                }
            }
            layout = if _depth % 2 == 0 {
                s(vec![layout])
            } else {
                Layout::Array(Box::new(layout), 1)
            };
        }
        for layout in [
            s(vec![t(Type::I128), t(Type::I8)]),
            Layout::Array(Box::new(t(Type::I128)), 2),
        ] {
            assert_eq!(classify(&layout), None);
        }
    }

    #[test]
    fn mixed_d8_fixed_classes_and_atomic_bank_rollback() {
        for vector in [
            VectorType::I8x8,
            VectorType::I16x4,
            VectorType::I32x2,
            VectorType::I64x1,
            VectorType::F32x2,
        ] {
            let d = t(Type::Vector(vector));
            for scalar in [Type::I64, Type::F64] {
                for reverse in [false, true] {
                    let fields = if reverse {
                        vec![t(scalar), d.clone()]
                    } else {
                        vec![d.clone(), t(scalar)]
                    };
                    let layout = s(fields);
                    assert!(mixed_d8_fixed(&layout));
                    assert_eq!((layout.size(), layout.align()), (16, 8));
                    let mut classes = vec![
                        Class::Sse,
                        if scalar == Type::I64 {
                            Class::Integer
                        } else {
                            Class::Sse
                        },
                    ];
                    if reverse {
                        classes.reverse();
                    }
                    assert_eq!(classify(&layout), Some(classes.clone()));
                    let result = arg_locs(&[ArgKind::Sret(&layout), ArgKind::Value(Type::I64)]);
                    assert_eq!(result.locs[0], ArgLoc::SretRegs(result_regs(&classes)));
                    assert_eq!(
                        result.locs[1],
                        ArgLoc::Value(Slot::Regs(Loc::Reg(RDI), None))
                    );
                    for gp in [5, 6] {
                        for fp in [6, 7, 8] {
                            let mut args = vec![ArgKind::Value(Type::I64); gp];
                            args.extend(vec![ArgKind::Value(Type::F64); fp]);
                            args.push(ArgKind::ByVal(&layout));
                            args.push(ArgKind::Value(Type::I64));
                            args.push(ArgKind::Value(Type::F64));
                            let p = arg_locs(&args);
                            let needs_gp = usize::from(scalar == Type::I64);
                            let needs_fp = 2 - needs_gp;
                            let fits = gp + needs_gp <= 6 && fp + needs_fp <= 8;
                            assert_eq!(matches!(p.locs[gp + fp], ArgLoc::InRegs(_)), fits);
                            if !fits {
                                assert_eq!(p.locs[gp + fp], ArgLoc::OnStack(0));
                                assert_eq!(
                                    matches!(p.locs[gp + fp + 1], ArgLoc::Value(Slot::Regs(_, _))),
                                    gp < 6
                                );
                                assert_eq!(
                                    matches!(p.locs[gp + fp + 2], ArgLoc::Value(Slot::Regs(_, _))),
                                    fp < 8
                                );
                            }
                        }
                    }
                    assert!(!mixed_d8_fixed(&s(vec![layout.clone()])));
                    assert!(!mixed_d8_fixed(&Layout::Array(Box::new(layout), 1)));
                }
            }
            assert!(!mixed_d8_fixed(&s(vec![d.clone(), d])));
        }
        assert!(!mixed_d8_fixed(&s(vec![
            t(Type::Vector(VectorType::F64x1)),
            t(Type::I64)
        ])));
        assert!(!mixed_d8_fixed(&s(vec![
            t(Type::Vector(VectorType::F32x2)),
            t(Type::I32)
        ])));
    }

    #[test]
    fn recursive_d8_layout_offsets_classes_and_rollback() {
        for vector in [
            VectorType::I8x8,
            VectorType::I16x4,
            VectorType::I32x2,
            VectorType::I64x1,
            VectorType::F32x2,
        ] {
            let d = t(Type::Vector(vector));
            for scalar in [
                Type::I8,
                Type::I16,
                Type::I32,
                Type::I64,
                Type::Ptr,
                Type::F32,
                Type::F64,
            ] {
                let sc = if scalar.is_float() {
                    Class::Sse
                } else {
                    Class::Integer
                };
                for (mut layout, expected) in [
                    (s(vec![d.clone(), t(scalar)]), vec![Class::Sse, sc]),
                    (s(vec![t(scalar), d.clone()]), vec![sc, Class::Sse]),
                ] {
                    for _depth in 0..6 {
                        assert!(natural_d8_aggregate(&layout));
                        assert_eq!((layout.size(), layout.align()), (16, 8));
                        assert_eq!(classify(&layout), Some(expected.clone()));
                        // Recursive wrappers and one-element arrays retain
                        // the exact same field offsets and ABI register plan.
                        let ret = arg_locs(&[ArgKind::Sret(&layout)]);
                        assert_eq!(ret.locs, [ArgLoc::SretRegs(result_regs(&expected))]);
                        for gp in [0, 5, 6] {
                            for fp in [0, 6, 7, 8] {
                                let mut args = vec![ArgKind::Value(Type::I64); gp];
                                args.extend(vec![ArgKind::Value(Type::F64); fp]);
                                args.extend([
                                    ArgKind::ByVal(&layout),
                                    ArgKind::Value(Type::I64),
                                    ArgKind::Value(Type::F64),
                                ]);
                                let plan = arg_locs(&args);
                                let ng = expected.iter().filter(|c| **c == Class::Integer).count();
                                let fits = gp + ng <= 6 && fp + expected.len() - ng <= 8;
                                assert_eq!(matches!(plan.locs[gp + fp], ArgLoc::InRegs(_)), fits);
                                if !fits {
                                    assert_eq!(plan.locs[gp + fp], ArgLoc::OnStack(0));
                                    assert_eq!(
                                        matches!(
                                            plan.locs[gp + fp + 1],
                                            ArgLoc::Value(Slot::Regs(_, _))
                                        ),
                                        gp < 6
                                    );
                                    assert_eq!(
                                        matches!(
                                            plan.locs[gp + fp + 2],
                                            ArgLoc::Value(Slot::Regs(_, _))
                                        ),
                                        fp < 8
                                    );
                                }
                            }
                        }
                        layout = s(vec![Layout::Array(Box::new(layout), 1)]);
                    }
                }
            }
            for layout in [
                s(vec![d.clone(), d.clone()]),
                Layout::Array(Box::new(d.clone()), 2),
            ] {
                assert!(natural_d8_aggregate(&layout));
                assert_eq!(classify(&layout), Some(vec![Class::Sse, Class::Sse]));
            }
            // Mixed scalar leaves at offsets8 and12 share the second half;
            // INTEGER must dominate SSE in either order and under nesting.
            for rest in [
                s(vec![t(Type::F32), t(Type::I8)]),
                s(vec![t(Type::I8), t(Type::F32)]),
            ] {
                let layout = s(vec![d.clone(), rest]);
                assert_eq!(layout.leaves()[1].0, 8);
                assert_eq!(layout.leaves()[2].0, 12);
                assert_eq!(classify(&layout), Some(vec![Class::Sse, Class::Integer]));
            }
            // A natural D8 beside a 16-byte scalar is always MEMORY:
            // 16-byte alignment puts the two leaves in a 32-byte object.
            for scalar in [Type::I128, Type::F80] {
                for layout in [s(vec![d.clone(), t(scalar)]), s(vec![t(scalar), d.clone()])] {
                    assert!(natural_d8_aggregate(&layout));
                    assert_eq!((layout.size(), layout.align()), (32, 16));
                    assert_eq!(classify(&layout), None);
                    let plan = arg_locs(&[
                        ArgKind::Sret(&layout),
                        ArgKind::ByVal(&layout),
                        ArgKind::Value(Type::I64),
                    ]);
                    assert_eq!(plan.locs[1], ArgLoc::OnStack(0));
                }
            }
            let big = s(vec![d.clone(), Layout::Array(Box::new(t(Type::I8)), 9)]);
            assert!(natural_d8_aggregate(&big));
            assert_eq!(classify(&big), None);
            for other in [
                s(vec![]),
                Layout::Array(Box::new(t(Type::I8)), 0),
                t(Type::Vector(VectorType::F64x1)),
                t(Type::Vector(VectorType::F32x4)),
            ] {
                assert!(!natural_d8_aggregate(&s(vec![d.clone(), other])));
            }
        }
    }

    #[test]
    fn extended_payload_class_and_abi_storage() {
        let wrapper = s(vec![t(Type::F80)]);
        assert_eq!(classify(&wrapper), Some(vec![Class::X87, Class::X87Up]));
        let nested = s(vec![s(vec![t(Type::F80)])]);
        assert_eq!(classify(&nested), Some(vec![Class::X87, Class::X87Up]));
        assert_eq!(classify(&s(vec![t(Type::F80), t(Type::I8)])), None);
        let placement = arg_locs(&[
            ArgKind::Value(Type::F80),
            ArgKind::ByVal(&wrapper),
            ArgKind::Value(Type::F64),
            ArgKind::Value(Type::I64),
        ]);
        assert_eq!(
            placement.locs,
            [
                ArgLoc::Value(Slot::Stack(0)),
                ArgLoc::OnStack(16),
                ArgLoc::Value(Slot::Regs(Loc::FReg(0), None)),
                ArgLoc::Value(Slot::Regs(Loc::Reg(RDI), None))
            ]
        );
        assert_eq!(
            (
                placement.stack,
                placement.stack_align,
                placement.ints,
                placement.floats
            ),
            (32, 16, 1, 1)
        );
        let ret = arg_locs(&[ArgKind::Sret(&wrapper), ArgKind::Value(Type::I64)]);
        assert_eq!(
            ret.locs,
            [
                ArgLoc::SretX87,
                ArgLoc::Value(Slot::Regs(Loc::Reg(RDI), None))
            ]
        );
        assert_eq!(ret.ints, 1);
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
        assert_eq!(locs[0], ArgLoc::Value(Slot::Regs(Loc::Reg(RDI), None)));
        assert_eq!(locs[5], ArgLoc::Value(Slot::Regs(Loc::Reg(9), None)));
        assert_eq!(locs[6], ArgLoc::Value(Slot::Stack(0)));
        assert_eq!(locs[7], ArgLoc::Value(Slot::Regs(Loc::FReg(0), None)));
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
        assert_eq!(locs[6], ArgLoc::Value(Slot::Regs(Loc::Reg(9), None)));
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
                ArgLoc::Value(Slot::Regs(Loc::Reg(RSI), None))
            ]
        );
        let locs = arg_locs(&[ArgKind::Sret(&dl)]).locs;
        assert_eq!(
            locs,
            [ArgLoc::SretRegs(pieces(vec![Loc::FReg(0), Loc::Reg(RAX)]))]
        );
    }
    #[test]
    fn wide_named_and_unnamed_vectors_have_distinct_placements() {
        let wide_ty = Type::Vector(keel_ir::VectorType::F32x8);
        let mut args = vec![ArgKind::Value(wide_ty); 10];
        let p = arg_locs(&args);
        assert_eq!(p.floats, 8);
        assert_eq!(p.stack_align, 32);
        assert_eq!(p.locs[7], ArgLoc::Value(Slot::Regs(Loc::FReg(7), None)));
        assert_eq!(p.locs[8], ArgLoc::Value(Slot::Stack(0)));
        assert_eq!(p.locs[9], ArgLoc::Value(Slot::Stack(32)));
        args = vec![
            ArgKind::Value(Type::I64),
            ArgKind::StackValue(wide_ty),
            ArgKind::Value(Type::F64),
            ArgKind::StackValue(wide_ty),
        ];
        let p = arg_locs(&args);
        assert_eq!(p.floats, 1);
        assert_eq!(p.stack, 64);
        assert_eq!(p.locs[1], ArgLoc::Value(Slot::Stack(0)));
        assert_eq!(p.locs[2], ArgLoc::Value(Slot::Regs(Loc::FReg(0), None)));
        assert_eq!(p.locs[3], ArgLoc::Value(Slot::Stack(32)));
    }
    #[test]
    fn single_vector_sseup_wrappers_and_boundary_rollback() {
        for ty in [
            Type::Vector(keel_ir::VectorType::F32x4),
            Type::Vector(keel_ir::VectorType::I8x32),
        ] {
            let nested = s(vec![Layout::Array(Box::new(s(vec![t(ty)])), 1)]);
            assert_eq!(vector_wrapper(&nested), Some(ty));
            for other in [
                s(vec![t(ty), t(Type::I64)]),
                Layout::Array(Box::new(t(ty)), 2),
            ] {
                assert_eq!(vector_wrapper(&other), None);
                assert_eq!(classify(&other), None);
            }
            let mut args = vec![ArgKind::Value(Type::F64); 7];
            args.push(ArgKind::ByVal(&nested));
            args.push(ArgKind::Value(Type::I64));
            let p = arg_locs(&args);
            assert_eq!(p.floats, 8);
            assert_eq!(p.locs[7], ArgLoc::InRegs(pieces(vec![Loc::FReg(7)])));
            assert_eq!(p.locs[8], ArgLoc::Value(Slot::Regs(Loc::Reg(RDI), None)));
            args.insert(0, ArgKind::Value(Type::F64));
            args.push(ArgKind::Value(Type::F64));
            let p = arg_locs(&args);
            assert_eq!(p.locs[8], ArgLoc::OnStack(0));
            assert_eq!(p.locs[9], ArgLoc::Value(Slot::Regs(Loc::Reg(RDI), None)));
            assert_eq!(p.locs[10], ArgLoc::Value(Slot::Stack(ty.bytes().unwrap())));
            let p = arg_locs(&[ArgKind::Sret(&nested), ArgKind::Value(Type::I64)]);
            assert_eq!(
                p.locs[0],
                ArgLoc::SretRegs(vec![RegPiece {
                    offset: 0,
                    loc: Loc::FReg(0)
                }])
            );
            assert_eq!(p.ints, 1);
            assert_eq!(p.floats, 0);
        }
    }
}

#[cfg(test)]
mod producer_policy_tests {
    use super::*;
    #[test]
    fn producer_plans_reuse_memory_and_bank_rollback() {
        for producer in [X86VectorAbi::Gnu, X86VectorAbi::Clang] {
            let l = Layout::Struct(vec![
                Layout::Array(Box::new(Layout::Scalar(Type::Vector(VectorType::F64x1))), 2),
                Layout::Scalar(Type::I8),
            ]);
            for gp in 0..=7 {
                let mut prepared = vec![ArgKind::ProducerSret(&l, producer)];
                let mut proven = vec![ArgKind::Sret(&l)];
                for _ in 0..gp {
                    prepared.push(ArgKind::Value(Type::I64));
                    proven.push(ArgKind::Value(Type::I64));
                }
                prepared.extend([
                    ArgKind::ProducerByVal(&l, producer),
                    ArgKind::Value(Type::F64),
                ]);
                proven.extend([ArgKind::MemoryByVal(&l), ArgKind::Value(Type::F64)]);
                assert_eq!(arg_locs(&prepared), arg_locs(&proven));
            }
        }
    }
    #[test]
    fn recursive_policy_preserves_empty_storage_offsets() {
        let v = Layout::Scalar(Type::Vector(VectorType::F64x1));
        for producer in [X86VectorAbi::Gnu, X86VectorAbi::Clang] {
            let mut nested = v.clone();
            for _ in 0..8 {
                nested = Layout::Struct(vec![
                    Layout::Array(Box::new(nested), 2),
                    Layout::Scalar(Type::I8),
                ]);
                assert_eq!(
                    producer_aggregate(&nested, producer),
                    Some(ProducerAggregate::Memory)
                );
            }
            let padding = Layout::Struct(vec![
                Layout::Scalar(Type::I32),
                Layout::Array(Box::new(v.clone()), 0),
            ]);
            assert_eq!(
                producer_aggregate(&padding, producer),
                Some(ProducerAggregate::Eightbytes(vec![Class::Integer]))
            );
        }
        let empty = Layout::Array(Box::new(v), 0);
        let y = Layout::Struct(vec![empty, Layout::Scalar(Type::Vector(VectorType::I64x4))]);
        assert_eq!(
            producer_aggregate(&y, X86VectorAbi::Gnu),
            Some(ProducerAggregate::Vector(Type::Vector(VectorType::I64x4)))
        );
        assert_eq!(
            producer_aggregate(&y, X86VectorAbi::Clang),
            Some(ProducerAggregate::Memory)
        );
    }
}

#[cfg(test)]
mod bounded_vector_tests {
    use super::*;
    #[test]
    fn physical_vector_helpers_bound_expansion_before_leaves() {
        let v = Layout::Scalar(Type::Vector(VectorType::F64x1));
        for layout in [
            Layout::Array(Box::new(v.clone()), 100_000_000),
            Layout::Array(
                Box::new(Layout::Array(Box::new(v.clone()), u32::MAX)),
                u32::MAX,
            ),
        ] {
            assert_eq!(register_vector(&layout), None);
            assert_eq!(vector_wrapper(&layout), None);
        }
        for ty in [VectorType::F64x1, VectorType::F32x4, VectorType::I64x4] {
            let leaf = Layout::Scalar(Type::Vector(ty));
            let empty = Layout::Array(Box::new(v.clone()), u32::MAX);
            // Huge arrays of zero-sized elements retain their structural
            // presence without making leaf expansion depend on array count.
            let zeros = Layout::Array(Box::new(Layout::Array(Box::new(empty), 0)), u32::MAX);
            let layout = Layout::Struct(vec![zeros, leaf]);
            assert_eq!(register_vector(&layout), Some(Type::Vector(ty)));
            assert_eq!(
                vector_wrapper(&layout),
                if ty == VectorType::I64x4 {
                    None
                } else {
                    Some(Type::Vector(ty))
                }
            );
        }
    }
}
