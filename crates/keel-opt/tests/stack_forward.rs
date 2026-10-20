use keel_ir::{InstData, Module};
fn run(s: &str) -> Module {
    let mut m = keel_ir::parse(s).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(
        &mut m,
        &["stack-forward", "canonicalize", "instcombine", "dce"],
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}
fn loads(m: &Module) -> usize {
    m.functions[0]
        .layout_insts()
        .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Load { .. }))
        .count()
}
#[test]
fn forwards_partial_union_write_and_removes_private_stores() {
    let m = run("func @f(i32) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: i32):
 v1 = stack_addr ss0
 v2 = iconst.i64 0
 store v2, v1
 store v0, v1
 v3 = load.i64 v1
 return v3
}");
    assert_eq!(loads(&m), 0);
    assert!(
        !m.functions[0]
            .layout_insts()
            .any(|(_, i)| matches!(m.functions[0].inst_data(i), InstData::Store { .. }))
    );
    assert!(m.to_string().contains("uext.i64 v0"), "{m}");
}
#[test]
fn preserves_uninitialized_bytes_and_reads_before_initialization() {
    for body in [
        "store v0, v1\n v3 = load.i64 v1\n return v3",
        "jump block1\nblock1:\n v3 = load.i64 v1\n store v2, v1\n return v3",
    ] {
        let m = run(&format!(
            "func @f(i32) -> i64 {{\n ss0 = stack_slot 8, align 8\nblock0(v0: i32):\n v1 = stack_addr ss0\n v2 = iconst.i64 0\n {body}\n}}"
        ));
        assert_eq!(loads(&m), 1);
        assert!(m.to_string().contains("store"));
    }
}
#[test]
fn escaped_addresses_and_volatile_atomic_accesses_are_untouched() {
    for effect in [
        "call @escape(v1)",
        "v5 = ptrtoint.i64 v1",
        "store volatile v2, v1",
        "v5 = atomic_load.i64 relaxed v1",
        "v5 = intrinsic frame_address()",
    ] {
        let m = run(&format!(
            "decl @escape(ptr)\nfunc @f() -> i64 {{\n ss0 = stack_slot 8, align 8\nblock0:\n v1 = stack_addr ss0\n v2 = iconst.i64 42\n store v2, v1\n {effect}\n v3 = load.i64 v1\n return v3\n}}"
        ));
        assert_eq!(loads(&m), 1, "{effect}");
    }
}
#[test]
fn private_stack_survives_unrelated_aliasing_call_but_public_writes_do_not() {
    let m = run("decl @effect(ptr)
func @f(ptr) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = stack_addr ss0
 v2 = iconst.i64 17
 store v2, v1
 call @effect(v0)
 v3 = load.i64 v1
 return v3
}");
    assert_eq!(loads(&m), 0);
    assert!(m.to_string().contains("call @effect"));
}
#[test]
fn overlapping_stores_observe_byte_offsets() {
    let m = run("func @f(i64, i16) -> i32 {
 ss0 = stack_slot 8, align 8
block0(v0: i64, v1: i16):
 v2 = stack_addr ss0
 v3 = iconst.i64 2
 v4 = ptradd v2, v3
 store v0, v2
 store v1, v4
 v5 = load.i32 v2
 return v5
}");
    assert_eq!(loads(&m), 0);
}

#[test]
fn debugger_visible_homes_keep_both_stores_and_loads() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    let mut m = keel_ir::parse(
        "func @f() -> i64 {
 ss0 = stack_slot 8, align 8
block0:
 v0 = stack_addr ss0
 v1 = iconst.i64 3
 store v1,v0
 jump block1
block1:
 v2 = load.i64 v0
 return v2
}",
    )
    .unwrap();
    m.debug_types.push(DebugTypeData::Base {
        name: "long".into(),
        encoding: Encoding::Signed,
        size: 8,
    });
    m.functions[0].debug_vars.push(DebugVar {
        name: "d".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
    assert_eq!(before, m.to_string());
}

#[test]
fn unique_aggregate_pieces_cross_a_diamond_without_pointer_reinterpretation() {
    // Reduced from retained JS_CallInternal ss493: the pointer read keeps
    // storage alive, while the two exact-type reloads can use the snapshot.
    let m = run("func @f(i32, i64, i64) -> i64 {
 ss0 = stack_slot 16, align 8
block0(v0: i32, v1: i64, v2: i64):
 v3 = stack_addr ss0
 v4 = iconst.i64 8
 v5 = ptradd v3, v4
 store v1, v3
 store v2, v5
 brif v0, block1, block2
block1:
 v6 = load.ptr v3
 v7 = load.i32 v6
 v8 = iconst.i32 1
 v9 = iadd v7, v8
 store v9, v6
 jump block2
block2:
 v10 = load.i64 v3
 v11 = load.i64 v5
 v12 = iadd v10, v11
 return v12
}");
    assert_eq!(loads(&m), 2, "{m}"); // pointer read and refcount read
    let text = m.to_string();
    assert!(text.contains("load.ptr"), "{text}");
    assert_eq!(text.matches("store").count(), 3, "{text}");
}

#[test]
fn fully_promoted_pieces_drop_stores_and_address() {
    let m = run("func @f(i64) -> i64 {
 ss0 = stack_slot 16, align 8
block0(v0: i64):
 v1 = stack_addr ss0
 v2 = iconst.i64 8
 v3 = ptradd v1, v2
 store v0, v3
 jump block1
block1:
 v4 = load.i64 v3
 return v4
}");
    assert_eq!(loads(&m), 0);
    assert!(!m.to_string().contains("stack_addr"), "{m}");
    assert!(!m.to_string().contains("store"), "{m}");
}

#[test]
fn cross_block_competing_or_partial_writes_keep_reads() {
    for effect in ["store v1, v3", "store v0, v3"] {
        let m = run(&format!(
            "func @f(i32, i64) -> i64 {{
 ss0 = stack_slot 8, align 8
block0(v0: i32, v1: i64):
 v3 = stack_addr ss0
 store v1, v3
 brif v0, block1, block2
block1:
 {effect}
 jump block2
block2:
 v4 = load.i64 v3
 return v4
}}"
        ));
        assert_eq!(loads(&m), 1, "{m}");
    }
}

#[test]
fn initialization_on_only_one_path_does_not_dominate_join() {
    let m = run("func @f(i32, i64) -> i64 {
 ss0 = stack_slot 8, align 8
block0(v0: i32, v1: i64):
 v2 = stack_addr ss0
 brif v0, block1, block2
block1:
 store v1, v2
 jump block2
block2:
 v3 = load.i64 v2
 return v3
}");
    assert_eq!(loads(&m), 1);
}

#[test]
fn loop_store_can_dominate_load_but_backedge_store_cannot_initialize_it() {
    for (body, expected) in [
        ("store v1, v2\n jump block1\nblock1:\n v3 = load.i64 v2", 0),
        ("jump block1\nblock1:\n v3 = load.i64 v2\n store v1, v2", 1),
    ] {
        let m = run(&format!(
            "func @f(i32, i64) -> i64 {{
 ss0 = stack_slot 8, align 8
block0(v0: i32, v1: i64):
 v2 = stack_addr ss0
 {body}
 brif v0, block1, block2
block2:
 return v3
}}"
        ));
        assert_eq!(loads(&m), expected, "{m}");
    }
}

#[test]
fn escaped_snapshot_and_stack_restore_do_not_cross_blocks() {
    for effect in ["call @escape(v0)", "intrinsic stack_restore(v0)"] {
        let m = run(&format!(
            "decl @escape(ptr)
func @f(i64) -> i64 {{
 ss0 = stack_slot 8, align 8
block0(v1: i64):
 v0 = stack_addr ss0
 store v1, v0
 {effect}
 jump block1
block1:
 v2 = load.i64 v0
 return v2
}}"
        ));
        assert_eq!(loads(&m), 1, "{m}");
        assert!(m.to_string().contains("store"), "{m}");
    }
}

#[test]
#[ignore = "requires the sealed profile's optimized quickjs.keel; source-only phase does not execute it"]
fn retained_quickjs_ss493_exact_reload_activation() {
    use keel_ir::{BinaryOp, StackSlot};
    use std::collections::HashMap;
    let path = std::env::var("KEEL_AGGREGATE_RETAINED_QUICKJS_IR").unwrap();
    let mut m = keel_ir::parse(&std::fs::read_to_string(path).unwrap()).unwrap();
    m.functions.retain(|f| f.name == "JS_CallInternal");
    assert_eq!(m.functions.len(), 1);
    keel_ir::verify(&m.functions[0]).unwrap();
    fn pieces(f: &keel_ir::Function) -> (usize, usize, usize) {
        let mut addresses = HashMap::<_, (StackSlot, i64)>::new();
        let mut constants = HashMap::new();
        let insts: Vec<_> = f.layout_insts().map(|(_, i)| i).collect();
        for &i in &insts {
            match *f.inst_data(i) {
                InstData::StackAddr { slot } => {
                    addresses.insert(f.first_result(i), (slot, 0));
                }
                InstData::Iconst { imm, .. } => {
                    constants.insert(f.first_result(i), imm);
                }
                _ => {}
            }
        }
        loop {
            let before = addresses.len();
            for &i in &insts {
                if let InstData::Binary {
                    op: BinaryOp::Ptradd,
                    args,
                } = *f.inst_data(i)
                    && let Some(&(slot, offset)) = addresses.get(&f.resolve(args[0]))
                    && let Some(&n) = constants.get(&f.resolve(args[1]))
                {
                    addresses.insert(f.first_result(i), (slot, offset + n));
                }
            }
            if addresses.len() == before {
                break;
            }
        }
        let (mut integer, mut pointer, mut stores) = (0, 0, 0);
        for &i in &insts {
            let addr = match *f.inst_data(i) {
                InstData::Load { addr, .. } | InstData::Store { addr, .. } => addr,
                _ => continue,
            };
            if let Some(&(slot, _)) = addresses.get(&f.resolve(addr)) {
                use keel_ir::EntityRef;
                if slot.index() == 493 {
                    match *f.inst_data(i) {
                        InstData::Load {
                            ty: keel_ir::Type::I64,
                            ..
                        } => integer += 1,
                        InstData::Load {
                            ty: keel_ir::Type::Ptr,
                            ..
                        } => pointer += 1,
                        InstData::Store { .. } => stores += 1,
                        _ => {}
                    }
                }
            }
        }
        (integer, pointer, stores)
    }
    assert_eq!(pieces(&m.functions[0]), (2, 1, 2));
    keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
    assert_eq!(pieces(&m.functions[0]), (0, 1, 2));
}

#[test]
fn returns_twice_invoke_keeps_memory_across_nonlocal_resumption() {
    // R1-P2-1: CFG dominance omits save/restore nonlocal resumption edges.
    // Keep the reviewer's full loop reducer, not just a straight-line call.
    let mut m = keel_ir::parse(
        r#"decl @save(ptr) -> i32 returns_twice
decl @restore(ptr)
decl @personality()
func @f(ptr) -> i64 personality @personality {
 ss0 = stack_slot 8, align 8
block0(v0: ptr):
 v1 = stack_addr ss0
 v2 = iconst.i64 1
 v3 = iconst.i64 2
 jump block1(v2)
block1(v4: i64):
 store v4, v1
 v5 = icmp.eq v4, v2
 brif v5, block2, block4
block2:
 v6 = invoke @save(v0), block3, block5
block3:
 v7 = load.i64 v1
 brif v6, block6, block1(v3)
block4:
 call @restore(v0)
 trap
block5:
 v8, v9 = landingpad cleanup
 trap
block6:
 return v7
}
"#,
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    let before = m.to_string();
    keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(before, m.to_string());
    assert_eq!(loads(&m), 1);
}

#[test]
fn direct_and_indirect_calls_use_declared_returns_twice_signature() {
    for indirect in [false, true] {
        for twice in [false, true] {
            let attrs = if twice { "returns_twice" } else { "" };
            let call = if indirect {
                format!("call_indirect v0() : () {attrs}")
            } else {
                "call @effect()".to_owned()
            };
            let mut m = keel_ir::parse(&format!(
                "decl @effect() {attrs}
func @f(ptr, i64) -> i64 {{
 ss0 = stack_slot 8, align 8
block0(v0: ptr, v1: i64):
 v2 = stack_addr ss0
 store v1, v2
 {call}
 jump block1
block1:
 v3 = load.i64 v2
 return v3
}}"
            ))
            .unwrap();
            keel_ir::verify_module(&m).unwrap();
            let before = m.to_string();
            keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
            if twice {
                assert_eq!(before, m.to_string());
            } else {
                assert_eq!(loads(&m), 0);
            }
        }
    }
}

// The shared scalar executor has no stack memory, calls or traps. This small
// fixture oracle models those effects independently, including executed traps. Memory here is deliberately limited to the single-byte fixtures.
fn call_trace(m: &Module) -> (Option<u64>, Vec<String>) {
    use keel_ir::{EntityRef, Value};
    let f = &m.functions[0];
    let mut values = vec![0u64; f.num_values()];
    let mut memory = std::collections::HashMap::new();
    let mut trace = Vec::new();
    let mut block = f.entry_block().unwrap();
    for _ in 0..100 {
        let mut next = None;
        for &inst in f.block_insts(block) {
            let read = |v: Value| values[f.resolve(v).index()];
            let result = match f.inst_data(inst) {
                InstData::Iconst { imm, .. } => Some(*imm as u64 & 255),
                InstData::StackAddr { slot } => Some(slot.index() as u64),
                InstData::Store { addr, value, .. } => {
                    memory.insert(read(*addr), read(*value));
                    None
                }
                InstData::Load { addr, .. } => Some(memory[&read(*addr)]),
                InstData::Call { func, args } => {
                    assert!(args.is_empty());
                    trace.push(f.callee(*func).name.clone());
                    Some(42)
                }
                InstData::Return { args } => return (Some(read(args[0])), trace),
                InstData::Trap => return (None, trace),
                InstData::Jump { dest } => {
                    next = Some(dest);
                    None
                }
                InstData::Brif { cond, dests } => {
                    next = Some(&dests[usize::from(read(*cond) == 0)]);
                    None
                }
                InstData::Switch { arg, dests, cases } => {
                    next = Some(
                        &dests[cases
                            .iter()
                            .position(|&c| c as u64 & 255 == read(*arg))
                            .map_or(0, |i| i + 1)],
                    );
                    None
                }
                data => panic!("unexpected executed instruction: {data:?}"),
            };
            if let Some(result) = result {
                values[f.first_result(inst).index()] = result;
            }
        }
        let dest = next.expect("terminator");
        let args: Vec<_> = dest
            .args
            .iter()
            .map(|&v| values[f.resolve(v).index()])
            .collect();
        block = dest.block;
        for (&param, arg) in f.block_params(block).iter().zip(args) {
            if f.resolve(param) == param {
                values[param.index()] = arg;
            }
        }
    }
    panic!("fixture exceeded step limit")
}

fn conditional_fixture(value: i64, terminator: &str) -> String {
    format!(
        "decl @yes() -> i64\ndecl @no() -> i64
func @f() -> i64 {{
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = iconst.i8 {value}
 store v1, v0
 v2 = load.i8 v0
 {terminator}
block1:
 v3 = call @yes()
 return v3
block2:
 v4 = call @no()
 return v4
block3:
 v6 = call @no()
 trap
}}"
    )
}

#[test]
fn full_and_standalone_consume_only_the_taken_call_or_trap_arm() {
    use keel_opt::OptimizationTier;
    for (value, term, expected) in [
        (1, "brif v2, block1, block3", "yes"),
        (0, "brif v2, block3, block1", "yes"),
        (7, "switch v2, block3, [7: block1, 9: block2]", "yes"),
        (9, "switch v2, block3, [7: block1, 9: block2]", "no"),
        (0, "switch v2, block1, [7: block3, 9: block2]", "yes"),
        (-1, "switch v2, block3, [255: block1, 9: block2]", "yes"),
    ] {
        let original = keel_ir::parse(&conditional_fixture(value, term)).unwrap();
        keel_ir::verify_module(&original).unwrap();
        let expected_trace = (Some(42), vec![expected.to_string()]);
        assert_eq!(call_trace(&original), expected_trace);
        for full in [false, true] {
            let mut m = original.clone();
            if full {
                keel_opt::optimize_with_tier(&mut m, OptimizationTier::Full).unwrap();
            } else {
                keel_opt::run_passes(&mut m, &["stack-forward-cfg"]).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            assert_eq!(
                call_trace(&m),
                expected_trace,
                "{value}, {term}, full={full}"
            );
            let f = &m.functions[0];
            assert_eq!(f.blocks().count(), 1, "{m}");
            assert_eq!(
                f.layout_insts()
                    .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
                    .count(),
                1
            );
            assert!(!f.layout_insts().any(|(_, i)| matches!(
                f.inst_data(i),
                InstData::Brif { .. } | InstData::Switch { .. } | InstData::Trap
            )));
        }
        for tier in [
            OptimizationTier::None,
            OptimizationTier::Fast,
            OptimizationTier::Basic,
        ] {
            let mut m = original.clone();
            keel_opt::optimize_with_tier(&mut m, tier).unwrap();
            keel_ir::verify_module(&m).unwrap();
            assert_eq!(loads(&m), 1, "{tier:?}: {m}");
            assert_eq!(call_trace(&m), expected_trace);
            if tier == OptimizationTier::None {
                assert_eq!(m.to_string(), original.to_string());
            }
        }
    }
}

#[test]
fn cleanup_resolves_preexisting_aliases_across_blocks_and_preserves_edge_args() {
    use keel_ir::{EntityRef, SourceLoc, Value};
    let mut m = keel_ir::parse(
        "decl @yes() -> i64
func @f() -> i8 {
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = iconst.i8 1
 store v1, v0
 v2 = load.i8 v0
 v3 = iconst.i8 0
 jump block1
block1:
 brif v3, block2(v2), block3
block2(v4: i8):
 v5 = call @yes()
 return v4
block3:
 trap
}",
    )
    .unwrap();
    let file = m.add_file("stack.c");
    let f = &mut m.functions[0];
    f.replace_all_uses(Value::new(3), Value::new(2));
    let call = f
        .layout_insts()
        .find(|(_, i)| matches!(f.inst_data(*i), InstData::Call { .. }))
        .unwrap()
        .1;
    let loc = SourceLoc {
        file,
        line: 12,
        col: 3,
        scope: None,
    };
    f.set_inst_loc(call, Some(loc));
    keel_ir::verify_module(&m).unwrap();
    let expected = call_trace(&m);
    keel_opt::run_passes(&mut m, &["stack-forward-cfg"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(call_trace(&m), expected);
    assert_eq!(m.functions[0].blocks().count(), 1);
    assert_eq!(m.functions[0].inst_loc(call), Some(loc));
    let before = m.to_string();
    assert!(
        !keel_opt::find_pass("stack-forward-cfg")
            .unwrap()
            .run(&mut m.functions[0])
    );
    assert_eq!(m.to_string(), before);
}

#[test]
fn excluded_memory_cannot_request_conditional_cleanup() {
    for (store, effect, load) in [
        ("store v1, v0", "call @escape(v0)", "load.i8 v0"),
        ("store volatile v1, v0", "", "load.i8 v0"),
        ("store v1, v0", "", "load.i8 volatile v0"),
        (
            "store v1, v0",
            "v5 = atomic_load.i8 relaxed v0",
            "load.i8 v0",
        ),
        ("", "", "load.i8 v0"),
        // One byte does not define the wider conditional's second byte.
        ("store v1, v0", "", "load.i16 v0"),
        (
            "store v1, v0",
            "v5 = intrinsic frame_address()",
            "load.i8 v0",
        ),
    ] {
        let src = conditional_fixture(1, "brif v2, block1, block3")
            .replacen("decl @yes", "decl @escape(ptr)\ndecl @yes", 1)
            .replace(
                "store v1, v0\n v2 = load.i8 v0",
                &format!("{store}\n {effect}\n v2 = {load}"),
            );
        let src = if load == "load.i16 v0" {
            src.replace("stack_slot 1, align 1", "stack_slot 2, align 1")
        } else {
            src
        };
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        assert!(
            !keel_opt::find_pass("stack-forward-cfg")
                .unwrap()
                .run(&mut m.functions[0]),
            "{src}"
        );
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(m.to_string(), before);
    }
}

#[test]
fn unrelated_constants_nonconstant_forwarding_and_later_folds_do_not_trigger() {
    for (store, cond) in [("v0", "v3"), ("v2", "v0"), ("v2", "v4")] {
        let src = format!(
            "func @f(i8) -> i8 {{
 ss0 = stack_slot 1, align 1
block0(v0: i8):
 v1 = stack_addr ss0
 v2 = iconst.i8 1
 store {store}, v1
 v3 = load.i8 v1
 v4 = xor v3, v2
 brif {cond}, block1, block2
block1:
 brif v2, block3, block4
block2:
 return v3
block3:
 return v2
block4:
 trap
}}"
        );
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(
            keel_opt::find_pass("stack-forward-cfg")
                .unwrap()
                .run(&mut m.functions[0])
        );
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(loads(&m), 0);
        assert_eq!(m.functions[0].blocks().count(), 5, "{m}");
        assert_eq!(
            m.functions[0]
                .layout_insts()
                .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Brif { .. }))
                .count(),
            2
        );
    }
}

#[test]
fn preexisting_constant_and_store_removal_alone_do_not_trigger() {
    let mut m = keel_ir::parse(
        "func @f() {
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = iconst.i8 1
 store v1, v0
 brif v1, block1, block2
block1:
 return
block2:
 trap
}",
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    let pass = keel_opt::find_pass("stack-forward-cfg").unwrap();
    assert!(pass.run(&mut m.functions[0]));
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(m.functions[0].blocks().count(), 3);
    let before = m.to_string();
    assert!(!pass.run(&mut m.functions[0]));
    assert_eq!(m.to_string(), before);
}

#[test]
fn debug_home_and_returns_twice_keep_conditional_memory_live() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    for debug_home in [false, true] {
        let src = conditional_fixture(1, "brif v2, block1, block3");
        let src = if debug_home {
            src
        } else {
            src.replacen(
                "decl @yes",
                "decl @checkpoint() returns_twice\ndecl @yes",
                1,
            )
            .replace("v2 = load.i8 v0", "call @checkpoint()\n v2 = load.i8 v0")
        };
        let mut m = keel_ir::parse(&src).unwrap();
        if debug_home {
            m.debug_types.push(DebugTypeData::Base {
                name: "byte".into(),
                encoding: Encoding::Unsigned,
                size: 1,
            });
            m.functions[0].debug_vars.push(DebugVar {
                name: "condition".into(),
                ty: DebugType::new(0),
                slot: StackSlot::new(0),
                indirect: false,
                param: false,
                scope: None,
            });
        }
        keel_ir::verify_module(&m).unwrap();
        let before = m.to_string();
        assert!(
            !keel_opt::find_pass("stack-forward-cfg")
                .unwrap()
                .run(&mut m.functions[0])
        );
        keel_ir::verify_module(&m).unwrap();
        assert_eq!(m.to_string(), before);
    }
}

#[test]
fn triggered_cleanup_preserves_taken_nontermination_and_exception_edges() {
    for (attrs, taken, expected) in [
        ("", "jump block1", "loop"),
        (
            "personality @personality",
            "v3 = invoke @may_throw(), block4, block5\nblock4:\n return v3\nblock5:\n v4, v5 = landingpad cleanup\n trap",
            "invoke",
        ),
    ] {
        let src = format!(
            "decl @may_throw() -> i64\ndecl @personality()
func @f() -> i64 {attrs} {{
 ss0 = stack_slot 1, align 1
block0:
 v0 = stack_addr ss0
 v1 = iconst.i8 1
 store v1, v0
 v2 = load.i8 v0
 brif v2, block1, block2
block1:
 {taken}
block2:
 trap
}}"
        );
        let mut m = keel_ir::parse(&src).unwrap();
        keel_ir::verify_module(&m).unwrap();
        keel_opt::run_passes(&mut m, &["stack-forward-cfg"]).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let f = &m.functions[0];
        assert!(
            !f.layout_insts()
                .any(|(_, i)| matches!(f.inst_data(i), InstData::Brif { .. }))
        );
        if expected == "loop" {
            assert!(f.layout_insts().any(
                |(b, i)| matches!(f.inst_data(i), InstData::Jump { dest } if dest.block == b)
            ));
            assert!(
                !f.layout_insts().any(|(_, i)| matches!(
                    f.inst_data(i),
                    InstData::Return { .. } | InstData::Trap
                ))
            );
        } else {
            assert_eq!(f.personality.as_deref(), Some("personality"));
            assert!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::Invoke { .. }))
            );
            assert!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), InstData::LandingPad { .. }))
            );
        }
    }
}

#[test]
fn cross_block_forwarding_now_exposes_constant_control_flow() {
    let source = conditional_fixture(1, "brif v2, block1, block3").replace(
        "store v1, v0\n v2 = load.i8 v0",
        "store v1, v0\n jump block4\nblock4:\n v2 = load.i8 v0",
    );
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let expected = call_trace(&module);
    keel_opt::run_passes(&mut module, &["stack-forward-cfg"]).unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert_eq!(call_trace(&module), expected);
    assert_eq!(module.functions[0].blocks().count(), 1);
}

#[test]
fn cfg_cleanup_is_explicit_and_defaults_keep_the_existing_schedule() {
    use keel_opt::OptimizationTier;
    for tier in [
        OptimizationTier::None,
        OptimizationTier::Fast,
        OptimizationTier::Basic,
        OptimizationTier::Full,
    ] {
        assert!(!tier.pipeline().contains(&"stack-forward-cfg"));
    }
    let source = conditional_fixture(1, "brif v2, block1, block3");
    let mut ordinary = keel_ir::parse(&source).unwrap();
    keel_opt::run_passes(&mut ordinary, &["stack-forward"]).unwrap();
    keel_ir::verify_module(&ordinary).unwrap();
    assert_eq!(loads(&ordinary), 0);
    assert_eq!(ordinary.functions[0].blocks().count(), 4);
    let expected = call_trace(&ordinary);
    let mut opt_in = keel_ir::parse(&source).unwrap();
    keel_opt::run_passes(&mut opt_in, &["stack-forward-cfg"]).unwrap();
    assert_eq!(opt_in.functions[0].blocks().count(), 1);
    assert_eq!(call_trace(&opt_in), expected);
}

#[test]
fn taken_trap_keeps_prior_call_trace() {
    let source = conditional_fixture(0, "brif v2, block1, block3");
    let original = keel_ir::parse(&source).unwrap();
    let expected = (None, vec!["no".to_string()]);
    assert_eq!(call_trace(&original), expected);
    for pipeline in [
        &["stack-forward-cfg"][..],
        keel_opt::OptimizationTier::Full.pipeline(),
    ] {
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, pipeline).unwrap();
        keel_ir::verify_module(&module).unwrap();
        assert_eq!(call_trace(&module), expected);
    }
}
