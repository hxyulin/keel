//! Scalar incoming-argument preferences, checked against transfer dataflow.
use super::*;

const STORE_JOIN: &str = "
func @f(ptr, i64, i64, i8) -> i64 {
block0(v0: ptr, v1: i64, v2: i64, v3: i8):
    brif v3, block2(v2, v1), block1
block1:
    store v1, v0
    jump block2(v2, v1)
block2(v4: i64, v5: i64):
    v6 = isub v4, v5
    return v6
}";

const POOLS: Pools<'static> = Pools {
    caller_saved: &[9, 10, 11, 12],
    callee_saved: &[19],
    float_caller_saved: &[16, 17],
    float_callee_saved: &[8],
};

fn function(src: &str) -> Function {
    let mut module = keel_ir::parse(src).expect("fixture parses");
    let func = module.functions.remove(0);
    keel_ir::verify(&func).expect("fixture verifies");
    func
}

fn checked(
    func: &Function,
    pools: &Pools,
    selection: &crate::select::Selection,
    tier: AllocationTier,
) -> Allocation {
    let is_call = |d: &InstData| {
        matches!(
            d,
            InstData::Call { .. } | InstData::CallIndirect { .. } | InstData::Invoke { .. }
        )
    };
    let returns_twice = |d: &InstData| match d {
        InstData::Call { func: c, .. } | InstData::Invoke { func: c, .. } => {
            func.callee(*c).sig.returns_twice
        }
        InstData::CallIndirect { sig, .. } => func.signature(*sig).returns_twice,
        _ => false,
    };
    let input = AllocationInput {
        func,
        pools,
        selection,
        is_call: &is_call,
        returns_twice: &returns_twice,
    };
    let mut allocation = tier.allocate(&input);
    allocation.finish(&input);
    allocation
        .validate(&input)
        .expect("allocation obeys constraints");
    // The token checker exercises real edge copies and call clobbers; it does
    // not infer correctness from which hint happened to be chosen.
    for seed in 0..16 {
        super::transfer_tests::check(&input, &allocation, seed);
    }
    // Independently check whole-value interference, including inactive holes.
    let cfg = ControlFlowGraph::compute(func);
    let ivs = build_intervals(
        func,
        &cfg,
        &allocation.order,
        &is_call,
        &returns_twice,
        selection,
    );
    for (i, a) in ivs.iter().enumerate() {
        for b in &ivs[i + 1..] {
            if a.float == b.float && a.intersects(b) {
                assert_ne!(
                    allocation.locs[a.half][a.value.index()],
                    allocation.locs[b.half][b.value.index()]
                );
            }
        }
    }
    allocation
}

fn incoming(func: &Function) -> IncomingAffinities {
    let cfg = ControlFlowGraph::compute(func);
    IncomingAffinities::collect(func, &cfg.reverse_postorder(func), &|_| false)
}

fn loc(allocation: &Allocation, n: usize) -> Option<Loc> {
    allocation.loc(Value::new(n))
}

#[test]
fn conditional_store_join_has_identity_copies_with_default_selection() {
    let func = function(STORE_JOIN);
    let selection = crate::select::Selection::default();
    assert!(selection.hints.is_empty());
    let full = checked(&func, &POOLS, &selection, AllocationTier::FullAffinity);
    assert_eq!(loc(&full, 4), Some(Loc::Reg(11)));
    assert_eq!(loc(&full, 5), Some(Loc::Reg(10)));
    assert_eq!(full.spill_slots, 0);
    assert!(full.callee_saved_used.is_empty());
    let mut edges = 0;
    for &block in &full.order {
        let term = *func.block_insts(block).last().unwrap();
        for dest in func.inst_data(term).branch_dests() {
            if dest.block != Block::new(2) {
                continue;
            }
            edges += 1;
            for (&param, &arg) in func.block_params(dest.block).iter().zip(&dest.args) {
                assert_eq!(
                    full.loc(param),
                    full.loc(func.resolve(arg)),
                    "edge must need no copy"
                );
            }
        }
    }
    assert_eq!(edges, 2);
    let baseline = checked(&func, &POOLS, &selection, AllocationTier::Full);
    assert_eq!(
        loc(&baseline, 4),
        Some(Loc::Reg(9)),
        "ordinary Full retains its choice"
    );
    assert_eq!(AllocationTier::default(), AllocationTier::Full);
    let fast = checked(&func, &POOLS, &selection, AllocationTier::Fast);
    assert_eq!(loc(&fast, 4), Some(Loc::Reg(9)));
    assert_eq!(loc(&fast, 5), Some(Loc::Reg(10)));
}

#[test]
fn opcode_hint_has_priority_over_consensus() {
    let func = function(STORE_JOIN);
    let mut selection = crate::select::Selection::default();
    selection.hints.insert(Value::new(4), Value::new(0));
    let full = checked(&func, &POOLS, &selection, AllocationTier::FullAffinity);
    assert_eq!(loc(&full, 4), Some(Loc::Reg(9)));
}

#[test]
fn parallel_edges_are_independent_and_conflicts_do_not_hint() {
    for (args, expected) in [("v2, v1", 11), ("v1, v2", 9)] {
        let src = STORE_JOIN.replace("block1\nblock1:", &format!("block2({args})\nblock1:"));
        // The now-unreachable store block is deliberately retained.
        let func = function(&src);
        let prefs = incoming(&func);
        assert_eq!(prefs.sources[4].len(), 2);
        let full = checked(
            &func,
            &POOLS,
            &Default::default(),
            AllocationTier::FullAffinity,
        );
        assert_eq!(loc(&full, 4), Some(Loc::Reg(expected)));
    }
    // Separate predecessors must also agree.
    let func = function(&STORE_JOIN.replace("jump block2(v2, v1)", "jump block2(v1, v2)"));
    let full = checked(
        &func,
        &POOLS,
        &Default::default(),
        AllocationTier::FullAffinity,
    );
    assert_eq!(loc(&full, 4), Some(Loc::Reg(9)));
}

#[test]
fn aliases_are_resolved_before_consensus() {
    let mut func = function(STORE_JOIN);
    // Keep the original operand entity on one incoming edge.
    func.replace_all_uses(Value::new(1), Value::new(2));
    keel_ir::verify(&func).unwrap();
    let prefs = incoming(&func);
    assert_eq!(prefs.sources[4], [Value::new(2), Value::new(2)]);
    assert_eq!(prefs.sources[5], [Value::new(2), Value::new(2)]);
    let full = checked(
        &func,
        &POOLS,
        &Default::default(),
        AllocationTier::FullAffinity,
    );
    assert_eq!(loc(&full, 4), loc(&full, 2));
}

#[test]
fn live_source_overlap_and_duplicate_arguments_remain_distinct() {
    for src in [
        STORE_JOIN.replace("isub v4, v5", "isub v4, v2"),
        STORE_JOIN.replace("block2(v2, v1)", "block2(v2, v2)"),
    ] {
        let func = function(&src);
        let full = checked(
            &func,
            &POOLS,
            &Default::default(),
            AllocationTier::FullAffinity,
        );
        assert_ne!(loc(&full, 4), loc(&full, 5));
        if src.contains("isub v4, v2") {
            assert_ne!(loc(&full, 4), loc(&full, 2));
        }
    }
}

#[test]
fn loops_require_already_assigned_sources_and_preserve_live_inputs() {
    let func = function(
        "func @f(i64, i8) -> i64 {
block0(v0: i64, v1: i8):
    jump block1(v0)
block1(v2: i64):
    v3 = iadd v2, v0
    brif v1, block1(v3), block2
block2:
    return v3
}",
    );
    let prefs = incoming(&func);
    assert_eq!(prefs.sources[2], [Value::new(0), Value::new(3)]);
    let mut locs = [vec![None; func.num_values()], vec![None; func.num_values()]];
    locs[0][0] = Some(Loc::Reg(9));
    assert_eq!(
        prefs.consensus(Value::new(2), &locs),
        None,
        "backedge unassigned"
    );
    let full = checked(
        &func,
        &POOLS,
        &Default::default(),
        AllocationTier::FullAffinity,
    );
    assert_ne!(loc(&full, 0), loc(&full, 2));
}

#[test]
fn entry_self_loop_is_rejected_and_never_gets_a_preference() {
    // Entry backedges are invalid IR, but must not manufacture an affinity.
    let module = keel_ir::parse(
        "func @f(i64) {
block0(v0: i64):
    jump block0(v0)
}",
    )
    .expect("entry-backedge fixture parses");
    let func = &module.functions[0];
    let result = keel_ir::verify(func);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].location, "block0");
    assert_eq!(
        errors[0].message,
        "the entry block cannot be a branch target"
    );
    assert!(incoming(func).sources[0].is_empty());
}

#[test]
fn scalar_fp_vector_and_i128_fall_back_without_affinities() {
    for ty in ["f32", "f64", "i128", "i32x4"] {
        let func = function(&format!(
            "func @f({ty}, i8) -> {ty} {{
block0(v0: {ty}, v1: i8):
    brif v1, block1(v0), block1(v0)
block1(v2: {ty}):
    return v2
}}"
        ));
        assert!(incoming(&func).sources[2].is_empty());
        checked(
            &func,
            &POOLS,
            &Default::default(),
            AllocationTier::FullAffinity,
        );
    }
}

#[test]
fn calls_and_returns_twice_keep_their_constraints() {
    let src = STORE_JOIN
        .replace("func @f", "decl @g()\nfunc @f")
        .replace("v6 = isub", "call @g()\n    v6 = isub");
    let func = function(&src);
    let full = checked(
        &func,
        &POOLS,
        &Default::default(),
        AllocationTier::FullAffinity,
    );
    for n in [4, 5] {
        assert!(!matches!(loc(&full, n), Some(Loc::Reg(9..=12))));
    }
    let func = function(&src.replace("decl @g()", "decl @g() returns_twice"));
    let cfg = ControlFlowGraph::compute(&func);
    assert!(
        IncomingAffinities::collect(&func, &cfg.reverse_postorder(&func), &|d| matches!(
            d,
            InstData::Call { .. }
        ))
        .sources
        .is_empty()
    );
    checked(
        &func,
        &POOLS,
        &Default::default(),
        AllocationTier::FullAffinity,
    );
}

#[test]
fn tiny_pools_spilled_sources_and_existing_callee_homes_are_safe() {
    let func = function(STORE_JOIN);
    for (caller_saved, callee_saved) in [(&[9][..], &[][..]), (&[][..], &[19, 20, 21, 22][..])] {
        let pools = Pools {
            caller_saved,
            callee_saved,
            ..POOLS
        };
        let full = checked(
            &func,
            &pools,
            &Default::default(),
            AllocationTier::FullAffinity,
        );
        if caller_saved.is_empty() {
            assert_eq!(loc(&full, 4), loc(&full, 2));
            assert_eq!(loc(&full, 5), loc(&full, 1));
            assert_eq!(full.callee_saved_used, [19, 20, 21, 22]);
            assert_eq!(full.spill_slots, 0);
        }
    }
    let prefs = incoming(&func);
    for source in [
        None,
        Some(Loc::Stack(0)),
        Some(Loc::Remat(Value::new(2))),
        Some(Loc::FReg(11)),
    ] {
        let mut locs = [vec![None; func.num_values()], vec![None; func.num_values()]];
        locs[0][2] = source;
        assert_eq!(prefs.consensus(Value::new(4), &locs), None);
    }
}

#[test]
fn invoke_targets_are_excluded() {
    let mut func = function(
        "decl @g() -> i64
 decl @personality()
 func @f(i64) -> i64 personality @personality {
 block0(v0: i64):
     v1 = invoke @g(), block1, block2
 block1:
     return v1
 block2:
     v2, v3 = landingpad cleanup
     return v0
 }",
    );
    checked(
        &func,
        &POOLS,
        &Default::default(),
        AllocationTier::FullAffinity,
    );
    // Verified IR disallows parameters at Invoke targets. Defensively test
    // exclusion if a caller presents such a target before verification.
    let term = *func.block_insts(Block::new(0)).last().unwrap();
    for (successor, target) in [(0, 1), (1, 2)] {
        let param = func.append_block_param(Block::new(target), Type::I64);
        func.inst_data_mut(term).branch_dests_mut()[successor]
            .args
            .push(Value::new(0));
        assert!(keel_ir::verify(&func).is_err());
        // A source really exists; only Invoke exclusion removes the hint.
        assert!(incoming(&func).sources[param.index()].is_empty());
    }
}

#[test]
fn returns_twice_function_signature_disables_preferences() {
    let mut func = function(STORE_JOIN);
    func.sig.returns_twice = true;
    assert!(incoming(&func).sources.is_empty());
}

#[test]
fn inactive_interference_blocks_an_otherwise_available_consensus_register() {
    // v0 has expired, but v1 has reclaimed r9 in a hole overlapping v2's
    // future range. The incoming source alone is insufficient evidence.
    let iv = |n, ranges| Interval {
        value: Value::new(n),
        half: 0,
        ranges: Ranges::from(ranges),
        crosses_call: false,
        preserved_gprs: 0,
        float: false,
        vector: false,
        spill_units: 1,
    };
    let intervals = [
        iv(0, vec![(1, 2)]),
        iv(1, vec![(3, 4), (9, 12)]),
        iv(2, vec![(5, 10)]),
    ];
    let mut locs = [vec![None; 3], vec![None; 3]];
    let incoming = IncomingAffinities {
        sources: vec![vec![], vec![], vec![Value::new(0)]],
    };
    let spilled = linear_scan_with_preference(
        &intervals,
        &POOLS,
        &mut locs,
        &Default::default(),
        |cur, locs| incoming.consensus(cur.value, locs),
        |a, b| b.end().cmp(&a.end()),
    );
    assert!(spilled.is_empty());
    assert_eq!(
        locs[0],
        [Some(Loc::Reg(9)), Some(Loc::Reg(9)), Some(Loc::Reg(10))]
    );
    assert!(intervals[1].intersects(&intervals[2]));
}

#[test]
fn scalar_gpr_classes_and_parameter_permutations_can_reuse_homes() {
    for ty in ["i8", "i16", "i32", "i64", "ptr"] {
        for args in ["v1, v0", "v0, v1"] {
            let func = function(&format!(
                "func @f({ty}, {ty}, i8) -> {ty} {{
block0(v0: {ty}, v1: {ty}, v2: i8):
    brif v2, block1({args}), block1({args})
block1(v3: {ty}, v4: {ty}):
    return v3
}}"
            ));
            let full = checked(
                &func,
                &POOLS,
                &Default::default(),
                AllocationTier::FullAffinity,
            );
            let first = if args == "v1, v0" { 1 } else { 0 };
            assert_eq!(loc(&full, 3), loc(&full, first));
            assert_eq!(loc(&full, 4), loc(&full, 1 - first));
            assert_eq!(full.spill_slots, 0);
            assert!(full.callee_saved_used.is_empty());
        }
    }
}
