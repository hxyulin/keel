#[test]
fn malformed_ordinary_weak_linkage_never_reaches_object_emission() {
    for src in [
        "func @f() weak local {\nblock0:\n return\n}\n",
        "func @f() weak weak_odr {\nblock0:\n return\n}\n",
        "data @d = local weak align 4 { i32 0 }\n",
        "data @d = weak weak_odr align 4 { i32 0 }\n",
        "decl data weak thread_local @d\n",
        "data @d = weak thread_local align 4 { i32 0 }\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        for target in keel_ir::Target::ALL {
            assert!(keel_codegen::compile(&m, target).is_err(), "{src}");
            assert!(keel_codegen::compile_program(&m, target).is_err(), "{src}");
        }
    }
}
