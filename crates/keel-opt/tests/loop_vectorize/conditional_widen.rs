#[path = "../conditional_widen/fixture.rs"]
mod fixture;
use fixture::source;
use keel_opt::VectorTarget;
fn optimize(source: &str, target: VectorTarget, pass: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &[pass], target).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn packed(m: &keel_ir::Module) -> bool {
    m.functions.iter().any(|f| {
        f.layout_insts()
            .any(|(_, i)| f.inst_data(i).is_vector_instruction())
    })
}
#[test]
fn source_rings_and_predicate_widths_are_explicit_on_native_targets() {
    for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
        let source = source();
        for pass in ["loop-reduce", "loop-reduce-multi"] {
            let m = optimize(&source, target, pass);
            let mut again = m.clone();
            keel_opt::run_passes_for_target(&mut again, &[pass], target).unwrap();
            assert_eq!(m.to_string(), again.to_string());
            for f in &m.functions {
                let text = f.to_string();
                assert!(text.contains("widen_low.i64x2"), "{text}");
                assert!(
                    !text.contains("load.i32x4"),
                    "must read exactly two i32 elements: {text}"
                );
                if f.name.starts_with("wrap_") {
                    assert!(text.contains("i32x4"));
                }
            }
        }
        {
            let target = VectorTarget::Portable;
            assert!(!packed(&optimize(&source, target, "loop-reduce")));
        }
        assert!(!packed(&optimize(&source, target, "fast-loop-reduce")));
    }
}
#[test]
fn volatile_effects_traps_and_multiple_faulting_reads_stay_scalar() {
    for source in [
        source().replace("load.i32 v9", "load.i32 volatile v9"),
        source().replace("v30 = iadd", "store v10,v9\n v30 = iadd"),
        source().replace("v11 = iconst.i32 0", "v11 = load.i32 v0"),
        source()
            .replace("iadd v10,v12", "sdiv v10,v12")
            .replace("isub v10,v12", "sdiv v10,v12"),
    ] {
        let m = optimize(&source, VectorTarget::Aarch64Neon, "loop-reduce");
        for f in &m.functions {
            if source.contains("sdiv") && !f.name.starts_with("wrap_") {
                continue;
            }
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| f.inst_data(i).is_vector_instruction()),
                "{}",
                f.name
            );
        }
    }
}
