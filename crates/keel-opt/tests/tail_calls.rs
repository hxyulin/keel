use keel_ir::{parse, verify_module};

fn optimize(source: &str) -> String {
    let mut module = parse(source).unwrap();
    verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["tail-calls"]).unwrap();
    verify_module(&module).unwrap();
    let result = module.to_string();
    keel_opt::run_passes(&mut module, &["tail-calls"]).unwrap();
    assert_eq!(
        module.to_string(),
        result,
        "tail recognition must be idempotent"
    );
    result
}

const LEAF: &str = "func @leaf(i64) -> i64 noinline local {
block0(v0: i64):
 v1 = iconst.i64 1
 v2 = iadd v0, v1
 return v2
}";

#[test]
fn recognizes_local_scalar_forwarding_and_transitive_calls() {
    let source = format!(
        "{LEAF}
func @middle(i64) -> i64 noinline local {{
block0(v0: i64):
 v1 = call @leaf(v0)
 return v1
}}
func @entry(i64) -> i64 {{
block0(v0: i64):
 v1 = call @middle(v0)
 return v1
}}"
    );
    let result = optimize(&source);
    assert!(result.contains("return_call @leaf(v0)"));
    assert!(result.contains("return_call @middle(v0)"));
}

#[test]
fn recognizes_recursive_scalar_tail() {
    let source = "func @recur(i64) -> i64 local {
block0(v0: i64):
 v1 = iconst.i64 0
 v2 = icmp.eq v0, v1
 brif v2, block1, block2
block1:
 return v0
block2:
 v3 = iconst.i64 1
 v4 = isub v0, v3
 v5 = call @recur(v4)
 return v5
}";
    assert!(optimize(source).contains("return_call @recur(v4)"));
    let mut module = parse(&source.replace(" local", " noinline used local")).unwrap();
    keel_opt::optimize_with_tier(&mut module, keel_opt::OptimizationTier::Full).unwrap();
    verify_module(&module).unwrap();
    assert!(module.to_string().contains("return_call @recur"));
}

#[test]
fn rejects_interposition_unknown_calls_and_post_call_work() {
    let wrapper = "func @entry(i64) -> i64 {
block0(v0: i64):
 v1 = call @leaf(v0)
 return v1
}";
    for source in [
        format!("{}\n{wrapper}", LEAF.replace(" local", "")),
        format!("{}\n{wrapper}", LEAF.replace(" local", " weak")),
        format!("decl @leaf(i64) -> i64\n{wrapper}"),
        format!(
            "{LEAF}\n{}",
            wrapper.replace(" return v1", " v2 = iadd v1, v0\n return v2")
        ),
    ] {
        assert!(!optimize(&source).contains("return_call"), "{source}");
    }
}

#[test]
fn rejects_hidden_frame_observer_and_propagates_through_a_cycle() {
    let source = "func @observer(i64) -> i64 local {
block0(v0: i64):
 v1 = intrinsic frame_address()
 v2 = ptrtoint.i64 v1
 return v2
}
func @a(i64) -> i64 local {
block0(v0: i64):
 v1 = call @observer(v0)
 v2 = call @b(v1)
 return v2
}
func @b(i64) -> i64 local {
block0(v0: i64):
 v1 = call @a(v0)
 return v1
}";
    assert!(!optimize(source).contains("return_call"));
}

#[test]
fn rejects_pointer_lifetimes_dynamic_stack_assembly_and_wide_signatures() {
    for source in [
        "func @leaf(ptr) -> ptr local {\nblock0(v0: ptr):\n return v0\n}\nfunc @entry(ptr) -> ptr {\nblock0(v0: ptr):\n v1 = call @leaf(v0)\n return v1\n}".into(),
        format!("{LEAF}\nfunc @entry(i64) -> i64 {{\nblock0(v0: i64):\n asm \"nop\", \"\"()\n v1 = call @leaf(v0)\n return v1\n}}"),
        format!("{LEAF}\nfunc @entry(i64) -> i64 {{\nblock0(v0: i64):\n v1 = intrinsic stack_save()\n v2 = call @leaf(v0)\n return v2\n}}"),
        "func @wide(i64,i64,i64,i64,i64,i64,i64) -> i64 local {\nblock0(v0:i64,v1:i64,v2:i64,v3:i64,v4:i64,v5:i64,v6:i64):\n v7 = call @wide(v0,v1,v2,v3,v4,v5,v6)\n return v7\n}".into(),
    ] {
        assert!(!optimize(&source).contains("return_call"), "{source}");
    }
}

#[test]
fn release_tiers_recognize_after_signature_finalization() {
    for tier in [
        keel_opt::OptimizationTier::Balanced,
        keel_opt::OptimizationTier::Size,
        keel_opt::OptimizationTier::Full,
    ] {
        let pipeline = tier.pipeline();
        assert_eq!(
            &pipeline[pipeline.len() - 2..],
            &["internal-fast-calls", "tail-calls"]
        );
    }
}

#[test]
fn rejects_implicit_fma_helpers_and_mismatched_return_extensions() {
    let source = "func @fused(f64) -> f64 local {
block0(v0:f64):
 v1 = fma v0, v0, v0
 return v1
}
func @wrapper(f64) -> f64 {
block0(v0:f64):
 v1 = call @fused(v0)
 return v1
}";
    assert!(!optimize(source).contains("return_call"));
    let source = "func @leaf(i8) -> i8 sext local {
block0(v0:i8):
 return v0
}
func @wrapper(i8) -> i8 {
block0(v0:i8):
 v1 = call @leaf(v0)
 return v1
}";
    assert!(!optimize(source).contains("return_call"));
}

#[test]
fn recognizes_void_and_mixed_six_register_boundaries() {
    let source = "func @leaf() noinline local {
block0:
 return
}
func @entry() {
block0:
 call @leaf()
 return
}";
    assert!(optimize(source).contains("return_call @leaf()"));
    let source = "func @leaf(i8 sext, f32, i32, f64, i64, i16 uext) -> f64 local {
block0(v0:i8,v1:f32,v2:i32,v3:f64,v4:i64,v5:i16):
 return v3
}
func @entry(i8 sext, f32, i32, f64, i64, i16 uext) -> f64 {
block0(v0:i8,v1:f32,v2:i32,v3:f64,v4:i64,v5:i16):
 v6 = call @leaf(v0,v1,v2,v3,v4,v5)
 return v6
}";
    assert!(optimize(source).contains("return_call @leaf"));
}
