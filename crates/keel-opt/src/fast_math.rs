//! Relaxed scalar/packed algebra and refined roots for explicitly marked functions.
use crate::ir_match::def;
use keel_ir::{
    BinaryOp as B, Function, InsertPoint, Inst, InstData as D, IntCC, Intrinsic,
    NeonIntrinsic as N, Type, UnaryOp, Value, VectorBinaryOp as V, VectorType,
};

fn constant(f: &Function, value: Value) -> Option<f64> {
    match *f.inst_data(def(f, value)?) {
        D::Fconst {
            ty: Type::F32,
            bits,
        } => Some(f32::from_bits(bits as u32).into()),
        D::Fconst {
            ty: Type::F64,
            bits,
        } => Some(f64::from_bits(bits)),
        _ => None,
    }
}

// The same matcher handles scalar and packed contraction without teaching
// optimizers target-specific accumulating instruction operand order.
fn contract(f: &mut Function, inst: Inst, args: [Value; 2]) -> bool {
    for (product, addend) in [(args[0], args[1]), (args[1], args[0])] {
        let Some(mul) = def(f, product) else { continue };
        let factors = match *f.inst_data(mul) {
            D::Binary { op: B::Fmul, args } | D::VectorBinary { op: V::Fmul, args } => args,
            _ => continue,
        };
        *f.inst_data_mut(inst) = D::Fma {
            args: [factors[0], factors[1], addend],
        };
        return true;
    }
    false
}

fn insert(f: &mut Function, before: Inst, data: D) -> Value {
    let inst = f.insert_inst(InsertPoint::Before(before), data);
    f.set_inst_loc(inst, f.inst_loc(before));
    f.first_result(inst)
}
fn splat_constant(f: &mut Function, data: D, ty: Type) -> Value {
    // Constants are pure and independent of the root's operands. Hoist them
    // before entry work so a late estimate rewrite does not rebuild broadcasts
    // on every vector iteration.
    let before = *f
        .block_insts(f.entry_block().expect("root belongs to a function"))
        .first()
        .expect("root function has an entry instruction");
    let scalar = insert(f, before, data);
    insert(f, before, D::VectorSplat { ty, arg: scalar })
}
fn splat(f: &mut Function, value: f32) -> Value {
    splat_constant(
        f,
        D::Fconst {
            ty: Type::F32,
            bits: u64::from(value.to_bits()),
        },
        Type::Vector(VectorType::F32x4),
    )
}
fn packed(f: &mut Function, before: Inst, op: V, a: Value, b: Value) -> Value {
    insert(f, before, D::VectorBinary { op, args: [a, b] })
}

// Only the reciprocal of a root uses an estimate. General division and
// x/sqrt(y) retain their existing lowering and accuracy/range behavior.
fn reciprocal_root(
    f: &mut Function,
    inst: Inst,
    args: [Value; 2],
    target: crate::VectorTarget,
) -> bool {
    if target == crate::VectorTarget::Portable {
        return false;
    }
    let Some(one) = def(f, args[0]) else {
        return false;
    };
    let D::VectorSplat {
        ty: Type::Vector(VectorType::F32x4),
        arg,
    } = *f.inst_data(one)
    else {
        return false;
    };
    if constant(f, arg) != Some(1.0) {
        return false;
    }
    let Some(root) = def(f, args[1]) else {
        return false;
    };
    let D::VectorUnary {
        op: UnaryOp::Fsqrt,
        arg: input,
    } = *f.inst_data(root)
    else {
        return false;
    };
    let ty = Type::Vector(VectorType::F32x4);
    let answer = if target == crate::VectorTarget::Aarch64Neon {
        let mut r = insert(
            f,
            inst,
            D::Neon {
                op: N::Frsqrte,
                ty,
                args: vec![input],
            },
        );
        for _ in 0..2 {
            // x*r stays representable even when r*r would overflow or underflow.
            let xr = packed(f, inst, V::Fmul, input, r);
            let step = insert(
                f,
                inst,
                D::Neon {
                    op: N::Frsqrts,
                    ty,
                    args: vec![xr, r],
                },
            );
            r = packed(f, inst, V::Fmul, r, step);
        }
        r
    } else {
        // SSE RSQRT treats subnormal inputs as zero. Scale those lanes into
        // the normal range without overflowing the other lanes, then undo the
        // square-root scale. No process-wide DAZ/FTZ mode change is required.
        let int_ty = Type::Vector(VectorType::I32x4);
        let minimum = splat_constant(f, D::iconst(Type::I32, 0x0080_0000), int_ty);
        let bits = insert(
            f,
            inst,
            D::Convert {
                op: keel_ir::ConvertOp::Bitcast,
                ty: int_ty,
                arg: input,
            },
        );
        // Positive IEEE encodings are monotone. Integer comparison keeps this
        // guard packed even on the baseline SSE2 backend.
        let small = insert(
            f,
            inst,
            D::VectorIcmp {
                cond: IntCC::Ult,
                args: [bits, minimum],
            },
        );
        let one = splat(f, 1.0);
        let scale = splat(f, 16777216.0);
        let scale = insert(
            f,
            inst,
            D::VectorSelect {
                args: [small, scale, one],
            },
        );
        let x = packed(f, inst, V::Fmul, input, scale);
        let r = insert(
            f,
            inst,
            D::Intrinsic {
                id: Intrinsic::X86SseRsqrtps128,
                args: vec![x],
            },
        );
        let half = splat(f, 0.5);
        let three = splat(f, 3.0);
        let xr = packed(f, inst, V::Fmul, x, r);
        let xrr = packed(f, inst, V::Fmul, xr, r);
        let correction = packed(f, inst, V::Fsub, three, xrr);
        let half_r = packed(f, inst, V::Fmul, half, r);
        let refined = packed(f, inst, V::Fmul, half_r, correction);
        let adjustment = splat(f, 4096.0);
        let adjustment = insert(
            f,
            inst,
            D::VectorSelect {
                args: [small, adjustment, one],
            },
        );
        packed(f, inst, V::Fmul, refined, adjustment)
    };
    f.replace_all_uses(f.first_result(inst), answer);
    true
}

pub(crate) fn run(f: &mut Function, context: &crate::OptimizationContext) -> bool {
    if !f.attrs.fast_math {
        return false;
    }
    let mut changed = false;
    let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    for inst in insts {
        if let D::VectorBinary { op: V::Fdiv, args } = *f.inst_data(inst)
            && reciprocal_root(f, inst, args.map(|v| f.resolve(v)), context.target)
        {
            changed = true;
            continue;
        }
        if let D::VectorBinary { op: V::Fadd, args } = *f.inst_data(inst)
            && context.target == crate::VectorTarget::Aarch64Neon
            && matches!(
                f.value_type(f.resolve(args[0])),
                Type::Vector(VectorType::F32x4 | VectorType::F64x2)
            )
        {
            changed |= contract(f, inst, args.map(|v| f.resolve(v)));
            continue;
        }
        let D::Binary { op, args: [a, b] } = *f.inst_data(inst) else {
            continue;
        };
        let a = f.resolve(a);
        let b = f.resolve(b);
        let ty = f.value_type(a);
        if !matches!(ty, Type::F32 | Type::F64) {
            continue;
        }
        let result = f.first_result(inst);
        // Ignore dead aliases left by another rewrite.
        if f.resolve(result) != result {
            continue;
        }
        let ca = constant(f, a);
        let cb = constant(f, b);
        let alias = match op {
            B::Fadd if cb == Some(0.0) => Some(a),
            B::Fadd if ca == Some(0.0) => Some(b),
            B::Fsub if cb == Some(0.0) => Some(a),
            B::Fmul if cb == Some(1.0) => Some(a),
            B::Fmul if ca == Some(1.0) => Some(b),
            B::Fdiv if cb == Some(1.0) => Some(a),
            B::Fmul if cb == Some(0.0) => Some(b),
            B::Fmul if ca == Some(0.0) => Some(a),
            _ => None,
        };
        if let Some(value) = alias {
            f.replace_all_uses(result, value);
            changed = true;
            continue;
        }
        if a == b && matches!(op, B::Fsub | B::Fdiv) {
            let value = if op == B::Fdiv { 1.0f64 } else { 0.0 };
            *f.inst_data_mut(inst) = D::Fconst {
                ty,
                bits: if ty == Type::F32 {
                    u64::from((value as f32).to_bits())
                } else {
                    value.to_bits()
                },
            };
            changed = true;
            continue;
        }
        // AArch64 has native scalar FMA in its baseline. Do not turn a cheap
        // x86 multiply/add into a library call on machines without FMA.
        if op == B::Fadd && context.target == crate::VectorTarget::Aarch64Neon {
            changed |= contract(f, inst, [a, b]);
        }
    }
    if changed {
        crate::dce::run(f);
    }
    changed
}
