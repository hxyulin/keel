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
        let m = run(&format!("func @f(i32, i64) -> i64 {{
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
}}"));
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
        let m = run(&format!("func @f(i32, i64) -> i64 {{
 ss0 = stack_slot 8, align 8
block0(v0: i32, v1: i64):
 v2 = stack_addr ss0
 {body}
 brif v0, block1, block2
block2:
 return v3
}}"));
        assert_eq!(loads(&m), expected, "{m}");
    }
}

#[test]
fn escaped_snapshot_and_stack_restore_do_not_cross_blocks() {
    for effect in ["call @escape(v0)", "intrinsic stack_restore(v0)"] {
        let m = run(&format!("decl @escape(ptr)
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
}}"));
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
                InstData::Iconst { imm, .. } => { constants.insert(f.first_result(i), imm); }
                _ => {}
            }
        }
        loop {
            let before = addresses.len();
            for &i in &insts {
                if let InstData::Binary { op: BinaryOp::Ptradd, args } = *f.inst_data(i)
                    && let Some(&(slot, offset)) = addresses.get(&f.resolve(args[0]))
                    && let Some(&n) = constants.get(&f.resolve(args[1]))
                {
                    addresses.insert(f.first_result(i), (slot, offset + n));
                }
            }
            if addresses.len() == before { break; }
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
                        InstData::Load { ty: keel_ir::Type::I64, .. } => integer += 1,
                        InstData::Load { ty: keel_ir::Type::Ptr, .. } => pointer += 1,
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
