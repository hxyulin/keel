//! Native Size costs preserve required inlining and removable private bodies.
use super::calls;
use keel_opt::VectorTarget;

const SMALL: &str = "v1 = iconst.i64 1
 v2 = iadd v0,v1
 v3 = xor v2,v1
 return v3";
const MIDDLE: &str = "v1 = iconst.i64 1311768467463790320
 v2 = xor v0,v1
 return v2";
const LARGE: &str = "v1 = iconst.i64 1311768467463790320
 v2 = xor v0,v1
 v3 = iconst.i64 -81985529216486896
 v4 = imul v2,v3
 return v4";

fn source(body: &str) -> String {
    format!(
        "func @helper(i64) -> i64 inlinehint local {{
block0(v0:i64):
 {body}
}}
func @caller(i64) -> i64 {{
block0(v0:i64):
 v1 = call @helper(v0)
 v2 = call @helper(v1)
 return v2
}}"
    )
}

fn optimize(source: &str, target: VectorTarget) -> keel_ir::Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes_for_target(&mut module, &["inline-size"], target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn duplicated_native_helpers_use_target_byte_costs() {
    for (body, arm_calls, x86_calls) in [(SMALL, 0, 0), (MIDDLE, 2, 0), (LARGE, 2, 2)] {
        for (target, expected) in [
            (VectorTarget::Aarch64Neon, arm_calls),
            (VectorTarget::X86Sse2, x86_calls),
            (VectorTarget::Portable, 0),
        ] {
            let module = optimize(&source(body), target);
            assert_eq!(calls(&module, "caller"), expected, "{target:?}: {body}");
        }
    }
}

#[test]
fn required_and_flatten_inlining_bypass_native_byte_costs() {
    let ordinary = source(LARGE);
    for source in [
        ordinary.replace("inlinehint local", "alwaysinline local"),
        ordinary.replace(
            "func @caller(i64) -> i64 {",
            "func @caller(i64) -> i64 flatten {",
        ),
    ] {
        for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
            let module = optimize(&source, target);
            assert_eq!(calls(&module, "caller"), 0, "{target:?}: {source}");
        }
    }
}

#[test]
fn a_single_use_needs_no_byte_allowance_only_when_its_body_can_disappear() {
    let single = source(LARGE).replace(" v2 = call @helper(v1)\n return v2", " return v1");
    let addressed = format!("{single}\ndata @held = align 8 {{ ptr @helper }}");
    for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
        assert_eq!(calls(&optimize(&single, target), "caller"), 0, "{target:?}");
        assert_eq!(
            calls(&optimize(&addressed, target), "caller"),
            1,
            "{target:?}"
        );
    }
}
