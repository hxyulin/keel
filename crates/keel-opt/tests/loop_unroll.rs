use keel_ir::{BinaryOp as B, Block, EntityRef, Function, InstData as D, IntCC, Type, Value};
use std::collections::HashMap;

fn kernel(ty: Type) -> String {
    let extend = if ty == Type::I32 {
        "v8 = uext.i64 v5"
    } else {
        "v8 = iconst.i64 0"
    };
    let index = if ty == Type::I32 { "v8" } else { "v5" };
    let bytes = ty.bytes().unwrap();
    format!(
        "func @hot(ptr,{ty},{ty},{ty}) -> {ty} cc(c) {{
block0(v0: ptr,v1: {ty},v2: {ty},v3: {ty}):
 v4 = iconst.{ty} 0
 jump block1(v4,v3)
block1(v5: {ty},v6: {ty}):
 v7 = icmp.ult v5,v1
 brif v7,block2,block3
block2:
 {extend}
 v9 = iconst.i64 {bytes}
 v10 = imul {index},v9
 v11 = ptradd v0,v10
 v12 = load.{ty} v11
 v13 = xor v12,v2
 v14 = iconst.{ty} 1
 v15 = iadd v5,v14
 v17 = imul v13,v15
 v18 = iadd v6,v17
 jump block1(v15,v18)
block3:
 return v6
}}"
    )
}
fn optimize(src: &str, passes: &[&str]) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, passes).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
fn loads(f: &Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), D::Load { .. }))
        .count()
}
fn mask(ty: Type) -> u64 {
    if ty == Type::Ptr {
        u64::MAX
    } else {
        u64::MAX >> (64 - ty.bytes().unwrap() * 8)
    }
}

// A bounded independent integer/CFG model with a checked byte-addressed memory.
// Load trace comparison verifies both exact trip count and per-iteration order.
fn execute(
    f: &Function,
    inputs: &[u64],
    memory: &HashMap<u64, u64>,
    resume: Option<(Block, Vec<u64>)>,
) -> (u64, Vec<u64>) {
    let mut values = vec![0; f.num_values()];
    for (&p, &v) in f.block_params(f.entry_block().unwrap()).iter().zip(inputs) {
        values[p.index()] = v & mask(f.value_type(p));
    }
    let mut block = f.entry_block().unwrap();
    if let Some((b, args)) = resume {
        block = b;
        for (&p, v) in f.block_params(b).iter().zip(args) {
            values[p.index()] = v;
        }
    }
    let mut trace = vec![];
    for _ in 0..10000 {
        let mut next = None;
        for &i in f.block_insts(block) {
            let read = |v: Value| values[f.resolve(v).index()];
            let result = match f.inst_data(i) {
                D::Iconst { imm, .. } => Some(*imm as u64),
                D::Binary { op, args } => Some(match op {
                    B::Iadd | B::Ptradd => read(args[0]).wrapping_add(read(args[1])),
                    B::Isub => read(args[0]).wrapping_sub(read(args[1])),
                    B::Imul => read(args[0]).wrapping_mul(read(args[1])),
                    B::Xor => read(args[0]) ^ read(args[1]),
                    B::And => read(args[0]) & read(args[1]),
                    B::Or => read(args[0]) | read(args[1]),
                    B::Shl | B::Ushr | B::Sshr => {
                        let width = f.value_type(args[0]).bytes().unwrap() * 8;
                        let shift = read(args[1]) % u64::from(width);
                        match op {
                            B::Shl => read(args[0]) << shift,
                            B::Ushr => read(args[0]) >> shift,
                            B::Sshr => {
                                (((read(args[0]) << (64 - width)) as i64 >> (64 - width)) >> shift)
                                    as u64
                            }
                            _ => unreachable!(),
                        }
                    }
                    _ => panic!("unmodeled {op:?}"),
                }),
                D::Convert { arg, .. } => Some(read(*arg)),
                D::Load { addr, .. } => {
                    let address = read(*addr);
                    trace.push(address);
                    Some(*memory.get(&address).expect("load outside exact input"))
                }
                D::Icmp { cond, args } => Some(u64::from(match cond {
                    IntCC::Ult => read(args[0]) < read(args[1]),
                    IntCC::Ugt => read(args[0]) > read(args[1]),
                    IntCC::Uge => read(args[0]) >= read(args[1]),
                    _ => panic!("unmodeled {cond:?}"),
                })),
                D::Return { args } => return (read(args[0]), trace),
                D::Jump { dest } => {
                    next = Some(dest);
                    None
                }
                D::Brif { cond, dests } => {
                    next = Some(&dests[usize::from(read(*cond) == 0)]);
                    None
                }
                d => panic!("unmodeled {d:?}"),
            };
            if let Some(v) = result {
                let p = f.first_result(i);
                values[p.index()] = v & mask(f.value_type(p));
            }
        }
        let dest = next.unwrap();
        let args: Vec<_> = dest
            .args
            .iter()
            .map(|v| values[f.resolve(*v).index()])
            .collect();
        block = dest.block;
        for (&p, v) in f.block_params(block).iter().zip(args) {
            values[p.index()] = v;
        }
    }
    panic!("bounded execution limit")
}

#[test]
fn actual_two_lane_shape_is_deterministic_idempotent_and_bounded() {
    for ty in [Type::I32, Type::I64] {
        let src = kernel(ty);
        let before = keel_ir::parse(&src).unwrap();
        let m = optimize(&src, &["loop-unroll"]);
        let f = &m.functions[0];
        assert_eq!(loads(f), 3, "original scalar plus two cloned loads");
        assert_eq!(f.num_blocks(), before.functions[0].num_blocks() + 3);
        assert!(f.layout_insts().count() - before.functions[0].layout_insts().count() <= 54);
        let pair_header = f.blocks().find(|&b| f.block_params(b).len() == 3).unwrap();
        let params = f.block_params(pair_header);
        let pair_body = match f.inst_data(*f.block_insts(pair_header).last().unwrap()) {
            D::Brif { dests, .. } => dests[0].block,
            _ => unreachable!(),
        };
        for lane in [params[1], params[2]] {
            assert!(f.block_insts(pair_body).iter().any(|&i|
                matches!(f.inst_data(i), D::Binary { op: B::Iadd, args } if args.contains(&lane))));
        }
        let text = m.to_string();
        assert_eq!(
            text,
            optimize(&src, &["loop-unroll", "loop-unroll"]).to_string()
        );
        assert_eq!(text, optimize(&src, &["loop-unroll"]).to_string());
        assert!(!keel_opt::DEFAULT_PIPELINE.contains(&"loop-unroll"));
    }
}

#[test]
fn wrapping_seeds_trip_counts_exact_loads_and_iv_liveout() {
    for ty in [Type::I32, Type::I64] {
        for iv_liveout in [false, true] {
            let src = if iv_liveout {
                kernel(ty).replace("return v6", "return v5")
            } else {
                kernel(ty)
            };
            let before = keel_ir::parse(&src).unwrap();
            let after = optimize(&src, &["loop-unroll"]);
            let mut cleanup = after.clone();
            keel_opt::optimize(&mut cleanup).unwrap();
            for n in [0, 1, 2, 3, 4, 7, 16, 31, 64, 257] {
                let memory: HashMap<_, _> = (0..n)
                    .map(|i| {
                        (
                            0x1000 + i * u64::from(ty.bytes().unwrap()),
                            u64::MAX.wrapping_sub(i * 0x102030405),
                        )
                    })
                    .collect();
                for seed in [0, 1, u64::MAX, u64::MAX - 7, 0x8123456789abcdef] {
                    let k = seed.rotate_left(13) & mask(ty);
                    let input = [0x1000, n, k, seed];
                    let want = if iv_liveout {
                        n
                    } else {
                        (0..n).fold(seed & mask(ty), |s, i| {
                            let x =
                                memory[&(0x1000 + i * u64::from(ty.bytes().unwrap()))] & mask(ty);
                            s.wrapping_add((x ^ k).wrapping_mul(i + 1)) & mask(ty)
                        })
                    };
                    let expected = execute(&before.functions[0], &input, &memory, None);
                    assert_eq!(expected.0, want);
                    assert_eq!(
                        execute(&after.functions[0], &input, &memory, None),
                        expected
                    );
                    assert_eq!(
                        execute(&cleanup.functions[0], &input, &memory, None),
                        expected
                    );
                }
            }
        }
    }
}

#[test]
fn near_unsigned_maximum_pair_guard_and_tail_are_bounded() {
    // Resume at states satisfying the inductive iv<=bound invariant. This
    // checks MAX boundaries without executing billions of prefix iterations.
    for ty in [Type::I32, Type::I64] {
        let m = optimize(&kernel(ty), &["loop-unroll"]);
        let f = &m.functions[0];
        let h = f.blocks().find(|&b| f.block_params(b).len() == 3).unwrap();
        let max = mask(ty);
        for n in [0, 1, 2, 3] {
            let start = max - n;
            let bytes = u64::from(ty.bytes().unwrap());
            let memory: HashMap<_, _> = (start..max).map(|i| (i.wrapping_mul(bytes), 7)).collect();
            // Address base zero intentionally exercises modular pointer math.
            let got = execute(f, &[0, max, 0, 0], &memory, Some((h, vec![start, max, 5])));
            let want = (start..max).fold(4, |s: u64, i| {
                s.wrapping_add(7u64.wrapping_mul(i + 1)) & max
            });
            assert_eq!(got.0, want);
            assert_eq!(
                got.1,
                (start..max)
                    .map(|i| i.wrapping_mul(bytes))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn unsupported_shapes_remain_byte_identical() {
    let src = kernel(Type::I64);
    let mut cases = vec![
        src.replace("icmp.ult", "icmp.slt"),
        src.replace("iconst.i64 0\n jump", "iconst.i64 -2\n jump"),
        src.replace("iconst.i64 1\n v15", "iconst.i64 2\n v15"),
        src.replace("load.i64 v11", "load.i64 volatile v11"),
        src.replace("v17 = imul v13,v15", "v17 = imul v13,v6"),
        src.replace("v17 = imul v13,v15", "v17 = udiv v13,v15"),
        src.replace("v18 = iadd v6,v17", "v18 = imul v6,v17"),
        src.replace("v18 = iadd v6,v17", "v18 = iadd v6,v6"),
        src.replace(" v18 =", " store v13,v11\n v18 ="),
        format!("decl @effect()\n{}", src.replace(" v18 =", " call @effect()\n v18 =")),
        src.replace(" v18 =", " v50 = atomic_load.i64 relaxed v11\n v18 ="),
        src.replace(" jump block1(v15,v18)", " brif v7,block1(v15,v18),block3"),
        src.replace(" v18 =", " v50 = iconst.i64 0\n v51 = iconst.i64 0\n v52 = iconst.i64 0\n v53 = iconst.i64 0\n v54 = iconst.i64 0\n v55 = iconst.i64 0\n v56 = iconst.i64 0\n v57 = iconst.i64 0\n v58 = iconst.i64 0\n v59 = iconst.i64 0\n v18 ="),
    ];
    // Floating-point carried arithmetic is outside the reassociation contract.
    cases.push(
        src.replace(
            "func @hot(ptr,i64,i64,i64) -> i64",
            "func @hot(ptr,i64,f64,f64) -> f64",
        )
        .replace("v2: i64,v3: i64", "v2: f64,v3: f64")
        .replace("v6: i64", "v6: f64")
        .replace("load.i64", "load.f64")
        .replace("xor v12,v2", "fmul v12,v2")
        .replace("v17 = imul v13,v15", "v17 = fadd v13,v2")
        .replace("v18 = iadd", "v18 = fadd"),
    );
    for case in cases {
        let before = keel_ir::parse(&case).unwrap();
        keel_ir::verify_module(&before).unwrap();
        assert_eq!(
            before.to_string(),
            optimize(&case, &["loop-unroll"]).to_string(),
            "{case}"
        );
    }
}

#[test]
fn accepts_commuted_adds_and_frontend_unsigned_comparison() {
    let src = kernel(Type::I64)
        .replace("icmp.ult v5,v1", "icmp.ugt v1,v5")
        .replace("iadd v5,v14", "iadd v14,v5")
        .replace("iadd v6,v17", "iadd v17,v6");
    assert_eq!(loads(&optimize(&src, &["loop-unroll"]).functions[0]), 3);
}

#[test]
fn preserves_two_load_order_in_both_iterations_and_scalar_tail() {
    let src = kernel(Type::I64).replace(
        " v13 = xor v12,v2",
        " v40 = iconst.i64 8\n v41 = ptradd v11,v40\n v42 = load.i64 v41\n v13 = xor v12,v42",
    );
    let before = keel_ir::parse(&src).unwrap();
    let after = optimize(&src, &["loop-unroll"]);
    assert_eq!(loads(&after.functions[0]), 6);
    for n in [0, 1, 2, 3, 17] {
        let memory = (0..=n).map(|i| (0x1000 + i * 8, i * 13)).collect();
        let inputs = [0x1000, n, 1, u64::MAX];
        let expected = execute(&before.functions[0], &inputs, &memory, None);
        assert_eq!(
            execute(&after.functions[0], &inputs, &memory, None),
            expected
        );
        assert_eq!(
            expected.1,
            (0..n)
                .flat_map(|i| [0x1000 + i * 8, 0x1008 + i * 8])
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn mixed_induction_and_reduction_widths() {
    for (iv, sum) in [(Type::I32, Type::I64), (Type::I64, Type::I32)] {
        let src = kernel(iv)
            .replace(
                &format!("func @hot(ptr,{iv},{iv},{iv}) -> {iv}"),
                &format!("func @hot(ptr,{iv},{sum},{sum}) -> {sum}"),
            )
            .replace(
                &format!("v2: {iv},v3: {iv}"),
                &format!("v2: {sum},v3: {sum}"),
            )
            .replace(&format!("v6: {iv}"), &format!("v6: {sum}"))
            .replace(&format!("load.{iv}"), &format!("load.{sum}"))
            .replace(
                " v17 = imul v13,v15",
                &format!(
                    " v16 = {}.{sum} v15\n v17 = imul v13,v16",
                    if sum == Type::I64 { "uext" } else { "trunc" }
                ),
            );
        let before = keel_ir::parse(&src).unwrap();
        let after = optimize(&src, &["loop-unroll"]);
        assert_eq!(loads(&after.functions[0]), 3);
        for n in [0, 1, 2, 3, 17] {
            // Address scale remains that of the original induction-width kernel.
            let memory = (0..n)
                .map(|i| (0x1000 + i * u64::from(iv.bytes().unwrap()), u64::MAX - i))
                .collect();
            let inputs = [0x1000, n, 0x123456789, u64::MAX];
            assert_eq!(
                execute(&after.functions[0], &inputs, &memory, None),
                execute(&before.functions[0], &inputs, &memory, None)
            );
        }
    }
}

#[test]
fn skips_debug_functions_and_does_not_grow_after_default_cleanup() {
    let src = kernel(Type::I64);
    let mut debug = keel_ir::parse(&src).unwrap();
    debug.functions[0].debug_scopes.push(None);
    let before = debug.to_string();
    keel_opt::run_passes(&mut debug, &["loop-unroll"]).unwrap();
    assert_eq!(before, debug.to_string());
    let mut m = optimize(&src, &["loop-unroll"]);
    keel_opt::optimize(&mut m).unwrap();
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["loop-unroll"]).unwrap();
    assert_eq!(before, m.to_string());
}

#[test]
fn rejects_nested_loop_and_excess_live_values() {
    let nested = kernel(Type::I64)
        .replace(
            "block0(v0: ptr,v1: i64,v2: i64,v3: i64):",
            "block4(v0: ptr,v1: i64,v2: i64,v3: i64):\n jump block0\nblock0:",
        )
        .replace(" return v6", " jump block0");
    let params = (20..34).map(|v| format!(",v{v}: i64")).collect::<String>();
    let types = ",i64".repeat(14);
    let chain = (20..34)
        .map(|v| {
            format!(
                " v{} = iadd {},v{v}\n",
                v + 20,
                if v == 20 {
                    "v12".to_owned()
                } else {
                    format!("v{}", v + 19)
                }
            )
        })
        .collect::<String>();
    let pressure = format!(
        "func @pressure(ptr,i64,i64{types}) -> i64 {{
block0(v0: ptr,v1: i64,v2: i64{params}):
 v3 = iconst.i64 0
 jump block1(v3,v2)
block1(v5: i64,v6: i64):
 v7 = icmp.ult v5,v1
 brif v7,block2,block3
block2:
 v12 = load.i64 v0
 v14 = iconst.i64 1
 v15 = iadd v5,v14
{chain} v18 = iadd v6,v53
 jump block1(v15,v18)
block3:
 return v6
}}"
    );
    for src in [nested, pressure] {
        let before = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&before).unwrap();
        assert_eq!(
            before.to_string(),
            optimize(&src, &["loop-unroll"]).to_string()
        );
    }
}

#[test]
fn in_memory_aliased_body_results_do_not_resurrect_dead_computations() {
    for alias_to_load in [false, true] {
        let mut m = keel_ir::parse(&kernel(Type::I64)).unwrap();
        let f = &mut m.functions[0];
        let xor = f
            .layout_insts()
            .find_map(|(_, i)| matches!(f.inst_data(i), D::Binary { op: B::Xor, .. }).then_some(i))
            .unwrap();
        let original = f.first_result(xor);
        let target = if alias_to_load {
            let load = f
                .layout_insts()
                .find_map(|(_, i)| matches!(f.inst_data(i), D::Load { .. }).then_some(i))
                .unwrap();
            f.first_result(load)
        } else {
            f.block_params(f.entry_block().unwrap())[2]
        };
        // Deliberately retain the original XOR in layout: a valid intermediate
        // alias state which printing/reparsing cannot preserve.
        f.replace_all_uses(original, target);
        keel_ir::verify(f).unwrap();
        let before = f.clone();
        let memory = HashMap::from([(0x1000, 1), (0x1008, 1)]);
        let inputs = [0x1000, 2, 3, 0];
        let expected = execute(&before, &inputs, &memory, None);
        let changed = keel_opt::find_pass("loop-unroll").unwrap().run(f);
        keel_ir::verify(f).unwrap();
        let actual = execute(f, &inputs, &memory, None);
        println!(
            "alias_to_load={alias_to_load}, expected={expected:?}, actual={actual:?}, changed={changed}"
        );
        assert_eq!(actual, expected);
        assert!(
            !changed,
            "aliased body result must be conservatively rejected"
        );
        assert_eq!(f.resolve(original), target);
        assert_eq!(f.layout_insts().count(), before.layout_insts().count());
    }
}

#[test]
fn in_memory_header_aliases_and_multiresult_body_are_rejected() {
    for which in 0..4 {
        let src = if which == 3 {
            kernel(Type::I64).replace(" v13 = xor", " v40,v41 = uadd_overflow v12,v2\n v13 = xor")
        } else {
            kernel(Type::I64)
        };
        let mut m = keel_ir::parse(&src).unwrap();
        let f = &mut m.functions[0];
        let entry = f.entry_block().unwrap();
        let header = f
            .blocks()
            .find(|&b| b != entry && f.block_params(b).len() == 2)
            .unwrap();
        let (original, target) = match which {
            0 => (f.block_params(header)[0], f.block_params(entry)[1]),
            1 => (f.block_params(header)[1], f.block_params(entry)[3]),
            2 => {
                let pre_jump = *f.block_insts(entry).last().unwrap();
                let zero = f.insert_inst(
                    keel_ir::InsertPoint::Before(pre_jump),
                    D::Iconst {
                        ty: Type::I8,
                        imm: 0,
                    },
                );
                (
                    f.first_result(f.block_insts(header)[0]),
                    f.first_result(zero),
                )
            }
            3 => {
                let i = f
                    .layout_insts()
                    .find_map(|(_, i)| matches!(f.inst_data(i), D::Overflow { .. }).then_some(i))
                    .unwrap();
                (f.first_result(i), f.block_params(entry)[2])
            }
            _ => unreachable!(),
        };
        f.replace_all_uses(original, target);
        keel_ir::verify(f).unwrap();
        let count = f.layout_insts().count();
        assert!(
            !keel_opt::find_pass("loop-unroll").unwrap().run(f),
            "case {which}"
        );
        keel_ir::verify(f).unwrap();
        assert_eq!(f.resolve(original), target);
        assert_eq!(f.layout_insts().count(), count);
    }
}
