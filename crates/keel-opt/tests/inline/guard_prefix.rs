use keel_ir::{Module, parse, verify_module};
use keel_ir_interp::{Interpreter, Val};

const SOURCE: &str = "func @maybe(ptr, i8) local {
 ss0 = stack_slot 8, align 8
block0(v0: ptr, v1: i8):
 v2 = stack_addr ss0
 v3 = iconst.i64 17
 store v3, v2
 v4 = iconst.i8 0
 v5 = icmp.ne v1, v4
 brif v5, block1, block2
block1:
 v6 = load.i64 v0
 v7 = iconst.i64 1
 v8 = iadd v6, v7
 store v8, v0
 jump block2
block2:
 return
}
func @run(i8) -> i64 {
 ss0 = stack_slot 2048, align 8
block0(v0: i8):
 v1 = stack_addr ss0
 v2 = iconst.i64 0
 store v2, v1
 call @maybe(v1, v0)
 v3 = load.i64 v1
 return v3
}";

fn transform(source: &str) -> (Module, Module) {
    let original = parse(source).unwrap();
    verify_module(&original).unwrap();
    let mut module = original.clone();
    keel_opt::run_passes(&mut module, &["inline-guards"]).unwrap();
    verify_module(&module).unwrap();
    (original, module)
}

#[test]
fn scalar_guard_keeps_slow_effects_without_cloning_stack_storage() {
    for source in [
        SOURCE.to_owned(),
        SOURCE.replace("brif v5, block1, block2", "brif v5, block2, block1"),
    ] {
        let (original, mut module) = transform(&source);
        assert_eq!(
            original.functions[0].to_string(),
            module.functions[0].to_string()
        );
        let caller = &module.functions[1];
        assert_eq!(
            caller.num_stack_slots(),
            original.functions[1].num_stack_slots()
        );
        assert_eq!(caller.blocks().count(), 3);
        assert!(caller.to_string().contains("call @maybe"));
        for input in 0..256 {
            let args = [Val::int(keel_ir::Type::I8, input)];
            assert_eq!(
                Interpreter::new(&original).call("run", &args),
                Interpreter::new(&module).call("run", &args),
                "input={input}"
            );
        }
        let once = module.to_string();
        keel_opt::run_passes(&mut module, &["inline-guards"]).unwrap();
        assert_eq!(
            once,
            module.to_string(),
            "retained slow calls must not acquire duplicate guards"
        );
    }
}

#[test]
fn prefix_reads_external_writes_and_volatile_private_writes_keep_the_call() {
    for source in [
        SOURCE.replace("store v3, v2", "store v3, v0"),
        SOURCE.replace("store v3, v2", "store volatile v3, v2"),
        SOURCE.replace("v4 = iconst.i8 0", "v9 = load.i64 v0\n v4 = iconst.i8 0"),
        SOURCE.replace(
            "v4 = iconst.i8 0",
            "v9 = load.i64 notrap v0\n v4 = iconst.i8 0",
        ),
        SOURCE.replace(
            "v4 = iconst.i8 0",
            "v9 = iconst.i64 0\n v10 = udiv v3, v9\n v4 = iconst.i8 0",
        ),
    ] {
        let (original, module) = transform(&source);
        assert_eq!(original.to_string(), module.to_string(), "{source}");
    }
}

#[test]
fn aliased_stack_addresses_do_not_prove_external_stores_private() {
    for source in [
        SOURCE.to_owned(),
        SOURCE.replace(
            "store v3, v2",
            "v9 = iconst.i64 0\n v10 = ptradd v2, v9\n store v3, v10",
        ),
    ] {
        let mut module = parse(&source).unwrap();
        let helper = &mut module.functions[0];
        let entry = helper.entry_block().unwrap();
        let pointer = helper.block_params(entry)[0];
        let address = helper
            .layout_insts()
            .find_map(|(_, i)| {
                matches!(
                    helper.inst_data(i),
                    keel_ir::InstData::Binary {
                        op: keel_ir::BinaryOp::Ptradd,
                        ..
                    }
                )
                .then(|| helper.first_result(i))
            })
            .unwrap_or_else(|| helper.first_result(helper.block_insts(entry)[0]));
        helper.replace_all_uses(address, pointer);
        verify_module(&module).unwrap();
        keel_opt::run_passes(&mut module, &["inline-guards"]).unwrap();
        verify_module(&module).unwrap();
        assert_eq!(module.functions[1].blocks().count(), 1);
    }
}

#[test]
fn explicit_aggregate_transport_keeps_its_call_boundary() {
    for purpose in ["byval {i64}", "sret {i64}"] {
        let source = SOURCE.replace(
            "func @maybe(ptr, i8)",
            &format!("func @maybe(ptr {purpose}, i8)"),
        );
        let (original, module) = transform(&source);
        assert_eq!(original.to_string(), module.to_string(), "{purpose}");
    }
}

#[test]
fn binding_and_function_policies_remain_boundaries() {
    for attributes in [
        "",
        "weak no_semantic_interposition",
        "noinline local",
        "optnone local",
        "alwaysinline local",
        "optimize(\"no-inline\") local",
    ] {
        let source = SOURCE.replace(
            "func @maybe(ptr, i8) local",
            &format!("func @maybe(ptr, i8) {attributes}"),
        );
        let (original, module) = transform(&source);
        assert_eq!(original.to_string(), module.to_string(), "{attributes}");
    }
    for attributes in ["optnone", "optimize(\"no-inline\")"] {
        let source = SOURCE.replace(
            "func @run(i8) -> i64",
            &format!("func @run(i8) -> i64 {attributes}"),
        );
        let (original, module) = transform(&source);
        assert_eq!(
            original.to_string(),
            module.to_string(),
            "caller {attributes}"
        );
    }
}

#[test]
fn guard_growth_is_bounded_and_call_argument_evaluation_survives() {
    let calls = " call @maybe(v1, v0)\n".repeat(600);
    let source = SOURCE.replace(" call @maybe(v1, v0)\n", &calls);
    let (original, module) = transform(&source);
    let caller = &module.functions[1];
    assert!(caller.layout_insts().count() <= original.functions[1].layout_insts().count() + 1024);
    assert_eq!(caller.to_string().matches("call @maybe").count(), 600);
    assert_eq!(caller.num_stack_slots(), 1);
    let source = SOURCE.replace(
        "call @maybe(v1, v0)",
        "v4 = load.i8 v1\n call @maybe(v1, v4)",
    );
    let (_, module) = transform(&source);
    assert!(
        module.functions[1].to_string().contains("load.i8"),
        "argument reads execute before the guard"
    );
}
