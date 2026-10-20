use keel_opt::VectorTarget;

fn source() -> &'static str {
    include_str!("../../keel-codegen/tests/fixtures/reduction_affine.ir")
}
fn run(src: &str, pass: &str, target: VectorTarget) -> keel_ir::Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &[pass], target).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn packed(m: &keel_ir::Module) -> bool {
    m.functions[0]
        .layout_insts()
        .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
}
#[test]
fn affine_offsets_are_opt_in_guarded_deterministic_and_idempotent() {
    for target in [
        VectorTarget::Portable,
        VectorTarget::Aarch64Neon,
        VectorTarget::X86Sse2,
    ] {
        for src in [
            source().to_owned(),
            source().replace("iadd v2,v5", "iadd v5,v2"),
        ] {
            assert!(!packed(&run(&src, "loop-reduce", target)));
            let m = run(&src, "loop-reduce-affine", target);
            assert!(packed(&m), "{m}");
            assert!(m.to_string().contains("icmp.uge"));
            assert_eq!(
                m.to_string(),
                run(&src, "loop-reduce-affine", target).to_string()
            );
            let mut twice = m.clone();
            keel_opt::run_passes_for_target(&mut twice, &["loop-reduce-affine"], target).unwrap();
            assert_eq!(m.to_string(), twice.to_string());
        }
    }
}
#[test]
fn rejects_traps_effects_narrow_wrapping_indices_and_non_affine_terms() {
    for src in [
        source().replace("load.i64", "load.i64 volatile"),
        source().replace("v15 = iadd", "store v11,v0\n v15 = iadd"),
        source().replace("iadd v2,v5", "isub v2,v5"),
        source().replace("iadd v2,v5", "iadd v5,v5"),
        source().replace(
            "v9 = iadd v2,v5",
            "v30 = trunc.i32 v2\n v31 = trunc.i32 v5\n v32 = iadd v30,v31\n v9 = uext.i64 v32",
        ),
    ] {
        assert!(
            !packed(&run(&src, "loop-reduce-affine", VectorTarget::Portable)),
            "{src}"
        );
    }
}
#[test]
fn two_streams_require_an_explicit_nontrapping_contract_even_when_aliasing() {
    let src = source().replace("v15 = iadd v6,v11", "v30 = shl v5,v8\n v31 = ptradd v0,v30\n v32 = load.i64 v31\n v33 = xor v32,v11\n v15 = iadd v6,v33");
    assert!(!packed(&run(
        &src,
        "loop-reduce-affine",
        VectorTarget::Portable
    )));
    assert!(packed(&run(
        &src.replace("load.i64 v31", "load.i64 notrap v31"),
        "loop-reduce-affine",
        VectorTarget::Portable
    )));
}

#[test]
fn actual_logic_remains_blocked_until_a_stream_is_explicitly_safe() {
    let src = include_str!("../../keel-codegen/tests/fixtures/reduction_logic.ir");
    for target in [
        VectorTarget::Portable,
        VectorTarget::Aarch64Neon,
        VectorTarget::X86Sse2,
    ] {
        assert!(!packed(&run(src, "loop-reduce-affine", target)));
        let positive = src.replace("load.i64 v16", "load.i64 notrap v16");
        assert!(packed(&run(&positive, "loop-reduce-affine", target)));
    }
}

#[test]
fn address_width_offsets_after_i32_extension_are_supported() {
    for (extend, predicate) in [("uext", "ult"), ("sext", "slt")] {
        let src = source()
            .replace("ptr,i64,i64,i64,i64", "ptr,i32,i64,i32,i64")
            .replace("v1: i64", "v1: i32")
            .replace("v3: i64", "v3: i32")
            .replace("v5: i64", "v5: i32")
            .replace("icmp.ult", &format!("icmp.{predicate}"))
            .replace(
                "v9 = iadd v2,v5",
                &format!("v30 = {extend}.i64 v5\n v9 = iadd v2,v30"),
            )
            .replace("v16 = iconst.i64", "v16 = iconst.i32");
        assert!(packed(&run(
            &src,
            "loop-reduce-affine",
            VectorTarget::Portable
        )));
    }
}
