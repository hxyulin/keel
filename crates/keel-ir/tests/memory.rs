use keel_ir::{
    BinaryOp, EntityRef, InstData, Intrinsic, MemoryEffects, MemoryFlags, Ordering, Type, Value,
};

#[test]
fn memory_flags_roundtrip_and_legacy_defaults() {
    let source = "func @f(ptr, i32) -> i32 {
block0(v0: ptr, v1: i32):
    v2 = load.i32 v0
    v3 = load.i32 notrap align 4 volatile v0
    store align 4 volatile notrap v1, v0
    return v3
}";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let printed = module.to_string();
    assert!(printed.contains("load.i32 volatile align 4 notrap"));
    assert!(printed.contains("store volatile align 4 notrap"));
    assert_eq!(printed, keel_ir::parse(&printed).unwrap().to_string());
    let accesses: Vec<_> = module.functions[0]
        .layout_insts()
        .filter_map(|(_, i)| match module.functions[0].inst_data(i) {
            InstData::Load { flags, .. } | InstData::Store { flags, .. } => Some(*flags),
            _ => None,
        })
        .collect();
    assert_eq!(accesses[0], MemoryFlags::default());
    assert!(accesses[1].volatile && accesses[1].notrap);
    assert_eq!(accesses[1].align, 4);
    assert_eq!(accesses[1], accesses[2]);
}

#[test]
fn malformed_flags_are_rejected() {
    for flag in [
        "volatile volatile",
        "notrap notrap",
        "align 4 align 8",
        "align -1",
        "align 4294967296",
    ] {
        let source = format!(
            "func @f(ptr) -> i32 {{\nblock0(v0: ptr):\n    v1 = load.i32 {flag} v0\n    return v1\n}}"
        );
        assert!(keel_ir::parse(&source).is_err(), "{flag}");
    }
    for align in [0, 3, 6] {
        let source = format!(
            "func @f(ptr) -> i32 {{\nblock0(v0: ptr):\n    v1 = load.i32 align {align} v0\n    store align {align} v1, v0\n    return v1\n}}"
        );
        let module = keel_ir::parse(&source).unwrap();
        let errors = keel_ir::verify_module(&module).unwrap_err();
        assert_eq!(
            errors
                .iter()
                .filter(|e| e.message.contains("memory alignment"))
                .count(),
            2
        );
    }
}

#[test]
fn effects_and_faults_do_not_conflate_reads_volatility_and_scheduling() {
    let addr = Value::new(0);
    let mut load = InstData::Load {
        ty: Type::I32,
        addr,
        flags: MemoryFlags::default(),
    };
    assert_eq!(load.memory_effects(), MemoryEffects::Read);
    assert!(load.may_trap());
    assert!(load.is_pinned());
    if let InstData::Load { flags, .. } = &mut load {
        flags.notrap = true;
    }
    assert!(!load.may_trap());
    assert!(
        load.is_pinned(),
        "notrap does not establish safe movement across stores"
    );
    if let InstData::Load { flags, .. } = &mut load {
        flags.volatile = true;
    }
    assert_eq!(load.memory_effects(), MemoryEffects::Barrier);
    assert!(!load.may_trap());
    assert_eq!(
        InstData::Store {
            value: addr,
            addr,
            flags: MemoryFlags::default()
        }
        .memory_effects(),
        MemoryEffects::Write
    );
    assert_eq!(
        InstData::AtomicLoad {
            ty: Type::I32,
            addr,
            ordering: Ordering::Relaxed
        }
        .memory_effects(),
        MemoryEffects::Barrier
    );
    assert_eq!(
        InstData::Fence {
            ordering: Ordering::Acquire
        }
        .memory_effects(),
        MemoryEffects::Barrier
    );
    for id in [Intrinsic::Memcpy, Intrinsic::Memmove] {
        assert_eq!(
            InstData::Intrinsic { id, args: vec![] }.memory_effects(),
            MemoryEffects::ReadWrite
        );
    }
    assert_eq!(
        InstData::Intrinsic {
            id: Intrinsic::Memset,
            args: vec![]
        }
        .memory_effects(),
        MemoryEffects::Write
    );
    assert_eq!(
        InstData::Intrinsic {
            id: Intrinsic::StackRestore,
            args: vec![]
        }
        .memory_effects(),
        MemoryEffects::Barrier
    );
    let call = InstData::Call {
        func: keel_ir::FuncRef::new(0),
        args: (vec![]).into(),
    };
    assert_eq!(call.memory_effects(), MemoryEffects::Barrier);
    assert!(call.may_trap());
    assert!(MemoryEffects::Barrier.may_read() && MemoryEffects::Barrier.may_write());
    let divide = InstData::Binary {
        op: BinaryOp::Sdiv,
        args: [addr, addr],
    };
    assert_eq!(divide.memory_effects(), MemoryEffects::None);
    assert!(divide.may_trap());
}
