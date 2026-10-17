//! Source checkpoint tests. These require the separate module-registration patch.
use super::*;
use keel_ir::{DebugType, DebugVar, EntityRef, SourceLoc};

fn fixture(offset: u32, flags: &str, reversed: bool) -> keel_ir::Module {
    let operands = if reversed { "v5, v4" } else { "v4, v5" };
    let mut module = keel_ir::parse(&format!(
        "func @f(i64) -> i64 {{
ss0 = stack_slot 24, align 8
block0(v0: i64):
    v1 = stack_addr ss0
    v2 = iconst.i64 {offset}
    v3 = ptradd v1, v2
    store v0, v3
    v4 = load.i64 {flags} v3
    v5 = iconst.i64 255
    v6 = and {operands}
    return v6
}}"
    )).unwrap();
    let file = module.add_file("load-narrow.c");
    let func = &mut module.functions[0];
    for (n, inst) in func.layout_insts().map(|(_, i)| i).collect::<Vec<_>>().into_iter().enumerate() {
        func.set_inst_loc(inst, Some(SourceLoc { file, line: n as u32 + 1, col: 1, scope: None }));
    }
    keel_ir::verify_module(&module).unwrap();
    module
}

fn unchanged(module: &mut keel_ir::Module) {
    let before = module.to_string();
    assert!(!LoadNarrow.run_for_target(&mut module.functions[0], VectorTarget::X86Sse2));
    assert_eq!(module.to_string(), before);
}

#[test]
fn same_address_offset_flags_locations_order_and_existing_uext() {
    for target in [VectorTarget::X86Sse2, VectorTarget::Aarch64Neon] {
        for offset in [0, 1, 8, 16] {
            for reversed in [false, true] {
                let mut module = fixture(offset, "align 1 notrap", reversed);
                let func = &mut module.functions[0];
                let old_layout: Vec<_> = func.layout_insts().map(|(_, i)| i).collect();
                let load = old_layout.iter().copied().find(|&i| matches!(func.inst_data(i), InstData::Load { .. })).unwrap();
                let mask = old_layout.iter().copied().find(|&i| matches!(func.inst_data(i), InstData::Binary { op: BinaryOp::And, .. })).unwrap();
                let InstData::Load { flags, addr, .. } = *func.inst_data(load) else { unreachable!() };
                let load_loc = func.inst_loc(load);
                let mask_loc = func.inst_loc(mask);
                let load_pos = old_layout.iter().position(|&i| i == load).unwrap();
                assert!(LoadNarrow.run_for_target(func, target));
                let layout: Vec<_> = func.layout_insts().map(|(_, i)| i).collect();
                assert_eq!(layout.len(), old_layout.len());
                let narrow = layout[load_pos];
                assert_eq!(*func.inst_data(narrow), InstData::Load { flags, addr, ty: Type::I8 });
                assert_eq!(func.inst_loc(narrow), load_loc);
                assert_eq!(func.inst_loc(mask), mask_loc);
                assert_eq!(*func.inst_data(mask), InstData::Convert { op: ConvertOp::Uext, ty: Type::I64, arg: func.first_result(narrow) });
                for (n, &inst) in old_layout.iter().enumerate() {
                    if n != load_pos { assert_eq!(layout[n], inst); }
                }
                assert!(!LoadNarrow.run_for_target(func, target));
                keel_ir::verify_module(&module).unwrap();
            }
        }
    }
    let mut aligned = fixture(8, "align 8 notrap", false);
    assert!(LoadNarrow.run_for_target(&mut aligned.functions[0], VectorTarget::X86Sse2));
    assert!(aligned.to_string().contains("load.i8 align 8 notrap"));
}

#[test]
fn missing_fault_contract_volatile_and_unknown_target_are_unchanged() {
    for flags in ["", "align 8", "volatile", "volatile notrap"] {
        unchanged(&mut fixture(0, flags, false));
    }
    let mut module = fixture(0, "notrap", false);
    let before = module.to_string();
    assert!(!LoadNarrow.run(&mut module.functions[0]));
    assert!(!LoadNarrow.run_for_target(&mut module.functions[0], VectorTarget::Portable));
    assert_eq!(module.to_string(), before);
}

#[test]
fn full_original_access_must_fit_the_known_allocation() {
    // A byte would fit at 17, but the original eight-byte access would not.
    for offset in [17, 24, 25, u32::MAX] {
        unchanged(&mut fixture(offset, "notrap", false));
    }
}

#[test]
fn multiple_uses_and_wrong_mask_or_width_are_unchanged() {
    let original = fixture(0, "notrap", false).to_string();
    for source in [
        original.replace("return v6", "v7 = iadd v4, v6\n    return v7"),
        original.replace("iconst.i64 255", "iconst.i64 511"),
        original.replace("load.i64", "load.i32").replace("v6 = and", "v8 = uext.i64 v4\n    v6 = and").replace("and v4, v5", "and v8, v5"),
    ] {
        let mut module = keel_ir::parse(&source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        unchanged(&mut module);
    }
}

#[test]
fn unknown_pointers_atomic_access_and_escaped_stack_are_unchanged() {
    for source in [
        "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 v0\n v2 = iconst.i64 255\n v3 = and v1, v2\n return v3\n}",
        "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 notrap v0\n v2 = iconst.i64 255\n v3 = and v1, v2\n return v3\n}",
        "func @f() -> i64 {\n ss0 = stack_slot 8, align 8\nblock0:\n v0 = stack_addr ss0\n v1 = atomic_load.i64 seq_cst v0\n v2 = iconst.i64 255\n v3 = and v1, v2\n return v3\n}",
    ] {
        let mut module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        unchanged(&mut module);
    }
    let source = fixture(0, "notrap", false).to_string().replace("return v6", "v7 = ptrtoint.i64 v1\n    return v6");
    let mut module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    unchanged(&mut module);
}

#[test]
fn debugger_homes_are_excluded() {
    let mut module = fixture(0, "notrap", false);
    let func = &mut module.functions[0];
    let slot = func.stack_slots().next().unwrap().0;
    func.debug_vars.push(DebugVar { name: "home".into(), ty: DebugType::new(0), slot, indirect: false, param: false, scope: None });
    unchanged(&mut module);
}

#[test]
fn atomic_lifetime_volatile_and_unknown_memory_elsewhere_reject_the_function() {
    let source = fixture(0, "notrap", false).to_string();
    for extra in [
        "v7 = atomic_load.i64 seq_cst v3",
        "v7 = load.i64 volatile v3",
        "v7 = intrinsic stack_save()",
        "v7 = ptrtoint.i64 v1",
    ] {
        let mut module = keel_ir::parse(&source.replace("return v6", &format!("{extra}\n    return v6"))).unwrap();
        keel_ir::verify_module(&module).unwrap();
        unchanged(&mut module);
    }
}

#[test]
fn named_registration_is_opt_in() {
    assert!(crate::find_pass("load-narrow").is_some());
    assert!(!crate::DEFAULT_PIPELINE.contains(&"load-narrow"));
}

#[test]
fn resolved_constant_alias_and_intervening_store_keep_the_original_read_position() {
    let mut module = fixture(8, "notrap", false);
    let func = &mut module.functions[0];
    let mask = func.layout_insts().find_map(|(_, i)| {
        matches!(func.inst_data(i), InstData::Binary { op: BinaryOp::And, .. }).then_some(i)
    }).unwrap();
    let InstData::Binary { args, .. } = *func.inst_data(mask) else { unreachable!() };
    let ValueDef::Result(load, _) = func.value_def(args[0]) else { unreachable!() };
    let InstData::Load { addr, flags, .. } = *func.inst_data(load) else { unreachable!() };
    let alias = func.insert_inst(InsertPoint::Before(mask), InstData::Iconst { ty: Type::I64, imm: 255 });
    func.replace_all_uses(args[1], func.first_result(alias));
    let stored = func.block_params(func.entry_block().unwrap())[0];
    let store = func.insert_inst(InsertPoint::Before(mask), InstData::Store { addr, flags, value: stored });
    assert!(LoadNarrow.run_for_target(func, VectorTarget::X86Sse2));
    let layout: Vec<_> = func.layout_insts().map(|(_, i)| i).collect();
    let narrow_pos = layout.iter().position(|&i| matches!(func.inst_data(i), InstData::Load { ty: Type::I8, .. })).unwrap();
    assert!(narrow_pos < layout.iter().position(|&i| i == store).unwrap());
    assert!(layout.iter().position(|&i| i == store).unwrap() < layout.iter().position(|&i| i == mask).unwrap());
    keel_ir::verify_module(&module).unwrap();
}

#[test]
fn real_bridge_shape_is_deliberately_unsupported() {
    // Retained optimized IR: compiler bridge capture-001, both targets. The
    // caller's allocation bounds are not present in this function's contract.
    let mut module = keel_ir::parse("func @bridge_narrow_load(i64, i64, ptr, i64) -> i64 cc(c) {
block0(v0: i64, v1: i64, v2: ptr, v3: i64):
    jump block1(v0, v1)
block1(v4: i64, v7: i64):
    v5 = iconst.i64 0
    v6 = icmp.ne v4, v5
    brif v6, block2, block3
block2:
    v8 = iconst.i64 8
    v9 = ushr v7, v8
    v11 = and v3, v9
    v21 = iconst.i64 3
    v14 = shl v11, v21
    v15 = ptradd v2, v14
    v16 = load.i64 v15
    v17 = iconst.i64 255
    v18 = and v16, v17
    v19 = iconst.i64 1
    v20 = isub v4, v19
    jump block1(v20, v18)
block3:
    return v7
}").unwrap();
    keel_ir::verify_module(&module).unwrap();
    let before = module.to_string();
    for target in [VectorTarget::X86Sse2, VectorTarget::Aarch64Neon] {
        assert!(!LoadNarrow.run_for_target(&mut module.functions[0], target));
        assert_eq!(module.to_string(), before);
    }
}

#[test]
fn direct_in_memory_aliases_never_resurrect_dead_load_or_mask() {
    for alias_mask in [false, true] {
        let mut module = fixture(0, "notrap", false);
        let func = &mut module.functions[0];
        let inst = func.layout_insts().find_map(|(_, i)| {
            let matches = if alias_mask {
                matches!(func.inst_data(i), InstData::Binary { op: BinaryOp::And, .. })
            } else {
                matches!(func.inst_data(i), InstData::Load { .. })
            };
            matches.then_some(i)
        }).unwrap();
        let original = func.first_result(inst);
        let replacement = func.block_params(func.entry_block().unwrap())[0];
        func.replace_all_uses(original, replacement);
        unchanged(&mut module);
        assert_eq!(module.functions[0].resolve(original), replacement);
    }
}
