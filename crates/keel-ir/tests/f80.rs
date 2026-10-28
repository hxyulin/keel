use keel_ir::{Layout, Type};

#[test]
fn payload_and_object_storage_are_distinct() {
    assert_eq!(Type::F80.bytes(), Some(10));
    assert!(Type::F80.is_float());
    let extended = Layout::Scalar(Type::F80);
    assert_eq!((extended.size(), extended.align()), (16, 16));
    let record = Layout::Struct(vec![
        Layout::Scalar(Type::I8),
        extended.clone(),
        Layout::Scalar(Type::I8),
    ]);
    assert_eq!((record.size(), record.align()), (48, 16));
    assert_eq!(
        record.leaves(),
        [(0, Type::I8), (16, Type::F80), (32, Type::I8)]
    );
    assert_eq!(Layout::Array(Box::new(extended), 2).size(), 32);
}

#[test]
fn constants_round_trip_without_f64() {
    for (exponent, significand) in [
        (0, 0),
        (0x8000, 0),
        (0x3fff, 0x8000000000000001),
        (0x7ffe, u64::MAX),
        (0x7fff, 0xc123456789abcdef),
        (0, 1),
    ] {
        let text = format!(
            "func @constant() -> f80 {{\nentry:\n v0 = fconst.f80 {exponent:#x}, {significand:#x}\n return v0\n}}\n"
        );
        let module = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let printed = module.to_string();
        assert_eq!(keel_ir::parse(&printed).unwrap().to_string(), printed);
        let f = &module.functions[0];
        assert!(f.layout_insts().any(|(_, i)| matches!(*f.inst_data(i), keel_ir::InstData::F80const {sign_exp, significand: s} if sign_exp == exponent && s == significand)));
    }
}
