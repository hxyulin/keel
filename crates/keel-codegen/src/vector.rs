//! Target-independent vector lane legalization and canonical-mask proofs.
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, MemoryFlags, Type, Value,
    ValueDef, VectorBinaryOp as V, VectorType,
};
use std::borrow::Cow;

/// Whether each lane is provably zero or all ones, resolving SSA aliases.
/// Signed widening preserves this property; unsigned widening does not:
/// e.g. an i32 true mask becomes 0x00000000ffffffff in an i64 lane.
/// Bitcasts and block parameters remain unproven, even when their inputs
/// happen to be comparisons.
pub(crate) fn canonical_mask(func: &Function, mut value: Value) -> bool {
    // Integer vector lanes can widen at most three times (8 -> 16 -> 32 -> 64).
    for _ in 0..4 {
        value = func.resolve(value);
        let ValueDef::Result(inst, _) = func.value_def(value) else {
            return false;
        };
        match *func.inst_data(inst) {
            InstData::VectorIcmp { .. } => return true,
            InstData::Convert {
                op: ConvertOp::SwidenLow,
                arg,
                ..
            } => value = arg,
            _ => return false,
        }
    }
    false
}

fn needs_lowering(
    func: &Function,
    data: &InstData,
    keep: &impl Fn(&Function, &InstData) -> bool,
) -> bool {
    // Ask the target only about vector instructions: most are scalar.
    let vector = match data {
        InstData::Fma { args } => func.value_type(func.resolve(args[0])).is_vector(),
        InstData::Convert {
            op:
                ConvertOp::Bitcast
                | ConvertOp::SwidenLow
                | ConvertOp::UwidenLow
                | ConvertOp::SwidenHigh
                | ConvertOp::UwidenHigh
                | ConvertOp::NarrowLow
                | ConvertOp::SpairwiseAddLong
                | ConvertOp::UpairwiseAddLong
                | ConvertOp::SnarrowSatLow
                | ConvertOp::UnarrowSatLow
                | ConvertOp::SunarrowSatLow,
            ty,
            arg,
        } => ty.is_vector() || func.value_type(*arg).is_vector(),
        d => d.is_vector_instruction(),
    };
    vector && !keep(func, data)
}

pub(crate) fn lower(
    func: &Function,
    keep: impl Fn(&Function, &InstData) -> bool,
) -> Cow<'_, Function> {
    if !func
        .layout_insts()
        .any(|(_, i)| needs_lowering(func, func.inst_data(i), &keep))
    {
        return Cow::Borrowed(func);
    }
    let mut f = func.clone();
    loop {
        let (next, expanded) = lower_round(f, &keep);
        f = next;
        // Match the recursive entry scan, including its short-circuiting keep
        // calls on the current IR. A compound expansion can create more work.
        if !expanded
            || !f
                .layout_insts()
                .any(|(_, i)| needs_lowering(&f, f.inst_data(i), &keep))
        {
            return Cow::Owned(f);
        }
    }
}

fn lower_round(mut f: Function, keep: &impl Fn(&Function, &InstData) -> bool) -> (Function, bool) {
    // Scratch slots belong to one round: sharing them across rounds changes
    // their numbering and the lifetime of captured lane inputs.
    // Inputs are captured before any scalar result is produced. Reuse these
    // buffers across straight-line expansions; no optimizer runs after this pass.
    let mut slots_by_width: [[Option<keel_ir::StackSlot>; 4]; 3] = [[None; 4]; 3];
    let mut expanded = false;
    let work: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    for inst in work {
        let data = f.inst_data(inst).clone();
        if !needs_lowering(&f, &data, keep) {
            continue;
        }
        if let InstData::Convert {
            op:
                op @ (ConvertOp::SnarrowSatLow | ConvertOp::UnarrowSatLow | ConvertOp::SunarrowSatLow),
            ty,
            arg,
        } = data
        {
            let source = f.value_type(arg);
            let width = ty.vector().unwrap().lane_type().bytes().unwrap() * 8;
            let mut l = LaneLowering {
                func: &mut f,
                at: inst,
            };
            let signed_result = op == ConvertOp::SnarrowSatLow;
            let signed_source = op != ConvertOp::UnarrowSatLow;
            let min = l.integer(
                source,
                if signed_result {
                    -(1i64 << (width - 1))
                } else {
                    0
                },
            );
            let max = l.integer(
                source,
                if signed_result {
                    (1i64 << (width - 1)) - 1
                } else {
                    ((1u64 << width) - 1) as i64
                },
            );
            let below = l.cmp(
                if signed_source {
                    IntCC::Slt
                } else {
                    IntCC::Ult
                },
                arg,
                min,
            );
            let value = l.choose(below, min, arg);
            let above = l.cmp(
                if signed_source {
                    IntCC::Sgt
                } else {
                    IntCC::Ugt
                },
                value,
                max,
            );
            let value = l.choose(above, max, value);
            let result = l.emit(InstData::Convert {
                op: ConvertOp::NarrowLow,
                ty,
                arg: value,
            });
            let old = f.first_result(inst);
            f.replace_all_uses(old, result);
            f.remove_inst(inst);
            expanded = true;
            continue;
        }
        if let InstData::VectorBinary { op, args } = data
            && let Some(op) = op.pairwise_binary()
        {
            let shape = f.value_type(args[0]).vector().unwrap();
            let mut l = LaneLowering {
                func: &mut f,
                at: inst,
            };
            let a = l.emit(InstData::VectorShuffle {
                args,
                lanes: (0..shape.lanes()).map(|i| i * 2).collect(),
            });
            let b = l.emit(InstData::VectorShuffle {
                args,
                lanes: (0..shape.lanes()).map(|i| i * 2 + 1).collect(),
            });
            let result = l.emit(InstData::VectorBinary { op, args: [a, b] });
            let old = f.first_result(inst);
            f.replace_all_uses(old, result);
            f.remove_inst(inst);
            expanded = true;
            continue;
        }
        if let InstData::VectorBinary { op, args } = data
            && matches!(
                op,
                V::Smin
                    | V::Smax
                    | V::Umin
                    | V::Umax
                    | V::Sabd
                    | V::Uabd
                    | V::Shadd
                    | V::Uhadd
                    | V::Srhadd
                    | V::Urhadd
                    | V::Shsub
                    | V::Uhsub
                    | V::SaddSat
                    | V::UaddSat
                    | V::SsubSat
                    | V::UsubSat
            )
        {
            let mut l = LaneLowering {
                func: &mut f,
                at: inst,
            };
            let result = l.binary(op, args[0], args[1]);
            let old = f.first_result(inst);
            f.replace_all_uses(old, result);
            f.remove_inst(inst);
            expanded = true;
            continue;
        }
        if let InstData::Convert {
            op: op @ (ConvertOp::SpairwiseAddLong | ConvertOp::UpairwiseAddLong),
            ty,
            arg,
        } = data
        {
            let source = f.value_type(arg).vector().unwrap();
            let mut l = LaneLowering {
                func: &mut f,
                at: inst,
            };
            let mut values = Vec::new();
            for odd in [0, 1] {
                let shuffled = l.emit(InstData::VectorShuffle {
                    args: [arg, arg],
                    lanes: (0..source.lanes())
                        .map(|i| (2 * i + odd) % source.lanes())
                        .collect(),
                });
                values.push(l.emit(InstData::Convert {
                    op: if op == ConvertOp::SpairwiseAddLong {
                        ConvertOp::SwidenLow
                    } else {
                        ConvertOp::UwidenLow
                    },
                    ty,
                    arg: shuffled,
                }));
            }
            let result = l.emit(InstData::VectorBinary {
                op: V::Iadd,
                args: [values[0], values[1]],
            });
            let old = f.first_result(inst);
            f.replace_all_uses(old, result);
            f.remove_inst(inst);
            expanded = true;
            continue;
        }
        let used = match data {
            InstData::Convert {
                op:
                    ConvertOp::SwidenLow
                    | ConvertOp::UwidenLow
                    | ConvertOp::SwidenHigh
                    | ConvertOp::UwidenHigh
                    | ConvertOp::NarrowLow
                    | ConvertOp::SpairwiseAddLong
                    | ConvertOp::UpairwiseAddLong,
                ..
            } => [true, false, false, true],
            InstData::Convert { .. }
            | InstData::VectorExtract { .. }
            | InstData::VectorReduce { .. } => [true, false, false, false],
            InstData::VectorSplat { .. } => [false, false, false, true],
            InstData::VectorUnary { .. } | InstData::VectorInsert { .. } => {
                [true, false, false, true]
            }
            InstData::VectorSelect { .. } | InstData::Fma { .. } => [true; 4],
            _ => [true, true, false, true],
        };
        let mut bytes = 8;
        data.for_each_operand(|v| {
            if let Some(t) = f.value_type(v).vector() {
                bytes = bytes.max(t.bytes());
            }
        });
        for &v in f.inst_results(inst) {
            if let Some(t) = f.value_type(v).vector() {
                bytes = bytes.max(t.bytes());
            }
        }
        let slots = &mut slots_by_width[match bytes {
            8 => 0,
            16 => 1,
            32 => 2,
            _ => unreachable!(),
        }];
        for i in 0..4 {
            if used[i] && slots[i].is_none() {
                slots[i] = Some(f.create_stack_slot(bytes, bytes));
            }
        }
        let mut l = LaneLowering {
            func: &mut f,
            at: inst,
        };
        let first = used.iter().position(|u| *u).unwrap();
        let pointer = l.emit(InstData::StackAddr {
            slot: slots[first].unwrap(),
        });
        let mut pointers = [pointer; 4];
        for i in first + 1..4 {
            if used[i] {
                pointers[i] = l.emit(InstData::StackAddr {
                    slot: slots[i].unwrap(),
                });
            }
        }
        let result = match data {
            InstData::Fma { args } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                for i in 0..3 {
                    l.store(pointers[i], args[i]);
                }
                for i in 0..shape.lanes() {
                    let a = l.lane(pointers[0], i, shape);
                    let b = l.lane(pointers[1], i, shape);
                    let c = l.lane(pointers[2], i, shape);
                    let value = l.emit(InstData::Fma { args: [a, b, c] });
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::Convert {
                op:
                    op @ (ConvertOp::SwidenLow
                    | ConvertOp::UwidenLow
                    | ConvertOp::SwidenHigh
                    | ConvertOp::UwidenHigh),
                ty,
                arg,
            } => {
                let source = l.ty(arg).vector().unwrap();
                let dest = ty.vector().unwrap();
                l.store(pointers[0], arg);
                for i in 0..dest.lanes() {
                    let lane = if matches!(op, ConvertOp::SwidenHigh | ConvertOp::UwidenHigh) {
                        source.lanes() - dest.lanes() + i
                    } else {
                        i
                    };
                    let value = l.lane(pointers[0], lane, source);
                    let value = l.emit(InstData::Convert {
                        op: if matches!(op, ConvertOp::SwidenLow | ConvertOp::SwidenHigh) {
                            ConvertOp::Sext
                        } else {
                            ConvertOp::Uext
                        },
                        ty: dest.lane_type(),
                        arg: value,
                    });
                    let at = l.address(pointers[3], i, dest);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::Convert {
                op: op @ (ConvertOp::SpairwiseAddLong | ConvertOp::UpairwiseAddLong),
                ty,
                arg,
            } => {
                let source = l.ty(arg).vector().unwrap();
                let dest = ty.vector().unwrap();
                l.store(pointers[0], arg);
                for i in 0..dest.lanes() {
                    let a = l.lane(pointers[0], i * 2, source);
                    let b = l.lane(pointers[0], i * 2 + 1, source);
                    let extend = if op == ConvertOp::SpairwiseAddLong {
                        ConvertOp::Sext
                    } else {
                        ConvertOp::Uext
                    };
                    let a = l.emit(InstData::Convert {
                        op: extend,
                        ty: dest.lane_type(),
                        arg: a,
                    });
                    let b = l.emit(InstData::Convert {
                        op: extend,
                        ty: dest.lane_type(),
                        arg: b,
                    });
                    let value = l.bin(BinaryOp::Iadd, a, b);
                    let at = l.address(pointers[3], i, dest);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::Convert {
                op: ConvertOp::NarrowLow,
                ty,
                arg,
            } => {
                let source = l.ty(arg).vector().unwrap();
                let dest = ty.vector().unwrap();
                l.store(pointers[0], arg);
                for i in 0..dest.lanes() {
                    let value = if i < source.lanes() {
                        let arg = l.lane(pointers[0], i, source);
                        l.emit(InstData::Convert {
                            op: ConvertOp::Trunc,
                            ty: dest.lane_type(),
                            arg,
                        })
                    } else {
                        l.integer(dest.lane_type(), 0)
                    };
                    let at = l.address(pointers[3], i, dest);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::Convert { ty, arg, .. } => {
                l.store(pointers[0], arg);
                l.load(pointers[0], ty)
            }
            InstData::VectorSplat { ty, arg } => {
                let shape = ty.vector().unwrap();
                for i in 0..shape.lanes() {
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, arg);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorReduce { op, arg } => {
                let shape = l.ty(arg).vector().unwrap();
                l.store(pointers[0], arg);
                let mut value = l.lane(pointers[0], 0, shape);
                for i in 1..shape.lanes() {
                    let b = l.lane(pointers[0], i, shape);
                    value = l.binary(op.binary(), value, b);
                }
                value
            }
            InstData::VectorExtract { arg, lane } => {
                let shape = l.ty(arg).vector().unwrap();
                l.store(pointers[0], arg);
                l.lane(pointers[0], lane, shape)
            }
            InstData::VectorInsert { args, lane } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                l.store(pointers[0], args[0]);
                for i in 0..shape.lanes() {
                    let value = if i == lane {
                        args[1]
                    } else {
                        l.lane(pointers[0], i, shape)
                    };
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorShuffle { args, lanes } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                l.store(pointers[0], args[0]);
                l.store(pointers[1], args[1]);
                for (i, index) in lanes.into_iter().enumerate() {
                    let value = l.lane(
                        pointers[usize::from(index / shape.lanes())],
                        index % shape.lanes(),
                        shape,
                    );
                    let at = l.address(pointers[3], i as u8, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorUnary { op, arg } => {
                let ty = l.ty(arg);
                let shape = ty.vector().unwrap();
                l.store(pointers[0], arg);
                for i in 0..shape.lanes() {
                    let a = l.lane(pointers[0], i, shape);
                    let value = l.emit(InstData::Unary { op, arg: a });
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorSelect { args } => {
                let ty = l.ty(args[1]);
                let shape = ty.vector().unwrap();
                for i in 0..3 {
                    l.store(pointers[i], args[i]);
                }
                for i in 0..shape.lanes() {
                    let mask = l.lane(pointers[0], i, shape.mask());
                    let a = l.lane(pointers[1], i, shape);
                    let b = l.lane(pointers[2], i, shape);
                    let value = l.emit(InstData::Select { args: [mask, a, b] });
                    let at = l.address(pointers[3], i, shape);
                    l.store(at, value);
                }
                l.load(pointers[3], ty)
            }
            InstData::VectorBinary { args, .. }
            | InstData::VectorIcmp { args, .. }
            | InstData::VectorFcmp { args, .. } => {
                let ty = l.ty(args[0]);
                let shape = ty.vector().unwrap();
                let compare = matches!(
                    data,
                    InstData::VectorIcmp { .. } | InstData::VectorFcmp { .. }
                );
                let out_shape = if compare { shape.mask() } else { shape };
                l.store(pointers[0], args[0]);
                l.store(pointers[1], args[1]);
                for i in 0..shape.lanes() {
                    let a = l.lane(pointers[0], i, shape);
                    let b = l.lane(pointers[1], i, shape);
                    let scalar = match data {
                        InstData::VectorBinary { op, .. } => {
                            let value = l.binary(op, a, b);
                            let at = l.address(pointers[3], i, out_shape);
                            l.store(at, value);
                            continue;
                        }
                        InstData::VectorIcmp { cond, .. } => InstData::Icmp { cond, args: [a, b] },
                        InstData::VectorFcmp { cond, .. } => InstData::Fcmp { cond, args: [a, b] },
                        _ => unreachable!(),
                    };
                    let mut value = l.emit(scalar);
                    if compare {
                        let ty = out_shape.lane_type();
                        let yes = l.emit(InstData::Iconst { ty, imm: -1 });
                        let no = l.emit(InstData::Iconst { ty, imm: 0 });
                        value = l.emit(InstData::Select {
                            args: [value, yes, no],
                        });
                    }
                    let at = l.address(pointers[3], i, out_shape);
                    l.store(at, value);
                }
                l.load(pointers[3], Type::Vector(out_shape))
            }
            _ => unreachable!("unhandled vector legalization: {data:?}"),
        };
        let old = f.first_result(inst);
        f.replace_all_uses(old, result);
        f.remove_inst(inst);
    }
    debug_assert!(keel_ir::verify(&f).is_ok());
    (f, expanded)
}
struct LaneLowering<'a> {
    func: &'a mut Function,
    at: Inst,
}
impl LaneLowering<'_> {
    fn integer(&mut self, ty: Type, imm: i64) -> Value {
        if let Some(shape) = ty.vector() {
            let arg = self.emit(InstData::Iconst {
                ty: shape.lane_type(),
                imm,
            });
            self.emit(InstData::VectorSplat { ty, arg })
        } else {
            self.emit(InstData::Iconst { ty, imm })
        }
    }
    fn bin(&mut self, op: BinaryOp, a: Value, b: Value) -> Value {
        if self.ty(a).is_vector() {
            let op = match op {
                BinaryOp::Iadd => V::Iadd,
                BinaryOp::Isub => V::Isub,
                BinaryOp::And => V::And,
                BinaryOp::Or => V::Or,
                BinaryOp::Xor => V::Xor,
                BinaryOp::Ushr => V::Ushr,
                BinaryOp::Sshr => V::Sshr,
                _ => unreachable!("portable compound vector arithmetic"),
            };
            self.emit(InstData::VectorBinary { op, args: [a, b] })
        } else {
            self.emit(InstData::Binary { op, args: [a, b] })
        }
    }
    fn cmp(&mut self, cond: IntCC, a: Value, b: Value) -> Value {
        self.emit(if self.ty(a).is_vector() {
            InstData::VectorIcmp { cond, args: [a, b] }
        } else {
            InstData::Icmp { cond, args: [a, b] }
        })
    }
    fn choose(&mut self, c: Value, a: Value, b: Value) -> Value {
        self.emit(if self.ty(a).is_vector() {
            InstData::VectorSelect { args: [c, a, b] }
        } else {
            InstData::Select { args: [c, a, b] }
        })
    }
    fn binary(&mut self, op: V, a: Value, b: Value) -> Value {
        let ty = self.ty(a);
        match op {
            V::Smin | V::Smax | V::Umin | V::Umax | V::Sabd | V::Uabd => {
                let cc = match op {
                    V::Smin | V::Sabd => IntCC::Slt,
                    V::Smax => IntCC::Sgt,
                    V::Umin | V::Uabd => IntCC::Ult,
                    V::Umax => IntCC::Ugt,
                    _ => unreachable!(),
                };
                let c = self.cmp(cc, a, b);
                if matches!(op, V::Sabd | V::Uabd) {
                    let ab = self.bin(BinaryOp::Isub, a, b);
                    let ba = self.bin(BinaryOp::Isub, b, a);
                    self.choose(c, ba, ab)
                } else {
                    self.choose(c, a, b)
                }
            }
            V::Shadd | V::Uhadd | V::Srhadd | V::Urhadd | V::Shsub | V::Uhsub => {
                // Bit identities retain the carry/borrow discarded by a W-bit
                // add/sub. No W+1 intermediate or undefined scalar overflow.
                let one = self.integer(ty, 1);
                let shift = if matches!(op, V::Shadd | V::Srhadd | V::Shsub) {
                    BinaryOp::Sshr
                } else {
                    BinaryOp::Ushr
                };
                if matches!(op, V::Shsub | V::Uhsub) {
                    let ah = self.bin(shift, a, one);
                    let bh = self.bin(shift, b, one);
                    let diff = self.bin(BinaryOp::Isub, ah, bh);
                    let na = self.emit(if ty.is_vector() {
                        InstData::VectorUnary {
                            op: keel_ir::UnaryOp::Bnot,
                            arg: a,
                        }
                    } else {
                        InstData::Unary {
                            op: keel_ir::UnaryOp::Bnot,
                            arg: a,
                        }
                    });
                    let borrow = self.bin(BinaryOp::And, na, b);
                    let borrow = self.bin(BinaryOp::And, borrow, one);
                    self.bin(BinaryOp::Isub, diff, borrow)
                } else {
                    let xor = self.bin(BinaryOp::Xor, a, b);
                    let half = self.bin(shift, xor, one);
                    if matches!(op, V::Srhadd | V::Urhadd) {
                        let or = self.bin(BinaryOp::Or, a, b);
                        self.bin(BinaryOp::Isub, or, half)
                    } else {
                        let and = self.bin(BinaryOp::And, a, b);
                        self.bin(BinaryOp::Iadd, and, half)
                    }
                }
            }
            V::SaddSat | V::UaddSat | V::SsubSat | V::UsubSat => {
                let sub = matches!(op, V::SsubSat | V::UsubSat);
                let r = self.bin(if sub { BinaryOp::Isub } else { BinaryOp::Iadd }, a, b);
                let zero = self.integer(ty, 0);
                if matches!(op, V::UaddSat | V::UsubSat) {
                    let c = if sub {
                        self.cmp(IntCC::Ult, a, b)
                    } else {
                        self.cmp(IntCC::Ult, r, a)
                    };
                    let limit = self.integer(ty, if sub { 0 } else { -1 });
                    self.choose(c, limit, r)
                } else {
                    let ar = self.bin(BinaryOp::Xor, a, r);
                    let other = self.bin(
                        BinaryOp::Xor,
                        if sub { a } else { b },
                        if sub { b } else { r },
                    );
                    let overflow = self.bin(BinaryOp::And, ar, other);
                    let c = self.cmp(IntCC::Slt, overflow, zero);
                    let sign = self.cmp(IntCC::Slt, a, zero);
                    let min = self.integer(
                        ty,
                        1i64.wrapping_shl(
                            ty.vector().map_or(ty, |s| s.lane_type()).bytes().unwrap() * 8 - 1,
                        ),
                    );
                    let max = self.integer(
                        ty,
                        (1u64.wrapping_shl(
                            ty.vector().map_or(ty, |s| s.lane_type()).bytes().unwrap() * 8 - 1,
                        ) - 1) as i64,
                    );
                    let limit = self.choose(sign, min, max);
                    self.choose(c, limit, r)
                }
            }
            _ => self.bin(scalar_op(op), a, b),
        }
    }
    fn emit(&mut self, data: InstData) -> Value {
        let loc = self.func.inst_loc(self.at);
        let i = self.func.insert_inst(InsertPoint::Before(self.at), data);
        self.func.set_inst_loc(i, loc);
        self.func.first_result(i)
    }
    fn ty(&self, v: Value) -> Type {
        self.func.value_type(v)
    }
    fn store(&mut self, addr: Value, value: Value) {
        let flags = MemoryFlags {
            notrap: true,
            align: self.ty(value).bytes().unwrap(),
            ..MemoryFlags::default()
        };
        let loc = self.func.inst_loc(self.at);
        let i = self.func.insert_inst(
            InsertPoint::Before(self.at),
            InstData::Store { flags, value, addr },
        );
        self.func.set_inst_loc(i, loc);
    }
    fn load(&mut self, addr: Value, ty: Type) -> Value {
        self.emit(InstData::Load {
            flags: MemoryFlags {
                notrap: true,
                align: ty.bytes().unwrap(),
                ..MemoryFlags::default()
            },
            ty,
            addr,
        })
    }
    fn address(&mut self, p: Value, lane: u8, shape: VectorType) -> Value {
        if lane == 0 {
            return p;
        }
        let offset = self.emit(InstData::Iconst {
            ty: Type::I64,
            imm: i64::from(lane) * i64::from(shape.lane_type().bytes().unwrap()),
        });
        self.emit(InstData::Binary {
            op: BinaryOp::Ptradd,
            args: [p, offset],
        })
    }
    fn lane(&mut self, p: Value, lane: u8, shape: VectorType) -> Value {
        let at = self.address(p, lane, shape);
        self.load(at, shape.lane_type())
    }
}
fn scalar_op(op: V) -> BinaryOp {
    match op {
        V::Iadd => BinaryOp::Iadd,
        V::Isub => BinaryOp::Isub,
        V::Imul => BinaryOp::Imul,
        V::And => BinaryOp::And,
        V::Or => BinaryOp::Or,
        V::Xor => BinaryOp::Xor,
        V::Shl => BinaryOp::Shl,
        V::Ushr => BinaryOp::Ushr,
        V::Sshr => BinaryOp::Sshr,
        V::Fadd => BinaryOp::Fadd,
        V::Fsub => BinaryOp::Fsub,
        V::Fmul => BinaryOp::Fmul,
        V::Fdiv => BinaryOp::Fdiv,
        V::Fmin => BinaryOp::Fmin,
        V::Fmax => BinaryOp::Fmax,
        V::Fcopysign => BinaryOp::Fcopysign,
        _ => unreachable!("compound vector operation is expanded explicitly"),
    }
}
