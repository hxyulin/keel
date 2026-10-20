mod support;
use keel_ir::{BlockCall, Function, InsertPoint, InstData, Signature, Type};
use support::execute;

#[test]
fn repeated_edge_arguments_stay_live_and_unused_parameters_die() {
    let mut func = Function::new("fanout", Signature::new(&[Type::I32], &[Type::I64]));
    let entry = func.create_block();
    let join = func.create_block();
    let input = func.append_block_param(entry, Type::I32);
    let one = func.ins(InsertPoint::End(entry)).iconst(Type::I64, 1);
    let dead = func.ins(InsertPoint::End(entry)).iconst(Type::I64, 99);
    let a = func.append_block_param(join, Type::I64);
    let b = func.append_block_param(join, Type::I64);
    func.append_block_param(join, Type::I64);
    let dest = BlockCall {
        block: join,
        args: vec![one, one, dead],
    };
    func.append_inst(
        entry,
        InstData::Switch {
            arg: input,
            dests: vec![dest; 257],
            cases: (0..256).collect(),
        },
    );
    let sum = func.ins(InsertPoint::End(join)).iadd(a, b);
    func.ins(InsertPoint::End(join)).return_(&[sum]);
    keel_ir::verify(&func).unwrap();
    assert!(keel_opt::find_pass("dce").unwrap().run(&mut func));
    keel_ir::verify(&func).unwrap();
    assert_eq!(func.block_params(join).len(), 2);
    assert!(
        func.layout_insts()
            .flat_map(|(_, i)| func.inst_data(i).branch_dests())
            .all(|d| d.args == [one, one])
    );
    for input in [0, 1, 255, 256, u32::MAX as u64] {
        assert_eq!(execute(&func, &[input]), 2);
    }
    assert!(!keel_opt::find_pass("dce").unwrap().run(&mut func));
}

#[test]
fn unused_traps_calls_loads_and_stores_are_preserved() {
    let source = "decl @effect(i64) -> i64
func @f(ptr, i64) -> i64 {
block0(v0: ptr, v1: i64):
    v2 = load.i64 v0
    store v1, v0
    v3 = call @effect(v1)
    v4 = iconst.i64 0
    v5 = udiv v1, v4
    v6 = iadd v1, v1
    return v1
}";
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let before: Vec<_> = module.functions[0]
        .layout_insts()
        .filter_map(|(_, i)| {
            let d = module.functions[0].inst_data(i);
            d.is_pinned().then(|| d.clone())
        })
        .collect();
    keel_opt::run_passes(&mut module, &["dce"]).unwrap();
    let after: Vec<_> = module.functions[0]
        .layout_insts()
        .filter_map(|(_, i)| {
            let d = module.functions[0].inst_data(i);
            d.is_pinned().then(|| d.clone())
        })
        .collect();
    assert_eq!(before, after);
    assert!(!module.to_string().contains("iadd"));
}

#[test]
fn only_unused_ordinary_nontrapping_reads_can_die() {
    for ty in [
        "i8", "i16", "i32", "i64", "i128", "f32", "f64", "ptr", "i32x4",
    ] {
        for (flags, retained) in [
            ("", true),
            ("align 8", true),
            ("volatile notrap", true),
            ("notrap", false),
        ] {
            let source = format!(
                "func @f(ptr, {ty}) -> {ty} {{
block0(v0: ptr, v1: {ty}):
 v2 = load.{ty} {flags} v0
 return v1
}}"
            );
            let original = keel_ir::parse(&source).unwrap();
            for tier in [
                keel_opt::OptimizationTier::Fast,
                keel_opt::OptimizationTier::Basic,
                keel_opt::OptimizationTier::Full,
            ] {
                let mut m = original.clone();
                keel_opt::optimize_with_tier(&mut m, tier).unwrap();
                keel_ir::verify_module(&m).unwrap();
                assert_eq!(
                    m.to_string().contains("load."),
                    retained,
                    "{tier:?}: {source}"
                );
            }
            // The access stays pinned for memory ordering, irrespective of
            // whether DCE is allowed to discard an unused result.
            let mut live = keel_ir::parse(&source.replace("return v1", "return v2")).unwrap();
            keel_opt::run_passes(&mut live, &["dce", "gcm"]).unwrap();
            assert!(
                live.functions[0]
                    .layout_insts()
                    .any(|(_, i)| matches!(live.functions[0].inst_data(i), InstData::Load { .. }))
            );
            assert!(
                !keel_opt::find_pass("dce")
                    .unwrap()
                    .run(&mut live.functions[0])
            );
        }
    }
}

#[test]
fn dead_loop_cycle_dies_in_every_pass_combination() {
    let source = "func @f(i64, i64) -> i64 {
block0(v0: i64, v1: i64):
    v2 = iconst.i64 0
    jump block1(v2, v1)
block1(v3: i64, v4: i64):
    v5 = iconst.i64 1
    v6 = iadd v3, v5
    v7 = imul v4, v4
    v8 = icmp.ult v6, v0
    brif v8, block1(v6, v7), block2(v6)
block2(v9: i64):
    return v9
}";
    let original = keel_ir::parse(source).unwrap();
    for subset in 0..32 {
        let other = ["const-fold", "sccp", "simplify-cfg", "gvn", "gcm"];
        let mut names = vec!["dce"];
        names.extend(
            other
                .iter()
                .enumerate()
                .filter(|(i, _)| subset & (1 << i) != 0)
                .map(|(_, &name)| name),
        );
        let mut module = original.clone();
        keel_opt::run_passes(&mut module, &names).unwrap();
        keel_opt::run_passes(&mut module, &names).unwrap();
        assert!(!module.to_string().contains("imul"));
        assert_eq!(
            module.functions[0]
                .block_params(module.functions[0].entry_block().unwrap())
                .len(),
            2,
            "entry ABI parameters remain"
        );
        for n in 0..32 {
            assert_eq!(
                execute(&module.functions[0], &[n, 17]),
                execute(&original.functions[0], &[n, 17]),
                "passes {names:?}, input {n}"
            );
        }
    }
}
