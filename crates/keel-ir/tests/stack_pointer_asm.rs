//! The stack token is an ordering dependency, not an integer installed in RSP.
use keel_ir::{AsmOperand, InlineAsm, InstData, Intrinsic, Type, is_stack_pointer_snapshot};

const SOURCE: &str = r#"
func @balanced(i64) -> i64 cc(c) {
block0(v0: i64):
    v1 = intrinsic stack_save()
    v2 = ptrtoint.i64 v1
    v3 = asm "pushfq\n popfq\n movq %1, %0", "=r,rsp,0,~{memory},~{cc}"(v2, v0) -> i64
    return v3
}
"#;

#[test]
fn balanced_stack_dependency_roundtrips_without_an_ssa_stack_output() {
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let function = &module.functions[0];
    let (asm, args) = function
        .layout_insts()
        .find_map(|(_, i)| match function.inst_data(i) {
            InstData::InlineAsm { asm, args } => Some((asm, args)),
            _ => None,
        })
        .unwrap();
    assert_eq!(asm.stack_pointer_input().unwrap(), Some(0));
    assert_eq!(asm.outputs, [(AsmOperand::Reg, Type::I64)]);
    assert_eq!(asm.inputs, [AsmOperand::StackPointer, AsmOperand::Tied(0)]);
    assert!(is_stack_pointer_snapshot(function, args[0]));
    let text = module.to_string();
    assert!(text.contains("=r,rsp,0,~{memory},~{cc}"), "{text}");
    let roundtrip = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&roundtrip).unwrap();
    assert_eq!(text, roundtrip.to_string());
}

#[test]
fn stack_capture_proof_resolves_value_aliases_at_both_steps() {
    let mut module = keel_ir::parse(
        r#"
func @aliases() {
block0:
    v0 = intrinsic stack_save()
    v1 = intrinsic stack_save()
    v2 = ptrtoint.i64 v1
    v3 = ptrtoint.i64 v0
    asm "", "rsp"(v2)
    return
}
"#,
    )
    .unwrap();
    let function = &mut module.functions[0];
    let captures: Vec<_> = function
        .layout_insts()
        .filter_map(|(_, i)| match function.inst_data(i) {
            InstData::Intrinsic {
                id: Intrinsic::StackSave,
                ..
            } => Some(function.inst_results(i)[0]),
            _ => None,
        })
        .collect();
    let converts: Vec<_> = function
        .layout_insts()
        .filter_map(|(_, i)| match function.inst_data(i) {
            InstData::Convert { .. } => Some(function.inst_results(i)[0]),
            _ => None,
        })
        .collect();
    function.replace_all_uses(captures[1], captures[0]);
    assert!(is_stack_pointer_snapshot(function, converts[0]));
    function.replace_all_uses(converts[0], converts[1]);
    assert!(is_stack_pointer_snapshot(function, converts[0]));
    keel_ir::verify_module(&module).unwrap();
}

#[test]
fn snapshots_require_i64_and_the_canonical_stack_save_producer() {
    for (params, body, value, expected) in [
        ("i64", "", "v0", "StackSave-to-i64 snapshot"),
        ("", "v0 = iconst.i64 0", "v0", "StackSave-to-i64 snapshot"),
        (
            "",
            "v0 = intrinsic frame_address()\n v1 = ptrtoint.i64 v0",
            "v1",
            "StackSave-to-i64 snapshot",
        ),
        ("", "v0 = intrinsic stack_save()", "v0", "cannot be ptr"),
        ("", "v0 = iconst.i32 0", "v0", "cannot be i32"),
        (
            "",
            "v0 = intrinsic stack_save()\n v1 = ptrtoint.i64 v0\n v2 = iconst.i64 0\n v3 = iadd v1, v2",
            "v3",
            "StackSave-to-i64 snapshot",
        ),
    ] {
        let block = if params.is_empty() {
            "block0:"
        } else {
            "block0(v0: i64):"
        };
        let source = format!(
            "func @bad({params}) {{\n{block}\n{body}\nasm \"\", \"rsp\"({value})\nreturn\n}}\n"
        );
        let module = keel_ir::parse(&source).unwrap();
        let errors = keel_ir::verify_module(&module).unwrap_err();
        assert!(
            errors.iter().any(|e| e.message.contains(expected)),
            "{source}\n{errors:?}"
        );
    }
    let module = keel_ir::parse(
        r#"
func @phi() {
block0:
    v0 = intrinsic stack_save()
    v1 = ptrtoint.i64 v0
    jump block1(v1)
block1(v2: i64):
    asm "", "rsp"(v2)
    return
}
"#,
    )
    .unwrap();
    assert!(
        keel_ir::verify_module(&module)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("StackSave-to-i64 snapshot"))
    );
}

#[test]
fn malformed_shapes_and_reserved_alias_conflicts_fail_before_emission() {
    for (constraints, outputs, expected) in [
        ("=rsp", vec![Type::I64], "input-only"),
        ("=&rsp", vec![Type::I64], "input-only"),
        ("=rsp,0", vec![Type::I64], "input-only"),
        ("rsp,rsp", vec![], "duplicate"),
        ("rsp,{rsp}", vec![], "conflicts"),
        ("rsp,{esp}", vec![], "conflicts"),
        ("rsp,{sp}", vec![], "conflicts"),
        ("={rsp},rsp", vec![Type::I64], "conflicts"),
        ("rsp,~{rsp}", vec![], "conflicts"),
    ] {
        let error = InlineAsm::new("", constraints, &outputs).unwrap_err();
        assert!(error.contains(expected), "{constraints}: {error}");
    }
    // Target-specific handling of ordinary fixed registers remains unchanged.
    assert_eq!(
        InlineAsm::new("", "{rsp}", &[])
            .unwrap()
            .stack_pointer_input()
            .unwrap(),
        None
    );
    let mut module = keel_ir::parse(SOURCE).unwrap();
    let function = &mut module.functions[0];
    let inst = function
        .layout_insts()
        .find_map(|(_, i)| matches!(function.inst_data(i), InstData::InlineAsm { .. }).then_some(i))
        .unwrap();
    if let InstData::InlineAsm { asm, args } = function.inst_data_mut(inst) {
        asm.inputs.push(AsmOperand::StackPointer);
        args.push(args[0]);
    }
    assert!(
        keel_ir::verify_module(&module)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("duplicate rsp"))
    );

    let function = &mut module.functions[0];
    if let InstData::InlineAsm { asm, args } = function.inst_data_mut(inst) {
        asm.inputs.pop();
        args.pop();
        asm.outputs[0].0 = AsmOperand::StackPointer;
    }
    assert!(
        keel_ir::verify_module(&module)
            .unwrap_err()
            .iter()
            .any(|e| e.message.contains("input-only"))
    );
}
