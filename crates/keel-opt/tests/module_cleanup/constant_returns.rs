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
fn size_removes_a_trivial_constant_call_and_keeps_argument_evaluation() {
    let mut module = parse(
        "\
decl @effect() -> ptr
func @constant(ptr) -> i32 noinline local {
block0(v0: ptr):
 v1 = iconst.i32 19
 return v1
}
func @entry() -> i32 {
block0:
 v0 = call @effect()
 v1 = call @constant(v0)
 return v1
}",
    )
    .unwrap();
    keel_opt::optimize_with_tier(&mut module, keel_opt::OptimizationTier::Size).unwrap();
    verify_module(&module).unwrap();
    let [entry] = module.functions.as_slice() else {
        panic!("unreachable constant helper was retained");
    };
    let text = entry.to_string();
    assert!(!text.contains("call @constant"), "{text}");
    assert!(text.contains("call @effect"), "{text}");
    assert!(text.contains("iconst.i32 19"), "{text}");
}

#[test]
fn size_forwards_a_trivial_parameter_return_and_keeps_observed_calls() {
    let mut module = parse(
        "\
decl @effect() -> ptr
func @identity(i32, ptr) -> ptr noinline local {
block0(v0: i32, v1: ptr):
 return v1
}
func @observe(ptr) -> ptr noinline local {
block0(v0: ptr):
 v1 = load.i32 volatile v0
 return v0
}
func @entry() -> ptr {
block0:
 v0 = call @effect()
 v1 = iconst.i32 0
 v2 = call @identity(v1, v0)
 v3 = call @identity(v1, v2)
 v4 = call @observe(v3)
 return v4
}",
    )
    .unwrap();
    keel_opt::optimize_with_tier(&mut module, keel_opt::OptimizationTier::Size).unwrap();
    verify_module(&module).unwrap();
    let entry = module.functions.iter().find(|f| f.name == "entry").unwrap();
    let text = entry.to_string();
    assert!(!text.contains("call @identity"), "{text}");
    assert!(text.contains("call @effect"), "{text}");
    assert!(text.contains("call @observe"), "{text}");
}

#[test]
fn size_reads_a_single_byval_word_without_assuming_source_alignment() {
    let source = r#"
func @word(ptr byval [1 x i64]) -> i64 noinline local {
block0(v0: ptr):
 v1 = load.i64 align 8 notrap v0
 return v1
}
func @read_word(ptr) -> i64 {
block0(v0: ptr):
 v1 = call @word(v0)
 return v1
}
"#;
    for ordinary in [true, false] {
        let body = if ordinary {
            source.to_owned()
        } else {
            source.replace("load.i64 align", "load.i64 volatile align")
        };
        let mut module = parse(&body).unwrap();
        keel_opt::optimize_with_tier(&mut module, keel_opt::OptimizationTier::Size).unwrap();
        verify_module(&module).unwrap();
        let entry = module
            .functions
            .iter()
            .find(|f| f.name == "read_word")
            .unwrap();
        let text = entry.to_string();
        assert_eq!(text.contains("call @word"), !ordinary, "{text}");
        if ordinary {
            assert!(text.contains("load.i64 v0"), "{text}");
        }
    }
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
