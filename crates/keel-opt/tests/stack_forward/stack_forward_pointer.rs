//! Exact native64 integer storage may be read as an integral pointer. These
//! regressions leave the load in its original block or replace it there.
use keel_ir::{ConvertOp, InstData, Module, Type, ValueDef};
use keel_opt::VectorTarget;

use super::loads;

fn optimize(source: &str, target: VectorTarget, pass: &str) -> Module {
    let mut module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_opt::run_passes_for_target(&mut module, &[pass], target).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

fn local_source(bits: i64, effect: &str) -> String {
    format!(
        "decl @escape(ptr)
func @f() -> ptr {{
 ss0 = stack_slot 16, align 8
block0:
 v0 = stack_addr ss0
 v1 = iconst.i64 {bits}
 store v1, v0
 {effect}
 v2 = load.ptr v0
 return v2
}}"
    )
}

#[test]
fn exact_pointer_forwarding_preserves_null_and_all_address_bits() {
    for target in [VectorTarget::X86Sse2, VectorTarget::Aarch64Neon] {
        for pass in ["stack-forward", "stack-forward-cfg"] {
            for bits in [0, i64::MIN, -1] {
                let module = optimize(&local_source(bits, ""), target, pass);
                assert_eq!(loads(&module), 0, "{target:?} {bits}: {module}");
                let f = &module.functions[0];
                let arg = f
                    .layout_insts()
                    .find_map(|(_, i)| match *f.inst_data(i) {
                        InstData::Convert {
                            op: ConvertOp::IntToPtr,
                            ty: Type::Ptr,
                            arg,
                        } => Some(arg),
                        _ => None,
                    })
                    .expect("forwarding needs an explicit integral address conversion");
                let ValueDef::Result(i, _) = f.value_def(f.resolve(arg)) else {
                    panic!("stored constant disappeared: {module}")
                };
                assert!(matches!(f.inst_data(i), InstData::Iconst { imm, .. } if *imm == bits));
                assert!(
                    !f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Store { .. })),
                    "private storage should disappear: {module}"
                );
            }
        }
    }
}

#[test]
fn unique_offset_piece_forwards_at_load_in_dominated_block() {
    let source = "decl @effect()
func @f(i32, i64) -> ptr {
 ss0 = stack_slot 16, align 8
block0(v0: i32, v1: i64):
 v2 = stack_addr ss0
 v3 = iconst.i64 8
 v4 = ptradd v2, v3
 store v1, v4
 brif v0, block1, block2
block1:
 call @effect()
 jump block2
block2:
 v5 = load.ptr v4
 return v5
}";
    let module = optimize(source, VectorTarget::X86Sse2, "stack-forward");
    assert_eq!(loads(&module), 0, "{module}");
    let f = &module.functions[0];
    let last = f.blocks().last().unwrap();
    assert!(f.layout_insts().any(|(b, i)| {
        b == last
            && matches!(
                f.inst_data(i),
                InstData::Convert {
                    op: ConvertOp::IntToPtr,
                    ..
                }
            )
    }));
    assert!(module.to_string().contains("call @effect()"));
}

#[test]
fn target_free_portable_and_wasm_keep_pointer_memory_interpretation() {
    let source = local_source(i64::MIN, "");
    for pass in ["stack-forward", "stack-forward-cfg"] {
        let portable = optimize(&source, VectorTarget::Portable, pass);
        assert_eq!(loads(&portable), 1, "{portable}");
        let mut legacy = keel_ir::parse(&source).unwrap();
        keel_opt::run_passes(&mut legacy, &[pass]).unwrap();
        assert_eq!(loads(&legacy), 1, "{legacy}");
        let mut direct = keel_ir::parse(&source).unwrap();
        keel_opt::find_pass(pass)
            .unwrap()
            .run(&mut direct.functions[0]);
        assert_eq!(loads(&direct), 1, "{direct}");
    }
    let mut wasm = keel_ir::parse(&source).unwrap();
    keel_opt::optimize_wasm32_basic(&mut wasm).unwrap();
    keel_ir::verify_module_with_layout(&wasm, keel_ir::DataLayout::WASM32).unwrap();
    assert_eq!(loads(&wasm), 1, "{wasm}");
}

#[test]
fn pointer_forwarding_rejects_partial_float_and_overaligned_reads() {
    let source = local_source(42, "");
    let cases = [
        source.replace("iconst.i64 42", "iconst.i32 42"),
        source.replace("iconst.i64 42", "fconst.f64 42.0"),
        source.replace("load.ptr v0", "load.ptr align 16 v0"),
        source.replace(
            "store v1, v0",
            "store v1, v0\n v3 = iconst.i64 4\n v4 = ptradd v0, v3\n v5 = iconst.i32 7\n store v5, v4",
        ),
    ];
    for source in cases {
        let module = optimize(&source, VectorTarget::X86Sse2, "stack-forward");
        assert_eq!(loads(&module), 1, "{module}");
    }
}

#[test]
fn pointer_forwarding_keeps_escape_and_frame_observer_barriers() {
    for effect in [
        "call @escape(v0)",
        "v3 = ptrtoint.i64 v0",
        "store volatile v1, v0",
        "v3 = atomic_load.i64 relaxed v0",
        "v3 = intrinsic frame_address()",
        "v3 = asm \"\", \"=r\"() -> i64",
    ] {
        let module = optimize(
            &local_source(42, effect),
            VectorTarget::X86Sse2,
            "stack-forward",
        );
        assert_eq!(loads(&module), 1, "{effect}: {module}");
    }
    let source = local_source(42, "").replace("load.ptr v0", "load.ptr volatile v0");
    assert_eq!(
        loads(&optimize(&source, VectorTarget::X86Sse2, "stack-forward")),
        1
    );
}

#[test]
fn pointer_cross_block_reads_require_unique_dominating_full_width_store() {
    let source = "func @f(i32, i64) -> ptr {
 ss0 = stack_slot 8, align 8
block0(v0: i32, v1: i64):
 v2 = stack_addr ss0
 store v1, v2
 brif v0, block1, block2
block1:
 jump block2
block2:
 v3 = load.ptr v2
 return v3
}";
    for source in [
        source
            .replace(" store v1, v2\n brif", " brif")
            .replace("block1:\n jump", "block1:\n store v1, v2\n jump"),
        source.replace("block1:\n jump", "block1:\n store v1, v2\n jump"),
        source.replace("block1:\n jump", "block1:\n store v0, v2\n jump"),
        source.replace("\n}", "\nblock3:\n store v1, v2\n return v2\n}"),
    ] {
        let module = optimize(&source, VectorTarget::X86Sse2, "stack-forward");
        assert_eq!(loads(&module), 1, "{module}");
    }
}

#[test]
fn same_block_pointer_read_uses_latest_complete_store() {
    let module = optimize(
        &local_source(42, "v3 = iconst.i64 -1\n store v3, v0"),
        VectorTarget::X86Sse2,
        "stack-forward",
    );
    assert_eq!(loads(&module), 0, "{module}");
    let f = &module.functions[0];
    let arg = f
        .layout_insts()
        .find_map(|(_, i)| match *f.inst_data(i) {
            InstData::Convert {
                op: ConvertOp::IntToPtr,
                arg,
                ..
            } => Some(arg),
            _ => None,
        })
        .expect("load must use an explicit pointer conversion");
    let ValueDef::Result(i, _) = f.value_def(f.resolve(arg)) else {
        panic!("last stored constant disappeared: {module}")
    };
    assert!(matches!(f.inst_data(i), InstData::Iconst { imm: -1, .. }));
}

#[test]
fn debugger_visible_integer_home_keeps_pointer_read() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    let mut module = keel_ir::parse(&local_source(42, "")).unwrap();
    module.debug_types.push(DebugTypeData::Base {
        name: "address_bits".into(),
        encoding: Encoding::Unsigned,
        size: 8,
    });
    module.functions[0].debug_vars.push(DebugVar {
        name: "bits".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    let before = module.to_string();
    keel_opt::run_passes_for_target(&mut module, &["stack-forward"], VectorTarget::X86Sse2)
        .unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert_eq!(module.to_string(), before);
}

#[test]
fn pointer_reads_do_not_cross_stack_lifetime_marks() {
    let source = local_source(
        42,
        "v3 = intrinsic stack_save()\n intrinsic stack_restore(v3)",
    );
    let module = optimize(&source, VectorTarget::X86Sse2, "stack-forward");
    assert_eq!(loads(&module), 1, "{module}");
}
