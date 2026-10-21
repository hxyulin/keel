use keel_ir::InstData;

#[test]
fn weak_definition_body_cannot_prove_calls_preserve_memory() {
    let src = "func @leaf(ptr,i64)->i64 noninterposable weak_odr {\nblock0(v0:ptr,v1:i64):\n return v1\n}\nfunc @f(ptr,i64)->i64 {\nblock0(v0:ptr,v1:i64):\n v2 = load.i64 v0\n v3 = call @leaf(v0,v1)\n v4 = load.i64 v0\n return v4\n}\n";
    for weak in [true, false] {
        let src = if weak {
            src.to_string()
        } else {
            src.replace(" weak_odr", "")
        };
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        keel_opt::run_passes(&mut m, &["call-load-reuse"]).unwrap();
        let f = &m.functions[1];
        let loads = f
            .layout_insts()
            .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
            .count();
        assert_eq!(loads, if weak { 2 } else { 1 });
    }
}
