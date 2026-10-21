//! Local instruction combining using the IR's wrapping integer semantics.
//!
//! No reassociation of IEEE floating-point operations is permitted. Division
//! identities require a known nonzero divisor; x/x and 0/x may still trap.
//! A use worklist revisits consumers after substitutions, including when
//! layout order differs from definition order across blocks.
//!
//! Width-aware rules look at most `DEPTH` producers deep for bits that may be
//! set (masks, shifts, extensions, comparisons, counts). They remove masks
//! and shifts that cannot change a value, decide integer comparisons against
//! constants, and turn tests adjacent to a range bound into equality. Shift
//! amounts are reduced modulo the width, as the IR defines. These rules and
//! single-use constant grouping (`iadd`/`isub`/`ptradd`, ...) run only in the
//! worklist; the bounded Fast visit also handles literal extension masks.

mod constant_division;

use crate::const_fold::{self, constant_data as constant};
use keel_ir::{
    BinaryOp, ConvertOp, EntityRef, Function, InsertPoint, Inst, InstData, IntCC, Type, UnaryOp,
    Value, ValueDef,
};

enum Fold {
    Value(Value),
    Data(InstData),
    Multi([InstData; 2]),
}

fn bits(func: &Function, v: Value) -> Option<u128> {
    let ValueDef::Result(inst, _) = func.value_def(func.resolve(v)) else {
        return None;
    };
    let InstData::Iconst { ty, imm } = *func.inst_data(inst) else {
        return None;
    };
    Some((imm as i128 as u128) & mask(ty))
}

fn mask(ty: Type) -> u128 {
    u128::MAX >> (128 - 8 * ty.bytes().unwrap())
}

fn zero(ty: Type) -> Fold {
    Fold::Data(InstData::Iconst { ty, imm: 0 })
}

fn binary(func: &Function, value: Value) -> Option<(BinaryOp, [Value; 2])> {
    let ValueDef::Result(inst, 0) = func.value_def(func.resolve(value)) else {
        return None;
    };
    let InstData::Binary { op, args } = *func.inst_data(inst) else {
        return None;
    };
    Some((op, args.map(|v| func.resolve(v))))
}

/// Use information for profitability gates only. User lists can overcount
/// (stale or removed users) but never undercount, so a shared value is never
/// mistaken for an unshared one. Only the worklist (`Users`) enables the
/// recursive known-bit rules: in Fast they cost about 3% more optimizer
/// instructions on the 44-file C corpus for a 0.2% object-size reduction
/// (2026-10-19).
#[derive(Clone, Copy)]
enum Uses<'a> {
    /// The worklist's user lists. Also enables reciprocal division expansion.
    Users(&'a LazyUsers),
    /// Fast keeps no use information: every value may be shared.
    Unknown,
}

impl Uses<'_> {
    fn single(self, func: &Function, value: Value) -> bool {
        match self {
            Uses::Users(users) => users
                .get(func)
                .len
                .get(value.index())
                .is_some_and(|&n| n == 1),
            Uses::Unknown => false,
        }
    }
}

/// The worklist's user lists, built on first need. Until the worklist first
/// changes the function, only new instructions can appear, so building from
/// the instructions present at entry gives the lists an eager build would.
/// Most functions never fold, nor ask for a single use.
struct LazyUsers {
    users: std::cell::OnceCell<Users>,
    insts: usize,
    values: usize,
}

impl LazyUsers {
    fn new(func: &Function) -> Self {
        Self {
            users: std::cell::OnceCell::new(),
            insts: func.num_insts(),
            values: func.num_values(),
        }
    }

    fn get(&self, func: &Function) -> &Users {
        self.users.get_or_init(|| {
            let mut users = Users::default();
            users.resize(self.values);
            for (_, inst) in func.layout_insts() {
                if inst.index() < self.insts {
                    func.inst_data(inst)
                        .for_each_operand(|v| users.push(func.resolve(v), inst));
                }
            }
            users
        })
    }

    /// Call before the worklist changes the function.
    fn get_mut(&mut self, func: &Function) -> &mut Users {
        self.get(func);
        self.users.get_mut().expect("built")
    }
}

const NIL: u32 = u32::MAX;

/// The users of each value, in insertion order, as linked lists threaded
/// through one node array instead of a vector per value. Only values below
/// the last `resize` have lists.
#[derive(Default)]
struct Users {
    head: Vec<u32>,
    tail: Vec<u32>,
    len: Vec<u32>,
    /// (user, next node)
    nodes: Vec<(Inst, u32)>,
}

impl Users {
    fn resize(&mut self, values: usize) {
        self.head.resize(values, NIL);
        self.tail.resize(values, NIL);
        self.len.resize(values, 0);
    }

    fn push(&mut self, value: Value, user: Inst) {
        let v = value.index();
        let node = u32::try_from(self.nodes.len()).expect("user list overflow");
        self.nodes.push((user, NIL));
        match self.tail[v] {
            NIL => self.head[v] = node,
            last => self.nodes[last as usize].1 = node,
        }
        self.tail[v] = node;
        self.len[v] += 1;
    }

    /// Unlinks every occurrence of `user` from `value`'s list.
    fn remove(&mut self, value: Value, user: Inst) {
        let v = value.index();
        let (mut prev, mut node) = (NIL, self.head[v]);
        while node != NIL {
            let (inst, next) = self.nodes[node as usize];
            if inst == user {
                match prev {
                    NIL => self.head[v] = next,
                    prev => self.nodes[prev as usize].1 = next,
                }
                if next == NIL {
                    self.tail[v] = prev;
                }
                self.len[v] -= 1;
            } else {
                prev = node;
            }
            node = next;
        }
    }

    fn for_each(&self, value: Value, mut f: impl FnMut(Inst)) {
        let mut node = self.head[value.index()];
        while node != NIL {
            let (inst, next) = self.nodes[node as usize];
            f(inst);
            node = next;
        }
    }
}

/// A literal before `inst`, or None if an i128 value is not representable as
/// the IR's sign-extended i64 immediate.
fn literal(func: &mut Function, inst: Inst, ty: Type, bits: u128) -> Option<Value> {
    let bits = bits & mask(ty);
    let imm = bits as i64;
    if imm as i128 as u128 & mask(ty) != bits {
        return None;
    }
    let c = func.insert_inst(InsertPoint::Before(inst), InstData::Iconst { ty, imm });
    func.set_inst_loc(c, func.inst_loc(inst));
    Some(func.first_result(c))
}

/// Producers examined by `possible`: at most 15 for binary trees of this depth.
const DEPTH: u32 = 3;

/// A constant shift or rotate amount, reduced modulo the operated width.
fn count(func: &Function, value: Value, width: u32) -> Option<u32> {
    bits(func, value).map(|n| (n % u128::from(width)) as u32)
}

/// Bits that may be one in integer `value`, looking through at most `depth`
/// producers. Unknown producers and parameters give the full width.
fn possible(func: &Function, value: Value, depth: u32) -> u128 {
    let value = func.resolve(value);
    let ty = func.value_type(value);
    if !ty.is_int() {
        return u128::MAX;
    }
    let full = mask(ty);
    let ValueDef::Result(inst, index) = func.value_def(value) else {
        return full;
    };
    let data = func.inst_data(inst);
    if index != 0 {
        return if matches!(data, InstData::Overflow { .. }) {
            1
        } else {
            full
        };
    }
    let width = 8 * ty.bytes().unwrap();
    let next = |v| possible(func, v, depth - 1);
    match *data {
        InstData::Iconst { imm, .. } => imm as i128 as u128 & full,
        InstData::Icmp { .. } | InstData::Fcmp { .. } => 1,
        _ if depth == 0 => full,
        // Counts are at most the width, which is a power of two.
        InstData::Unary {
            op: UnaryOp::Clz | UnaryOp::Ctz | UnaryOp::Popcnt,
            ..
        } => (u128::MAX >> (127 - width.ilog2())) & full,
        InstData::Convert {
            op: ConvertOp::Uext,
            arg,
            ..
        } => next(arg),
        InstData::Convert {
            op: ConvertOp::Sext,
            arg,
            ..
        } => {
            let from = mask(func.value_type(arg));
            let bits = next(arg);
            if bits & (from ^ (from >> 1)) == 0 {
                bits
            } else {
                bits | (full & !from)
            }
        }
        InstData::Convert {
            op: ConvertOp::Trunc,
            arg,
            ..
        } => next(arg) & full,
        InstData::Binary { op, args: [a, b] } => match op {
            BinaryOp::And => next(a) & next(b),
            BinaryOp::Or | BinaryOp::Xor => next(a) | next(b),
            BinaryOp::Shl | BinaryOp::Ushr => match count(func, b, width) {
                Some(n) if op == BinaryOp::Shl => (next(a) << n) & full,
                Some(n) => next(a) >> n,
                None => full,
            },
            _ => full,
        },
        InstData::Select { args: [_, a, b] } => next(a) | next(b),
        _ => full,
    }
}

/// Removes masks and shifts using bits that cannot be set, and combines
/// constant shift pairs. Shift amounts are reduced modulo the width first.
fn bits_and_shifts(
    func: &mut Function,
    inst: Inst,
    op: BinaryOp,
    a: Value,
    b: Value,
    ty: Type,
) -> Option<Fold> {
    if !ty.is_int() {
        return None;
    }
    let full = mask(ty);
    let width = 8 * ty.bytes().unwrap();
    match op {
        BinaryOp::And => {
            let set = possible(func, a, DEPTH);
            if set & possible(func, b, DEPTH) == 0 {
                return Some(zero(ty));
            }
            let c = bits(func, b)?;
            (set & !c == 0).then_some(Fold::Value(a))
        }
        BinaryOp::Or => {
            let c = bits(func, b)?;
            (possible(func, a, DEPTH) & !c == 0).then_some(Fold::Value(b))
        }
        BinaryOp::Shl | BinaryOp::Ushr | BinaryOp::Sshr => {
            let n = count(func, b, width)?;
            let set = possible(func, a, DEPTH);
            let shifted = if op == BinaryOp::Shl {
                (set << n) & full
            } else {
                set >> n
            };
            if shifted == 0 {
                return Some(zero(ty));
            }
            let (inner, [x, k]) = binary(func, a)?;
            let m = count(func, k, width)?;
            let count_ty = func.value_type(b);
            let combined = |func: &mut Function, op, total: u32| {
                let total = literal(func, inst, count_ty, u128::from(total))?;
                Some(Fold::Data(InstData::Binary {
                    op,
                    args: [x, total],
                }))
            };
            match (inner, op) {
                // Clearing high or low bits is one mask operation.
                (BinaryOp::Shl, BinaryOp::Ushr)
                | (BinaryOp::Ushr | BinaryOp::Sshr, BinaryOp::Shl)
                    if m == n =>
                {
                    let keep = if op == BinaryOp::Ushr {
                        full >> n
                    } else {
                        (full << n) & full
                    };
                    let keep = literal(func, inst, ty, keep)?;
                    Some(Fold::Data(InstData::Binary {
                        op: BinaryOp::And,
                        args: [x, keep],
                    }))
                }
                (BinaryOp::Shl, BinaryOp::Shl) | (BinaryOp::Ushr, BinaryOp::Ushr) => {
                    if m + n >= width {
                        Some(zero(ty))
                    } else {
                        combined(func, op, m + n)
                    }
                }
                (BinaryOp::Sshr, BinaryOp::Sshr) => combined(func, op, (m + n).min(width - 1)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Integer comparisons decided by the operand's possible bits, the type's
/// bounds, or an invertible operation; adjacent ordered tests become equality.
fn compare(func: &mut Function, inst: Inst, cond: IntCC, a: Value, b: Value) -> Option<Fold> {
    let ty = func.value_type(a);
    if !ty.is_int() {
        return None;
    }
    let full = mask(ty);
    let k = bits(func, b);
    if matches!(cond, IntCC::Eq | IntCC::Ne)
        && let Some((op, [x, y])) = binary(func, a)
    {
        // x - y and x ^ y are zero exactly when x == y.
        if k == Some(0) && matches!(op, BinaryOp::Isub | BinaryOp::Xor) {
            let mut data = InstData::Icmp { cond, args: [x, y] };
            crate::canonicalize::normalize(func, &mut data);
            return Some(Fold::Data(data));
        }
        let target = match (op, k, bits(func, y)) {
            (BinaryOp::Iadd, Some(k), Some(c)) => Some(k.wrapping_sub(c)),
            (BinaryOp::Isub, Some(k), Some(c)) => Some(k.wrapping_add(c)),
            (BinaryOp::Xor, Some(k), Some(c)) => Some(k ^ c),
            _ => None,
        };
        if let Some(target) = target.and_then(|t| literal(func, inst, ty, t)) {
            return Some(Fold::Data(InstData::Icmp {
                cond,
                args: [x, target],
            }));
        }
    }
    let k = k?;
    let set = possible(func, a, DEPTH);
    if cond == IntCC::Eq || cond == IntCC::Ne {
        return (k & !set != 0).then(|| truth(cond == IntCC::Ne));
    }
    // Compare in an unsigned key space; signed order is unsigned order with
    // the sign bit flipped. [lo, hi] bounds every value of `a`.
    let signed = matches!(cond, IntCC::Slt | IntCC::Sle | IntCC::Sgt | IntCC::Sge);
    let sign = full ^ (full >> 1);
    let (key, lo, hi) = if !signed {
        (k, 0, set)
    } else if set & sign == 0 {
        (k ^ sign, sign, sign | set)
    } else {
        (k ^ sign, 0, full)
    };
    let decided = match cond {
        IntCC::Slt | IntCC::Ult => (hi < key).then_some(true).or((lo >= key).then_some(false)),
        IntCC::Sle | IntCC::Ule => (hi <= key).then_some(true).or((lo > key).then_some(false)),
        IntCC::Sgt | IntCC::Ugt => (lo > key).then_some(true).or((hi <= key).then_some(false)),
        _ => (lo >= key).then_some(true).or((hi < key).then_some(false)),
    };
    if let Some(value) = decided {
        return Some(truth(value));
    }
    // Undecided: lo < key <= hi for < and >=, lo <= key < hi for <= and >.
    let (cond, bound) = match cond {
        IntCC::Slt | IntCC::Ult if key == lo + 1 => (IntCC::Eq, lo),
        IntCC::Slt | IntCC::Ult if key == hi => (IntCC::Ne, hi),
        IntCC::Sle | IntCC::Ule if key == lo => (IntCC::Eq, lo),
        IntCC::Sle | IntCC::Ule if key + 1 == hi => (IntCC::Ne, hi),
        IntCC::Sgt | IntCC::Ugt if key == lo => (IntCC::Ne, lo),
        IntCC::Sgt | IntCC::Ugt if key + 1 == hi => (IntCC::Eq, hi),
        IntCC::Sge | IntCC::Uge if key == lo + 1 => (IntCC::Ne, lo),
        IntCC::Sge | IntCC::Uge if key == hi => (IntCC::Eq, hi),
        _ => return None,
    };
    let bound = if signed { bound ^ sign } else { bound };
    let bound = if bound == k {
        b
    } else {
        literal(func, inst, ty, bound)?
    };
    Some(Fold::Data(InstData::Icmp {
        cond,
        args: [a, bound],
    }))
}

fn truth(value: bool) -> Fold {
    Fold::Data(InstData::Iconst {
        ty: Type::I8,
        imm: i64::from(value),
    })
}

fn boolean(func: &Function, op: BinaryOp, a: Value, b: Value, ty: Type) -> Option<Fold> {
    if !matches!(op, BinaryOp::And | BinaryOp::Or | BinaryOp::Xor) {
        return None;
    }
    for (a, b) in [(a, b), (b, a)] {
        if let ValueDef::Result(inst, 0) = func.value_def(b)
            && let InstData::Unary {
                op: UnaryOp::Bnot,
                arg,
            } = *func.inst_data(inst)
            && func.resolve(arg) == a
        {
            return Some(if op == BinaryOp::And {
                zero(ty)
            } else {
                Fold::Data(InstData::Iconst { ty, imm: -1 })
            });
        }
        if let Some((inner, args)) = binary(func, b) {
            let other = if args[0] == a {
                Some(args[1])
            } else if args[1] == a {
                Some(args[0])
            } else {
                None
            };
            if let Some(other) = other {
                match (op, inner) {
                    (BinaryOp::And, BinaryOp::Or) | (BinaryOp::Or, BinaryOp::And) => {
                        return Some(Fold::Value(a));
                    }
                    (BinaryOp::Xor, BinaryOp::Xor) => return Some(Fold::Value(other)),
                    (BinaryOp::And, BinaryOp::And) | (BinaryOp::Or, BinaryOp::Or) => {
                        return Some(Fold::Value(b));
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

fn algebra(
    func: &mut Function,
    inst: Inst,
    op: BinaryOp,
    a: Value,
    b: Value,
    ty: Type,
    uses: Uses,
) -> Option<Fold> {
    if op == BinaryOp::Ptradd {
        return offsets(func, inst, op, a, b, uses);
    }
    if !matches!(
        op,
        BinaryOp::Iadd
            | BinaryOp::Isub
            | BinaryOp::Imul
            | BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Xor
    ) {
        return None;
    }
    if op == BinaryOp::Isub {
        if let Some((BinaryOp::Iadd, args)) = binary(func, a) {
            if args[0] == b {
                return Some(Fold::Value(args[1]));
            }
            if args[1] == b {
                return Some(Fold::Value(args[0]));
            }
        }
        if let Some((BinaryOp::Isub, [left, right])) = binary(func, b)
            && a == left
        {
            return Some(Fold::Value(right));
        }
        if let (Some((BinaryOp::Iadd, left)), Some((BinaryOp::Iadd, right))) =
            (binary(func, a), binary(func, b))
        {
            for i in 0..2 {
                for j in 0..2 {
                    if left[i] == right[j] {
                        return Some(Fold::Data(InstData::Binary {
                            op: BinaryOp::Isub,
                            args: [left[1 - i], right[1 - j]],
                        }));
                    }
                }
            }
        }
    }
    if op == BinaryOp::Iadd {
        for (a, b) in [(a, b), (b, a)] {
            if let Some((BinaryOp::Isub, [left, right])) = binary(func, a)
                && b == right
            {
                return Some(Fold::Value(left));
            }
            if let ValueDef::Result(inner, 0) = func.value_def(b)
                && let InstData::Unary {
                    op: UnaryOp::Ineg,
                    arg,
                } = *func.inst_data(inner)
                && func.resolve(arg) == a
            {
                return Some(zero(ty));
            }
        }
    }
    if matches!(op, BinaryOp::Iadd | BinaryOp::Isub) {
        return offsets(func, inst, op, a, b, uses);
    }
    // Reassociation must remove the inner operation, rather than duplicate a
    // shared computation or merely move constants between live expressions.
    // The use count is consulted last, once the shape matches.
    if let Some((inner_op, args)) = binary(func, a)
        && inner_op == op
        && let (Some(c1), Some(c2)) = (bits(func, args[1]), bits(func, b))
        && uses.single(func, a)
    {
        let combined = match op {
            BinaryOp::Imul => c1.wrapping_mul(c2),
            BinaryOp::And => c1 & c2,
            BinaryOp::Or => c1 | c2,
            BinaryOp::Xor => c1 ^ c2,
            _ => unreachable!(),
        };
        let c = literal(func, inst, ty, combined)?;
        return Some(Fold::Data(InstData::Binary {
            op,
            args: [args[0], c],
        }));
    }
    None
}

/// Groups constant integer offsets (`iadd`/`isub`) or constant pointer
/// offsets (`ptradd`) into one, wrapping at the offset's width, under the
/// same single-use gate as other reassociation.
fn offsets(
    func: &mut Function,
    inst: Inst,
    op: BinaryOp,
    a: Value,
    b: Value,
    uses: Uses,
) -> Option<Fold> {
    let offset = |op, c: u128| match op {
        BinaryOp::Iadd | BinaryOp::Ptradd => Some(c),
        BinaryOp::Isub => Some(c.wrapping_neg()),
        _ => None,
    };
    let outer = offset(op, bits(func, b)?)?;
    // The use count is consulted last, once the shape matches.
    let (inner_op, [x, k]) = binary(func, a)?;
    if (inner_op == BinaryOp::Ptradd) != (op == BinaryOp::Ptradd) {
        return None;
    }
    let inner = offset(inner_op, bits(func, k)?)?;
    if !uses.single(func, a) {
        return None;
    }
    let ty = func.value_type(b);
    let m = mask(ty);
    let total = inner.wrapping_add(outer) & m;
    if total == 0 {
        return Some(Fold::Value(x));
    }
    // Integer offsets keep a positive literal, as frontends write them.
    let (op, total) = match op {
        BinaryOp::Ptradd => (op, total),
        _ if total & (m ^ (m >> 1)) != 0 => (BinaryOp::Isub, total.wrapping_neg() & m),
        _ => (BinaryOp::Iadd, total),
    };
    let total = literal(func, inst, ty, total)?;
    Some(Fold::Data(InstData::Binary {
        op,
        args: [x, total],
    }))
}

// Extensions preserve zero/nonzero only when they strictly widen integers.
// Resolve both the producer and its operand: aliases need not be in layout order.
fn widening_source(func: &Function, value: Value) -> Option<Value> {
    let value = func.resolve(value);
    let ValueDef::Result(inst, 0) = func.value_def(value) else {
        return None;
    };
    let InstData::Convert {
        op: ConvertOp::Uext | ConvertOp::Sext,
        ty,
        arg,
    } = *func.inst_data(inst)
    else {
        return None;
    };
    let arg = func.resolve(arg);
    let from = func.value_type(arg);
    (from.is_int() && ty.is_int() && func.value_type(value) == ty && from.bytes() < ty.bytes())
        .then_some(arg)
}

fn invert_integer(cond: IntCC) -> IntCC {
    match cond {
        IntCC::Eq => IntCC::Ne,
        IntCC::Ne => IntCC::Eq,
        IntCC::Slt => IntCC::Sge,
        IntCC::Sle => IntCC::Sgt,
        IntCC::Sgt => IntCC::Sle,
        IntCC::Sge => IntCC::Slt,
        IntCC::Ult => IntCC::Uge,
        IntCC::Ule => IntCC::Ugt,
        IntCC::Ugt => IntCC::Ule,
        IntCC::Uge => IntCC::Ult,
    }
}

fn simplify_branch(func: &Function, inst: Inst) -> Option<InstData> {
    let InstData::Brif { cond, .. } = func.inst_data(inst) else {
        return None;
    };
    let mut source = widening_source(func, *cond)?;
    while let Some(narrower) = widening_source(func, source) {
        source = narrower;
    }
    let mut data = func.inst_data(inst).clone();
    if let InstData::Brif { cond, .. } = &mut data {
        *cond = source;
    }
    crate::canonicalize::normalize(func, &mut data);
    Some(data)
}

fn select(func: &Function, cond: Value, a: Value, b: Value, ty: Type, full: bool) -> Option<Fold> {
    if a == b {
        return Some(Fold::Value(a));
    }
    if let Some(c) = bits(func, cond) {
        return Some(Fold::Value(if c == 0 { b } else { a }));
    }
    if !full {
        return None;
    }
    // The selection already tests nonzero; an equality with zero only
    // decides the arm order. i128 conditions stay compared.
    if let ValueDef::Result(test, 0) = func.value_def(cond)
        && let InstData::Icmp {
            cond: cc @ (IntCC::Eq | IntCC::Ne),
            args: [x, zero],
        } = *func.inst_data(test)
        && bits(func, zero) == Some(0)
        && func.value_type(x).bytes().is_some_and(|n| n <= 8)
        && func.value_type(x).is_int()
    {
        let x = func.resolve(x);
        let args = if cc == IntCC::Ne {
            [x, a, b]
        } else {
            [x, b, a]
        };
        return Some(Fold::Data(InstData::Select { args }));
    }
    // A 0/1 condition selecting 1 or 0 is the condition at the result width.
    let from = func.value_type(cond);
    if ty.is_int()
        && (bits(func, a), bits(func, b)) == (Some(1), Some(0))
        && possible(func, cond, DEPTH) & !1 == 0
    {
        return Some(match ty.bytes().cmp(&from.bytes()) {
            std::cmp::Ordering::Equal => Fold::Value(cond),
            std::cmp::Ordering::Greater => Fold::Data(InstData::Convert {
                op: ConvertOp::Uext,
                ty,
                arg: cond,
            }),
            std::cmp::Ordering::Less => Fold::Data(InstData::Convert {
                op: ConvertOp::Trunc,
                ty,
                arg: cond,
            }),
        });
    }
    None
}

fn simplify(func: &mut Function, inst: Inst, uses: Uses) -> Option<Fold> {
    let result = *func.inst_results(inst).first()?;
    // A stale layout instruction can remain after its result was aliased.
    // Never let a later fold overwrite that existing replacement.
    if func.inst_results(inst).len() == 1
        && !matches!(func.value_def(result), ValueDef::Result(owner, 0) if owner == inst)
    {
        return None;
    }
    // Other opcodes have no rules here; at most canonicalization changes them.
    let data = func.inst_data(inst);
    let normalize = crate::canonicalize::needs_normalize(func, data);
    if !normalize
        && !matches!(
            data,
            InstData::Binary { .. }
                | InstData::Unary { .. }
                | InstData::Convert { .. }
                | InstData::Icmp { .. }
                | InstData::Fcmp { .. }
                | InstData::Fma { .. }
                | InstData::Select { .. }
                | InstData::Overflow { .. }
        )
    {
        return None;
    }
    let mut data = data.clone();
    let canonicalized = normalize && {
        crate::canonicalize::normalize(func, &mut data);
        data != *func.inst_data(inst)
    };

    if let Some(constants) = const_fold::fold_overflow(&data, |v| constant(func, v)) {
        return Some(Fold::Multi(constants));
    }
    let ty = func.value_type(result);
    // Recursive known-bit rules, reassociation and division expansion are
    // measured costs (see `Uses`); Fast keeps only the local rules.
    let full = matches!(uses, Uses::Users(_));
    if let InstData::Select { args: [cond, a, b] } = data
        && let Some(fold) = select(func, cond, a, b, ty, full)
    {
        return Some(fold);
    }
    if !matches!(data, InstData::Iconst { .. } | InstData::Fconst { .. })
        && func.inst_results(inst).len() == 1
        && let Some(folded) = const_fold::fold(&data, |v| constant(func, v))
    {
        return Some(Fold::Data(folded));
    }
    if let InstData::Icmp { cond, args: [a, b] } = data
        && full
        && a != b
        && let Some(fold) = compare(func, inst, cond, a, b)
    {
        return Some(fold);
    }
    match data {
        InstData::Binary { op, args: [a, b] } if !op.is_float() => {
            let (x, y) = (bits(func, a), bits(func, b));
            match op {
                BinaryOp::Iadd | BinaryOp::Or | BinaryOp::Xor if y == Some(0) => {
                    return Some(Fold::Value(a));
                }
                BinaryOp::Iadd | BinaryOp::Or | BinaryOp::Xor if x == Some(0) => {
                    return Some(Fold::Value(b));
                }
                BinaryOp::Isub if y == Some(0) => return Some(Fold::Value(a)),
                BinaryOp::Isub | BinaryOp::Xor if a == b => return Some(zero(ty)),
                BinaryOp::And | BinaryOp::Or if a == b => return Some(Fold::Value(a)),
                BinaryOp::Imul if y == Some(1) => return Some(Fold::Value(a)),
                BinaryOp::Imul if x == Some(1) => return Some(Fold::Value(b)),
                BinaryOp::Imul | BinaryOp::And if x == Some(0) || y == Some(0) => {
                    return Some(zero(ty));
                }
                BinaryOp::And if x == Some(mask(ty)) => return Some(Fold::Value(b)),
                BinaryOp::And if y == Some(mask(ty)) => return Some(Fold::Value(a)),
                BinaryOp::Or if x == Some(mask(ty)) || y == Some(mask(ty)) => {
                    return Some(Fold::Data(InstData::Iconst { ty, imm: -1 }));
                }
                BinaryOp::Udiv | BinaryOp::Sdiv if y == Some(1) => return Some(Fold::Value(a)),
                BinaryOp::Urem | BinaryOp::Srem if y == Some(1) => return Some(zero(ty)),
                // Unlike signed division, MIN % -1 is defined and equals zero.
                BinaryOp::Srem if y == Some(mask(ty)) => return Some(zero(ty)),
                BinaryOp::Shl
                | BinaryOp::Ushr
                | BinaryOp::Sshr
                | BinaryOp::Rotl
                | BinaryOp::Rotr
                    if y.is_some_and(|n| n % (8 * ty.bytes().unwrap()) as u128 == 0) =>
                {
                    return Some(Fold::Value(a));
                }
                BinaryOp::Ptradd if y == Some(0) => return Some(Fold::Value(a)),
                _ => {}
            }
            // A widening zero extension has no possible bits above its source
            // width. The literal's upper bits do not matter to this identity.
            if op == BinaryOp::And && ty.is_int() {
                for (extended, literal) in [(a, b), (b, a)] {
                    if let Some(source) = widening_source(func, extended)
                        && let ValueDef::Result(def, 0) = func.value_def(func.resolve(extended))
                        && matches!(
                            func.inst_data(def),
                            InstData::Convert {
                                op: ConvertOp::Uext,
                                ..
                            }
                        )
                        && func.value_type(extended) == ty
                        && func.value_type(literal) == ty
                        && bits(func, literal).is_some_and(|m| {
                            let source_mask = mask(func.value_type(source));
                            m & source_mask == source_mask
                        })
                    {
                        return Some(Fold::Value(extended));
                    }
                }
            }
            if let Some(fold) = boolean(func, op, a, b, ty) {
                return Some(fold);
            }
            if full && let Some(fold) = bits_and_shifts(func, inst, op, a, b, ty) {
                return Some(fold);
            }
            if full && let Some(fold) = algebra(func, inst, op, a, b, ty, uses) {
                return Some(fold);
            }
            let power = match op {
                BinaryOp::Imul => y.or(x),
                BinaryOp::Udiv | BinaryOp::Urem => y,
                _ => None,
            };
            if let Some(power) = power.filter(|n| n.is_power_of_two()) {
                let shift = power.trailing_zeros();
                let operand = if op == BinaryOp::Imul && y.is_none() {
                    b
                } else {
                    a
                };
                let new_op = match op {
                    BinaryOp::Imul => BinaryOp::Shl,
                    BinaryOp::Udiv => BinaryOp::Ushr,
                    BinaryOp::Urem => {
                        // Remainder by a power of two is a mask; use a
                        // same-width constant and retain the result handle.
                        let c = literal(func, inst, ty, power - 1)?;
                        return Some(Fold::Data(InstData::Binary {
                            op: BinaryOp::And,
                            args: [a, c],
                        }));
                    }
                    _ => return None,
                };
                let c = func.insert_inst(
                    InsertPoint::Before(inst),
                    InstData::Iconst {
                        ty: Type::I64,
                        imm: shift as i64,
                    },
                );
                func.set_inst_loc(c, func.inst_loc(inst));
                return Some(Fold::Data(InstData::Binary {
                    op: new_op,
                    args: [operand, func.first_result(c)],
                }));
            }
            if matches!(op, BinaryOp::Udiv | BinaryOp::Urem)
                && full
                && let Some(divisor) = y
                && let Some(data) = constant_division::reduce(func, inst, op, a, b, ty, divisor)
            {
                return Some(Fold::Data(data));
            }
        }
        InstData::Unary {
            op: UnaryOp::Ineg | UnaryOp::Bnot,
            arg,
        } => {
            let ValueDef::Result(inner, _) = func.value_def(arg) else {
                return None;
            };
            if let InstData::Unary {
                op: inner_op,
                arg: original,
            } = *func.inst_data(inner)
                && let InstData::Unary { op, .. } = data
                && inner_op == op
            {
                return Some(Fold::Value(original));
            }
        }
        InstData::Convert { op, ty, arg } => {
            let from = func.value_type(arg);
            if op == ConvertOp::Sext
                && from.is_int()
                && ty.is_int()
                && from.bytes() < ty.bytes()
                && let Some((BinaryOp::And, args)) = binary(func, arg)
            {
                // Test the sign bit at the AND's width, before widening. No
                // literal is constructed (including for an i128 destination).
                let sign = 1u128 << (8 * from.bytes().unwrap() - 1);
                if args.into_iter().any(|literal| {
                    func.value_type(literal) == from
                        && bits(func, literal).is_some_and(|m| m & sign == 0)
                }) {
                    return Some(Fold::Data(InstData::Convert {
                        op: ConvertOp::Uext,
                        ty,
                        arg,
                    }));
                }
            }
            if matches!(op, ConvertOp::Sext | ConvertOp::Uext | ConvertOp::Trunc)
                && ty == func.value_type(arg)
            {
                return Some(Fold::Value(arg));
            }
            // Truncation ignores an operation that cannot change its low bits.
            if op == ConvertOp::Trunc
                && full
                && let Some((inner, [x, y])) = binary(func, arg)
                && let Some(c) = bits(func, y)
                && match inner {
                    BinaryOp::And => c & mask(ty) == mask(ty),
                    BinaryOp::Or | BinaryOp::Xor => c & mask(ty) == 0,
                    _ => false,
                }
            {
                return Some(Fold::Data(InstData::Convert { op, ty, arg: x }));
            }
            let ValueDef::Result(inner, _) = func.value_def(arg) else {
                return None;
            };
            if let InstData::Convert {
                op: inner_op,
                arg: original,
                ..
            } = *func.inst_data(inner)
                && matches!(op, ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext)
                && matches!(
                    inner_op,
                    ConvertOp::Trunc | ConvertOp::Uext | ConvertOp::Sext
                )
            {
                let original = func.resolve(original);
                let from = func.value_type(original);
                let combined = match (op, inner_op) {
                    (ConvertOp::Trunc, _) if ty == from => return Some(Fold::Value(original)),
                    (ConvertOp::Trunc, _) if ty.bytes() < from.bytes() => Some(ConvertOp::Trunc),
                    (ConvertOp::Trunc, ConvertOp::Uext | ConvertOp::Sext) => Some(inner_op),
                    (ConvertOp::Uext | ConvertOp::Sext, ConvertOp::Uext) => Some(ConvertOp::Uext),
                    (ConvertOp::Sext, ConvertOp::Sext) => Some(ConvertOp::Sext),
                    // Zero-extending a sign extension preserves the intermediate
                    // width's sign bits, not those of the final width.
                    _ => None,
                };
                if let Some(op) = combined {
                    return Some(Fold::Data(InstData::Convert {
                        op,
                        ty,
                        arg: original,
                    }));
                }
            }
        }
        InstData::Icmp { cond, args: [a, b] } if a == b => {
            let truth = matches!(
                cond,
                IntCC::Eq | IntCC::Sle | IntCC::Sge | IntCC::Ule | IntCC::Uge
            );
            return Some(Fold::Data(InstData::Iconst {
                ty: Type::I8,
                imm: i64::from(truth),
            }));
        }
        InstData::Icmp { cond, args: [a, b] } if matches!(cond, IntCC::Eq | IntCC::Ne) => {
            if bits(func, b) == Some(0) {
                if let Some(source) = widening_source(func, a) {
                    let zero = func.insert_inst(
                        InsertPoint::Before(inst),
                        InstData::Iconst {
                            ty: func.value_type(source),
                            imm: 0,
                        },
                    );
                    func.set_inst_loc(zero, func.inst_loc(inst));
                    return Some(Fold::Data(InstData::Icmp {
                        cond,
                        args: [source, func.first_result(zero)],
                    }));
                }
                if func.value_type(a) == Type::I8
                    && let ValueDef::Result(inner, 0) = func.value_def(a)
                    && let InstData::Icmp {
                        cond: inner_cond,
                        args,
                    } = *func.inst_data(inner)
                {
                    // Integer comparisons are canonical i8 0/1. Copying their
                    // payload preserves this result's identity and location,
                    // without changing any other use of the inner comparison.
                    return Some(Fold::Data(InstData::Icmp {
                        cond: if cond == IntCC::Eq {
                            invert_integer(inner_cond)
                        } else {
                            inner_cond
                        },
                        args: args.map(|v| func.resolve(v)),
                    }));
                }
            }
            if let ValueDef::Result(left, 0) = func.value_def(a)
                && let ValueDef::Result(right, 0) = func.value_def(b)
                && let InstData::Convert { op, ty, arg: x } = *func.inst_data(left)
                && matches!(op, ConvertOp::Sext | ConvertOp::Uext)
                && let InstData::Convert {
                    op: right_op,
                    ty: right_ty,
                    arg: y,
                } = *func.inst_data(right)
                && op == right_op
                && ty == right_ty
            {
                let (x, y) = (func.resolve(x), func.resolve(y));
                let from = func.value_type(x);
                // Matching widening extensions are injective. Truncation and
                // mixed sign/zero extensions do not preserve equality this way.
                if from.is_int()
                    && ty.is_int()
                    && from == func.value_type(y)
                    && from.bytes() < ty.bytes()
                {
                    return Some(Fold::Data(InstData::Icmp { cond, args: [x, y] }));
                }
            }
        }
        _ => {}
    }
    canonicalized.then_some(Fold::Data(data))
}

/// Reuse only a quotient already executed earlier in this block. Keeping that
/// division preserves zero-divisor traps and signed MIN/-1 overflow traps;
/// introducing a signed division for a standalone remainder would not.
fn reuse_quotients(func: &mut Function) -> bool {
    // Only a remainder can be rewritten; most functions have none.
    let remainder = func.layout_insts().any(|(_, inst)| {
        matches!(
            func.inst_data(inst),
            InstData::Binary {
                op: BinaryOp::Srem | BinaryOp::Urem,
                ..
            }
        )
    });
    if !remainder {
        return false;
    }
    let mut quotients = keel_ir::fxhash::FxHashMap::default();
    let mut changed = false;
    for block in func.blocks().collect::<Vec<_>>() {
        quotients.clear();
        for inst in func.block_insts(block).to_vec() {
            changed |= reuse_quotient(func, inst, &mut quotients);
        }
    }
    changed
}

pub(crate) type Quotients = keel_ir::fxhash::FxHashMap<(BinaryOp, [Value; 2]), Value>;

/// The caller clears this table at every block boundary and may cap its size.
/// Only an earlier quotient can guard the remainder's zero/overflow behavior.
pub(crate) fn reuse_quotient(func: &mut Function, inst: Inst, quotients: &mut Quotients) -> bool {
    let InstData::Binary { op, args } = *func.inst_data(inst) else {
        return false;
    };
    let args = args.map(|v| func.resolve(v));
    let division = match op {
        BinaryOp::Sdiv | BinaryOp::Udiv => {
            quotients.insert((op, args), func.first_result(inst));
            return false;
        }
        BinaryOp::Srem => BinaryOp::Sdiv,
        BinaryOp::Urem => BinaryOp::Udiv,
        _ => return false,
    };
    let Some(&quotient) = quotients.get(&(division, args)) else {
        return false;
    };
    let product = func.insert_inst(
        InsertPoint::Before(inst),
        InstData::Binary {
            op: BinaryOp::Imul,
            args: [quotient, args[1]],
        },
    );
    func.set_inst_loc(product, func.inst_loc(inst));
    *func.inst_data_mut(inst) = InstData::Binary {
        op: BinaryOp::Isub,
        args: [args[0], func.first_result(product)],
    };
    true
}

pub fn run(func: &mut Function) -> bool {
    let reused_quotients = reuse_quotients(func);
    let mut lazy = LazyUsers::new(func);
    let layout: usize = func.blocks().map(|b| func.block_insts(b).len()).sum();
    let mut work: Vec<Inst> = Vec::with_capacity(layout);
    work.extend(func.layout_insts().map(|(_, i)| i));
    // Start with definitions first within the usual layout; dependency
    // notifications make the final result independent of layout order.
    work.reverse();
    let mut queued = vec![true; func.num_insts()];
    let mut changed = reused_quotients;
    let mut waiting = Vec::new();
    while let Some(inst) = work.pop() {
        queued[inst.index()] = false;
        if func.is_removed(inst) {
            continue;
        }
        if func.inst_results(inst).is_empty() {
            if let Some(data) = simplify_branch(func, inst) {
                let users = lazy.get_mut(func);
                // A terminator has no result users to notify, but its new
                // condition must notify it if that producer changes later.
                func.inst_data(inst)
                    .for_each_operand(|v| users.remove(func.resolve(v), inst));
                *func.inst_data_mut(inst) = data;
                func.inst_data(inst)
                    .for_each_operand(|v| users.push(func.resolve(v), inst));
                changed = true;
            }
            continue;
        }
        let Some(fold) = simplify(func, inst, Uses::Users(&lazy)) else {
            continue;
        };
        let users = lazy.get_mut(func);
        let result = func.first_result(inst);
        waiting.clear();
        for &result in func.inst_results(inst) {
            users.for_each(result, |user| waiting.push(user));
        }
        match fold {
            Fold::Multi(constants) => {
                const_fold::replace_with_constants(func, inst, &constants);
                users.resize(func.num_values());
                queued.resize(func.num_insts(), false);
            }
            Fold::Value(value) => {
                let value = func.resolve(value);
                if value == result {
                    continue;
                }
                func.replace_all_uses(result, value);
                func.remove_inst(inst);
                for &user in &waiting {
                    users.push(value, user);
                }
            }
            Fold::Data(data) => {
                if *func.inst_data(inst) == data {
                    continue;
                }
                *func.inst_data_mut(inst) = data;
                // Strength reduction can create constants with fresh values.
                users.resize(func.num_values());
                func.inst_data(inst)
                    .for_each_operand(|v| users.push(func.resolve(v), inst));
                if queued.len() < func.num_insts() {
                    queued.resize(func.num_insts(), false);
                }
                // Revisit the rewritten instruction (e.g. shift by zero).
                queued[inst.index()] = true;
                work.push(inst);
            }
        }
        for &user in &waiting {
            if !queued[user.index()] {
                queued[user.index()] = true;
                work.push(user);
            }
        }
        changed = true;
    }
    changed
}

/// One visit, with no user graph, reassociation, reciprocal division expansion,
/// recursive known-bit rules, or consumer requeue. Used by the bounded Fast
/// pipeline.
pub(crate) fn simplify_once(func: &mut Function, inst: Inst) -> bool {
    if func.inst_results(inst).is_empty() {
        return false;
    }
    let Some(fold) = simplify(func, inst, Uses::Unknown) else {
        return false;
    };
    match fold {
        Fold::Value(value) => {
            let value = func.resolve(value);
            let result = func.first_result(inst);
            if value == result {
                return false;
            }
            func.replace_all_uses(result, value);
            func.remove_inst(inst);
        }
        Fold::Data(data) => {
            if *func.inst_data(inst) == data {
                return false;
            }
            *func.inst_data_mut(inst) = data;
        }
        Fold::Multi(constants) => const_fold::replace_with_constants(func, inst, &constants),
    }
    true
}
