use keel_ir::{InstData, Module, parse, verify_module};
use keel_ir_interp::{Interpreter, Outcome, Val};
use keel_opt::VectorTarget;

const SOURCE: &str = r#"
data @ops = local const align 8 { ptr @a, ptr @b }
func @a(i64) -> i64 local {
block0(v0: i64):
 v1 = iconst.i64 1
 v2 = iadd v0, v1
 return v2
}
func @b(i64) -> i64 local {
block0(v0: i64):
 v1 = iconst.i64 3
 v2 = imul v0, v1
 return v2
}
func @choose(i64) -> i64 {
block0(v0: i64):
 v1 = iconst.i64 1
 v2 = and v0, v1
 v3 = iconst.i64 3
 v4 = shl v2, v3
 v5 = symbol_addr @ops
 v6 = ptradd v5, v4
 v7 = load.ptr align 8 v6
 v8 = call_indirect v7(v0) : (i64) -> i64
 v9 = xor v8, v1
 return v9
}
"#;
fn apply(m: &mut Module, target: VectorTarget) {
    verify_module(m).unwrap();
    keel_opt::run_passes_for_target(m, &["devirtualize-const-table"], target).unwrap();
    verify_module(m).unwrap();
}
#[test]
fn bounded_immutable_table_calls_are_direct_idempotent_and_preserve_results() {
    for target in [VectorTarget::Aarch64Neon, VectorTarget::X86Sse2] {
        let mut m = parse(SOURCE).unwrap();
        apply(&mut m, target);
        let caller = m.functions.iter().find(|f| f.name == "choose").unwrap();
        assert!(!caller.to_string().contains("call_indirect"), "{m}");
        assert_eq!(
            caller
                .layout_insts()
                .filter(|(_, i)| matches!(caller.inst_data(*i), InstData::Call { .. }))
                .count(),
            2
        );
        let once = m.to_string();
        apply(&mut m, target);
        assert_eq!(m.to_string(), once);
        keel_opt::run_passes_for_target(&mut m, &["dce", "simplify-cfg"], target).unwrap();
        assert!(
            !m.functions
                .iter()
                .find(|f| f.name == "choose")
                .unwrap()
                .to_string()
                .contains("load.ptr")
        );
        for x in [0, 1, 2, 3, 127, u64::MAX, 1 << 63, (1 << 63) - 1] {
            let expected = (if x & 1 == 0 {
                x.wrapping_add(1)
            } else {
                x.wrapping_mul(3)
            }) ^ 1;
            assert_eq!(
                Interpreter::new(&m).call("choose", &[Val::I64(x)]),
                Outcome::Return(vec![Val::I64(expected)])
            );
        }
    }
}
#[test]
fn mutable_interposable_unbounded_volatile_and_mismatched_tables_are_unchanged() {
    for source in [
        SOURCE.replace("local const align", "local align"),
        SOURCE.replace("local const align", "const align"),
        SOURCE.replace("v2 = and", "v2 = iadd"),
        SOURCE.replace("v1 = iconst.i64 1", "v1 = iconst.i64 3"),
        SOURCE.replace("load.ptr align 8", "load.ptr volatile align 8"),
        SOURCE.replace("const align 8", "const align 1"),
        SOURCE.replace("func @b(i64) -> i64 local", "func @b(i64) -> i64"),
    ] {
        let mut m = parse(&source).unwrap();
        let before = m.to_string();
        apply(&mut m, VectorTarget::Aarch64Neon);
        assert_eq!(m.to_string(), before);
    }
    let mut portable = parse(SOURCE).unwrap();
    let before = portable.to_string();
    apply(&mut portable, VectorTarget::Portable);
    assert_eq!(portable.to_string(), before);
}

#[test]
fn call_contracts_function_policies_and_single_target_boundaries_are_preserved() {
    let mut m = parse(SOURCE).unwrap();
    let caller = m.functions.iter_mut().find(|f| f.name == "choose").unwrap();
    let indirect = caller
        .layout_insts()
        .find(|(_, i)| matches!(caller.inst_data(*i), InstData::CallIndirect { .. }))
        .unwrap()
        .1;
    let contract = keel_ir::CallContract {
        nofree: true,
        preserves_access: true,
        ..Default::default()
    };
    caller
        .set_call_contract(indirect, contract.clone())
        .unwrap();
    apply(&mut m, VectorTarget::Aarch64Neon);
    let caller = m.functions.iter().find(|f| f.name == "choose").unwrap();
    for (_, i) in caller
        .layout_insts()
        .filter(|(_, i)| matches!(caller.inst_data(*i), InstData::Call { .. }))
    {
        assert_eq!(caller.call_contract(i), Some(&contract));
    }
    let mut single = parse(&SOURCE.replace("v1 = iconst.i64 1", "v1 = iconst.i64 0")).unwrap();
    apply(&mut single, VectorTarget::Aarch64Neon);
    assert!(
        !single
            .functions
            .iter()
            .find(|f| f.name == "choose")
            .unwrap()
            .to_string()
            .contains("call_indirect")
    );
    for source in [
        SOURCE.replace(
            "func @choose(i64) -> i64",
            "func @choose(i64) -> i64 optnone",
        ),
        SOURCE.replace("-> i64 local", "-> i64 noinline local"),
    ] {
        let mut m = parse(&source).unwrap();
        let before = m.to_string();
        apply(&mut m, VectorTarget::Aarch64Neon);
        assert_eq!(m.to_string(), before);
    }
    let mut wasm_layout = parse(SOURCE).unwrap();
    let before = wasm_layout.to_string();
    keel_opt::Pipeline::named(&["devirtualize-const-table"])
        .unwrap()
        .context(keel_opt::OptimizationContext {
            layout: Some(keel_ir::DataLayout::WASM32),
            ..VectorTarget::Aarch64Neon.into()
        })
        .run(&mut wasm_layout)
        .unwrap();
    assert_eq!(wasm_layout.to_string(), before);
}

#[test]
fn narrow_mask_extensions_keep_the_same_bounded_table_index() {
    for extension in ["uext", "sext"] {
        let source = SOURCE.replace("v2 = and v0, v1", &format!(
            "v20 = trunc.i32 v0\n v21 = iconst.i32 1\n v22 = and v20, v21\n v2 = {extension}.i64 v22"));
        let mut m = parse(&source).unwrap();
        apply(&mut m, VectorTarget::Aarch64Neon);
        assert!(
            !m.functions
                .iter()
                .find(|f| f.name == "choose")
                .unwrap()
                .to_string()
                .contains("call_indirect")
        );
        for x in [0, 1, 2, 3, u64::MAX, 1 << 63, 1 << 32] {
            let expected = (if x & 1 == 0 {
                x.wrapping_add(1)
            } else {
                x.wrapping_mul(3)
            }) ^ 1;
            assert_eq!(
                Interpreter::new(&m).call("choose", &[Val::I64(x)]),
                Outcome::Return(vec![Val::I64(expected)])
            );
        }
    }
}
