//! Host-only structural tests: no emitted instructions or raw stack execution.
use RawReg::{A, AnchorDx, Bp, C, Sp};
use Width::{W16, W32, W64};
use keel_ir::{Type, stack_region::*};

fn report(ops: Vec<Operation>, observations: u8) -> StructuralReport {
    analyze(
        &Plan {
            operations: ops,
            observations,
        },
        &INPUT_TYPES,
    )
    .unwrap()
}
fn finished(op: Operation) -> StructuralReport {
    report(vec![op, Operation::Finish], 0)
}
fn mem(base: RawReg) -> Mem64 {
    Mem64 {
        base: Some(base),
        index: None,
        scale: 1,
        disp: 0,
    }
}
fn rejects(ops: Vec<Operation>, observations: u8, expected: &str) {
    let e = analyze(
        &Plan {
            operations: ops,
            observations,
        },
        &INPUT_TYPES,
    )
    .unwrap_err();
    assert!(e.message.contains(expected), "{e}; expected {expected}");
}

#[test]
fn structural_success_never_certifies_caller_memory_or_monitor() {
    let r = report(vec![Operation::Finish], 0);
    assert_eq!(
        r.unresolved,
        [
            Obligation::CallerArenaFootprint,
            Obligation::CaptureAllocationAndPublication,
            Obligation::ExactMachineExpansion,
            Obligation::CompilerIdentityRestoration,
            Obligation::CfaAndObjectRepresentation,
            Obligation::LegalEntryAndNormalExit,
            Obligation::IndependentStackTerminalMonitor,
            Obligation::AsynchronousEnvironment,
        ]
    );
    assert!(!r.has_data_loaded_stack_or_frame);
    let r = finished(Operation::Pop {
        width: W64,
        dst: PopDest::Reg(Sp),
    });
    assert!(r.has_data_loaded_stack_or_frame);
    assert!(r.unresolved.contains(&Obligation::CallerArenaFootprint));
    assert!(
        r.unresolved
            .contains(&Obligation::IndependentStackTerminalMonitor)
    );
    assert!(
        r.effects
            .iter()
            .all(|e| e.terminal_boundary && e.flags == FlagsEffect::None)
    );
    assert_eq!(r.effects[1].stack, StackTransition::RestoreCompilerIdentity);
    // Finish is only a descriptor intent. It does not remove any obligation.
    assert_eq!(
        r.effects[1].frame_after,
        FrameKnowledge::CompilerEntryIdentity
    );
}

#[test]
fn six_capture_types_and_all_plan_bounds_are_checked() {
    let p = Plan {
        operations: vec![Operation::Finish],
        observations: 0,
    };
    for n in [0, 1, 5, 7] {
        assert!(analyze(&p, &vec![Type::I64; n]).is_err());
    }
    for i in 0..6 {
        let mut types = INPUT_TYPES;
        types[i] = if i == 0 { Type::I64 } else { Type::Ptr };
        assert!(analyze(&p, &types).is_err());
    }
    rejects(vec![], 0, "1..=64");
    rejects(vec![Operation::Finish], 17, "at most 16");
    let push = Operation::Push {
        width: W16,
        src: PushSource::Imm8(-1),
    };
    let mut ops = vec![push; 63];
    ops.push(Operation::Finish);
    assert_eq!(report(ops.clone(), 0).effects.len(), 64);
    ops.insert(0, push);
    rejects(ops, 0, "1..=64");
}

#[test]
fn finish_and_observation_mutations_cannot_hide_missing_or_repeated_state() {
    let obs = Operation::Observe { reg: Bp, slot: 0 };
    rejects(vec![obs], 1, "missing sole finish");
    rejects(vec![Operation::Finish, obs], 1, "after sole finish");
    rejects(
        vec![Operation::Finish, Operation::Finish],
        0,
        "after sole finish",
    );
    rejects(vec![obs, obs, Operation::Finish], 1, "more than once");
    rejects(vec![obs, Operation::Finish], 0, "out of range");
    rejects(vec![Operation::Finish], 1, "undefined");
    let mut ops: Vec<_> = (0..16)
        .rev()
        .map(|slot| Operation::Observe { reg: Sp, slot })
        .collect();
    ops.push(Operation::Finish);
    let r = report(ops, 16);
    assert_eq!(r.result_types, vec![Type::I64; 16]);
    assert_eq!(
        r.effects[0].memory,
        [MemoryEvent::ObservationHomeWrite64 { slot: 15 }]
    );
}

#[test]
fn legal_register_and_immediate_forms_retain_exact_encoding_intent() {
    for width in [W16, W64] {
        for reg in [A, C, Sp, Bp] {
            for op in [
                Operation::Push {
                    width,
                    src: PushSource::Reg(reg),
                },
                Operation::Pop {
                    width,
                    dst: PopDest::Reg(reg),
                },
            ] {
                assert_eq!(finished(op).effects[0].intent, op);
            }
        }
        for immediate in [i8::MIN, -1, 0, i8::MAX] {
            let op = Operation::Push {
                width,
                src: PushSource::Imm8(immediate),
            };
            assert_eq!(finished(op).effects[0].intent, op);
        }
    }
    for immediate in [0, 32767, 32768, u16::MAX] {
        let op = Operation::Push {
            width: W16,
            src: PushSource::Imm16(immediate),
        };
        assert_eq!(finished(op).effects[0].intent, op);
    }
    for immediate in [i32::MIN, -1, 0, i32::MAX] {
        let op = Operation::Push {
            width: W64,
            src: PushSource::Imm32(immediate),
        };
        assert_eq!(finished(op).effects[0].intent, op);
    }
    rejects(
        vec![
            Operation::Push {
                width: W64,
                src: PushSource::Imm16(1),
            },
            Operation::Finish,
        ],
        0,
        "imm16",
    );
    rejects(
        vec![
            Operation::Push {
                width: W16,
                src: PushSource::Imm32(1),
            },
            Operation::Finish,
        ],
        0,
        "imm32",
    );
    for op in [
        Operation::Push {
            width: W32,
            src: PushSource::Reg(A),
        },
        Operation::Pop {
            width: W32,
            dst: PopDest::Reg(A),
        },
        Operation::Enter {
            width: W32,
            alloc: 0,
            nesting: 0,
        },
        Operation::Leave { width: W32 },
    ] {
        rejects(vec![op, Operation::Finish], 0, "32-bit");
    }
}

#[test]
fn addresses_enumerate_encodable_forms_and_reject_anchor_scale_index_mutations() {
    for base in [None, Some(A), Some(C), Some(Sp), Some(Bp)] {
        for index in [None, Some(A), Some(C), Some(Bp)] {
            for scale in [1, 2, 4, 8] {
                if index.is_none() && scale != 1 {
                    continue;
                }
                for disp in [i32::MIN as i64, -1, 0, i32::MAX as i64] {
                    let address = Mem64 {
                        base,
                        index,
                        scale,
                        disp,
                    };
                    for width in [W16, W64] {
                        finished(Operation::Push {
                            width,
                            src: PushSource::Mem(address),
                        });
                        finished(Operation::Pop {
                            width,
                            dst: PopDest::Mem(address),
                        });
                    }
                }
            }
        }
    }
    let invalid = [
        (
            Mem64 {
                base: Some(AnchorDx),
                ..mem(A)
            },
            "RDX",
        ),
        (
            Mem64 {
                index: Some(AnchorDx),
                ..mem(A)
            },
            "RDX",
        ),
        (
            Mem64 {
                index: Some(Sp),
                ..mem(A)
            },
            "RSP cannot",
        ),
        (Mem64 { scale: 2, ..mem(A) }, "scale"),
        (
            Mem64 {
                index: Some(C),
                scale: 3,
                ..mem(A)
            },
            "scale",
        ),
        (
            Mem64 {
                disp: i32::MAX as i64 + 1,
                ..mem(A)
            },
            "signed32",
        ),
        (
            Mem64 {
                disp: i32::MIN as i64 - 1,
                ..mem(A)
            },
            "signed32",
        ),
    ];
    for (address, error) in invalid {
        for op in [
            Operation::Push {
                width: W64,
                src: PushSource::Mem(address),
            },
            Operation::Pop {
                width: W16,
                dst: PopDest::Mem(address),
            },
        ] {
            rejects(vec![op, Operation::Finish], 0, error);
        }
    }
    for op in [
        Operation::Push {
            width: W64,
            src: PushSource::Reg(AnchorDx),
        },
        Operation::Pop {
            width: W16,
            dst: PopDest::Reg(AnchorDx),
        },
        Operation::Observe {
            reg: AnchorDx,
            slot: 0,
        },
    ] {
        rejects(vec![op, Operation::Finish], 0, "RDX");
    }
}

#[test]
fn independent_old_sp_push_and_postincrement_pop_templates() {
    let address = mem(Sp);
    let push = finished(Operation::Push {
        width: W64,
        src: PushSource::Mem(address),
    });
    assert_eq!(
        push.effects[0].memory,
        [
            MemoryEvent::ExplicitRead {
                address,
                bytes: 8,
                at: AddressMoment::BeforeStackChange
            },
            MemoryEvent::StackPush { bytes: 8 },
        ]
    );
    let pop = finished(Operation::Pop {
        width: W16,
        dst: PopDest::Mem(address),
    });
    assert_eq!(
        pop.effects[0].memory,
        [
            MemoryEvent::StackRead { bytes: 2 },
            MemoryEvent::ExplicitWrite {
                address,
                bytes: 2,
                at: AddressMoment::AfterPopIncrement
            },
        ]
    );
    let sp_push = finished(Operation::Push {
        width: W16,
        src: PushSource::Reg(Sp),
    });
    assert_eq!(sp_push.effects[0].stack, StackTransition::Push { bytes: 2 });
    assert!(sp_push.effects[0].register_reads.contains(&Sp));
    assert_eq!(
        finished(Operation::Pop {
            width: W16,
            dst: PopDest::Reg(Sp)
        })
        .effects[0]
            .stack,
        StackTransition::PopIntoSp
    );
    assert_eq!(
        finished(Operation::Pop {
            width: W64,
            dst: PopDest::Reg(Sp)
        })
        .effects[0]
            .stack,
        StackTransition::PopIntoRsp
    );
}

#[test]
fn full_register_observation_after_partial_pop_has_no_numeric_stack_oracle() {
    let r = report(
        vec![
            Operation::Pop {
                width: W16,
                dst: PopDest::Reg(Bp),
            },
            Operation::Observe { reg: Bp, slot: 0 },
            Operation::Finish,
        ],
        1,
    );
    assert_eq!(
        r.effects[0].register_writes,
        [
            RegisterWrite::Full64(Sp),
            RegisterWrite::Low16PreserveUpper(Bp),
        ]
    );
    assert_eq!(r.effects[1].frame_after, FrameKnowledge::Partial16Write);
    assert_eq!(r.result_types, [Type::I64]);
    assert_eq!(
        r.effects[1].memory,
        [MemoryEvent::ObservationHomeWrite64 { slot: 0 }]
    );
}

#[test]
fn enter_reference_cases_pin_display_count_order_and_final_permission_check() {
    // Explicit expected traces for the semantic boundaries, not copied loops.
    for (nesting, expected) in [
        (
            0,
            vec![
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::FinalSpWritePermissionCheck,
            ],
        ),
        (
            1,
            vec![
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::FinalSpWritePermissionCheck,
            ],
        ),
        (
            2,
            vec![
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::DisplayRead {
                    ordinal: 1,
                    bytes: 8,
                },
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::FinalSpWritePermissionCheck,
            ],
        ),
        (
            32,
            vec![
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::FinalSpWritePermissionCheck,
            ],
        ),
        (
            33,
            vec![
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::StackPush { bytes: 8 },
                MemoryEvent::FinalSpWritePermissionCheck,
            ],
        ),
    ] {
        assert_eq!(
            finished(Operation::Enter {
                width: W64,
                alloc: 0,
                nesting
            })
            .effects[0]
                .memory,
            expected
        );
    }
    for width in [W16, W64] {
        for nesting in 0..=255 {
            for alloc in [0, 1, 32767, 32768, 65535] {
                let op = Operation::Enter {
                    width,
                    alloc,
                    nesting,
                };
                let r = finished(op);
                assert_eq!(r.effects[0].intent, op); // no imm8 normalization in encoding
                let trace = &r.effects[0].memory;
                let reads = trace
                    .iter()
                    .filter(|e| matches!(e, MemoryEvent::DisplayRead { .. }))
                    .count();
                let pushes = trace
                    .iter()
                    .filter(|e| matches!(e, MemoryEvent::StackPush { .. }))
                    .count();
                let level = usize::from(nesting % 32);
                assert_eq!(reads, level.saturating_sub(1));
                assert_eq!(pushes, if level == 0 { 1 } else { level + 1 });
                assert_eq!(
                    trace.last(),
                    Some(&MemoryEvent::FinalSpWritePermissionCheck)
                );
            }
        }
    }
    let r = finished(Operation::Enter {
        width: W64,
        alloc: 65535,
        nesting: 255,
    });
    assert_eq!(
        r.effects[0].stack,
        StackTransition::Enter {
            bytes: 8,
            pushes: 32,
            alloc: 65535
        }
    );
    assert_eq!(r.effects[0].memory.len(), 63); //32 writes +30 reads +check
    assert_eq!(
        r.effects[0].memory[59],
        MemoryEvent::DisplayRead {
            ordinal: 30,
            bytes: 8
        }
    );
}

#[test]
fn enter16_unknown_survives_observation_and_later_address_use() {
    let r = report(
        vec![
            Operation::Enter {
                width: W16,
                alloc: 65535,
                nesting: 2,
            },
            Operation::Observe { reg: Bp, slot: 0 },
            Operation::Push {
                width: W16,
                src: PushSource::Mem(mem(Bp)),
            },
            Operation::Finish,
        ],
        1,
    );
    for effect in &r.effects[..3] {
        assert_eq!(effect.frame_after, FrameKnowledge::UnknownEnter16FrameTemp);
    }
    assert!(
        r.effects[0]
            .register_writes
            .contains(&RegisterWrite::UnknownEnter16Bp)
    );
    assert!(
        !r.effects[0]
            .register_writes
            .contains(&RegisterWrite::Full64(Bp))
    );
    assert!(
        !r.effects[0]
            .register_writes
            .contains(&RegisterWrite::Low16PreserveUpper(Bp))
    );
    assert!(r.unresolved.contains(&Obligation::CallerArenaFootprint));
    assert_eq!(
        r.effects[0].memory,
        [
            MemoryEvent::StackPush { bytes: 2 },
            MemoryEvent::DisplayRead {
                ordinal: 1,
                bytes: 2
            },
            MemoryEvent::StackPush { bytes: 2 },
            MemoryEvent::StackPush { bytes: 2 },
            MemoryEvent::FinalSpWritePermissionCheck,
        ]
    );
}

#[test]
fn leave16_uses_full_bp_for_stack_then_partial_bp_pop() {
    let r = finished(Operation::Leave { width: W16 });
    assert_eq!(r.effects[0].register_reads, [Bp]);
    assert_eq!(
        r.effects[0].register_writes,
        [
            RegisterWrite::Full64(Sp),
            RegisterWrite::Low16PreserveUpper(Bp)
        ]
    );
    assert_eq!(r.effects[0].stack, StackTransition::Leave { bytes: 2 });
    assert_eq!(r.effects[0].memory, [MemoryEvent::StackRead { bytes: 2 }]);
    let r = finished(Operation::Leave { width: W64 });
    assert_eq!(
        r.effects[0].register_writes,
        [RegisterWrite::Full64(Sp), RegisterWrite::Full64(Bp)]
    );
    assert_eq!(r.effects[0].frame_after, FrameKnowledge::LoadedFromStack);
}

#[test]
fn enter16_partial_bp_writes_retain_uncertainty_for_full_observation_and_address() {
    // R1-01 repros: the two partial writers differ in their SP dependency.
    for (partial, after) in [
        (
            Operation::Pop {
                width: W16,
                dst: PopDest::Reg(Bp),
            },
            Enter16Dependencies {
                bp: true,
                sp: false,
            },
        ),
        (
            Operation::Leave { width: W16 },
            Enter16Dependencies { bp: true, sp: true },
        ),
    ] {
        let r = report(
            vec![
                Operation::Enter {
                    width: W16,
                    alloc: 0,
                    nesting: 0,
                },
                partial,
                Operation::Observe { reg: Bp, slot: 0 },
                Operation::Push {
                    width: W64,
                    src: PushSource::Mem(mem(Bp)),
                },
                Operation::Observe { reg: Sp, slot: 1 },
                Operation::Push {
                    width: W64,
                    src: PushSource::Mem(mem(Sp)),
                },
                Operation::Pop {
                    width: W16,
                    dst: PopDest::Reg(Sp),
                },
                Operation::Finish,
            ],
            2,
        );
        assert_eq!(r.effects[1].frame_after, FrameKnowledge::Partial16Write);
        assert_eq!(
            r.effects[1].enter16_before,
            Enter16Dependencies {
                bp: true,
                sp: false
            }
        );
        // Includes full64 BP/SP observations, both address bases, ordinary
        // stack increments and POP16 SP. None replaces the retained upper bits.
        for effect in &r.effects[2..7] {
            assert_eq!(effect.enter16_before, after);
            assert_eq!(effect.enter16_after, after);
        }
        assert_eq!(r.result_types, [Type::I64, Type::I64]);
        assert_eq!(
            r.effects[2].memory,
            [MemoryEvent::ObservationHomeWrite64 { slot: 0 }]
        );
        assert_eq!(
            r.effects[3].memory[0],
            MemoryEvent::ExplicitRead {
                address: mem(Bp),
                bytes: 8,
                at: AddressMoment::BeforeStackChange,
            }
        );
        assert_eq!(
            r.effects[5].memory[0],
            MemoryEvent::ExplicitRead {
                address: mem(Sp),
                bytes: 8,
                at: AddressMoment::BeforeStackChange,
            }
        );
        assert_eq!(r.effects[7].enter16_before, after);
        assert_eq!(
            r.effects[7].enter16_after,
            Enter16Dependencies {
                bp: false,
                sp: false
            }
        );
        assert_eq!(r.unresolved, UNRESOLVED);
    }
}

#[test]
fn full_replacement_controls_do_not_clear_another_registers_dependency() {
    let r = report(
        vec![
            Operation::Enter {
                width: W16,
                alloc: 0,
                nesting: 0,
            },
            Operation::Leave { width: W16 },
            Operation::Pop {
                width: W64,
                dst: PopDest::Reg(Bp),
            },
            Operation::Observe { reg: Sp, slot: 0 },
            // Full BP replacement from SP inherits uncertain SP; width alone is
            // not justification to discard the provenance.
            Operation::Enter {
                width: W64,
                alloc: 0,
                nesting: 0,
            },
            Operation::Pop {
                width: W64,
                dst: PopDest::Reg(Sp),
            },
            Operation::Leave { width: W64 },
            Operation::Observe { reg: Sp, slot: 1 },
            Operation::Pop {
                width: W64,
                dst: PopDest::Reg(Sp),
            },
            Operation::Finish,
        ],
        2,
    );
    let bp_only = Enter16Dependencies {
        bp: true,
        sp: false,
    };
    let both = Enter16Dependencies { bp: true, sp: true };
    let sp_only = Enter16Dependencies {
        bp: false,
        sp: true,
    };
    let neither = Enter16Dependencies {
        bp: false,
        sp: false,
    };
    let expected = [
        bp_only, both, sp_only, sp_only, both, bp_only, sp_only, sp_only, neither, neither,
    ];
    for (i, expected_after) in expected.into_iter().enumerate() {
        assert_eq!(r.effects[i].enter16_after, expected_after, "operation {i}");
        assert_eq!(
            r.effects[i].enter16_before,
            if i == 0 { neither } else { expected[i - 1] }
        );
    }
    assert_eq!(r.unresolved, UNRESOLVED);
}

#[test]
fn independent_full_bp_replacements_and_finish_clear_only_retained_register_bits() {
    // With independent SP, POP64 BP and ENTER64 each replace BP's retained
    // ambiguous bits. LEAVE64 replaces BP but first transfers uncertainty to SP.
    for (replacement, expected) in [
        (
            Operation::Pop {
                width: W64,
                dst: PopDest::Reg(Bp),
            },
            Enter16Dependencies {
                bp: false,
                sp: false,
            },
        ),
        (
            Operation::Enter {
                width: W64,
                alloc: 65535,
                nesting: 31,
            },
            Enter16Dependencies {
                bp: false,
                sp: false,
            },
        ),
        (
            Operation::Leave { width: W64 },
            Enter16Dependencies {
                bp: false,
                sp: true,
            },
        ),
    ] {
        let r = report(
            vec![
                Operation::Enter {
                    width: W16,
                    alloc: 0,
                    nesting: 0,
                },
                Operation::Pop {
                    width: W16,
                    dst: PopDest::Reg(Bp),
                },
                replacement,
                Operation::Observe { reg: Bp, slot: 0 },
                Operation::Push {
                    width: W64,
                    src: PushSource::Mem(mem(Bp)),
                },
                Operation::Finish,
            ],
            1,
        );
        for effect in &r.effects[2..5] {
            assert_eq!(effect.enter16_after, expected);
        }
        assert_eq!(r.effects[5].enter16_before, expected);
        assert_eq!(
            r.effects[5].enter16_after,
            Enter16Dependencies {
                bp: false,
                sp: false
            }
        );
        // Even an entirely cleared provenance marker NEVER proves memory safety.
        assert_eq!(r.unresolved, UNRESOLVED);
        assert!(r.unresolved.contains(&Obligation::CallerArenaFootprint));
    }
}
