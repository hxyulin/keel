//! Bounded, bit-preserving lane rewrites. No memory or architectural operations
//! are inspected or created; floating lanes are transported without arithmetic.
use super::{Fold, Uses, bits};
use keel_ir::{
    BinaryOp, ConvertOp, Function, InstData as D, Type, Value, ValueDef, VectorBinaryOp as B,
    VectorReduceOp, VectorType,
};

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
        D::Binary {
            op: BinaryOp::Iadd,
            args,
        } => wrapping_sum(f, *args, uses),
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

// Pilot for the C u32 lane sum. Inspect at most a four-leaf add tree, with
// no casts, seeds, repeated/missing lanes or other scalar operations. All
// intermediate sums and extracts must become dead; Fast has no use proof.
// i32 addition and vreduce_add both wrap modulo 2^32. No FP is inspected.
fn wrapping_sum(f: &Function, args: [Value; 2], uses: Uses<'_>) -> Option<Fold> {
    fn visit(
        f: &Function,
        value: Value,
        uses: Uses<'_>,
        depth: u8,
        source: &mut Option<Value>,
        lanes: &mut u8,
    ) -> Option<()> {
        let value = f.resolve(value);
        if depth == 0 || f.value_type(value) != Type::I32 || !uses.single(f, value) {
            return None;
        }
        match *producer(f, value)? {
            D::Binary {
                op: BinaryOp::Iadd,
                args,
            } => {
                visit(f, args[0], uses, depth - 1, source, lanes)?;
                visit(f, args[1], uses, depth - 1, source, lanes)
            }
            D::VectorExtract { arg, lane } => {
                let arg = f.resolve(arg);
                if f.value_type(arg) != Type::Vector(VectorType::I32x4)
                    || lane >= 4
                    || *lanes & (1 << lane) != 0
                    || source.is_some_and(|s| s != arg)
                {
                    return None;
                }
                *source = Some(arg);
                *lanes |= 1 << lane;
                Some(())
            }
            _ => None,
        }
    }
    let mut source = None;
    let mut lanes = 0;
    for arg in args {
        visit(f, arg, uses, 3, &mut source, &mut lanes)?;
    }
    (lanes == 0b1111).then(|| {
        Fold::Data(D::VectorReduce {
            op: VectorReduceOp::Add,
            arg: source.expect("complete lane set"),
        })
    })
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
    // Eliminate a written identity before inspecting its producer: the selected
    // input may itself be a permutation that must retain its native recipe.
    for (slot, &arg) in args.iter().enumerate() {
        if lanes
            .iter()
            .copied()
            .eq(slot as u8 * n..(slot as u8 + 1) * n)
        {
            return Some(Fold::Value(arg));
        }
    }
    // Flatten exactly one level, only through unshared shuffles. This cannot
    // duplicate a live permutation. At most two source vectors are expressible.
    // Fixed-width IR has at most 32 lanes. Do not allocate scratch vectors on
    // every visit, especially for shuffles that will remain unchanged.
    let mut sources = [args[0]; 2];
    let mut source_count = 0;
    let mut storage = [0u8; 32];
    let indices = &mut storage[..lanes.len()];
    let mut composed = false;
    for (&index, output) in lanes.iter().zip(indices.iter_mut()) {
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
        let slot = match sources[..source_count].iter().position(|&v| v == value) {
            Some(slot) => slot,
            None if source_count < 2 => {
                sources[source_count] = value;
                source_count += 1;
                source_count - 1
            }
            None => return None,
        };
        *output = lane + slot as u8 * n;
    }
    if indices.iter().copied().eq(0..n) {
        return Some(Fold::Value(sources[0]));
    }
    // Only use composition when it eliminates the permutation entirely above.
    // Fewer IR shuffles can lose packed ZIP/SHUFPS recipes and scalarize. Keep
    // every nontrivial chain until target cost feedback can justify rewriting it.
    // Uncomposed two-source forms also retain their written operand order.
    if composed || source_count == 2 {
        return None;
    }
    let new_args = [sources[0], sources[source_count - 1]];
    if new_args == args && indices == lanes {
        return None;
    }
    Some(Fold::Data(D::VectorShuffle {
        args: new_args,
        lanes: indices.to_vec(),
    }))
}
