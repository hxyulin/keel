use keel_ir::{
    Block, BlockCall, ConvertOp, EntityRef, FuncRef, InstData, SigRef, StackSlot, Symbol, Type,
    UnaryOp, Value,
};

fn module() -> keel_ir::Module {
    keel_ir::parse(
        "func @f(f64) -> i64 {\nblock0(v0: f64):\n    v1 = iconst.i64 42\n    return v1\n}",
    )
    .unwrap()
}

fn rewrite(data: InstData) -> Vec<keel_ir::VerifierError> {
    let mut module = module();
    let f = &mut module.functions[0];
    let inst = f.layout_insts().next().unwrap().1;
    *f.inst_data_mut(inst) = data;
    keel_ir::verify_module(&module).expect_err("malformed rewrite accepted")
}

#[test]
fn same_arity_rewrites_cannot_leave_stale_result_types() {
    for data in [
        InstData::Fconst {
            ty: Type::F64,
            bits: 0,
        },
        InstData::Unary {
            op: UnaryOp::Fneg,
            arg: Value::new(0),
        },
        InstData::Convert {
            op: ConvertOp::Fptosi,
            ty: Type::I32,
            arg: Value::new(0),
        },
    ] {
        let errors = rewrite(data);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("result 0 must have type")),
            "{errors:?}"
        );
    }
}

#[test]
fn rewrites_cannot_silently_change_result_arity() {
    let errors = rewrite(InstData::Trap);
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("must have 0 result(s), got 1"))
    );
    let errors = rewrite(InstData::Overflow {
        op: keel_ir::OverflowOp::Sadd,
        args: [Value::new(1); 2],
    });
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("must have 2 result(s), got 1"))
    );
}

#[test]
fn invalid_instruction_handles_are_errors_instead_of_panics() {
    for data in [
        InstData::Unary {
            op: UnaryOp::Ineg,
            arg: Value::new(999),
        },
        InstData::Jump {
            dest: BlockCall {
                block: Block::new(999),
                args: vec![],
            },
        },
        InstData::Call {
            func: FuncRef::new(999),
            args: (vec![]).into(),
        },
        InstData::CallIndirect {
            sig: SigRef::new(999),
            callee: Value::new(0),
            args: (vec![]).into(),
        },
        InstData::StackAddr {
            slot: StackSlot::new(999),
        },
        InstData::SymbolAddr {
            symbol: Symbol::Data(keel_ir::DataRef::new(999)),
        },
        InstData::SymbolAddr {
            symbol: Symbol::Func(FuncRef::new(999)),
        },
        InstData::LandingPad {
            clauses: vec![keel_ir::Clause::Catch(Some(keel_ir::DataRef::new(999)))],
        },
    ] {
        let errors = rewrite(data);
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("reference 999 is not in the function")),
            "{errors:?}"
        );
    }
}

#[test]
fn branches_to_removed_blocks_are_rejected_before_cfg_analysis() {
    let mut module = module();
    let f = &mut module.functions[0];
    let removed = f.create_block();
    f.remove_block(removed);
    let inst = f.layout_insts().next().unwrap().1;
    *f.inst_data_mut(inst) = InstData::Jump {
        dest: BlockCall {
            block: removed,
            args: vec![],
        },
    };
    assert!(
        keel_ir::verify(f)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("block reference"))
    );
}

#[test]
fn aliased_results_of_valid_rewrites_remain_valid() {
    let mut module = keel_ir::parse("func @f(i64) -> i64 {\nblock0(v0: i64):\n    v1 = iconst.i64 0\n    v2 = iadd v0, v1\n    return v2\n}").unwrap();
    let f = &mut module.functions[0];
    f.replace_all_uses(Value::new(2), Value::new(0));
    keel_ir::verify(f).unwrap();
}

#[test]
fn parameter_and_result_indices_do_not_wrap_at_65536() {
    let mut module = keel_ir::parse("func @f() {\nblock0:\n    return\n}").unwrap();
    let f = &mut module.functions[0];
    let block = f.create_block();
    let mut last = Value::new(0);
    for _ in 0..65_537 {
        last = f.append_block_param(block, Type::I8);
    }
    assert_eq!(f.value_def(last), keel_ir::ValueDef::Param(block, 65_536));
    f.remove_block_param(block, 0);
    assert_eq!(f.value_def(last), keel_ir::ValueDef::Param(block, 65_535));
    f.append_inst(block, InstData::Return { args: vec![] });
    let mut signature = f.sig.clone();
    signature.returns = vec![keel_ir::AbiParam::new(Type::I8); 65_537];
    let callee = f.import_function("many_results", signature);
    let inst = f.insert_inst(
        keel_ir::InsertPoint::Before(f.block_insts(f.entry_block().unwrap())[0]),
        InstData::Call {
            func: callee,
            args: (vec![]).into(),
        },
    );
    let last = *f.inst_results(inst).last().unwrap();
    assert_eq!(f.value_def(last), keel_ir::ValueDef::Result(inst, 65_536));
    keel_ir::verify(f).unwrap();
}

fn expect_errors(f: &keel_ir::Function, expected: &[(keel_ir::Inst, String)]) {
    let expected: Vec<_> = expected
        .iter()
        .map(|(inst, message)| keel_ir::VerifierError {
            location: format!("{inst} `{}`", f.display_inst(*inst)),
            message: message.clone(),
        })
        .collect();
    assert_eq!(keel_ir::verify(f), Err(expected));
}

#[test]
fn sparse_instruction_positions_preserve_same_block_dominance() {
    for holes in [0, 1021, 1022, 4096] {
        sparse_instruction_positions_preserve_same_block_dominance_with_holes(holes);
    }
}

fn sparse_instruction_positions_preserve_same_block_dominance_with_holes(holes: usize) {
    let mut f = keel_ir::Function::new("sparse", keel_ir::Signature::new(&[], &[]));
    let block = f.create_block();
    remove_instructions(&mut f, block, holes);
    let def = f.append_inst(
        block,
        InstData::Iconst {
            ty: Type::I64,
            imm: 1,
        },
    );
    let value = f.first_result(def);
    let use_ = f.append_inst(
        block,
        InstData::Unary {
            op: UnaryOp::Ineg,
            arg: value,
        },
    );
    f.append_inst(block, InstData::Return { args: vec![] });
    keel_ir::verify(&f).unwrap();

    f.move_inst(def, keel_ir::InsertPoint::Before(use_));
    keel_ir::verify(&f).unwrap();
    f.move_inst(use_, keel_ir::InsertPoint::Before(def));
    expect_errors(
        &f,
        &[(
            use_,
            format!("use of {value} is not dominated by its definition"),
        )],
    );

    // A definition at the use itself must also fail, including after holes.
    let self_value = f.first_result(use_);
    *f.inst_data_mut(use_) = InstData::Unary {
        op: UnaryOp::Ineg,
        arg: self_value,
    };
    expect_errors(
        &f,
        &[(
            use_,
            format!("use of {self_value} is not dominated by its definition"),
        )],
    );
}

#[test]
fn large_call_and_asm_operands_keep_resolved_first_occurrence_order() {
    for holes in [0, 4096] {
        large_call_and_asm_operands_keep_resolved_first_occurrence_order_with_holes(holes);
    }
}

fn large_call_and_asm_operands_keep_resolved_first_occurrence_order_with_holes(holes: usize) {
    let mut f = keel_ir::Function::new("many_operands", keel_ir::Signature::new(&[], &[]));
    let block = f.create_block();
    remove_instructions(&mut f, block, holes);
    let mut defs = Vec::new();
    for n in 0..257 {
        defs.push(f.append_inst(
            block,
            InstData::Iconst {
                ty: Type::I64,
                imm: n,
            },
        ));
    }
    let values: Vec<_> = defs
        .iter()
        .rev()
        .map(|&inst| f.first_result(inst))
        .collect();
    let alias = f.append_inst(
        block,
        InstData::Iconst {
            ty: Type::I64,
            imm: 0,
        },
    );
    let alias_value = f.first_result(alias);
    f.replace_all_uses(alias_value, values[0]);
    let alias2 = f.append_inst(
        block,
        InstData::Iconst {
            ty: Type::I64,
            imm: 0,
        },
    );
    let alias2_value = f.first_result(alias2);
    f.replace_all_uses(alias2_value, alias_value);
    let mut args = vec![alias2_value, alias_value];
    args.extend(values.iter().copied());
    args.extend(values.iter().copied());
    let callee = f.import_function(
        "callee",
        keel_ir::Signature::new(&vec![Type::I64; args.len()], &[]),
    );
    let call = f.insert_inst(
        keel_ir::InsertPoint::Before(defs[0]),
        InstData::Call {
            func: callee,
            args: (args.clone()).into(),
        },
    );
    let constraints = vec!["r"; args.len()].join(",");
    let asm = f.insert_inst(
        keel_ir::InsertPoint::Before(defs[0]),
        InstData::InlineAsm {
            asm: Box::new(keel_ir::InlineAsm::new("", &constraints, &[]).unwrap()),
            args,
        },
    );
    // A zero-operand instruction between uses must not retain scratch entries.
    let zero = f.insert_inst(
        keel_ir::InsertPoint::Before(asm),
        InstData::InlineAsm {
            asm: Box::new(keel_ir::InlineAsm::new("nop", "", &[]).unwrap()),
            args: vec![],
        },
    );
    f.append_inst(block, InstData::Return { args: vec![] });
    let expected: Vec<_> = [call, asm]
        .into_iter()
        .flat_map(|inst| {
            values.iter().map(move |value| {
                (
                    inst,
                    format!("use of {value} is not dominated by its definition"),
                )
            })
        })
        .collect();
    expect_errors(&f, &expected);
    // All three become valid after moving the definitions ahead of the uses.
    for def in defs {
        f.move_inst(def, keel_ir::InsertPoint::Before(call));
    }
    keel_ir::verify(&f).unwrap();
    assert!(f.inst_results(zero).is_empty());
}

#[test]
fn switch_operands_deduplicate_aliases_and_keep_type_errors_before_removed_uses() {
    for holes in [0, 4096] {
        switch_operands_deduplicate_aliases_and_keep_type_errors_before_removed_uses_with_holes(
            holes,
        );
    }
}

fn switch_operands_deduplicate_aliases_and_keep_type_errors_before_removed_uses_with_holes(
    holes: usize,
) {
    let mut f = keel_ir::Function::new("switch_removed", keel_ir::Signature::new(&[], &[]));
    let entry = f.create_block();
    remove_instructions(&mut f, entry, holes);
    let target = f.create_block();
    f.append_block_param(target, Type::I32);
    f.append_block_param(target, Type::I64);
    let a = f.append_inst(
        entry,
        InstData::Iconst {
            ty: Type::I64,
            imm: 1,
        },
    );
    let b = f.append_inst(
        entry,
        InstData::Iconst {
            ty: Type::I64,
            imm: 2,
        },
    );
    let alias = f.append_inst(
        entry,
        InstData::Iconst {
            ty: Type::I64,
            imm: 0,
        },
    );
    let (av, bv, aliasv) = (f.first_result(a), f.first_result(b), f.first_result(alias));
    f.replace_all_uses(aliasv, av);
    let switch = f.append_inst(
        entry,
        InstData::Switch {
            arg: aliasv,
            dests: vec![
                BlockCall {
                    block: target,
                    args: vec![bv, aliasv],
                },
                BlockCall {
                    block: target,
                    args: vec![av, bv],
                },
            ],
            cases: vec![0],
        },
    );
    f.append_inst(target, InstData::Return { args: vec![] });
    f.remove_inst(a);
    f.remove_inst(b);
    f.remove_inst(alias);
    let param = f.block_params(target)[0];
    expect_errors(
        &f,
        &[
            (
                switch,
                format!("argument for {target} parameter {param} must be i32, got i64"),
            ),
            (
                switch,
                format!("argument for {target} parameter {param} must be i32, got i64"),
            ),
            (switch, format!("uses {av}, whose definition was removed")),
            (switch, format!("uses {bv}, whose definition was removed")),
        ],
    );
}

#[test]
fn malformed_operands_keep_preflight_order_and_duplicates() {
    for holes in [0, 4096] {
        malformed_operands_keep_preflight_order_and_duplicates_with_holes(holes);
    }
}

fn malformed_operands_keep_preflight_order_and_duplicates_with_holes(holes: usize) {
    let mut f = keel_ir::Function::new("bad_refs", keel_ir::Signature::new(&[], &[]));
    let block = f.create_block();
    remove_instructions(&mut f, block, holes);
    let callee = f.import_function("callee", keel_ir::Signature::new(&[], &[]));
    let call = f.append_inst(
        block,
        InstData::Call {
            func: callee,
            args: (vec![Value::new(999), Value::new(998), Value::new(999)]).into(),
        },
    );
    f.append_inst(block, InstData::Return { args: vec![] });
    assert_eq!(
        keel_ir::verify(&f),
        Err([999, 998, 999]
            .map(|index| keel_ir::VerifierError {
                location: call.to_string(),
                message: format!("value reference {index} is not in the function"),
            })
            .to_vec())
    );
}

// Removed traps create historical instruction IDs without adding values, so
// malformed operand handles remain malformed in both storage regimes.
fn remove_instructions(f: &mut keel_ir::Function, block: Block, count: usize) {
    for _ in 0..count {
        let inst = f.append_inst(block, InstData::Trap);
        f.remove_inst(inst);
    }
}

#[test]
fn holey_positions_preserve_cross_block_and_invoke_dominance() {
    let mut module = keel_ir::parse(
        "decl @may_throw(i32) -> i32
         decl @personality()
         func @f(i32) -> i32 cc(c) personality @personality {
         block0(v0: i32):
             v1 = invoke @may_throw(v0), block1, block2
         block1:
             v2 = iadd v1, v0
             return v2
         block2:
             v3, v4 = landingpad cleanup
             v5 = iadd v1, v2
             return v5
         }",
    )
    .unwrap();
    let f = &mut module.functions[0];
    let entry = f.entry_block().unwrap();
    let bad_use = f
        .layout_insts()
        .find(|&(_, inst)| {
            matches!(f.inst_data(inst), InstData::Binary { .. })
                && f.first_result(inst) == Value::new(5)
        })
        .unwrap()
        .1;
    let expected = [Value::new(1), Value::new(2)].map(|value| {
        (
            bad_use,
            format!("use of {value} is not dominated by its definition"),
        )
    });
    expect_errors(f, &expected);
    let compact_errors = keel_ir::verify(f);
    remove_instructions(f, entry, 4096);
    assert_eq!(keel_ir::verify(f), compact_errors);
    // Both ordinary and invoke results remain valid in the normal destination;
    // replacing the unwind uses with an entry parameter repairs the function.
    f.inst_data_mut(bad_use)
        .for_each_operand_mut(|v| *v = Value::new(0));
    keel_ir::verify(f).unwrap();
}

#[test]
fn holey_arenas_preserve_all_golden_verifier_outcomes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "keel"))
        .collect();
    paths.sort();
    let mut checked = 0;
    for path in paths {
        let Ok(mut module) = keel_ir::parse(&std::fs::read_to_string(&path).unwrap()) else {
            continue;
        };
        let compact = keel_ir::verify_module(&module);
        for f in &mut module.functions {
            if let Some(entry) = f.entry_block() {
                let holes = f.num_insts().saturating_mul(4).max(4096);
                remove_instructions(f, entry, holes);
            }
        }
        assert_eq!(
            keel_ir::verify_module(&module),
            compact,
            "{}",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0);
}
