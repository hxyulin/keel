//! Relaxed scalar algebra. Only explicitly marked functions participate.
use crate::ir_match::def;
use keel_ir::{BinaryOp as B, Function, InstData as D, Type, Value};

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

pub(crate) fn run(f: &mut Function, context: &crate::OptimizationContext) -> bool {
    if !f.attrs.fast_math {
        return false;
    }
    let mut changed = false;
    let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    for inst in insts {
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
            for (product, addend) in [(a, b), (b, a)] {
                let Some(mul) = def(f, product) else { continue };
                if let D::Binary {
                    op: B::Fmul,
                    args: factors,
                } = *f.inst_data(mul)
                {
                    *f.inst_data_mut(inst) = D::Fma {
                        args: [factors[0], factors[1], addend],
                    };
                    changed = true;
                    break;
                }
            }
        }
    }
    if changed {
        crate::dce::run(f);
    }
    changed
}
