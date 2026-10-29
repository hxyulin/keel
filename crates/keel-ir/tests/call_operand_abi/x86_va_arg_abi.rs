use keel_ir::{InstData, X86VectorAbi};
fn source(producer: &str) -> String {
    format!(
        "func @read(i64,...)->f64x1 x86_vector_abi({producer}) {{\n ss0 = stack_slot 24, align 8\nentry(v0:i64):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg.f64x1 v1 x86_vector_abi({producer})\n return v2\n}}\n"
    )
}
#[test]
fn explicit_provenance_roundtrips_and_clones() {
    for (name, abi) in [("gnu", X86VectorAbi::Gnu), ("clang", X86VectorAbi::Clang)] {
        let m = keel_ir::parse(&source(name)).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let cloned = m.clone();
        let text = cloned.to_string();
        let n = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&n).unwrap();
        assert_eq!(text, n.to_string());
        let f = &n.functions[0];
        let (inst, tag) = f
            .layout_insts()
            .find_map(|(_, i)| match f.inst_data(i) {
                InstData::VaArg { x86_vector_abi, .. } => Some((i, *x86_vector_abi)),
                _ => None,
            })
            .unwrap();
        assert_eq!(tag, Some(abi));
        assert!(f.has_x86_va_arg_provenance(inst, abi));
        assert!(!f.has_x86_va_arg_provenance(
            inst,
            if abi == X86VectorAbi::Gnu {
                X86VectorAbi::Clang
            } else {
                X86VectorAbi::Gnu
            }
        ));
    }
}
#[test]
fn current_signature_and_local_initialization_are_required() {
    let good = source("gnu");
    for bad in [
        good.replace(
            "va_arg.f64x1 v1 x86_vector_abi(gnu)",
            "va_arg.f64x1 v1 x86_vector_abi(clang)",
        ),
        good.replace("intrinsic va_start(v1)", ""),
        good.replace(
            "intrinsic va_start(v1)",
            "intrinsic va_start(v1)\n v3 = iconst.i64 0\n store v3,v1",
        ),
        good.replace("v1 = stack_addr ss0", "v1 = inttoptr.ptr v0"),
        good.replace(
            "func @read(i64,...)->f64x1 x86_vector_abi(gnu)",
            "func @read(i64,...)->f64x1",
        ),
        good.replace("va_arg.f64x1", "va_arg.f64"),
    ] {
        let m = keel_ir::parse(&bad).unwrap();
        assert!(keel_ir::verify_module(&m).is_err(), "{bad}");
    }
    assert!(
        keel_ir::parse(&good.replace(
            "x86_vector_abi(gnu)\n return",
            "x86_vector_abi(ms)\n return"
        ))
        .is_err()
    );
}

#[test]
fn only_bounded_disjoint_local_stores_preserve_provenance() {
    let good = source("gnu").replace(" ss0 =", " ss1 = stack_slot 16, align 8\n ss0 =");
    for (offset, accepted) in [(0, true), (8, true), (-1, false), (9, false), (16, false)] {
        let src = good.replace("intrinsic va_start(v1)", &format!("intrinsic va_start(v1)\n v3 = stack_addr ss1\n v4 = iconst.i64 {offset}\n v5 = ptradd v3,v4\n v6 = iconst.i64 42\n store v6,v5"));
        let m = keel_ir::parse(&src).unwrap();
        assert_eq!(keel_ir::verify_module(&m).is_ok(), accepted, "{src}");
    }
}

#[test]
fn aggregate_uses_same_current_signature_provenance_proof() {
    let src = "func @read(i64,...) x86_vector_abi(gnu) {\n ss0 = stack_slot 24, align 8\n ss1 = stack_slot 16, align 8\nentry(v0:i64):\n v1 = stack_addr ss0\n intrinsic va_start(v1)\n v2 = va_arg_aggregate {{f64x1}}, v1, ss1 x86_vector_abi(gnu)\n return\n}".replace("{{", "{").replace("}}", "}");
    for (home, expected) in [("ss1", true), ("ss0", false)] {
        let m = keel_ir::parse(&src.replace(", ss1", &format!(", {home}"))).unwrap();
        let f = &m.functions[0];
        let inst = f
            .layout_insts()
            .find_map(|(_, i)| {
                matches!(f.inst_data(i), InstData::AggregateVaArg { .. }).then_some(i)
            })
            .unwrap();
        assert_eq!(
            f.has_x86_va_arg_provenance(inst, X86VectorAbi::Gnu),
            expected
        );
        assert!(!f.has_x86_va_arg_provenance(inst, X86VectorAbi::Clang));
        assert_eq!(keel_ir::verify_module(&m).is_ok(), expected);
    }
}
