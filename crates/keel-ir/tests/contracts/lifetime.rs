//! Fixed-stack lifetime markers are explicit compiler barriers, without runtime effects.
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};

const SOURCE: &str = "func @f() -> i64 {
    ss0 = stack_slot 8, align 8
block0:
    v0 = stack_addr ss0
    intrinsic lifetime_start(v0)
    v1 = iconst.i64 1
    store v1, v0
    intrinsic lifetime_end(v0)
    intrinsic lifetime_start(v0)
    v2 = iconst.i64 2
    store v2, v0
    v3 = load.i64 v0
    intrinsic lifetime_end(v0)
    return v3
}";

#[test]
fn fixed_stack_start_end_and_restart_roundtrip_as_barriers() {
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let text = module.to_string();
    let reparsed = keel_ir::parse(&text).unwrap();
    keel_ir::verify_module(&reparsed).unwrap();
    let function = &reparsed.functions[0];
    let markers = function
        .layout_insts()
        .filter_map(|(_, inst)| match function.inst_data(inst) {
            data @ InstData::Intrinsic {
                id: Intrinsic::LifetimeStart | Intrinsic::LifetimeEnd,
                ..
            } => Some(data),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 4, "both epochs must retain their boundaries");
    for marker in markers {
        assert!(marker.is_pinned());
        assert_eq!(marker.memory_effects(), MemoryEffects::Barrier);
        assert!(!marker.may_trap());
    }
    for id in [Intrinsic::LifetimeStart, Intrinsic::LifetimeEnd] {
        assert_eq!(id.info().params, &[Type::Ptr]);
        assert!(id.info().returns.is_empty());
        assert!(!id.info().pure);
        assert_eq!(Intrinsic::from_name(id.info().name), Some(id));
    }
}

#[test]
fn lifetime_markers_require_direct_fixed_stack_addresses() {
    let cases = [
        "func @entry_pointer(ptr) {\nblock0(v0: ptr):\n intrinsic lifetime_start(v0)\n return\n}",
        "func @derived() {\n ss0 = stack_slot 8, align 8\nblock0:\n v0 = stack_addr ss0\n v1 = iconst.i64 0\n v2 = ptradd v0, v1\n intrinsic lifetime_start(v2)\n return\n}",
        "func @dynamic(i64) {\nblock0(v0: i64):\n v1 = intrinsic stack_alloc(v0)\n intrinsic lifetime_end(v1)\n return\n}",
    ];
    for source in cases {
        let module = keel_ir::parse(source).unwrap();
        let errors = keel_ir::verify_module(&module).unwrap_err();
        assert!(
            errors.iter().any(|e| e.to_string().contains("lifetime")),
            "{source}: {errors:?}"
        );
    }
    let missing =
        keel_ir::parse("func @missing() {\nblock0:\n intrinsic lifetime_start()\n return\n}")
            .unwrap();
    assert!(keel_ir::verify_module(&missing).is_err());
}
