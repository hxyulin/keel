use keel_ir::InstData;

#[test]
fn weak_definition_body_cannot_prove_calls_preserve_memory() {
    let src = "func @leaf(ptr,i64)->i64 noninterposable weak_odr {\nblock0(v0:ptr,v1:i64):\n return v1\n}\nfunc @f(ptr,i64)->i64 {\nblock0(v0:ptr,v1:i64):\n v2 = load.i64 v0\n v3 = call @leaf(v0,v1)\n v4 = load.i64 v0\n return v4\n}\n";
    for binding in ["weak_odr", "weak", ""] {
        let src = src.replace("weak_odr", binding);
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
        let f = &m.functions[1];
        let loads = f
            .layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
            .count();
        assert_eq!(loads, if binding.is_empty() { 1 } else { 2 });
    }
}

#[test]
fn ordinary_weak_body_cannot_be_inlined_even_with_noninterposable_promise() {
    let src = "func @leaf(i32)->i32 weak noninterposable alwaysinline {\nblock0(v0:i32):\n return v0\n}\nfunc @f(i32)->i32 {\nblock0(v0:i32):\n v1 = call @leaf(v0)\n return v1\n}\n";
    let mut m = keel_ir::parse(src).unwrap();
    keel_opt::run_passes(
        &mut m,
        &["inline", "inline-noninterposable", "internal-fast-calls"],
    )
    .unwrap();
    assert!(!m.functions[0].local);
    let caller = &m.functions[1];
    assert!(
        caller
            .layout_insts()
            .any(|(_, i)| matches!(caller.inst_data(i), InstData::Call { .. }))
    );
}
