#[path = "load_reuse/asm_memory_effects.rs"]
mod asm_memory_effects;
#[path = "load_reuse/lifetime.rs"]
mod lifetime;

use keel_ir::{InstData, Module};

fn optimize(source: &str) -> Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["load-reuse"]).unwrap();
    m
}
fn loads(m: &Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Load { .. }))
        .count()
}
fn edge(middle: &str, ty: &str, second: &str) -> String {
    format!(
        "decl @effect(ptr)\nfunc @f(ptr, ptr, i64, i8) -> {ty} {{
block0(v0: ptr, v1: ptr, v2: i64, v3: i8):
 v4 = load.{ty} v0
 jump block1
block1:
 {middle}
 v5 = load.{ty} {second}
 return v5
}}"
    )
}

#[test]
fn successful_loads_reused_without_hoisting_and_with_exact_types() {
    for ty in ["i8", "i16", "i32", "i64", "i128", "f32", "f64", "ptr"] {
        let mut m = optimize(&edge("", ty, "v0"));
        assert_eq!(loads(&m), 1, "{ty}");
        assert!(!m.to_string().contains("return v1"));
        keel_opt::run_passes(&mut m, &["dce", "gcm"]).unwrap();
        assert_eq!(loads(&m), 1);
    }
    let src = edge("", "i64", "v0")
        .replace("v5 = load.i64", "v5 = load.i32")
        .replace("-> i64", "-> i32");
    assert_eq!(loads(&optimize(&src)), 2);
}

#[test]
fn stale_load_result_alias_cannot_supply_a_memory_fact() {
    let mut m = keel_ir::parse(&edge("", "i64", "v0")).unwrap();
    let f = &mut m.functions[0];
    let load = f
        .layout_insts()
        .find_map(|(_, i)| matches!(f.inst_data(i), InstData::Load { .. }).then_some(i))
        .unwrap();
    let entry = f.entry_block().unwrap();
    let unrelated = f.block_params(entry)[2];
    // The observable read remains in layout but its old result is redirected.
    // That unrelated value is not evidence of what memory contains.
    f.replace_all_uses(f.first_result(load), unrelated);
    keel_opt::run_passes(&mut m, &["load-reuse"]).unwrap();
    assert_eq!(loads(&m), 2);
    assert!(m.to_string().contains("return v5"));
}

#[test]
fn barriers_include_debugger_edits_and_lifetimes() {
    for middle in [
        "v8 = load.i64 volatile v0",
        "store volatile v2, v1",
        "v8 = atomic_load.i64 relaxed v0",
        "atomic_store relaxed v2, v0",
        "fence seq_cst",
        "call @effect(v0)",
        "asm \"\", \"~{memory}\"()",
        "intrinsic debugtrap()",
        "intrinsic stack_restore(v1)",
        "intrinsic memset(v0, v3, v2)",
    ] {
        assert_eq!(
            loads(&optimize(&edge(middle, "i64", "v0"))),
            if middle.contains("load.i64 volatile") {
                3
            } else {
                2
            },
            "{middle}"
        );
    }
}

#[test]
fn writes_use_aliases_and_circular_footprints() {
    for (offset, width, expected) in [
        ("8", "i64", 1),
        ("-8", "i64", 1),
        ("4", "i64", 2),
        ("-4", "i64", 2),
        ("-1", "i8", 1),
        ("0", "i8", 2),
        ("8", "i128", 1),
        ("-8", "i128", 2),
    ] {
        let src = format!(
            "func @f(ptr, {width}) -> i64 {{
block0(v0: ptr, v1: {width}):
 v2 = load.i64 v0
 jump block1
block1:
 v3 = iconst.i64 {offset}
 v4 = ptradd v0, v3
 store v1, v4
 v5 = load.i64 v0
 return v5
}}"
        );
        assert_eq!(loads(&optimize(&src)), expected, "{offset}/{width}");
    }
    for (store, expected) in [
        ("store v2, v1", 2),
        ("store v3, v0", 2),
        ("store v2, v0", 1),
    ] {
        assert_eq!(loads(&optimize(&edge(store, "i64", "v0"))), expected);
    }
}

#[test]
fn canonical_constant_paths_and_checked_overflow() {
    for (offset, expected) in [("-8", 1), ("9223372036854775807", 2)] {
        let src = format!(
            "func @f(ptr, i64) -> i64 {{
block0(v0: ptr, v1: i64):
 v2 = iconst.i64 8
 v3 = ptradd v0, v2
 v4 = iconst.i64 {offset}
 v5 = ptradd v3, v4
 v6 = load.i64 v0
 jump block1
block1:
 v7 = load.i64 v5
 return v7
}}"
        );
        assert_eq!(loads(&optimize(&src)), expected);
    }
    let src = edge(
        "v8 = iconst.i64 9223372036854775807\n v9 = ptradd v0, v8\n v10 = iconst.i64 1\n v11 = ptradd v9, v10\n store v2, v11",
        "i64",
        "v0",
    );
    assert_eq!(loads(&optimize(&src)), 2);
}

#[test]
fn siblings_and_merges_have_separate_states() {
    let m = optimize(
        "func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
 brif v1, block1, block2
block1:
 v2 = load.i64 v0
 jump block3
block2:
 v3 = load.i64 v0
 jump block3
block3:
 v4 = load.i64 v0
 return v4
}",
    );
    assert_eq!(loads(&m), 3);
    // A write on one sibling cannot contaminate the other sibling's snapshot.
    let m = optimize(
        "func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
 v2 = load.i64 v0
 brif v1, block1, block2
block1:
 store v1, v0
 v3 = load.i64 v0
 return v3
block2:
 v4 = load.i64 v0
 return v4
}",
    );
    assert_eq!(loads(&m), 1);
    assert!(m.to_string().contains("return v2"));
    assert!(m.to_string().contains("return v1"));
}

#[test]
fn loops_reset_at_headers_and_never_reuse_a_previous_iteration() {
    let m = optimize(
        "func @f(ptr, i64) {
block0(v0: ptr, v1: i64):
 v2 = load.i64 v0
 jump block1
block1:
 v3 = load.i64 v0
 jump block2
block2:
 v4 = load.i64 v0
 store v1, v0
 jump block1
}",
    );
    assert_eq!(loads(&m), 2);
    assert!(!m.to_string().contains("v4 = load"));
}

#[test]
fn block_arguments_are_not_assumed_equal_and_raw_aliases_resolve() {
    let m = optimize(
        "func @f(ptr) -> i64 {
block0(v0: ptr):
 v1 = load.i64 v0
 jump block1(v0)
block1(v2: ptr):
 v3 = load.i64 v2
 return v3
}",
    );
    assert_eq!(loads(&m), 2);
    let mut m = keel_ir::parse(&edge("", "i64", "v1")).unwrap();
    let f = &mut m.functions[0];
    let params = f.block_params(f.entry_block().unwrap()).to_vec();
    f.replace_all_uses(params[1], params[0]);
    keel_opt::run_passes(&mut m, &["load-reuse"]).unwrap();
    assert_eq!(loads(&m), 1);
}

#[test]
fn intervening_faulting_loads_stay_in_order_and_volatile_is_never_reused() {
    let m = optimize(&edge("v8 = load.i8 v1", "i64", "v0"));
    assert_eq!(loads(&m), 2);
    let text = m.to_string();
    assert!(text.find("load.i64").unwrap() < text.find("load.i8").unwrap());
    assert!(text.contains("return v4"));
    assert_eq!(loads(&optimize(&edge("", "i64", "volatile v0"))), 2);
    let src = edge("", "i64", "v0").replace("v4 = load.i64 v0", "v4 = load.i64 volatile v0");
    assert_eq!(loads(&optimize(&src)), 2);
}

#[test]
fn chain_and_fact_budgets_are_conservative() {
    let mut source =
        String::from("func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 v0\n jump block1\n");
    for i in 1..10 {
        source += &format!("block{i}:\n jump block{}\n", i + 1);
    }
    source += "block10:\n v2 = load.i64 v0\n return v2\n}";
    assert_eq!(loads(&optimize(&source)), 2);
    let mut source = String::from("func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 v0\n");
    for i in 0..128 {
        source += &format!(
            " v{} = iconst.i64 {}\n v{} = ptradd v0, v{}\n v{} = load.i64 v{}\n",
            3 * i + 2,
            (i + 1) * 8,
            3 * i + 3,
            3 * i + 2,
            3 * i + 4,
            3 * i + 3
        );
    }
    source += " jump block1\nblock1:\n v400 = load.i64 v0\n return v400\n}";
    assert_eq!(loads(&optimize(&source)), 130);
}

#[test]
fn full_only_default_admission() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        assert_eq!(
            tier.pipeline().contains(&"load-reuse"),
            tier == keel_opt::OptimizationTier::Full
        );
    }
}

#[test]
fn debugger_variable_homes_are_not_reused() {
    let m = optimize(
        "dtype dt0 = base \"long\" signed 8
func @f() -> i64 {
 ss0 = stack_slot 8, align 8
 dvar \"x\" dt0 ss0
block0:
 v0 = stack_addr ss0
 v1 = load.i64 v0
 jump block1
block1:
 v2 = load.i64 v0
 return v2
}",
    );
    assert_eq!(loads(&m), 2);
}

#[test]
fn restrict_only_retains_disjoint_eligible_roots() {
    for (contract, expected) in [
        (" restrict_params(0, 1)", 1),
        (" restrict_params(0)", 2),
        ("", 2),
    ] {
        let src =
            edge("store v2, v1", "i64", "v0").replace("-> i64 {", &format!("-> i64{contract} {{"));
        assert_eq!(loads(&optimize(&src)), expected);
    }
}

#[test]
fn equal_width_different_types_and_vector_shapes_do_not_match() {
    for (first, second) in [("i32", "f32"), ("i64", "f64"), ("i8x16", "i32x4")] {
        let src = edge("", first, "v0")
            .replace(&format!("-> {first}"), &format!("-> {second}"))
            .replace(
                &format!("v5 = load.{first}"),
                &format!("v5 = load.{second}"),
            );
        assert_eq!(loads(&optimize(&src)), 2);
    }
    for shape in keel_ir::VectorType::ALL_128 {
        assert_eq!(loads(&optimize(&edge("", &shape.to_string(), "v0"))), 1);
    }
}

fn written(prior: &str, middle: &str, flags: &str) -> String {
    format!(
        "decl @effect(ptr)\nfunc @f(ptr, i64, ptr) -> i64 {{
block0(v0: ptr, v1: i64, v2: ptr):
 {prior}
 store v1, v0
 jump block1
block1:
 {middle}
 v4 = load.i64 {flags} v0
 return v4
}}"
    )
}
#[test]
fn exact_stores_forward_across_edges_after_successful_reads() {
    let m = optimize(&written("v3 = load.i64 v0", "", ""));
    assert_eq!(loads(&m), 1);
    assert!(m.to_string().contains("return v1"));
    assert_eq!(
        m.functions[0]
            .layout_insts()
            .filter(|&(_, i)| matches!(m.functions[0].inst_data(i), InstData::Store { .. }))
            .count(),
        1
    );
}
#[test]
fn a_store_does_not_erase_the_first_potential_read_fault() {
    for prior in ["", "v3 = iconst.i64 0"] {
        let m = optimize(&written(prior, "", ""));
        assert_eq!(loads(&m), 1);
        assert!(!m.to_string().contains("return v1"));
    }
    let source = written("", "", "").replace("store v1, v0", "store notrap v1, v0");
    assert_eq!(loads(&optimize(&source)), 1);
    let m = optimize(&written("", "", "notrap"));
    assert_eq!(loads(&m), 0);
    assert!(m.to_string().contains("return v1"));
}
#[test]
fn writes_barriers_and_type_changes_discard_store_readability() {
    for middle in [
        "call @effect(v0)",
        "store v1, v2",
        "fence acquire",
        "v5 = iconst.i8 0\n store v5, v0",
    ] {
        assert_eq!(
            loads(&optimize(&written("v3 = load.i64 v0", middle, ""))),
            2,
            "{middle}"
        );
    }
    let text = written("v3 = load.i64 v0", "", "").replace("store v1, v0", "store volatile v1, v0");
    assert_eq!(loads(&optimize(&text)), 2);
    let text = written("v3 = load.i64 v0", "", "")
        .replace("store v1, v0", "v5 = iconst.i32 9\n store v5, v0")
        .replace("v4 = load.i64", "v4 = load.i32")
        .replace("-> i64", "-> i32");
    assert_eq!(loads(&optimize(&text)), 2);
}
#[test]
fn exact_store_values_keep_all_scalar_bit_patterns_and_types() {
    for ty in ["i8", "i16", "i32", "i64", "i128", "f32", "f64", "ptr"] {
        let src = format!(
            "func @f(ptr, {ty}) -> {ty} {{
block0(v0: ptr, v1: {ty}):
 v2 = load.{ty} v0
 store v1, v0
 jump block1
block1:
 v3 = load.{ty} v0
 return v3
}}"
        );
        let m = optimize(&src);
        assert_eq!(loads(&m), 1, "{ty}");
        assert!(m.to_string().contains("return v1"), "{m}");
        keel_ir::verify_module(&m).unwrap();
    }
}

#[test]
fn merges_retain_only_identical_successful_reads_on_every_path() {
    for (middle, expected) in [
        ("", 1),
        ("store v2, v0", 2),
        ("call @effect(v0)", 2),
        ("v9 = load.i64 volatile v0", 3),
    ] {
        let source = format!(
            "decl @effect(ptr)
func @f(ptr, i8, i64) -> i64 {{
block0(v0: ptr, v1: i8, v2: i64):
 v3 = load.i64 v0
 brif v1, block1, block2
block1:
 {middle}
 jump block3
block2:
 jump block3
block3:
 v4 = load.i64 v0
 return v4
}}"
        );
        assert_eq!(loads(&optimize(&source)), expected, "{source}");
    }
}

#[test]
fn equal_stores_at_merge_do_not_establish_readability() {
    for notrap in ["", " notrap"] {
        let source = format!(
            "func @f(ptr, i8, i64) -> i64 {{
block0(v0: ptr, v1: i8, v2: i64):
 brif v1, block1, block2
block1:
 store v2, v0
 jump block3
block2:
 store v2, v0
 jump block3
block3:
 v3 = load.i64{notrap} v0
 return v3
}}"
        );
        assert_eq!(loads(&optimize(&source)), usize::from(notrap.is_empty()));
    }
}

#[test]
fn merge_readability_requires_a_successful_read_on_every_path() {
    let m = optimize(
        "func @f(ptr, i8, i64) -> i64 {
block0(v0: ptr, v1: i8, v2: i64):
 brif v1, block1, block2
block1:
 v3 = load.i64 v0
 store v2, v0
 jump block3
block2:
 store v2, v0
 jump block3
block3:
 v4 = load.i64 v0
 return v4
}",
    );
    assert_eq!(loads(&m), 2);
    assert!(m.to_string().contains("v4 = load.i64"));
}

#[test]
fn unreachable_predecessor_cannot_supply_or_preserve_memory_facts() {
    let m = optimize(
        "func @f(ptr) -> i64 {
block0(v0: ptr):
 v1 = load.i64 v0
 jump block1
block1:
 v2 = load.i64 v0
 return v2
block2:
 jump block1
}",
    );
    assert_eq!(loads(&m), 2);
}

#[test]
fn completed_merge_predecessor_budget_has_an_exact_boundary() {
    for count in [8, 9] {
        let join = count + 1;
        let cases = (2..=count)
            .map(|i| format!("{}: block{i}", i - 2))
            .collect::<Vec<_>>()
            .join(", ");
        let mut source = format!(
            "func @f(ptr, i64) -> i64 {{\nblock0(v0: ptr, v1: i64):\n v2 = load.i64 v0\n switch v1, block1, [{cases}]\n"
        );
        for i in 1..=count {
            source += &format!("block{i}:\n jump block{join}\n");
        }
        source += &format!("block{join}:\n v3 = load.i64 v0\n return v3\n}}");
        assert_eq!(loads(&optimize(&source)), if count == 8 { 1 } else { 2 });
    }
}
