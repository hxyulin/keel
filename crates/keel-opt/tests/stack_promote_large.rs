mod support;
use keel_ir::{Function, InstData as D, Module};

fn execute(f: &Function, inputs: &[u64]) -> u64 {
    let mut f = f.clone();
    keel_opt::find_pass("dce").unwrap().run(&mut f);
    support::execute(&f, inputs)
}
fn memory(f: &Function) -> usize {
    f.layout_insts()
        .filter(|(_, i)| matches!(f.inst_data(*i), D::Load { .. } | D::Store { .. }))
        .count()
}
fn params(f: &Function) -> usize {
    f.blocks().map(|b| f.block_params(b).len()).sum()
}
fn optimize(src: &str) -> Module {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["stack-promote-large"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
fn chain(n: usize) -> String {
    let mut s = "func @f(i64) -> i64 {\n ss0 = stack_slot 8, align 8\nblock0(v0: i64):\n v1 = stack_addr ss0\n store v0,v1\n jump block1\n".to_owned();
    for b in 1..n {
        s += &format!("block{b}:\n jump block{}\n", b + 1);
    }
    s += &format!("block{n}:\n v2 = load.i64 v1\n return v2\n}}");
    s
}
#[test]
fn large_chain_is_opt_in_and_dead_suffix_does_not_get_parameters() {
    let src = chain(1000).replace(
        "v2 = load.i64 v1\n return v2",
        "v2 = load.i64 v1\n jump block1001\nblock1001:\n return v2",
    );
    let mut default = keel_ir::parse(&src).unwrap();
    let original = default.to_string();
    keel_opt::run_passes(&mut default, &["stack-promote"]).unwrap();
    assert_eq!(original, default.to_string());
    let m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0);
    let tail = m.functions[0].blocks().last().unwrap();
    assert!(m.functions[0].block_params(tail).is_empty());
    assert_eq!(execute(&m.functions[0], &[73]), 73);
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"stack-promote-large"));
    }
}
fn many_fields(n: usize, escape: bool) -> String {
    let mut s = "decl @escape(ptr)\nfunc @f(i64) -> i64 {\n".to_owned();
    // Interspersed escaped slots used to exhaust the global field cap.
    for slot in 0..n {
        s += &format!(" ss{slot} = stack_slot 8, align 8\n");
    }
    s += "block0(v0: i64):\n";
    for slot in 0..n {
        let v = 1 + slot * 2;
        s += &format!(" v{v} = stack_addr ss{slot}\n store v0,v{v}\n");
        if escape && slot % 2 == 0 {
            s += &format!(" call @escape(v{v})\n");
        }
        s += &format!(" v{} = load.i64 v{v}\n", v + 1);
    }
    s += &format!(" return v{}\n}}", n * 2);
    s
}
#[test]
fn escaped_objects_do_not_spend_field_budget_and_growth_is_bounded() {
    let m = optimize(&many_fields(200, true));
    // Exactly 32 private fields selected despite 100 interspersed escapes.
    assert_eq!(memory(&m.functions[0]), 400 - 64);
    assert_eq!(params(&m.functions[0]), 1);
    let m = optimize(&many_fields(200, false));
    assert_eq!(memory(&m.functions[0]), 400 - 64);
}
#[test]
fn many_fields_in_one_slot_do_not_trigger_global_bailout() {
    let mut src = many_fields(100, false);
    for slot in 1..100 {
        src = src.replace(&format!(" ss{slot} = stack_slot 8, align 8\n"), "");
    }
    src = src.replace("ss0 = stack_slot 8", "ss0 = stack_slot 800");
    for slot in 1..100 {
        let v = 1 + slot * 2;
        src = src.replace(
            &format!("v{v} = stack_addr ss{slot}"),
            &format!(
                "v{} = iconst.i64 {}\n v{v} = ptradd v1,v{}",
                1000 + slot,
                slot * 8,
                1000 + slot
            ),
        );
    }
    assert_eq!(memory(&optimize(&src).functions[0]), 200 - 64);
}
fn loop_fixture() -> String {
    "func @f(i64, i64) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: i64,v1: i64):
 v2 = stack_addr ss0
 store v1,v2
 v3 = iconst.i64 0
 jump block1(v3)
block1(v4: i64):
 v5 = icmp.ult v4,v0
 brif v5,block2,block3
block2:
 v6 = load.i64 v2
 v7 = iadd v6,v4
 store v7,v2
 v8 = iconst.i64 1
 v9 = iadd v4,v8
 jump block1(v9)
block3:
 v10 = load.i64 v2
 return v10
}"
    .to_owned()
}
#[test]
fn loops_zero_trips_partial_coverage_and_unreachable_predecessors() {
    let src = loop_fixture();
    let m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0);
    for n in [0u64, 1, 2, 63] {
        for seed in [0u64, 1, u64::MAX] {
            assert_eq!(
                execute(&m.functions[0], &[n, seed]),
                seed.wrapping_add(n * n.saturating_sub(1) / 2)
            );
        }
    }
    for bad in [
        src.replace("store v1,v2", ""),
        src.replace("store v1,v2", "v11 = iconst.i8 0\n store v11,v2"),
        src.replace("store v1,v2", "")
            .replace("v6 = load.i64 v2", "store v1,v2\n v6 = load.i64 v2"),
        src.replace("return v10", "return v10\nblock4:\n jump block1(v1)"),
        src.replace(
            "store v1,v2",
            "store v1,v2\n v11 = intrinsic frame_address()",
        ),
    ] {
        assert_eq!(
            keel_ir::parse(&bad).unwrap().to_string(),
            optimize(&bad).to_string()
        );
    }
}
#[test]
fn join_values_duplicate_edges_and_alias_substitution() {
    let src = "func @f(i8,i64,i64) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: i8,v1: i64,v2: i64):
 v3 = stack_addr ss0
 brif v0,block1,block2
block1:
 store v1,v3
 jump block3
block2:
 store v2,v3
 jump block3
block3:
 v4 = load.i64 v3
 store v4,v3
 brif v0,block4,block4
block4:
 v5 = load.i64 v3
 return v5
}";
    let m = optimize(src);
    assert_eq!(memory(&m.functions[0]), 0);
    for c in [0, 1, 255] {
        assert_eq!(
            execute(&m.functions[0], &[c, 31, 79]),
            if c == 0 { 79 } else { 31 }
        );
    }
    let mut m = keel_ir::parse(&loop_fixture()).unwrap();
    let f = &mut m.functions[0];
    let load = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), D::Load { .. }))
        .unwrap()
        .1;
    let old = f.first_result(load);
    let replacement = f.block_params(f.entry_block().unwrap())[1];
    f.replace_all_uses(old, replacement);
    keel_opt::find_pass("stack-promote-large").unwrap().run(f);
    assert_eq!(f.resolve(old), replacement);
    assert_eq!(memory(f), 4);
}

// Optional corpus evidence runner: invoked explicitly under the shared lock.
#[test]
fn original_c_activation() {
    let Ok(input) = std::env::var("KEEL_STACK_LARGE_INPUT") else {
        return;
    };
    let output = std::env::var("KEEL_STACK_LARGE_OUTPUT").unwrap();
    let mut m = keel_ir::parse(&std::fs::read_to_string(input).unwrap()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    let mut events = Vec::new();
    for f in &mut m.functions {
        let before = memory(f);
        let old_params = params(f);
        let blocks = f.blocks().count();
        let insts = f.layout_insts().count();
        let changed = keel_opt::find_pass("stack-promote-large").unwrap().run(f);
        let after = memory(f);
        if changed || f.name == "JS_CallInternal" {
            events.push(format!("{{\"function\":\"{}\",\"blocks\":{blocks},\"instructions\":{insts},\"changed\":{changed},\"memory_before\":{before},\"memory_after\":{after},\"params_before\":{old_params},\"params_after\":{}}}",f.name,params(f)));
        }
    }
    keel_ir::verify_module(&m).unwrap();
    std::fs::write(format!("{output}.keel"), m.to_string()).unwrap();
    std::fs::write(
        format!("{output}.json"),
        format!("[\n{}\n]", events.join(",\n")),
    )
    .unwrap();
    assert!(
        events.iter().any(|e| e.contains("\"changed\":true")),
        "no corpus activation"
    );
}

#[test]
fn dead_fields_in_large_cfg_have_no_phi_growth() {
    let src = chain(1000)
        .replace("store v0,v1", "store v0,v1\n v3 = load.i64 v1")
        .replace("v2 = load.i64 v1\n return v2", "return v3");
    let m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0);
    assert_eq!(params(&m.functions[0]), 1);
    assert_eq!(execute(&m.functions[0], &[u64::MAX]), u64::MAX);
}

#[test]
fn instruction_and_parameter_growth_limits_are_independent() {
    let mut src = chain(1000);
    for slot in 1..20 {
        src = src.replace(
            "block0(v0",
            &format!("ss{slot} = stack_slot 8, align 8\nblock0(v0"),
        );
        src = src.replace(
            "jump block1\n",
            &format!(
                "v{} = stack_addr ss{slot}\n store v0,v{}\n jump block1\n",
                100 + slot,
                100 + slot
            ),
        );
        src = src.replace(
            "return v2",
            &format!("v{} = load.i64 v{}\n return v2", 200 + slot, 100 + slot),
        );
    }
    let m = optimize(&src);
    assert!(
        memory(&m.functions[0]) > 0,
        "parameter cap must leave some fields in memory"
    );
    assert!(params(&m.functions[0]) <= 8193);
    assert!(memory(&m.functions[0]) < 40);
    let mut src = chain(1);
    let noise: String = (0..9000)
        .map(|i| format!("v{} = iconst.i64 0\n", i + 100))
        .collect();
    src = src.replace("store v0,v1", &format!("{noise}store v0,v1"));
    let m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0);
    assert_eq!(execute(&m.functions[0], &[17]), 17);
    let oversized = chain(4096);
    assert_eq!(
        optimize(&oversized).to_string(),
        keel_ir::parse(&oversized).unwrap().to_string()
    );
}

#[test]
fn large_mode_retains_splat_restrictions_source_locations_and_observer_barriers() {
    let src = loop_fixture()
        .replace("stack_slot 8", "stack_slot 16")
        .replace("store v1,v2", "v11 = iconst.i64 -1\n store v11,v2")
        .replace("i64", "i32")
        .replace("v11 = iconst.i32 -1", "v11 = iconst.i64 -1");
    let mut m = optimize(&src);
    assert_eq!(memory(&m.functions[0]), 0);
    for n in [0u64, 1, 2, 31] {
        assert_eq!(
            execute(&m.functions[0], &[n, 0]),
            (u64::from(u32::MAX) + n * n.saturating_sub(1) / 2) & u64::from(u32::MAX)
        );
    }
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    m = keel_ir::parse(&src).unwrap();
    let f = &mut m.functions[0];
    let store = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), D::Store { .. }))
        .unwrap()
        .1;
    let loc = keel_ir::SourceLoc {
        file: keel_ir::SourceFile::new(0),
        line: 27,
        col: 5,
        scope: None,
    };
    f.set_inst_loc(store, Some(loc));
    keel_opt::find_pass("stack-promote-large").unwrap().run(f);
    assert!(f.layout_insts().any(|(_, i)| f.inst_loc(i) == Some(loc)
        && matches!(
            f.inst_data(i),
            D::Iconst {
                ty: keel_ir::Type::I32,
                ..
            }
        )));
    for effect in [
        "call @observe(v2)",
        "call @twice()",
        "v12 = intrinsic stack_save()",
        "v12 = load.i8 volatile v2",
        "v12 = atomic_load.i8 relaxed v2",
    ] {
        let bad = format!(
            "decl @observe(ptr)\ndecl @twice() returns_twice\n{}",
            src.replace("store v11,v2", &format!("store v11,v2\n {effect}"))
        );
        assert_eq!(
            keel_ir::parse(&bad).unwrap().to_string(),
            optimize(&bad).to_string()
        );
    }
    let bad = src.replace("v11 = iconst.i64 -1", "v11 = iconst.i64 4294967296");
    assert_eq!(
        keel_ir::parse(&bad).unwrap().to_string(),
        optimize(&bad).to_string()
    );
    let mut m = keel_ir::parse(&src).unwrap();
    m.debug_types.push(DebugTypeData::Base {
        name: "word".into(),
        encoding: Encoding::Unsigned,
        size: 4,
    });
    m.functions[0].debug_vars.push(DebugVar {
        name: "home".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["stack-promote-large"]).unwrap();
    assert_eq!(before, m.to_string());
}
