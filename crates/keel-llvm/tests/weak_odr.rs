#[test]
fn unsupported_odr_linkage_fails_whole_module_export() {
    for src in [
        "func @f() weak_odr {\nblock0:\n return\n}\n",
        "data @d = weak_odr align 8 { i64 1 }\n",
        "data @d = weak_odr hidden align 8 { i64 1 }\n",
        "decl data @name\ndata @d = align 8 { rtti_name @name }\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        for target in [keel_llvm::TargetProfile::AppleArm64] {
            assert!(
                keel_llvm::export(&m, target)
                    .unwrap_err()
                    .contains("weak_odr")
            );
        }
    }
}
