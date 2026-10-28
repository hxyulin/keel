//! Instruction-result scope and conservative invalidation.
use keel_ir::{InsertPoint, InstData, MemoryEffects};

fn parsed(source: &str) -> keel_ir::Module {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}
const SOURCE: &str = "func @f(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.ptr align 8 v0 facts(align 16, nonnull, readable 32, readable_or_null 64)\n v2 = load.i64 v1 facts(range(0x0, 0x10))\n return v2\n}\n";

#[test]
fn result_facts_roundtrip_and_do_not_change_access_or_function_contracts() {
    let module = parsed(SOURCE);
    let function = &module.functions[0];
    let loads: Vec<_> = function
        .layout_insts()
        .filter_map(|(_, i)| matches!(function.inst_data(i), InstData::Load { .. }).then_some(i))
        .collect();
    assert_eq!(function.inst_facts(loads[0]).unwrap().align, 16);
    let InstData::Load { flags, .. } = function.inst_data(loads[0]) else {
        unreachable!()
    };
    assert_eq!(flags.align, 8);
    assert!(!flags.notrap);
    assert!(function.inst_data(loads[0]).is_pinned());
    assert_eq!(
        function.inst_data(loads[0]).memory_effects(),
        MemoryEffects::Read
    );
    assert!(function.attrs.contract.is_unknown());
    assert_eq!(parsed(&module.to_string()).to_string(), module.to_string());
}

#[test]
fn rewriting_moving_and_cloning_instructions_forget_facts() {
    let mut module = parsed(SOURCE);
    let function = &mut module.functions[0];
    let (block, load) = function.layout_insts().next().unwrap();
    let original = function.inst_facts(load).copied().unwrap();
    assert_eq!(function.clone().inst_facts(load), Some(&original));
    let data = function.inst_data(load).clone();
    let inserted = function.insert_inst(InsertPoint::Before(load), data.clone());
    assert!(function.inst_facts(inserted).is_none());
    function.remove_inst(inserted);
    *function.inst_data_mut(load) = data;
    assert!(function.inst_facts(load).is_none());
    function.set_inst_facts(load, original).unwrap();
    let next = function.block_insts(block)[1];
    function.move_inst(load, InsertPoint::Before(next));
    assert!(function.inst_facts(load).is_none());
    function.set_inst_facts(load, original).unwrap();
    function.clear_inst_facts(load);
    assert!(function.inst_facts(load).is_none());
    keel_ir::verify_module(&module).unwrap();
}

#[test]
fn bulk_moves_and_removed_blocks_do_not_retain_pointer_extents() {
    let mut module = parsed(SOURCE);
    let function = &mut module.functions[0];
    let entry = function.entry_block().unwrap();
    let load = function.block_insts(entry)[0];
    let original = *function.inst_facts(load).unwrap();
    let exit = function.create_block();
    function.move_insts_to_end(entry, 0, exit);
    assert!(function.inst_facts(load).is_none());
    function.append_inst(
        entry,
        InstData::Jump {
            dest: keel_ir::BlockCall {
                block: exit,
                args: vec![],
            },
        },
    );
    keel_ir::verify_module(&module).unwrap();
    let function = &mut module.functions[0];
    function.set_inst_facts(load, original).unwrap();
    let jump = function.block_insts(entry)[0];
    function.remove_inst(jump);
    let zero = function.append_inst(
        entry,
        InstData::Iconst {
            ty: keel_ir::Type::I64,
            imm: 0,
        },
    );
    let value = function.inst_results(zero)[0];
    function.append_inst(entry, InstData::Return { args: vec![value] });
    function.remove_block(exit);
    assert!(function.inst_facts(load).is_none());
    keel_ir::verify_module(&module).unwrap();
}

#[test]
fn result_facts_do_not_survive_call_target_changes_or_bulk_removal() {
    let mut module = parsed(
        "decl @g() -> i64\nfunc @f() -> i64 {\nblock0:\n v0 = call @g() contract(nofree) facts(range(0, 8))\n return v0\nblock1:\n v1 = iconst.i64 2 facts(range(0, 4))\n return v1\n}\n",
    );
    let function = &mut module.functions[0];
    let call = function.layout_insts().next().unwrap().1;
    let InstData::Call { func, .. } = *function.inst_data(call) else {
        unreachable!()
    };
    assert!(function.inst_facts(call).is_some());
    function.callee_mut(func).name = "other".into();
    assert!(function.inst_facts(call).is_none());
    let block = function.blocks().nth(1).unwrap();
    let inst = function.block_insts(block)[0];
    function.remove_blocks(&[block]);
    assert!(function.inst_facts(inst).is_none());
}

#[test]
fn malformed_result_facts_are_rejected() {
    for instruction in [
        "v1 = load.i64 v0 facts(nonnull)",
        "v1 = load.i8 v0 facts(range(0, 0x100))",
        "v1 = load.i64 v0 facts(range(8, 8))",
        "v1 = load.ptr v0 facts(range(0, 8))",
        "v1 = load.ptr v0 facts(align 3)",
        "v1 = load.ptr v0 facts(readable -1)",
        "v1 = load.ptr v0 facts(nonnull, nonnull)",
        "store v0, v0 facts(nonnull)",
        "v1 = load.ptr v0 facts(nofree)",
    ] {
        let source = format!("func @f(ptr) {{\nblock0(v0: ptr):\n {instruction}\n return\n}}\n");
        let invalid = match keel_ir::parse(&source) {
            Err(_) => true,
            Ok(m) => keel_ir::verify_module(&m).is_err(),
        };
        assert!(invalid, "{source}");
    }
}
