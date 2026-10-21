//! Bounded expansion of ordinary memory intrinsics before register allocation.
use keel_ir::{
    BinaryOp, ConvertOp, Function, InsertPoint, Inst, InstData, IntCC, Intrinsic, MemoryFlags,
    Type, Value, ValueDef,
};

const MAX_BYTES: i64 = 32;

pub fn run(f: &mut Function) -> bool {
    let work: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
    let mut changed = false;
    for at in work {
        let InstData::Intrinsic { id, args } = f.inst_data(at) else {
            continue;
        };
        // Intrinsic metadata is compiler-owned semantic provenance; never
        // recognize an ordinary call by its linker symbol.
        if id.info().name == "memcmp" {
            let args = args.clone();
            changed |= expand_memcmp_equality(f, at, &args);
            continue;
        }
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

/// Expand only a semantically proven memcmp whose complete use set observes
/// zero/nonzero. The caller supplies builtin provenance, never a symbol match.
fn expand_memcmp_equality(f: &mut Function, at: Inst, args: &[Value]) -> bool {
    let Some(&result) = f.inst_results(at).first() else {
        return false;
    };
    if args.len() != 3 || f.value_type(result) != Type::I32 {
        return false;
    }
    let Some(size) = integer(f, args[2]) else {
        return false;
    };
    if !(0..=MAX_BYTES).contains(&size) {
        return false;
    }
    let mut used = false;
    for (_, i) in f.layout_insts() {
        let data = f.inst_data(i);
        let mut observes = false;
        data.for_each_operand(|v| observes |= f.resolve(v) == result);
        if !observes {
            continue;
        }
        let InstData::Icmp {
            cond: IntCC::Eq | IntCC::Ne,
            args,
        } = data
        else {
            return false;
        };
        if !((f.resolve(args[0]) == result && integer(f, args[1]) == Some(0))
            || (f.resolve(args[1]) == result && integer(f, args[0]) == Some(0)))
        {
            return false;
        }
        used = true;
    }
    if !used {
        return false;
    }
    // Keep loads at the original operation, including across intervening
    // writes/calls. Exact nonoverlapping chunks cover [0,size) on each side.
    let mut unequal = emit(
        f,
        at,
        InstData::Iconst {
            ty: Type::I8,
            imm: 0,
        },
    );
    let mut offset = 0;
    for (width, ty) in [
        (8, Type::I64),
        (4, Type::I32),
        (2, Type::I16),
        (1, Type::I8),
    ] {
        while offset + width <= size {
            let mut values = Vec::new();
            for base in &args[..2] {
                let addr = address(f, at, *base, offset);
                values.push(emit(
                    f,
                    at,
                    InstData::Load {
                        ty,
                        addr,
                        flags: MemoryFlags::default(),
                    },
                ));
            }
            let differs = emit(
                f,
                at,
                InstData::Icmp {
                    cond: IntCC::Ne,
                    args: [values[0], values[1]],
                },
            );
            unequal = emit(
                f,
                at,
                InstData::Binary {
                    op: BinaryOp::Or,
                    args: [unequal, differs],
                },
            );
            offset += width;
        }
    }
    let replacement = emit(
        f,
        at,
        InstData::Convert {
            op: ConvertOp::Uext,
            ty: Type::I32,
            arg: unequal,
        },
    );
    // Only zero/nonzero is observed, so 0/1 is sufficient; no ordering or
    // magnitude of a lexicographic memcmp result is exposed.
    f.replace_all_uses(result, replacement);
    f.remove_inst(at);
    true
}

fn integer(f: &Function, value: Value) -> Option<i64> {
    let ValueDef::Result(i, _) = f.value_def(f.resolve(value)) else {
        return None;
    };
    match f.inst_data(i) {
        InstData::Iconst { imm, .. } => Some(*imm),
        _ => None,
    }
}

#[cfg(test)]
mod memcmp_tests {
    use super::*;

    fn candidate(length: &str, consumers: &str) -> keel_ir::Module {
        keel_ir::parse(&format!("decl @proven_builtin(ptr, ptr, i64) -> i32\nfunc @f(ptr, ptr, i64) -> i8 {{\nblock0(v0: ptr, v1: ptr, v2: i64):\n{length}\nv4 = call @proven_builtin(v0, v1, v3)\nv5 = iconst.i32 0\n{consumers}\n}}" )).unwrap()
    }

    fn apply(m: &mut keel_ir::Module) -> bool {
        let f = &mut m.functions[0];
        let (at, args) = f
            .layout_insts()
            .find_map(|(_, i)| match f.inst_data(i) {
                InstData::Call { args, .. } => Some((i, args.clone())),
                _ => None,
            })
            .unwrap();
        expand_memcmp_equality(f, at, &args)
    }

    #[test]
    fn exact_ranges_for_all_sizes_and_equality_directions() {
        for size in 0..=32 {
            for cmp in ["eq v4, v5", "ne v4, v5", "eq v5, v4", "ne v5, v4"] {
                let mut m = candidate(
                    &format!("v3 = iconst.i64 {size}"),
                    &format!("v6 = icmp.{cmp}\nreturn v6"),
                );
                assert!(apply(&mut m));
                keel_ir::verify_module(&m).unwrap();
                let f = &m.functions[0];
                let mut ranges = [Vec::new(), Vec::new()];
                for (_, i) in f.layout_insts() {
                    if let InstData::Load { ty, addr, flags } = f.inst_data(i) {
                        assert_eq!(*flags, MemoryFlags::default());
                        let mut base = *addr;
                        let mut offset = 0;
                        if let ValueDef::Result(ai, _) = f.value_def(base) {
                            if let InstData::Binary {
                                op: BinaryOp::Ptradd,
                                args,
                            } = f.inst_data(ai)
                            {
                                base = args[0];
                                offset = integer(f, args[1]).unwrap();
                            }
                        }
                        let side = if base == f.block_params(f.entry_block().unwrap())[0] {
                            0
                        } else {
                            1
                        };
                        ranges[side].extend(offset..offset + i64::from(ty.bytes().unwrap()));
                    }
                }
                assert_eq!(ranges[0], (0..size).collect::<Vec<_>>());
                assert_eq!(ranges[1], ranges[0]);
            }
        }
    }

    #[test]
    fn rejects_nonzero_ordering_general_and_mixed_users() {
        for consumers in [
            "v6 = icmp.slt v4, v5\nreturn v6",
            "v7 = iconst.i32 1\nv6 = icmp.eq v4, v7\nreturn v6",
            "v6 = icmp.eq v4, v5\nv7 = iadd v4, v5\nreturn v6",
            "v6 = trunc.i8 v4\nreturn v6",
        ] {
            let mut m = candidate("v3 = iconst.i64 16", consumers);
            let before = m.to_string();
            assert!(!apply(&mut m));
            assert_eq!(m.to_string(), before);
        }
        for size in ["33", "-1", "-9223372036854775808"] {
            let mut m = candidate(
                &format!("v3 = iconst.i64 {size}"),
                "v6 = icmp.eq v4, v5\nreturn v6",
            );
            assert!(!apply(&mut m));
        }
    }

    #[test]
    fn multiple_equality_users_keep_operation_location_and_effects() {
        let mut m = candidate(
            "v3 = iconst.i64 16",
            "v6 = icmp.eq v4, v5\nv7 = icmp.ne v5, v4\nv8 = or v6, v7\nreturn v8",
        );
        let loc = keel_ir::SourceLoc {
            file: m.add_file("memcmp.c"),
            line: 23,
            col: 7,
            scope: None,
        };
        let old: Vec<_> = m.functions[0].layout_insts().map(|(_, i)| i).collect();
        let at = old
            .iter()
            .copied()
            .find(|i| matches!(m.functions[0].inst_data(*i), InstData::Call { .. }))
            .unwrap();
        m.functions[0].set_inst_loc(at, Some(loc));
        assert!(apply(&mut m));
        keel_ir::verify_module(&m).unwrap();
        for (_, i) in m.functions[0]
            .layout_insts()
            .filter(|(_, i)| !old.contains(i))
        {
            assert_eq!(m.functions[0].inst_loc(i), Some(loc));
        }
        let mut variable = candidate("v3 = iadd v2, v2", "v6 = icmp.eq v4, v5\nreturn v6");
        assert!(!apply(&mut variable));
    }

    #[test]
    fn arbitrary_symbol_calls_are_not_builtin_provenance() {
        let m = candidate("v3 = iconst.i64 16", "v6 = icmp.eq v4, v5\nreturn v6");
        let mut m = keel_ir::parse(&m.to_string().replace("proven_builtin", "memcmp")).unwrap();
        let before = m.to_string();
        assert!(!run(&mut m.functions[0]));
        assert_eq!(m.to_string(), before);
    }
}
