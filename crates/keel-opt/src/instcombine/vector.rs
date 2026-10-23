//! Bounded, bit-preserving lane rewrites. No memory or architectural operations
//! are inspected or created; floating lanes are transported without arithmetic.
use super::{Fold, Uses, bits};
use keel_ir::{ConvertOp, Function, InstData as D, Type, Value, ValueDef, VectorBinaryOp as B};

fn producer(f: &Function, v: Value) -> Option<&D> {
    let ValueDef::Result(i, 0) = f.value_def(f.resolve(v)) else {
        return None;
    };
    Some(f.inst_data(i))
}

fn splat(f: &Function, v: Value) -> Option<Value> {
    match *producer(f, v)? {
        D::VectorSplat { arg, .. } => Some(f.resolve(arg)),
        _ => None,
    }
}

fn splat_bits(f: &Function, v: Value) -> Option<u128> {
    bits(f, splat(f, v)?)
}

pub(super) fn simplify(f: &Function, data: &D, uses: Uses<'_>) -> Option<Fold> {
    match data {
        D::VectorExtract { arg, lane } => match producer(f, *arg)? {
            D::VectorSplat { arg, .. } => Some(Fold::Value(*arg)),
            D::VectorInsert {
                args,
                lane: inserted,
            } => Some(if lane == inserted {
                Fold::Value(args[1])
            } else {
                Fold::Data(D::VectorExtract {
                    arg: args[0],
                    lane: *lane,
                })
            }),
            D::VectorShuffle { args, lanes } => {
                let Type::Vector(ty) = f.value_type(*arg) else {
                    return None;
                };
                let index = lanes[*lane as usize];
                Some(Fold::Data(D::VectorExtract {
                    arg: args[usize::from(index >= ty.lanes())],
                    lane: index % ty.lanes(),
                }))
            }
            _ => None,
        },
        D::VectorInsert {
            args: [vector, scalar],
            lane,
        } => {
            if splat(f, *vector) == Some(*scalar) {
                return Some(Fold::Value(*vector));
            }
            if let Some(D::VectorExtract {
                arg,
                lane: extracted,
            }) = producer(f, *scalar)
                && f.resolve(*arg) == *vector
                && extracted == lane
            {
                return Some(Fold::Value(*vector));
            }
            if let Some(D::VectorInsert {
                args,
                lane: inserted,
            }) = producer(f, *vector)
                && inserted == lane
            {
                return Some(Fold::Data(D::VectorInsert {
                    args: [f.resolve(args[0]), *scalar],
                    lane: *lane,
                }));
            }
            None
        }
        D::VectorShuffle { args, lanes } => shuffle(f, *args, lanes, uses),
        D::VectorSelect { args: [mask, a, b] } => {
            if a == b {
                return Some(Fold::Value(*a));
            }
            splat_bits(f, *mask).map(|bits| Fold::Value(if bits != 0 { *a } else { *b }))
        }
        D::VectorBinary { op, args: [a, b] } => {
            if a == b && matches!(op, B::And | B::Or) {
                return Some(Fold::Value(*a));
            }
            // Only integer bitwise identities. A select's arbitrary nonzero
            // mask must never be treated as an all-ones bit blend.
            if !matches!(op, B::And | B::Or | B::Xor) {
                return None;
            }
            let Type::Vector(ty) = f.value_type(*a) else {
                return None;
            };
            let ones = super::mask(ty.lane_type());
            for (value, constant) in [(*a, *b), (*b, *a)] {
                match (op, splat_bits(f, constant)) {
                    (B::And, Some(0)) => return Some(Fold::Value(constant)),
                    (B::And, Some(x)) if x == ones => return Some(Fold::Value(value)),
                    (B::Or | B::Xor, Some(0)) => return Some(Fold::Value(value)),
                    (B::Or, Some(x)) if x == ones => return Some(Fold::Value(constant)),
                    _ => {}
                }
            }
            None
        }
        D::Convert {
            op: ConvertOp::Bitcast,
            ty,
            arg,
        } if ty.is_vector() || f.value_type(*arg).is_vector() => {
            if *ty == f.value_type(*arg) {
                return Some(Fold::Value(*arg));
            }
            if let Some(D::Convert {
                op: ConvertOp::Bitcast,
                arg: original,
                ..
            }) = producer(f, *arg)
            {
                let original = f.resolve(*original);
                return Some(if *ty == f.value_type(original) {
                    Fold::Value(original)
                } else {
                    Fold::Data(D::Convert {
                        op: ConvertOp::Bitcast,
                        ty: *ty,
                        arg: original,
                    })
                });
            }
            None
        }
        D::Convert { op, ty, arg } if matches!(op, ConvertOp::SwidenLow | ConvertOp::UwidenLow) => {
            let D::Convert {
                op: inner,
                arg: original,
                ..
            } = *producer(f, *arg)?
            else {
                return None;
            };
            // Sign extending a zero extension is still zero extension. Zero
            // extending a sign extension must retain its intermediate width.
            let combined = match (*op, inner) {
                (_, ConvertOp::UwidenLow) => ConvertOp::UwidenLow,
                (ConvertOp::SwidenLow, ConvertOp::SwidenLow) => ConvertOp::SwidenLow,
                _ => return None,
            };
            Some(Fold::Data(D::Convert {
                op: combined,
                ty: *ty,
                arg: f.resolve(original),
            }))
        }
        _ => None,
    }
}

fn shuffle(f: &Function, args: [Value; 2], lanes: &[u8], uses: Uses<'_>) -> Option<Fold> {
    let Type::Vector(ty) = f.value_type(args[0]) else {
        return None;
    };
    let n = ty.lanes();
    let lane_source = |index: u8| (args[usize::from(index >= n)], index % n);
    if let Some(scalar) = splat(f, lane_source(lanes[0]).0)
        && lanes
            .iter()
            .all(|&i| splat(f, lane_source(i).0) == Some(scalar))
    {
        return Some(Fold::Data(D::VectorSplat {
            ty: Type::Vector(ty),
            arg: scalar,
        }));
    }
    // Flatten exactly one level, only through unshared shuffles. This cannot
    // duplicate a live permutation. At most two source vectors are expressible.
    let mut sources = Vec::with_capacity(2);
    let mut indices = Vec::with_capacity(lanes.len());
    let mut composed = false;
    for &index in lanes {
        let (mut value, mut lane) = lane_source(index);
        if let Some(D::VectorShuffle {
            args: inner,
            lanes: map,
        }) = producer(f, value)
            && uses.single(f, value)
        {
            let i = map[lane as usize];
            value = f.resolve(inner[usize::from(i >= n)]);
            lane = i % n;
            composed = true;
        }
        let slot = match sources.iter().position(|&v| v == value) {
            Some(slot) => slot,
            None if sources.len() < 2 => {
                sources.push(value);
                sources.len() - 1
            }
            None => return None,
        };
        indices.push(lane + slot as u8 * n);
    }
    if indices.iter().copied().eq(0..n) {
        return Some(Fold::Value(sources[0]));
    }
    // Preserve the written order of genuine two-source shuffles unless a
    // composition removed an intermediate permutation (ZIP/window selectors).
    if !composed && sources.len() == 2 {
        return None;
    }
    if composed && sources.len() == 2 {
        // Retain source order from the written producer operands, rather than
        // ordering by the first selected lane. Backend two-source recipes can
        // depend on which input supplies the low and high groups.
        let first = args.into_iter().find_map(|arg| {
            let candidates = match producer(f, arg) {
                Some(D::VectorShuffle { args, .. }) if uses.single(f, arg) => *args,
                _ => [arg, arg],
            };
            candidates
                .into_iter()
                .map(|v| f.resolve(v))
                .find(|v| sources.contains(v))
        });
        if first == Some(sources[1]) {
            sources.swap(0, 1);
            for i in &mut indices {
                *i = if *i < n { *i + n } else { *i - n };
            }
        }
    }
    let new_args = [sources[0], *sources.get(1).unwrap_or(&sources[0])];
    if new_args == args && indices == lanes {
        return None;
    }
    Some(Fold::Data(D::VectorShuffle {
        args: new_args,
        lanes: indices,
    }))
}
