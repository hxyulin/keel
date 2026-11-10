use keel_ir::{InstData, Module, parse, verify_module};
use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

fn apply(m: &mut Module, target: VectorTarget) {
    verify_module(m).unwrap();
    keel_opt::run_passes_for_target(m, &["constant-data"], target).unwrap();
    verify_module(m).unwrap();
}

#[test]
fn scalar_reads_across_initializer_items_preserve_bits_and_types() {
    let source = r#"
data @numbers = local const align 8 { b"\x80\xff", zero 2, i32 1065353216, i64 -9223372036854775808 }
func @read() -> (i8, i16, i32, f32, f64) {
block0:
 v0 = symbol_addr @numbers
 v1 = load.i8 v0
 v2 = load.i16 align 2 v0
 v3 = load.i32 align 4 v0
 v4 = iconst.i64 4
 v5 = ptradd v0, v4
 v6 = load.f32 align 4 v5
 v7 = iconst.i64 8
 v8 = ptradd v0, v7
 v9 = load.f64 align 8 v8
 return v1, v2, v3, v6, v9
}
"#;
    for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
        let original = parse(source).unwrap();
        let expected = Outcome::Return(vec![
            Val::I8(128),
            Val::I16(65408),
            Val::I32(65408),
            Val::f32(1.0),
            Val::f64(-0.0),
        ]);
        assert_eq!(Interpreter::new(&original).call("read", &[]), expected);
        let mut m = original;
        apply(&mut m, target);
        assert!(
            !m.functions[0]
                .layout_insts()
                .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::Load { .. }))
        );
        assert_eq!(Interpreter::new(&m).call("read", &[]), expected);
        let once = m.to_string();
        apply(&mut m, target);
        assert_eq!(once, m.to_string());
    }
}

#[test]
fn binding_volatility_bounds_alignment_and_placement_remain_explicit() {
    let source = "data @n = local const align 4 { i32 7 }\nfunc @read() -> i32 {\nblock0:\n v0 = symbol_addr @n\n v1 = load.i32 align 4 v0\n return v1\n}\n";
    for source in [
        source.replace("local const", "const"),
        source.replace("local const", "local"),
        source.replace("local const", "local thread_local"),
        source.replace("local const", "weak const"),
        source.replace("local const", "local section \".special\" const"),
        source.replace("local const", "weak_odr const"),
        source.replace("load.i32 align 4", "load.i32 volatile align 4"),
        source.replace("load.i32 align 4", "atomic_load.i32 relaxed"),
        source.replace("load.i32 align 4", "load.i32 align 8"),
        source
            .replace("load.i32 align 4", "load.i64 align 4")
            .replace("-> i32", "-> i64"),
        source.replace("{ i32 7 }", "{ ptr @n }"),
    ] {
        let mut m = parse(&source).unwrap();
        let before = m.to_string();
        apply(&mut m, VectorTarget::Aarch64Neon);
        assert_eq!(m.to_string(), before, "{source}");
    }
    let mut m = parse(source).unwrap();
    let before = m.to_string();
    apply(&mut m, VectorTarget::Portable);
    assert_eq!(m.to_string(), before);
    let mut m =
        parse(&source.replace("func @read() -> i32", "func @read() -> i32 optnone")).unwrap();
    let before = m.to_string();
    apply(&mut m, VectorTarget::Aarch64Neon);
    assert_eq!(m.to_string(), before);
}
