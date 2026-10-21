use super::*;

fn run(source: &str, masks: &[u64], twice: bool, expected: Loc) {
    let module = keel_ir::parse(source).unwrap();
    let func = &module.functions[0];
    let mut selection = crate::select::Selection::default();
    let calls: Vec<_> = func
        .layout_insts()
        .filter_map(|(_, i)| matches!(func.inst_data(i), InstData::Call { .. }).then_some(i))
        .collect();
    for (&inst, &mask) in calls.iter().zip(masks) {
        selection.preserved_call_gprs.insert(inst, mask);
    }
    let pools = Pools {
        caller_saved: &[9, 10],
        callee_saved: &[],
        float_caller_saved: &[9, 10],
        float_callee_saved: &[],
    };
    let is_call = |d: &InstData| matches!(d, InstData::Call { .. });
    let returns_twice = |d: &InstData| twice && is_call(d);
    let input = AllocationInput {
        func,
        pools: &pools,
        selection: &selection,
        is_call: &is_call,
        returns_twice: &returns_twice,
    };
    for tier in [AllocationTier::Fast, AllocationTier::Full] {
        let alloc = tier.allocate(&input);
        assert_eq!(alloc.loc(Value::new(0)), Some(expected), "{tier:?}");
        alloc.validate(&input).unwrap();
        for seed in 0..16 {
            super::transfer_tests::check(&input, &alloc, seed);
        }
    }
}

#[test]
fn proven_call_register_survives_both_scans_and_transfer_checks() {
    run(
        "decl @h()\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n call @h()\n return v0\n}",
        &[1 << 10],
        false,
        Loc::Reg(10),
    );
}

#[test]
fn unknown_calls_and_returns_twice_remove_extra_permissions() {
    let one = "decl @h()\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n call @h()\n return v0\n}";
    run(one, &[], false, Loc::Stack(0));
    run(one, &[u64::MAX], true, Loc::Stack(0));
    let two =
        "decl @h()\nfunc @f(i64) -> i64 {\nblock0(v0: i64):\n call @h()\n call @h()\n return v0\n}";
    run(two, &[u64::MAX], false, Loc::Stack(0));
    run(two, &[1 << 9, 1 << 10], false, Loc::Stack(0));
    run(two, &[(1 << 9) | (1 << 10), 1 << 10], false, Loc::Reg(10));
}

#[test]
fn gpr_proofs_do_not_preserve_float_or_vector_registers() {
    run(
        "decl @h()\nfunc @f(f64) -> f64 {\nblock0(v0: f64):\n call @h()\n return v0\n}",
        &[u64::MAX],
        false,
        Loc::Stack(0),
    );
    run(
        "decl @h()\nfunc @f(i64x2) -> i64x2 {\nblock0(v0: i64x2):\n call @h()\n return v0\n}",
        &[u64::MAX],
        false,
        Loc::Stack(0),
    );
}

#[test]
fn intersect_only_calls_strictly_inside_actual_lifetime_ranges() {
    let calls = [(2, 1 << 9), (4, 0), (8, (1 << 9) | (1 << 10)), (12, 0)];
    assert_eq!(call_preserves(&calls, &[(1, 3), (7, 10)]), 1 << 9);
    assert_eq!(call_preserves(&calls, &[(2, 8)]), 0);
    assert_eq!(call_preserves(&calls, &[(4, 8)]), u64::MAX);
    assert_eq!(call_preserves(&calls, &[(7, 12)]), (1 << 9) | (1 << 10));
}

#[test]
fn proof_work_exhaustion_falls_back_to_abi_clobbers() {
    let calls: Vec<_> = (1..=65).map(|i| (i, 1 << 9)).collect();
    assert_eq!(call_preserves(&calls[..64], &[(0, 66)]), 1 << 9);
    assert_eq!(call_preserves(&calls, &[(0, 66)]), 0);
}

#[test]
fn finalized_mentions_include_all_sparse_transfer_locations() {
    let func = keel_ir::parse("func @f(i64)->i64 {\nblock0(v0:i64):\n return v0\n}")
        .unwrap()
        .functions
        .remove(0);
    let mut alloc = AllocationTier::Full.allocate(&AllocationInput {
        func: &func,
        pools: &Pools {
            caller_saved: &[9],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        },
        selection: &crate::select::Selection::default(),
        is_call: &|_| false,
        returns_twice: &|_| false,
    });
    let (block, inst) = func.layout_insts().next().unwrap();
    let edit = alloc.edits.instruction_mut(inst);
    edit.before.push((Loc::Reg(10), Loc::Reg(11)));
    edit.after.push((Loc::Reg(12), Loc::Reg(13)));
    edit.uses
        .push((ValuePart::low(Value::new(0)), Loc::Reg(14)));
    edit.defs
        .push((ValuePart::low(Value::new(0)), Loc::Reg(15)));
    alloc
        .edits
        .set_param(block, ValuePart::low(Value::new(0)), Loc::Reg(16));
    alloc
        .edits
        .edge_mut(inst, 0)
        .push((Loc::Reg(17), Loc::Reg(18)));
    assert_eq!(
        alloc.mentioned_gprs(),
        (9..=18).fold(0, |mask, r| mask | (1 << r))
    );
}
