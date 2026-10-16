//! Bounded expansion of ordinary memory intrinsics before register allocation.
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, Intrinsic, MemoryFlags, Type,
    Value, ValueDef,
};

const MAX_BYTES: i64 = 32;

pub fn run(f: &mut Function) -> bool {
    let work: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    let mut changed = false;
    for at in work {
        let InstData::Intrinsic { id, args } = f.inst_data(at) else {
            continue;
        };
        if !matches!(
            id,
            Intrinsic::Memcpy | Intrinsic::Memmove | Intrinsic::Memset
        ) {
            continue;
        }
        let (id, args) = (*id, args.clone());
        let ValueDef::Result(length, _) = f.value_def(f.resolve(args[2])) else {
            continue;
        };
        let InstData::Iconst {
            ty: Type::I64,
            imm: size,
        } = *f.inst_data(length)
        else {
            continue;
        };
        // Negative/wrapping lengths are deliberately not interpreted as small.
        if !(0..=MAX_BYTES).contains(&size) {
            continue;
        }
        let mut chunks = Vec::new();
        let mut offset = 0;
        for (width, ty) in [
            (8, Type::I64),
            (4, Type::I32),
            (2, Type::I16),
            (1, Type::I8),
        ] {
            let mut fill = None;
            while offset + width <= size {
                let value = if id == Intrinsic::Memset {
                    *fill.get_or_insert_with(|| {
                        if ty == Type::I8 {
                            args[1]
                        } else {
                            let byte = emit(
                                f,
                                at,
                                InstData::Convert {
                                    op: ConvertOp::Uext,
                                    ty,
                                    arg: args[1],
                                },
                            );
                            let repeat = emit(
                                f,
                                at,
                                InstData::Iconst {
                                    ty,
                                    imm: (u64::MAX / 255 >> (64 - width * 8)) as i64,
                                },
                            );
                            emit(
                                f,
                                at,
                                InstData::Binary {
                                    op: BinaryOp::Imul,
                                    args: [byte, repeat],
                                },
                            )
                        }
                    })
                } else {
                    let addr = address(f, at, args[1], offset);
                    emit(
                        f,
                        at,
                        InstData::Load {
                            ty,
                            addr,
                            flags: MemoryFlags::default(),
                        },
                    )
                };
                chunks.push((offset, value));
                offset += width;
            }
        }
        // Snapshot every source chunk before ANY destination write: memmove
        // must work for either overlap direction, including sub-chunk overlap.
        for (offset, value) in chunks {
            let addr = address(f, at, args[0], offset);
            insert(
                f,
                at,
                InstData::Store {
                    value,
                    addr,
                    flags: MemoryFlags::default(),
                },
            );
        }
        // Leave operand producers in place, including zero-length side effects.
        f.remove_inst(at);
        changed = true;
    }
    changed
}

fn insert(f: &mut Function, at: Inst, data: InstData) -> Inst {
    let loc = f.inst_loc(at);
    f.ins(InsertPoint::Before(at)).loc(loc).build(data)
}

fn emit(f: &mut Function, at: Inst, data: InstData) -> Value {
    let i = insert(f, at, data);
    f.first_result(i)
}

fn address(f: &mut Function, at: Inst, base: Value, offset: i64) -> Value {
    if offset == 0 {
        base
    } else {
        let offset = emit(
            f,
            at,
            InstData::Iconst {
                ty: Type::I64,
                imm: offset,
            },
        );
        emit(
            f,
            at,
            InstData::Binary {
                op: BinaryOp::Ptradd,
                args: [base, offset],
            },
        )
    }
}
