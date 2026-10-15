use keel_ir::{InstData, Module};

fn optimize(source: &str) -> Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes(&mut module, &["memory-forward"]).unwrap();
    assert!(
        !keel_opt::find_pass("memory-forward")
            .unwrap()
            .run(&mut module.functions[0])
    );
    module
}

fn loads(module: &Module) -> usize {
    let f = &module.functions[0];
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
        .count()
}

#[test]
fn successful_access_forwards_only_the_same_type() {
    for ty in ["i8", "i16", "i32", "i64", "i128", "f32", "f64", "ptr"] {
        let source = format!(
            "func @f(ptr, {ty}) -> {ty} {{
block0(v0: ptr, v1: {ty}):
    store v1, v0
    v2 = load.{ty} v0
    v3 = load.{ty} notrap align 8 v0
    return v3
}}"
        );
        let module = optimize(&source);
        assert_eq!(loads(&module), 0, "{ty}");
        assert!(module.to_string().contains("return v1"));
    }
    let module = optimize(
        "func @f(ptr, i64) -> i32 {
block0(v0: ptr, v1: i64):
    store v1, v0
    v2 = load.i32 v0
    return v2
}",
    );
    assert_eq!(loads(&module), 1);
}

#[test]
fn first_potentially_faulting_load_is_preserved_even_if_unused() {
    let module = optimize(
        "func @f(ptr) -> i64 {
block0(v0: ptr):
    v1 = load.i64 v0
    v2 = load.i64 v0
    return v2
}",
    );
    assert_eq!(loads(&module), 1);
    assert!(module.to_string().contains("return v1"));
    let mut module = module;
    keel_opt::run_passes(&mut module, &["dce", "gcm"]).unwrap();
    assert_eq!(loads(&module), 1);
}

#[test]
fn unknown_aliases_and_partial_writes_invalidate_facts() {
    for store in ["store v2, v1", "store v2, v0", "store v3, v1"] {
        let source = format!(
            "func @f(ptr, ptr, i8, i64) -> i64 {{
block0(v0: ptr, v1: ptr, v2: i8, v3: i64):
    v4 = load.i64 v0
    {store}
    v5 = load.i64 v0
    return v5
}}"
        );
        assert_eq!(loads(&optimize(&source)), 2, "{store}");
    }
}

#[test]
fn observable_accesses_and_lifetime_barriers_invalidate_facts() {
    for barrier in [
        "v8 = load.i64 volatile v0",
        "store volatile v2, v1",
        "v8 = atomic_load.i64 relaxed v0",
        "fence seq_cst",
        "call @effect(v0)",
        "intrinsic debugtrap()",
        "intrinsic stack_restore(v1)",
        "intrinsic memset(v0, v3, v2)",
    ] {
        let source = format!(
            "decl @effect(ptr)
func @f(ptr, ptr, i64, i8) -> i64 {{
block0(v0: ptr, v1: ptr, v2: i64, v3: i8):
    v4 = load.i64 v0
    {barrier}
    v5 = load.i64 v0
    return v5
}}"
        );
        let expected = if barrier.contains("load.i64 volatile") {
            3
        } else {
            2
        };
        assert_eq!(loads(&optimize(&source)), expected, "{barrier}");
    }
}

#[test]
fn facts_do_not_escape_blocks_or_flow_back_around_loops() {
    let module = optimize(
        "func @f(ptr) -> i64 {
block0(v0: ptr):
    v1 = load.i64 v0
    jump block1
block1:
    v2 = load.i64 v0
    return v2
}",
    );
    assert_eq!(loads(&module), 2);
    let module = optimize(
        "func @f(ptr) {
block0(v0: ptr):
    jump block1
block1:
    v1 = load.i64 v0
    v2 = load.i64 v0
    jump block1
}",
    );
    assert_eq!(loads(&module), 1);
    assert!(module.to_string().contains("jump block1"));
}

#[test]
fn resolved_addresses_allow_forwarding_after_gvn() {
    let mut module = keel_ir::parse(
        "func @f(ptr, i64, i64) -> i64 {
block0(v0: ptr, v1: i64, v2: i64):
    v3 = ptradd v0, v1
    v4 = ptradd v0, v1
    store v2, v3
    v5 = load.i64 v4
    return v5
}",
    )
    .unwrap();
    keel_opt::run_passes(&mut module, &["gvn", "memory-forward", "dce", "gcm"]).unwrap();
    assert_eq!(loads(&module), 0);
}

#[test]
fn volatile_loads_are_never_forwarded_from_an_ordinary_store() {
    let module = optimize(
        "func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
    store v1, v0
    v2 = load.i64 volatile v0
    v3 = load.i64 volatile v0
    return v3
}",
    );
    assert_eq!(loads(&module), 2);
}
