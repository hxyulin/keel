use keel_ir::{parse, verify_module};

const SOURCE: &str = "\
decl @unavailable()
func @disabled(ptr) -> i8 noinline local {
block0(v0: ptr):
 v1 = load.i32 trap_ub v0
 v2 = iconst.i8 0
 return v2
}
func @entry(ptr) {
block0(v0: ptr):
 v1 = call @disabled(v0)
 brif v1, block1, block2
block1:
 call @unavailable()
 return
block2:
 return
}";

#[test]
fn size_folds_a_bound_result_and_keeps_the_call_and_its_effects() {
    let mut module = parse(SOURCE).unwrap();
    keel_opt::optimize_with_tier(&mut module, keel_opt::OptimizationTier::Size).unwrap();
    verify_module(&module).unwrap();
    let text = module.functions[1].to_string();
    assert!(!text.contains("call @unavailable"), "{text}");
    assert!(text.contains("call @disabled"), "{text}");
    assert!(module.functions[0].to_string().contains("load.i32 trap_ub"));
}

#[test]
fn unknown_and_interposable_results_keep_the_branch() {
    for source in [
        SOURCE.replace("noinline local", "noinline weak"),
        SOURCE.replace("v2 = iconst.i8 0", "v2 = trunc.i8 v1"),
    ] {
        let mut module = parse(&source).unwrap();
        keel_opt::optimize_with_tier(&mut module, keel_opt::OptimizationTier::Size).unwrap();
        verify_module(&module).unwrap();
        let text = module.functions[1].to_string();
        assert!(text.contains("call @unavailable"), "{text}");
    }
}
