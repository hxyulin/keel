use keel_ir::{InstData, Purpose};
const SOURCE: &str = "decl @sink(i64, ...)\nfunc @caller(ptr) {\nentry(v0:ptr):\n v1 = iconst.i64 7\n call @sink(v1 : i64, v0 : ptr byval {f64, i64})\n return\n}\n";
#[test]
fn clone_remap_and_text_keep_layout_and_named_prefix() {
    let m = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let mut n = m.clone();
    let f = &mut n.functions[0];
    let (_, i) = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
        .unwrap();
    let original = f.inst_data(i).call_args().unwrap().abi(1).unwrap().clone();
    f.inst_data_mut(i).for_each_operand_mut(|v| {
        let saved = *v;
        *v = saved;
    });
    assert_eq!(f.inst_data(i).call_args().unwrap().abi(1), Some(&original));
    assert!(matches!(original.purpose, Purpose::ByVal(_)));
    let text = n.to_string();
    let p = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&p).unwrap();
    assert_eq!(text, p.to_string());
    assert_eq!(
        p.functions[0].callees().next().unwrap().1.sig.params.len(),
        1
    );
}
#[test]
fn invalid_operand_contracts_reject() {
    for source in [
        SOURCE.replace("v1 : i64", "v1 : ptr"),
        SOURCE.replace("v1 : i64", "v1 : i64 sext"),
        SOURCE.replace("ptr byval {f64, i64}", "ptr sret {f64, i64}"),
        SOURCE.replace("ptr byval {f64, i64}", "ptr byval {}"),
        SOURCE.replace("v0 : ptr byval", "v0 : i64 byval"),
    ] {
        let m = keel_ir::parse(&source).unwrap();
        assert!(keel_ir::verify_module(&m).is_err(), "{source}");
    }
}

#[test]
fn aggregate_home_is_explicit_and_survives_clone_and_roundtrip() {
    let source = "func @read(ptr) -> ptr {\n ss0 = stack_slot 16, align 8\nentry(v0:ptr):\n v1 = va_arg_aggregate {f64,i64},v0,ss0\n return v1\n}\n";
    let m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let f = &m.functions[0];
    let (_, i) = f.layout_insts().next().unwrap();
    let InstData::AggregateVaArg { home, .. } = f.inst_data(i) else {
        panic!()
    };
    let InstData::AggregateVaArg { home: clone, .. } = f.inst_data(i).clone() else {
        panic!()
    };
    assert_eq!(*home, clone);
    let text = m.to_string();
    assert_eq!(keel_ir::parse(&text).unwrap().to_string(), text);
    for bad in [
        source.replace("stack_slot 16", "stack_slot 8"),
        source.replace("align 8", "align 4"),
    ] {
        assert!(keel_ir::verify_module(&keel_ir::parse(&bad).unwrap()).is_err());
    }
}

#[test]
fn aggregate_home_must_not_overlap_the_va_list_object() {
    for (offset, valid) in [(0, false), (8, false), (15, false), (16, true), (32, true)] {
        let source = format!(
            "func @read() -> ptr {{\n ss0=stack_slot 64, align 8\nentry:\n v0=stack_addr ss0\n v1=iconst.i64 {offset}\n v2=ptradd v0,v1\n v3=va_arg_aggregate {{f64,i64}},v2,ss0\n return v3\n}}\n"
        );
        let m = keel_ir::parse(&source).unwrap();
        assert_eq!(keel_ir::verify_module(&m).is_ok(), valid, "{source}");
    }
}

#[test]
fn common_aggregate_cursor_advance_preserves_scalar_producer_provenance() {
    for producer in ["gnu", "clang"] {
        let source = format!(
            "func @read(i64,...) -> f64x1 x86_vector_abi({producer}) {{\n ss0=stack_slot 24, align 8\n ss1=stack_slot 16, align 8\nentry(v0:i64):\n v1=stack_addr ss0\n intrinsic va_start(v1)\n v2=va_arg_aggregate {{i64,f64}},v1,ss1\n v3=va_arg.f64x1 v1 x86_vector_abi({producer})\n return v3\n}}\n"
        );
        keel_ir::verify_module(&keel_ir::parse(&source).unwrap()).unwrap();
    }
}
