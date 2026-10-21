#[path = "../src/adjacent_abs.rs"]
mod adjacent_abs;
#[path = "../src/adjacent_diff.rs"]
mod adjacent_diff;
mod support;

fn canonical() -> keel_ir::Module {
    let mut m = keel_ir::parse(include_str!("adjacent_abs/adjacent.keel")).unwrap();
    assert!(adjacent_abs::run(&mut m.functions[0]));
    m
}

#[test]
fn accepts_the_merged_rotated_loop_emitted_by_the_real_frontend() {
    let mut m = keel_ir::parse(include_str!("adjacent_diff/rotated.keel")).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(adjacent_diff::run(&mut m.functions[0]));
    keel_ir::verify_module(&m).unwrap();
    assert!(!adjacent_diff::run(&mut m.functions[0]));
}

#[test]
fn derives_inputs_from_dataflow_for_ordinary_and_reordered_signatures() {
    let raw = include_str!("adjacent_abs/adjacent.keel");
    for source in [
        raw.replace("(ptr, ptr, ptr, i64)", "(ptr, i64)")
            .replace("(v0: ptr, v1: ptr, v2: ptr, v3: i64)", "(v0: ptr, v3: i64)"),
        raw.replace("(ptr, ptr, ptr, i64)", "(i64, ptr)")
            .replace("(v0: ptr, v1: ptr, v2: ptr, v3: i64)", "(v3: i64, v0: ptr)"),
        raw.replace("(ptr, ptr, ptr, i64)", "(ptr, i64, ptr, ptr)")
            .replace("(v0: ptr, v1: ptr, v2: ptr, v3: i64)", "(v1: ptr, v3: i64, v0: ptr, v2: ptr)"),
    ] {
        let mut m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(adjacent_abs::run(&mut m.functions[0]));
        assert!(adjacent_diff::run(&mut m.functions[0]));
        keel_ir::verify_module(&m).unwrap();
    }
}

#[test]
fn exact_shape_verifies_and_is_idempotent() {
    let mut m = canonical();
    assert!(adjacent_diff::run(&mut m.functions[0]));
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    assert!(!adjacent_diff::run(&mut m.functions[0]));
    if let Some(dir) = std::env::var_os("KEEL_ADJACENT_DIFF_EVIDENCE_DIR") {
        std::fs::write(
            std::path::PathBuf::from(dir).join("adjacent-diff-after.keel"),
            m.to_string(),
        )
        .unwrap();
    }
}

#[test]
fn raw_triangle_and_memory_contract_changes_are_rejected() {
    let mut raw = keel_ir::parse(include_str!("adjacent_abs/adjacent.keel")).unwrap();
    assert!(!adjacent_diff::run(&mut raw.functions[0]));
    let source = canonical().to_string();
    for source in [
        source.replace("load.i32 ", "load.i32 volatile "),
        source.replace("load.i32 ", "load.i32 notrap "),
        source.replace("load.i32 ", "load.i32 align 4 "),
        source.replace("icmp.ugt", "icmp.sgt"),
        source.replace("icmp.slt", "icmp.ult"),
        source.replace("iconst.i64 2", "iconst.i64 3"),
        source.replace("sext.i64", "uext.i64"),
    ] {
        let mut m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        assert!(!adjacent_diff::run(&mut m.functions[0]), "{source}");
        assert_eq!(before, m.to_string());
    }
}

#[test]
fn rejects_extra_load_write_atomic_call_and_custom_operations() {
    let base = include_str!("adjacent_abs/adjacent.keel");
    for source in [
        base.replace(
            "v14 = sext.i64 v13",
            "v80 = load.i32 v12\n v14 = sext.i64 v13",
        ),
        base.replace("v14 = sext.i64 v13", "store v13, v12\n v14 = sext.i64 v13"),
        base.replace("load.i32", "atomic_load.i32 relaxed"),
        format!(
            "decl @barrier()\n{}",
            base.replace("v14 = sext.i64 v13", "call @barrier()\n v14 = sext.i64 v13")
        ),
    ] {
        let mut m = keel_ir::parse(&source).unwrap();
        assert!(adjacent_abs::run(&mut m.functions[0]));
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        assert!(!adjacent_diff::run(&mut m.functions[0]));
        assert_eq!(before, m.to_string());
    }
}

#[test]
fn original_instructions_locations_and_function_contracts_are_untouched() {
    use keel_ir::{EntityRef, InstData as D, Type};
    let mut m = canonical();
    let f = &mut m.functions[0];
    f.restrict_params = vec![0, 1, 2];
    let insts: Vec<_> = f.layout_insts().collect();
    for &(_, i) in &insts {
        f.set_inst_loc(
            i,
            Some(keel_ir::SourceLoc {
                file: keel_ir::SourceFile::new(0),
                line: i.index() as u32 + 20,
                col: 3,
                scope: None,
            }),
        );
    }
    let original: Vec<_> = insts
        .iter()
        .map(|&(b, i)| (b, i, f.inst_data(i).clone(), f.inst_loc(i)))
        .collect();
    let attrs = f.attrs.clone();
    let sig = f.sig.clone();
    assert!(adjacent_diff::run(f));
    assert_eq!(f.restrict_params, [0, 1, 2]);
    assert_eq!(f.attrs, attrs);
    assert_eq!(f.sig, sig);
    for (b, i, d, loc) in original {
        assert_eq!(f.inst_block(i), b);
        assert_eq!(f.inst_loc(i), loc);
        // Only the original entry-to-loop guard changes an edge.
        if !matches!(d,D::Brif { ref dests,.. } if dests[0].args.len()==2 && dests[1].args.len()==1 && f.block_params(b).is_empty())
        {
            assert_eq!(f.inst_data(i), &d);
        }
    }
    let new_loads: Vec<_> = f
        .layout_insts()
        .filter(|(_, i)| !insts.iter().any(|&(_, old)| old == *i))
        .filter_map(|(_, i)| match f.inst_data(i) {
            D::Load { flags, ty, .. } => Some((*flags, *ty, f.inst_loc(i))),
            _ => None,
        })
        .collect();
    assert_eq!(
        new_loads.iter().map(|(_, t, _)| *t).collect::<Vec<_>>(),
        [Type::I32, Type::I32, Type::I64, Type::I32]
    );
    assert!(
        new_loads
            .iter()
            .all(|(flags, _, loc)| *flags == keel_ir::MemoryFlags::default() && loc.is_some())
    );
}

#[test]
fn aliased_definitions_are_rejected_without_mutation() {
    use keel_ir::InstData as D;
    let mut m = canonical();
    let f = &mut m.functions[0];
    let (i, x) = f
        .layout_insts()
        .find_map(|(_, i)| match f.inst_data(i) {
            D::Unary { arg, .. } => Some((i, *arg)),
            _ => None,
        })
        .unwrap();
    f.replace_all_uses(f.inst_results(i)[0], x);
    let before = f.to_string();
    assert!(!adjacent_diff::run(f));
    assert_eq!(before, f.to_string());
}

#[test]
fn emitted_range_guard_matches_exact_u128_arithmetic_at_wrap_boundaries() {
    use keel_ir::{ConvertOp, InstData as D};
    use std::collections::HashMap;
    let mut m = canonical();
    assert!(adjacent_diff::run(&mut m.functions[0]));
    let f = &m.functions[0];
    let guard = f
        .blocks()
        .find(|&b| {
            f.block_insts(b)
                .iter()
                .any(|&i| matches!(f.inst_data(i),D::Iconst {imm,..} if *imm==(u64::MAX/4) as i64))
        })
        .unwrap();
    // Interpret the actual emitted guard with the shared scalar interpreter.
    // Its pointer-to-bits input is supplied as i64 to avoid host pointer rules.
    let mut scalar = keel_ir::parse(
        "func @guard(i64,i64) -> i8 {\nblock0(v0:i64,v1:i64):\n v2=iconst.i8 0\n return v2\n}",
    )
    .unwrap();
    let g = &mut scalar.functions[0];
    let entry = g.entry_block().unwrap();
    for i in g.block_insts(entry).to_vec() {
        g.remove_inst(i);
    }
    let input = f.block_params(f.entry_block().unwrap());
    let mut values = HashMap::from([
        (input[0], g.block_params(entry)[0]),
        (input[3], g.block_params(entry)[1]),
    ]);
    for &i in f.block_insts(guard) {
        let mut data = f.inst_data(i).clone();
        if let D::Brif { cond, .. } = data {
            g.append_inst(
                entry,
                D::Return {
                    args: vec![values[&cond]],
                },
            );
            break;
        }
        if let D::Convert {
            op: ConvertOp::PtrToInt,
            arg,
            ..
        } = data
        {
            values.insert(f.inst_results(i)[0], values[&arg]);
            continue;
        }
        data.for_each_operand_mut(|v| *v = values[&f.resolve(*v)]);
        let j = g.append_inst(entry, data);
        values.insert(f.inst_results(i)[0], g.inst_results(j)[0]);
    }
    keel_ir::verify_module(&scalar).unwrap();
    let edges = [
        0,
        1,
        2,
        3,
        4,
        7,
        8,
        u64::MAX / 4 - 1,
        u64::MAX / 4,
        u64::MAX / 4 + 1,
        u64::MAX - 8,
        u64::MAX - 7,
        u64::MAX - 4,
        u64::MAX - 3,
        u64::MAX - 1,
        u64::MAX,
    ];
    let check = |base, n| {
        let bytes = u128::from(n) * 4;
        let exact =
            bytes <= u128::from(u64::MAX) && u128::from(base) + bytes <= u128::from(u64::MAX);
        assert_eq!(
            support::execute(&scalar.functions[0], &[base, n]),
            u64::from(exact),
            "base={base} n={n}"
        );
    };
    for base in edges {
        for n in edges {
            check(base, n);
        }
    }
    let mut random = 0xf1a2937b5839a84du64;
    for _ in 0..10000 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let base = random;
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        check(base, random);
    }
}
