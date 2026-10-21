#[path = "../src/adjacent_abs.rs"]
mod adjacent_abs;
#[path = "../src/adjacent_diff.rs"]
mod adjacent_diff;

fn canonical() -> keel_ir::Module {
    let mut m = keel_ir::parse(include_str!("adjacent_abs/adjacent.keel")).unwrap();
    assert!(adjacent_abs::run(&mut m.functions[0]));
    m
}

#[test]
fn exact_shape_verifies_and_is_idempotent() {
    let mut m = canonical();
    assert!(adjacent_diff::run(&mut m.functions[0]));
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    assert!(!adjacent_diff::run(&mut m.functions[0]));
    if let Some(dir) = std::env::var_os("KEEL_ADJACENT_DIFF_EVIDENCE_DIR") {
        std::fs::write(
            std::path::PathBuf::from(dir).join("adjacent-diff-after.keel"),
            m.to_string(),
        )
        .unwrap();
    }
}

#[test]
fn raw_triangle_and_memory_contract_changes_are_rejected() {
    let mut raw = keel_ir::parse(include_str!("adjacent_abs/adjacent.keel")).unwrap();
    assert!(!adjacent_diff::run(&mut raw.functions[0]));
    let source = canonical().to_string();
    for source in [
        source.replace("load.i32 ", "load.i32 volatile "),
        source.replace("load.i32 ", "load.i32 notrap "),
        source.replace("load.i32 ", "load.i32 align 4 "),
        source.replace("icmp.ugt", "icmp.sgt"),
        source.replace("icmp.slt", "icmp.ult"),
        source.replace("iconst.i64 2", "iconst.i64 3"),
        source.replace("sext.i64", "uext.i64"),
    ] {
        let mut m = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        assert!(!adjacent_diff::run(&mut m.functions[0]), "{source}");
        assert_eq!(before, m.to_string());
    }
}
