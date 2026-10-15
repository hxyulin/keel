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
            args: vec![],
        },
        InstData::CallIndirect {
            sig: SigRef::new(999),
            callee: Value::new(0),
            args: vec![],
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
            args: vec![],
        },
    );
    let last = *f.inst_results(inst).last().unwrap();
    assert_eq!(f.value_def(last), keel_ir::ValueDef::Result(inst, 65_536));
    keel_ir::verify(f).unwrap();
}
