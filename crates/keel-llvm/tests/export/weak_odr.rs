#[test]
fn unsupported_odr_linkage_fails_whole_module_export() {
    for src in [
        "decl @optional() weak\nfunc @f() {\nblock0:\n return\n}\n",
        "func @f() weak {\nblock0:\n return\n}\n",
        "data @d = weak align 8 { i64 1 }\n",
        "decl data weak @d\n",
        "func @f() weak_odr {\nblock0:\n return\n}\n",
        "data @d = weak_odr align 8 { i64 1 }\n",
        "data @d = weak_odr hidden align 8 { i64 1 }\n",
        "decl data @name\ndata @d = align 8 { rtti_name @name }\n",
    ] {
        let m = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        {
            let target = keel_llvm::TargetProfile::AppleArm64;
            assert!(
                keel_llvm::export(&m, target)
                    .unwrap_err()
                    .contains("weak_odr")
            );
        }
    }
}
