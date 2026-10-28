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
fn aliased_load_results_are_neither_memory_facts_nor_replacement_targets() {
    for aliased_load in [0, 1] {
        let mut module = keel_ir::parse(
            "func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
    v2 = load.i64 v0
    v3 = load.i64 v0
    return v3
}",
        )
        .unwrap();
        let f = &mut module.functions[0];
        let load = f
            .layout_insts()
            .filter_map(|(_, i)| matches!(f.inst_data(i), InstData::Load { .. }).then_some(i))
            .nth(aliased_load)
            .unwrap();
        let unrelated = f.block_params(f.entry_block().unwrap())[1];
        // The read remains observable, but its redirected result no longer
        // describes memory and cannot be redirected again by this pass.
        f.replace_all_uses(f.first_result(load), unrelated);
        keel_ir::verify_module(&module).unwrap();
        let before = module.to_string();
        keel_opt::run_passes(&mut module, &["memory-forward"]).unwrap();
        assert_eq!(loads(&module), 2, "aliased load {aliased_load}");
        assert_eq!(module.to_string(), before, "aliased load {aliased_load}");
    }
}

#[test]
fn debugger_variable_home_reads_are_not_forwarded() {
    let source = "dtype dt0 = base \"long\" signed 8
func @f(i64) -> i64 {
    ss0 = stack_slot 8, align 8
    dvar \"x\" dt0 ss0
block0(v0: i64):
    v1 = stack_addr ss0
    store v0, v1
    v2 = load.i64 v1
    v3 = load.i64 v1
    return v3
}";
    let module = optimize(source);
    assert_eq!(loads(&module), 2);
    assert!(module.to_string().contains("return v3"));
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

#[test]
fn restrict_keeps_a_load_across_a_distinct_restricted_store() {
    for (contract, expected) in [
        (" restrict_params(0, 1)", 1),
        ("", 2),
        (" restrict_params(0)", 2),
    ] {
        let source = format!(
            "func @f(ptr, ptr, i32) -> i32{contract} {{
block0(v0: ptr, v1: ptr, v2: i32):
 v3 = load.i32 v0
 store v2, v1
 v4 = load.i32 v0
 return v4
}}"
        );
        assert_eq!(loads(&optimize(&source)), expected, "{source}");
    }
}

#[test]
fn restricted_read_only_aliases_are_not_folded_together() {
    let m = optimize(
        "func @f(ptr, ptr) -> i32 restrict_params(0, 1) {
block0(v0: ptr, v1: ptr):
 v2 = load.i32 v0
 v3 = load.i32 v1
 v4 = iadd v2, v3
 return v4
}",
    );
    assert_eq!(loads(&m), 2);
}

#[test]
fn restrict_unknown_uses_and_observable_operations_are_conservative() {
    for barrier in [
        "v8 = ptradd v0, v3",
        "store v0, v4",
        "call @effect(v0)",
        "call @nothing()",
        "v8 = load.i32 volatile v0",
        "v8 = atomic_load.i32 relaxed v0",
        "v8 = icmp.eq v0, v1",
    ] {
        let source = format!(
            "decl @effect(ptr)
decl @nothing()
func @f(ptr, ptr, i32, i64, ptr) -> i32 restrict_params(0, 1) {{
block0(v0: ptr, v1: ptr, v2: i32, v3: i64, v4: ptr):
 v5 = load.i32 v0
 {barrier}
 store v2, v1
 v6 = load.i32 v0
 return v6
}}"
        );
        let expected = if barrier.contains("load.i32 volatile") {
            3
        } else {
            2
        };
        assert_eq!(loads(&optimize(&source)), expected, "{source}");
    }
}

#[test]
fn constant_paths_forward_equivalent_addresses_and_keep_adjacent_fields() {
    for (middle, expected) in [
        ("store v1, v6", 0),
        ("store v1, v7", 1),
        ("store v9, v7", 1),
        ("store v1, v10", 1),
        ("store v1, v11", 1),
    ] {
        let source = format!(
            "func @f(ptr, i64, i64, ptr) -> i64 {{
block0(v0: ptr, v1: i64, v2: i64, v3: ptr):
 v4 = iconst.i64 8
 v5 = ptradd v0, v4
 v6 = ptradd v5, v4
 v8 = iconst.i64 12
 v7 = ptradd v0, v8
 v9 = iconst.i8 7
 v10 = ptradd v0, v2
 v11 = ptradd v3, v4
 store v1, v5
 {middle}
 v12 = ptradd v0, v4
 v13 = load.i64 v12
 return v13
}}"
        );
        assert_eq!(loads(&optimize(&source)), expected, "{middle}");
    }
}

#[test]
fn negative_paths_and_checked_overflow_are_conservative() {
    for (offset, width, expected) in [
        ("-8", "i64", 1),
        ("-4", "i64", 2),
        ("-1", "i8", 1),
        ("4", "i64", 2),
    ] {
        let source = format!(
            "func @f(ptr, {width}) -> i64 {{
block0(v0: ptr, v1: {width}):
 v2 = iconst.i64 {offset}
 v3 = ptradd v0, v2
 v4 = load.i64 v0
 store v1, v3
 v5 = load.i64 v0
 return v5
}}"
        );
        assert_eq!(loads(&optimize(&source)), expected, "{offset}/{width}");
    }
    let m = optimize(
        "func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 9223372036854775807
 v3 = ptradd v0, v2
 v4 = iconst.i64 1
 v5 = ptradd v3, v4
 v6 = load.i64 v0
 store v1, v5
 v7 = load.i64 v0
 return v7
}",
    );
    assert_eq!(loads(&m), 2);
}

#[test]
fn restricted_derived_addresses_require_both_unescaped_roots() {
    for (extra, expected) in [
        ("", 1),
        ("v12 = icmp.eq v5, v6", 2),
        ("v12 = ptradd v5, v2", 2),
        ("store v5, v3", 2),
        ("call @escape(v5)", 2),
        ("v12 = load.i64 volatile v5", 3),
        ("v12 = atomic_load.i64 relaxed v5", 2),
    ] {
        let source = format!(
            "decl @escape(ptr)
func @f(ptr, ptr, i64, ptr) -> i64 restrict_params(0, 1) {{
block0(v0: ptr, v1: ptr, v2: i64, v3: ptr):
 v4 = iconst.i64 8
 v5 = ptradd v0, v4
 v6 = ptradd v1, v4
 v7 = load.i64 v5
 {extra}
 store v2, v6
 v8 = load.i64 v5
 return v8
}}"
        );
        assert_eq!(loads(&optimize(&source)), expected, "{extra}");
    }
}

#[test]
fn block_argument_use_disqualifies_restricted_derived_root() {
    let m = optimize(
        "func @f(ptr, ptr, i64) -> i64 restrict_params(0, 1) {
block0(v0: ptr, v1: ptr, v2: i64):
 v3 = iconst.i64 8
 v4 = ptradd v0, v3
 v5 = load.i64 v4
 store v2, v1
 v6 = load.i64 v4
 jump block1(v4, v6)
block1(v7: ptr, v8: i64):
 return v8
}",
    );
    assert_eq!(loads(&m), 2);
}

#[test]
fn signed_paths_cancel_but_unknown_offsets_stay_opaque() {
    for (offset, expected) in [("v2", 1), ("v7", 0)] {
        let source = format!(
            "func @f(ptr, i64, i64) -> i64 {{
block0(v0: ptr, v1: i64, v2: i64):
 v3 = iconst.i64 8
 v4 = ptradd v0, v3
 v7 = iconst.i64 -8
 v5 = ptradd v4, {offset}
 store v1, v0
 v6 = load.i64 v5
 return v6
}}"
        );
        assert_eq!(loads(&optimize(&source)), expected);
    }
}

#[test]
fn wide_store_invalidates_every_overlapping_fact() {
    let m = optimize(
        "func @f(ptr, i128) -> i64 {
block0(v0: ptr, v1: i128):
 v2 = iconst.i64 8
 v3 = ptradd v0, v2
 v4 = load.i64 v3
 store v1, v0
 v5 = load.i64 v3
 return v5
}",
    );
    assert_eq!(loads(&m), 2);
}
