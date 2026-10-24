use keel_ir::{DataLayout, verify_module_with_layout};

#[test]
fn folds_integer_values_and_phi_without_changing_pointer_operations_or_storage() {
    let mut module = keel_ir::parse(
        r#"
data @bytes = align 4 { i32 7 }
data @record = align 4 { ptr @bytes, i32 9 }
func @f(i64) -> i64 {
block0(v0: i64):
    v1 = iconst.i64 4294967295
    v2 = inttoptr.ptr v1
    v3 = ptradd v2, v0
    v4 = ptrtoint.i64 v3
    v5 = iconst.i64 20
    v6 = iconst.i64 22
    v7 = iadd v5, v6
    jump block1(v7)
block1(v8: i64):
    v9 = iadd v4, v8
    return v9
}
"#,
    )
    .unwrap();
    assert!(
        keel_ir::verify_module(&module).is_err(),
        "native layout must reject align-4 pointer data"
    );
    let data = format!("{:?}", module.data);
    keel_opt::optimize_wasm32_basic(&mut module).unwrap();
    verify_module_with_layout(&module, DataLayout::WASM32).unwrap();
    let text = module.to_string();
    assert!(text.contains("iconst.i64 42"), "{text}");
    assert!(text.contains("inttoptr.ptr"));
    assert!(text.contains("ptradd"));
    assert!(text.contains("ptrtoint.i64"));
    assert_eq!(data, format!("{:?}", module.data));
    assert!(
        module.functions[0]
            .blocks()
            .skip(1)
            .all(|b| module.functions[0].block_params(b).is_empty())
    );
}

#[test]
fn trapping_arithmetic_and_unused_memory_accesses_stay_in_order() {
    let mut module = keel_ir::parse(
        r#"
func @f(i64) -> i64 {
block0(v0: i64):
    v1 = inttoptr.ptr v0
    v2 = load.i64 v1
    v3 = load.i32 volatile v1
    v4 = load.i64 notrap v1
    store volatile v3, v1
    v5 = iconst.i64 0
    v6 = iconst.i64 1
    v7 = udiv v6, v5
    v8 = iconst.i64 -9223372036854775808
    v9 = iconst.i64 -1
    v10 = sdiv v8, v9
    return v6
}
"#,
    )
    .unwrap();
    let before = module.to_string();
    keel_opt::optimize_wasm32_basic(&mut module).unwrap();
    assert_eq!(before, module.to_string());
}

#[test]
fn invalid_global_is_rejected_before_any_transformation() {
    let mut module = keel_ir::parse(
        r#"
data @bytes = align 4 { i32 7 }
data @record = align 2 { ptr @bytes }
func @f() -> i32 {
block0:
    v0 = iconst.i32 20
    v1 = iconst.i32 22
    v2 = iadd v0, v1
    return v2
}
"#,
    )
    .unwrap();
    let before = module.to_string();
    let error = keel_opt::optimize_wasm32_basic(&mut module).unwrap_err();
    assert!(error.to_string().contains("wasm32-input"));
    assert_eq!(before, module.to_string());
}
