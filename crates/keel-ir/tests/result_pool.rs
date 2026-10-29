#[path = "result_pool/imports.rs"]
mod imports;

use keel_ir::{
    EntityRef, Function, InsertPoint, InstData, Signature, SourceFile, SourceLoc, Type, ValueDef,
};

#[test]
fn builder_result_arities_preserve_order_handles_locations_and_aliases() {
    for count in [0, 1, 2, 3, 17, 257, 65_537] {
        let types: Vec<_> = [Type::I8, Type::I32, Type::Ptr, Type::F64]
            .into_iter()
            .cycle()
            .take(count)
            .collect();
        let mut f = Function::new("caller", Signature::new(&[Type::Ptr], &[]));
        let block = f.create_block();
        let pointer = f.append_block_param(block, Type::Ptr);
        let callee = f.import_function("callee", Signature::new(&[], &types));
        let sig = f.import_signature(Signature::new(&[], &types));
        let ret = f.ins(InsertPoint::End(block)).return_(&[]);
        let loc = SourceLoc {
            file: SourceFile::new(0),
            line: 12,
            col: 7,
            scope: None,
        };
        let mut scratch = vec![Type::F64; 3];
        let direct = f
            .ins_with_scratch(InsertPoint::Before(ret), &mut scratch)
            .loc(Some(loc))
            .call(callee, &[]);
        let indirect = f
            .ins(InsertPoint::Before(ret))
            .call_indirect(sig, pointer, &[]);
        assert_eq!(f.block_insts(block), &[direct, indirect, ret]);
        assert_eq!(f.inst_loc(direct), Some(loc));
        assert_eq!(f.inst_loc(indirect), None);
        assert!(f.inst_results(ret).is_empty());
        for inst in [direct, indirect] {
            assert_eq!(f.inst_results(inst).len(), count);
            for (n, (&value, &ty)) in f.inst_results(inst).iter().zip(&types).enumerate() {
                assert_eq!(f.value_type(value), ty);
                assert_eq!(f.value_def(value), ValueDef::Result(inst, n as u32));
            }
        }
        for (&old, &new) in f
            .inst_results(direct)
            .to_vec()
            .iter()
            .zip(f.inst_results(indirect).to_vec().iter())
        {
            f.replace_all_uses(old, new);
            assert_eq!(f.resolve(old), new);
        }
        keel_ir::verify(&f).unwrap();
    }
}

// The parser supplies result types directly. Reinsert its instructions through
// inference, comparing every type and result index, including exceptional edges.
fn reinsert(source: &str) {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let expected = module.to_string();
    for f in &mut module.functions {
        let insts: Vec<_> = f.layout_insts().map(|(_, inst)| inst).collect();
        for old in insts {
            let data = f.inst_data(old).clone();
            let loc = f.inst_loc(old);
            let old_results = f.inst_results(old).to_vec();
            let mut scratch = vec![Type::I8; 5];
            let new_inst = f
                .ins_with_scratch(InsertPoint::Before(old), &mut scratch)
                .loc(loc)
                .build(data);
            let new_results = f.inst_results(new_inst).to_vec();
            assert_eq!(old_results.len(), new_results.len());
            for (n, (old, new)) in old_results.into_iter().zip(new_results).enumerate() {
                assert_eq!(f.value_type(old), f.value_type(new));
                assert_eq!(f.value_def(new), ValueDef::Result(new_inst, n as u32));
                f.replace_all_uses(old, new);
            }
            f.remove_inst(old);
        }
    }
    keel_ir::verify_module(&module).unwrap();
    let printed = module.to_string();
    let reparsed = keel_ir::parse(&printed).unwrap();
    keel_ir::verify_module(&reparsed).unwrap();
    // Reinsertion leaves sparse value handles; parsing assigns dense handles.
    // Compare against the original parsed module in that same canonical naming.
    // This checks instruction/result order, types, operands and exception edges,
    // while the checks above validate the actual newly allocated result indices.
    assert_eq!(expected, reparsed.to_string());
}

#[test]
fn inferred_exception_results_match_parser() {
    reinsert(include_str!("golden/exceptions.keel"));
}

#[test]
fn inferred_overflow_results_match_parser() {
    reinsert(include_str!("golden/overflow.keel"));
}

#[test]
fn inferred_five_result_invoke_matches_parser() {
    reinsert(
        "decl @many() -> (i8, f64, ptr, i32, i64)
decl @personality()
func @f() -> (i8, f64, ptr, i32, i64) personality @personality {
block0:
    v0, v1, v2, v3, v4 = invoke @many(), block1, block2
block1:
    return v0, v1, v2, v3, v4
block2:
    v5, v6 = landingpad cleanup
    trap
}",
    );
}

#[test]
fn invalid_callee_panics_before_creating_an_instruction() {
    let mut f = Function::new("f", Signature::new(&[], &[]));
    let block = f.create_block();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.append_inst(
            block,
            InstData::Call {
                func: keel_ir::FuncRef::new(999),
                args: (vec![]).into(),
            },
        );
    }));
    assert!(result.is_err());
    assert_eq!(f.num_insts(), 0);
    assert!(f.block_insts(block).is_empty());
}

#[test]
fn scratch_reuse_panic_recovery_and_detached_clone_independence() {
    let mut f = Function::new("f", Signature::new(&[], &[]));
    let b = f.create_block();
    let mut scratch = Vec::new();
    let a = f
        .ins_with_scratch(InsertPoint::End(b), &mut scratch)
        .iconst(Type::I64, 42);
    let capacity = scratch.capacity();
    let first = f.layout_insts().next().unwrap().1;
    for _ in 0..20 {
        f.ins_with_scratch(InsertPoint::Before(first), &mut scratch)
            .iconst(Type::I32, 1);
        assert_eq!(scratch.capacity(), capacity);
    }
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.ins_with_scratch(InsertPoint::End(b), &mut scratch)
            .call(keel_ir::FuncRef::new(999), &[]);
    }));
    assert!(failure.is_err());
    let end = f
        .ins_with_scratch(InsertPoint::End(b), &mut scratch)
        .return_(&[]);
    assert!(f.inst_results(end).is_empty());
    assert!(scratch.is_empty());
    f.move_inst(first, InsertPoint::Before(end));
    let mut copy = f.clone();
    copy.remove_inst(first);
    assert_eq!(copy.inst_results(first), &[a]);
    assert!(copy.is_removed(first));
    assert!(!f.is_removed(first));
    let replacement = copy.ins(InsertPoint::End(b)).iconst(Type::I64, 0);
    copy.replace_all_uses(a, replacement);
    assert_eq!(copy.resolve(a), replacement);
    assert_eq!(f.resolve(a), a);
    // Growing either pool cannot alter the clone's stored result ranges.
    for _ in 0..300 {
        f.ins(InsertPoint::Before(end)).iconst(Type::I64, 3);
    }
    assert_eq!(copy.inst_results(first), &[a]);
    assert_eq!(copy.value_type(a), Type::I64);
}

#[test]
fn supplied_result_types_allow_forward_operands() {
    let m = keel_ir::parse(
        "func @f() -> i64 {
block0:
 v0 = iadd v1, v1
 v1 = iconst.i64 7
 return v0
}",
    )
    .unwrap();
    let f = &m.functions[0];
    let i = f.layout_insts().next().unwrap().1;
    assert_eq!(f.value_type(f.first_result(i)), Type::I64);
    // The forward reference parses; dominance remains the verifier's concern.
    assert!(keel_ir::verify_module(&m).is_err());
}

#[test]
fn inferred_memory_intrinsic_asm_and_metadata_results_match_parser() {
    for source in [
        include_str!("golden/atomics.keel"),
        include_str!("golden/intrinsics.keel"),
        include_str!("golden/inline_asm.keel"),
        include_str!("golden/calls.keel"),
        include_str!("golden/srclocs.keel"),
    ] {
        reinsert(source);
    }
}
