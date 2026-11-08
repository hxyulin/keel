use keel_ir::{InstData, Module};
use keel_opt::VectorTarget;

fn optimize(source: &str, target: VectorTarget) -> Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let roundtrip = keel_ir::parse(&module.to_string()).unwrap();
    assert_eq!(module.to_string(), roundtrip.to_string());
    keel_opt::run_passes_for_target(&mut module, &["fast-math", "dce"], target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn relaxed_algebra_requires_explicit_permission() {
    for ty in ["f32", "f64"] {
        for expression in ["fsub v0,v0", "fdiv v0,v0", "fmul v0,v1", "fadd v0,v1"] {
            let source = format!(
                "func @f({ty}) -> {ty} {{\nblock0(v0:{ty}):\n v1 = fconst.{ty} 0.0\n v2 = {expression}\n return v2\n}}"
            );
            let strict = optimize(&source, VectorTarget::Aarch64Neon);
            assert!(
                strict.functions[0].layout_insts().any(|(_, i)| matches!(
                    strict.functions[0].inst_data(i),
                    InstData::Binary { .. }
                )),
                "{strict}"
            );
            let relaxed = optimize(
                &source.replacen(" {\n", " fast_math {\n", 1),
                VectorTarget::Aarch64Neon,
            );
            assert!(
                !relaxed.functions[0].layout_insts().any(|(_, i)| matches!(
                    relaxed.functions[0].inst_data(i),
                    InstData::Binary { .. }
                )),
                "{relaxed}"
            );
        }
    }
}

#[test]
fn contraction_requires_native_fma_and_permission() {
    let src = "func @f(f32,f32,f32) -> f32 fast_math {\nblock0(v0:f32,v1:f32,v2:f32):\n v3 = fmul v0,v1\n v4 = fadd v3,v2\n return v4\n}";
    for (source, target, expected) in [
        (src.to_string(), VectorTarget::Aarch64Neon, true),
        (src.to_string(), VectorTarget::X86Sse2, false),
        (src.to_string(), VectorTarget::Portable, false),
        (
            src.replace(" fast_math", ""),
            VectorTarget::Aarch64Neon,
            false,
        ),
    ] {
        let m = optimize(&source, target);
        assert_eq!(m.to_string().contains(" = fma "), expected, "{m}");
    }
}

#[test]
fn mixed_math_modes_cannot_inline_in_either_direction() {
    for (caller, callee) in [
        ("", " fast_math"),
        (" fast_math", ""),
        (" fast_math", " fast_math"),
    ] {
        let source = format!(
            "func @helper(f32) -> f32{callee} local {{\nblock0(v0:f32):\n return v0\n}}\nfunc @caller(f32) -> f32{caller} {{\nblock0(v0:f32):\n v1 = call @helper(v0)\n return v1\n}}"
        );
        let mut m = keel_ir::parse(&source).unwrap();
        keel_opt::run_passes(&mut m, &["inline"]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let f = m.functions.iter().find(|f| f.name == "caller").unwrap();
        assert_eq!(
            f.to_string().contains("call @helper"),
            caller != callee,
            "{m}"
        );
    }
}

#[test]
fn mandatory_inlining_diagnoses_mixed_math_modes() {
    let source = "func @helper(f32) -> f32 fast_math alwaysinline local {\nblock0(v0:f32):\n return v0\n}\nfunc @caller(f32) -> f32 {\nblock0(v0:f32):\n v1 = call @helper(v0)\n return v1\n}";
    let mut module = keel_ir::parse(source).unwrap();
    let error = keel_opt::required_inline_asm(&mut module).unwrap_err();
    assert!(error.contains("math mode mismatch"), "{error}");
}

#[test]
fn merging_modules_preserves_body_permissions() {
    let body = |name: &str, mode: &str| {
        format!("func @{name}(f32) -> f32{mode} {{\nblock0(v0:f32):\n return v0\n}}")
    };
    let strict = keel_ir::parse(&body("strict", "")).unwrap();
    let fast = keel_ir::parse(&body("fast", " fast_math")).unwrap();
    let merged = keel_ir::merge_modules(vec![strict, fast]);
    let merged = merged.unwrap();
    assert!(
        !merged
            .functions
            .iter()
            .find(|f| f.name == "strict")
            .unwrap()
            .attrs
            .fast_math
    );
    assert!(
        merged
            .functions
            .iter()
            .find(|f| f.name == "fast")
            .unwrap()
            .attrs
            .fast_math
    );
    keel_ir::verify_module(&merged).unwrap();
}

#[test]
fn packed_contraction_is_gated_and_uses_generic_fused_ir() {
    for ty in ["f32x4", "f64x2"] {
        let src = format!(
            "func @f({ty},{ty},{ty}) -> {ty} fast_math {{\nblock0(v0:{ty},v1:{ty},v2:{ty}):\n v3 = vfmul v0,v1\n v4 = vfadd v3,v2\n return v4\n}}"
        );
        for target in [
            VectorTarget::Aarch64Neon,
            VectorTarget::X86Sse2,
            VectorTarget::Portable,
        ] {
            let m = optimize(&src, target);
            assert_eq!(
                m.to_string().contains(" = fma "),
                target == VectorTarget::Aarch64Neon,
                "{m}"
            );
            let strict = optimize(&src.replace(" fast_math", ""), target);
            assert!(!strict.to_string().contains(" = fma "), "{strict}");
        }
    }
}

#[test]
fn reciprocal_root_estimates_are_f32_native_and_opt_in() {
    for (ty, scalar) in [("f32x4", "f32"), ("f64x2", "f64")] {
        let src = format!(
            "func @f({ty}) -> {ty} fast_math {{\nblock0(v0:{ty}):\n v1 = fconst.{scalar} 1.0\n v2 = vsplat.{ty} v1\n v3 = vfsqrt v0\n v4 = vfdiv v2,v3\n return v4\n}}"
        );
        for target in [
            VectorTarget::Aarch64Neon,
            VectorTarget::X86Sse2,
            VectorTarget::Portable,
        ] {
            let m = optimize(&src, target);
            let estimate = ty == "f32x4" && target != VectorTarget::Portable;
            assert_eq!(!m.to_string().contains(" = vfdiv "), estimate, "{m}");
            let strict = optimize(&src.replace(" fast_math", ""), target);
            assert!(strict.to_string().contains(" = vfdiv "), "{strict}");
            let general = optimize(&src.replace("vfdiv v2,v3", "vfdiv v0,v3"), target);
            assert!(general.to_string().contains(" = vfdiv "), "{general}");
            let mut twice = m.clone();
            keel_opt::run_passes_for_target(&mut twice, &["fast-math", "dce"], target).unwrap();
            assert_eq!(twice.to_string(), m.to_string());
        }
    }
}
