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

#[test]
fn late_size_inlining_only_revisits_non_cold_framed_x86_callers() {
    for (target, attributes, expected) in [
        (VectorTarget::X86Sse2, "frame_pointer ", 0),
        (VectorTarget::X86Sse2, "", 2),
        (VectorTarget::X86Sse2, "frame_pointer cold ", 2),
        (VectorTarget::Aarch64Neon, "frame_pointer ", 2),
    ] {
        let source = source(LARGE)
            .replace("inlinehint local", "inlinehint frame_pointer local")
            .replace(
                "func @caller(i64) -> i64 {",
                &format!("func @caller(i64) -> i64 {attributes}{{"),
            );
        let mut module = keel_ir::parse(&source).unwrap();
        keel_opt::run_passes_for_target(&mut module, &["inline-size-framed"], target).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(calls(&module, "caller"), expected, "{target:?}: {source}");
    }
}

#[test]
fn late_size_cleanup_keeps_foldable_addresses_local_after_value_numbering() {
    use keel_ir::InstData;

    let mut module = keel_ir::parse(
        r#"
func @helper(i64) -> i64 local {
block0(v0: i64):
 v1 = iconst.i64 1
 v2 = iadd v0, v1
 return v2
}
func @caller(ptr, i64) -> i64 frame_pointer {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 24
 v3 = ptradd v0, v2
 v4 = load.i64 v3
 v5 = call @helper(v4)
 brif v1, block1, block2
block1:
 v6 = iconst.i64 24
 v7 = ptradd v0, v6
 store v5, v7
 return v5
block2:
 return v4
}
"#,
    )
    .unwrap();
    keel_opt::run_passes_for_target(&mut module, &["inline-size-framed"], VectorTarget::X86Sse2)
        .unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert_eq!(calls(&module, "caller"), 0);
    let caller = module
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .unwrap();
    let addresses: Vec<_> = caller
        .layout_insts()
        .filter_map(|(_, inst)| match *caller.inst_data(inst) {
            InstData::Load { addr, .. } | InstData::Store { addr, .. } => {
                Some(caller.resolve(addr))
            }
            _ => None,
        })
        .collect();
    let [load, store] = addresses.as_slice() else {
        panic!("expected one load and one store: {caller}");
    };
    assert_ne!(load, store, "{caller}");
}

#[test]
fn framed_x86_size_readonly_cost_ignores_unused_stack_declarations() {
    let source = r#"
func @get(ptr) -> ptr frame_pointer local {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = load.ptr v0
 return v1
}
func @caller(ptr) -> ptr {
block0(v0: ptr):
 v1 = call @get(v0)
 v2 = call @get(v1)
 return v2
}
"#;
    assert_eq!(calls(&optimize(source, VectorTarget::X86Sse2), "caller"), 0);
    assert_eq!(
        calls(&optimize(source, VectorTarget::Aarch64Neon), "caller"),
        2
    );
}
