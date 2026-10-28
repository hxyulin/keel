//! Structural plan tests only: no machine code or target execution.
use keel_ir::{Type, rflags_region::*};

fn block(operations: Vec<Operation>, exit: Exit) -> Block {
    Block { operations, exit }
}
fn swap() -> Operation {
    Operation::Swap {
        capture: 0,
        observation: 0,
    }
}
fn simple() -> Plan {
    Plan {
        captures: vec![Type::I64],
        observations: 1,
        blocks: vec![block(
            vec![Operation::SaveEntry, swap(), Operation::ReapplyEntryImage],
            Exit::Normal,
        )],
    }
}
fn branch(taken: usize, not_taken: usize) -> Exit {
    Exit::FlagsBranch {
        condition: Condition::Equal,
        taken,
        not_taken,
    }
}
fn diamond() -> Plan {
    Plan {
        captures: vec![Type::I64],
        observations: 1,
        blocks: vec![
            block(vec![Operation::SaveEntry], branch(1, 2)),
            block(vec![swap()], Exit::Jump(3)),
            block(vec![Operation::Read { observation: 0 }], Exit::Jump(3)),
            block(vec![Operation::ReapplyEntryImage], Exit::Normal),
        ],
    }
}
fn rejects(plan: &Plan, message: &str) {
    let e = analyze(plan).unwrap_err();
    assert!(e.message.contains(message), "expected {message:?}, got {e}");
}
#[test]
fn structural_success_always_retains_all_production_obligations() {
    for p in [simple(), diamond()] {
        let report = analyze(&p).unwrap();
        assert_eq!(report.blocks, p.blocks.len());
        assert_eq!(
            report.operations,
            p.blocks.iter().map(|b| b.operations.len()).sum::<usize>()
        );
        assert_eq!(report.normal_exits, 1);
        assert_eq!(
            report.unresolved,
            [
                Obligation::LegalEntryAndNormalExit,
                Obligation::CaptureAllocationAndPublication,
                Obligation::ExactMachineExpansion,
                Obligation::TerminalExceptionBoundary,
                Obligation::AsynchronousEnvironment
            ]
        );
    }
    // No full-word I64 input value is inspected or masked by this shape analysis.
    assert_eq!(simple().captures, vec![Type::I64]);
    // Multiple normal exits are valid only when every taken arm reapplies.
    let mut p = diamond();
    p.blocks.pop();
    for b in &mut p.blocks[1..] {
        b.operations.push(Operation::ReapplyEntryImage);
        b.exit = Exit::Normal;
    }
    assert_eq!(analyze(&p).unwrap().normal_exits, 2);
}
#[test]
fn rejects_ownership_escape_and_image_reapplication_errors() {
    let mut p = simple();
    p.blocks[0].operations.remove(0);
    rejects(&p, "sole entry");
    let mut p = simple();
    p.blocks[0].operations.pop();
    rejects(&p, "normal exit");
    let mut p = simple();
    p.blocks[0].operations.push(Operation::SaveEntry);
    rejects(&p, "exactly once");
    let mut p = simple();
    p.blocks[0].operations.push(Operation::ReapplyEntryImage);
    rejects(&p, "active owner");
    let mut p = simple();
    p.blocks[0]
        .operations
        .push(Operation::Read { observation: 0 });
    rejects(&p, "active ownership");
    let mut p = simple();
    p.blocks[0].operations.push(swap());
    rejects(&p, "active ownership");
    let mut p = simple();
    p.blocks.push(block(vec![], Exit::Normal));
    p.blocks[0].exit = branch(1, 1);
    rejects(&p, "Jcc requires");
    let mut p = diamond();
    p.blocks[1].operations.push(Operation::ReapplyEntryImage);
    rejects(&p, "join mixes");
    let mut p = diamond();
    p.blocks[1].operations.insert(0, Operation::SaveEntry);
    rejects(&p, "exactly once");
    // A branch cannot hide an un-restored normal exit behind a restored one.
    let mut p = diamond();
    p.blocks[1].exit = Exit::Normal;
    rejects(&p, "normal exit");
}
#[test]
fn must_and_may_definitions_survive_cfg_merges() {
    let mut p = diamond();
    p.blocks[2].operations.clear();
    rejects(&p, "undefined observations");
    // A later definition fixes the undefined arm but overwrites the defined arm.
    p.blocks[3]
        .operations
        .insert(0, Operation::Read { observation: 0 });
    rejects(&p, "may be overwritten");
    let mut p = simple();
    p.blocks[0].operations.insert(2, swap());
    rejects(&p, "may be overwritten");
    let mut p = simple();
    p.blocks[0].operations[1] = Operation::Read { observation: 1 };
    rejects(&p, "index out of range");
    let mut p = simple();
    p.blocks[0].operations[1] = Operation::Swap {
        capture: 1,
        observation: 0,
    };
    rejects(&p, "uncaptured");
    let mut p = simple();
    p.captures.clear();
    rejects(&p, "uncaptured");
    // Both alternatives defining the same output are allowed; only one executes.
    assert!(analyze(&diamond()).is_ok());
}
#[test]
fn cfg_is_single_entry_reachable_and_acyclic() {
    let mut p = simple();
    p.blocks.clear();
    rejects(&p, "1..=64");
    let mut p = simple();
    p.blocks[0].exit = Exit::Jump(0);
    rejects(&p, "later in-range");
    let mut p = simple();
    p.blocks[0].exit = Exit::Jump(1);
    rejects(&p, "later in-range");
    let mut p = diamond();
    p.blocks[1].exit = Exit::Jump(0);
    rejects(&p, "later in-range");
    let mut p = diamond();
    p.blocks[0].exit = Exit::Jump(2);
    rejects(&p, "unreachable");
    // Restored direct edges may converge into the sole wrapper continuation.
    let mut p = diamond();
    for b in &mut p.blocks[1..3] {
        b.operations.push(Operation::ReapplyEntryImage);
    }
    p.blocks[3].operations.clear();
    assert!(analyze(&p).is_ok());
}
#[test]
fn explicit_limits_accept_boundaries_and_reject_overflow() {
    let mut p = simple();
    p.captures = vec![Type::I64; MAX_CAPTURES];
    assert!(analyze(&p).is_ok());
    p.captures.push(Type::I64);
    rejects(&p, "at most 16 exact I64");
    let mut p = simple();
    p.captures[0] = Type::I32;
    rejects(&p, "exact I64");
    let mut p = simple();
    p.observations = MAX_OBSERVATIONS + 1;
    rejects(&p, "at most 16 observations");
    let p = Plan {
        captures: vec![],
        observations: 0,
        blocks: (0..MAX_BLOCKS)
            .map(|i| {
                if i == 0 {
                    block(vec![Operation::SaveEntry], Exit::Jump(1))
                } else if i == MAX_BLOCKS - 1 {
                    block(vec![Operation::ReapplyEntryImage], Exit::Normal)
                } else {
                    block(vec![], Exit::Jump(i + 1))
                }
            })
            .collect(),
    };
    assert_eq!(analyze(&p).unwrap().blocks, MAX_BLOCKS);
    let mut bad = p;
    bad.blocks.push(block(vec![], Exit::Normal));
    rejects(&bad, "1..=64");
    // Fourteen branch nodes choose one of fifteen leaves; every leaf defines
    // all sixteen observations and reapplies. 1 + 15*(16+1) = 256 operations.
    let mut blocks = Vec::new();
    for i in 0..14 {
        blocks.push(block(
            if i == 0 {
                vec![Operation::SaveEntry]
            } else {
                vec![]
            },
            branch(14 + i, if i == 13 { 28 } else { i + 1 }),
        ));
    }
    for _ in 0..15 {
        let mut ops = (0..16)
            .map(|observation| Operation::Read { observation })
            .collect::<Vec<_>>();
        ops.push(Operation::ReapplyEntryImage);
        blocks.push(block(ops, Exit::Normal));
    }
    let mut p = Plan {
        captures: vec![],
        observations: 16,
        blocks,
    };
    let report = analyze(&p).unwrap();
    assert_eq!(report.operations, MAX_OPERATIONS);
    assert_eq!(report.normal_exits, 15);
    p.blocks[1]
        .operations
        .push(Operation::Read { observation: 0 });
    rejects(&p, "at most 256");
}

// Independent path enumerator for a family of diamonds. It follows the actual
// paths, rather than reproducing the analyzer's must/may fixed-point rules.
fn path_valid(ops: &[Operation], observation_count: usize) -> bool {
    let mut active = false;
    let mut reapplied = false;
    let mut observations = vec![false; observation_count];
    for op in ops {
        match *op {
            Operation::SaveEntry => {
                if active || reapplied {
                    return false;
                }
                active = true;
            }
            Operation::ReapplyEntryImage => {
                if !active {
                    return false;
                }
                active = false;
                reapplied = true;
            }
            Operation::Read { observation } | Operation::Swap { observation, .. } => {
                if !active || observations[observation] {
                    return false;
                }
                observations[observation] = true;
            }
        }
    }
    reapplied && observations.into_iter().all(|x| x)
}
#[test]
fn diamond_merges_match_independent_path_enumeration() {
    let variants = vec![
        vec![],
        vec![Operation::Read { observation: 0 }],
        vec![Operation::ReapplyEntryImage],
        vec![
            Operation::Read { observation: 0 },
            Operation::ReapplyEntryImage,
        ],
        vec![
            Operation::ReapplyEntryImage,
            Operation::Read { observation: 0 },
        ],
    ];
    for left in &variants {
        for right in &variants {
            for tail in &variants {
                let p = Plan {
                    captures: vec![],
                    observations: 1,
                    blocks: vec![
                        block(vec![Operation::SaveEntry], branch(1, 2)),
                        block(left.clone(), Exit::Jump(3)),
                        block(right.clone(), Exit::Jump(3)),
                        block(tail.clone(), Exit::Normal),
                    ],
                };
                let path = |arm: &Vec<Operation>| {
                    let mut ops = vec![Operation::SaveEntry];
                    ops.extend(arm);
                    ops.extend(tail);
                    path_valid(&ops, 1)
                };
                // A merge also needs equal token phases, even when a later instruction
                // might otherwise equalize state. For these variants normal-valid paths
                // already imply the same phase at this join.
                assert_eq!(
                    analyze(&p).is_ok(),
                    path(left) && path(right),
                    "left={left:?} right={right:?} tail={tail:?}"
                );
            }
        }
    }
}

const WRAPPED: &str = "func @owned(i64)->i64 {\nblock0(v0:i64):\n v1 = x86_rflags_region terminal \"save; swap 0 0; reapply; normal\"(v0)\n return v1\n}\n";
#[test]
fn wrapper_parser_closed_program_types_and_effects() {
    let m = keel_ir::parse(WRAPPED).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let (_, i) = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), keel_ir::InstData::X86RflagsRegion { .. }))
        .unwrap();
    let data = f.inst_data(i);
    assert!(data.is_pinned());
    assert!(data.may_trap());
    assert_eq!(data.memory_effects(), keel_ir::MemoryEffects::Barrier);
    let mut operands = Vec::new();
    data.for_each_operand(|v| operands.push(v));
    assert_eq!(operands.len(), 1);
    assert_eq!(
        keel_ir::parse(&m.to_string()).unwrap().to_string(),
        m.to_string()
    );
    for bad in [
        WRAPPED.replace(" terminal ", " "),
        WRAPPED.replace("terminal", "resume"),
        WRAPPED.replace("swap 0 0", "xor rax rax"),
        WRAPPED.replace("reapply; ", ""),
        WRAPPED.replace("normal", "jump 0"),
        WRAPPED.replace("swap 0 0", "swap 1 0"),
        WRAPPED.replace("normal", "normal; read 0"),
        WRAPPED.replace("reapply; normal", "jcc 16 0 0"),
        WRAPPED.replace("swap 0 0", "swap -1 0"),
    ] {
        assert!(keel_ir::parse(&bad).is_err(), "{bad}");
    }
    let bad = WRAPPED
        .replace("(i64)", "(i32)")
        .replace("v0:i64", "v0:i32");
    assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    assert!(keel_ir::Intrinsic::ALL.len() >= 731);
}
#[test]
fn programmatic_wrapper_mutations_do_not_bypass_verifier() {
    for mutation in 0..4 {
        let mut m = keel_ir::parse(WRAPPED).unwrap();
        let f = &mut m.functions[0];
        let i = f
            .layout_insts()
            .find(|(_, i)| matches!(f.inst_data(*i), keel_ir::InstData::X86RflagsRegion { .. }))
            .unwrap()
            .1;
        let keel_ir::InstData::X86RflagsRegion { plan, args } = f.inst_data_mut(i) else {
            unreachable!()
        };
        match mutation {
            0 => args.clear(),
            1 => plan.captures[0] = Type::I32,
            2 => plan.observations = 2,
            _ => {
                plan.blocks[0].operations.pop();
            }
        }
        assert!(keel_ir::verify_module(&m).is_err());
    }
}
