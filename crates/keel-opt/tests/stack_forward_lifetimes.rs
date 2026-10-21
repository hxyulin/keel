//! Local forwarding in large functions with dynamic stack lifetimes. These
//! tests inspect in-memory IR directly; printing is only for failure messages.
use keel_ir::{InstData, Module};

fn optimize(source: &str) -> Module {
    let mut m = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["stack-forward"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    m
}

fn loads(m: &Module) -> usize {
    let f = &m.functions[0];
    f.layout_insts()
        .filter(|&(_, i)| matches!(f.inst_data(i), InstData::Load { .. }))
        .count()
}

fn fixture(body: &str) -> String {
    format!(
        "decl @effect(ptr)\ndecl @checkpoint() returns_twice
func @f(ptr, i64, i64) -> i64 {{
 ss0 = stack_slot 16, align 8
block0(v0: ptr, v1: i64, v2: i64):
 v3 = stack_addr ss0
 v4 = iconst.i64 8
 v5 = ptradd v3, v4
 v6 = intrinsic stack_save()
 {body}
}}"
    )
}

#[test]
fn jsvalue_payload_and_tag_copy_forward_in_large_dynamic_function() {
    // Two copied JSValue fields, followed by payload reloads and an unrelated
    // effect. Retain stores: this restricted mode does not prove lifetimes
    // globally. Exceed both promotion size limits without changing policy.
    let padding = (20..8220)
        .map(|n| format!("v{n} = iconst.i64 0\n"))
        .collect::<String>();
    let tail = (1..258)
        .map(|n| format!("block{n}:\n jump block{}\n", n + 1))
        .collect::<String>();
    let source = fixture(&format!(
        "{padding}
 store v1, v3
 store v2, v5
 call @effect(v0)
 v7 = load.i32 v3
 v8 = load.i64 v5
 v9 = sext.i64 v7
 v10 = iadd v9, v8
 intrinsic stack_restore(v6)
 jump block1
 {tail}
block258:
 return v10"
    ));
    let mut m = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&m).unwrap();
    assert!(
        !keel_opt::find_pass("stack-promote")
            .unwrap()
            .run(&mut m.functions[0])
    );
    assert!(
        keel_opt::find_pass("stack-forward")
            .unwrap()
            .run(&mut m.functions[0])
    );
    keel_ir::verify_module(&m).unwrap();
    assert_eq!(loads(&m), 0);
    let f = &m.functions[0];
    assert_eq!(
        f.layout_insts()
            .filter(|&(_, i)| matches!(f.inst_data(i), InstData::Store { .. }))
            .count(),
        2
    );
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Call { .. }))
    );
}

#[test]
fn neither_save_nor_restore_nor_block_edges_carry_facts() {
    for barrier in [
        "v11 = intrinsic stack_save()",
        "intrinsic stack_restore(v6)",
        "jump block1\nblock1:",
    ] {
        let m = optimize(&fixture(&format!(
            "store v1, v3\n {barrier}\n v7 = load.i64 v3\n return v7"
        )));
        assert_eq!(loads(&m), 1, "{barrier}: {m}");
        assert!(m.to_string().contains("store"));
    }
    // New writes after a restore establish a new local interval.
    let m = optimize(&fixture(
        "store v1, v3\n intrinsic stack_restore(v6)\n store v2, v3\n v7 = load.i64 v3\n return v7",
    ));
    assert_eq!(loads(&m), 0);
    let f = &m.functions[0];
    let ret = *f.block_insts(f.entry_block().unwrap()).last().unwrap();
    let InstData::Return { args } = f.inst_data(ret) else {
        panic!()
    };
    assert_eq!(
        f.resolve(args[0]),
        f.block_params(f.entry_block().unwrap())[2]
    );
}

#[test]
fn observers_and_escapes_keep_loads_even_outside_local_interval() {
    for effect in [
        "call @effect(v3)",
        "v11 = ptrtoint.i64 v3",
        "v11 = intrinsic frame_address()",
        "call @checkpoint()",
        "asm \"nop\", \"\"()",
        "v11 = call_indirect v0() : () -> i64 returns_twice",
        "v11 = atomic_load.i64 relaxed v3",
        "v11 = load.i64 volatile v3",
        "store volatile v2, v3",
    ] {
        // Escape after the otherwise forwardable read is still disqualifying.
        let m = optimize(&fixture(&format!(
            "store v1, v3\n v7 = load.i64 v3\n {effect}\n return v7"
        )));
        assert!(loads(&m) >= 1, "{effect}: {m}");
    }
}

#[test]
fn initialization_neighbor_bytes_and_types_remain_distinct() {
    for (body, expected) in [
        ("v7 = load.i64 v3\n store v1, v3\n return v7", 1),
        (
            "v11 = trunc.i32 v1\n store v11, v3\n v7 = load.i64 v3\n return v7",
            1,
        ),
        ("store v1, v3\n v7 = load.i64 v5\n return v7", 1),
        (
            "store v1, v3\n v11 = load.ptr v3\n v7 = ptrtoint.i64 v11\n return v7",
            1,
        ),
        (
            "store v1, v3\n store v2, v5\n v7 = load.i64 v3\n return v7",
            0,
        ),
    ] {
        assert_eq!(loads(&optimize(&fixture(body))), expected, "{body}");
    }
}

#[test]
fn debug_home_is_never_forwarded_in_dynamic_function() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    let mut m = keel_ir::parse(&fixture("store v1, v3\n v7 = load.i64 v3\n return v7")).unwrap();
    m.debug_types.push(DebugTypeData::Base {
        name: "value".into(),
        encoding: Encoding::Unsigned,
        size: 16,
    });
    m.functions[0].debug_vars.push(DebugVar {
        name: "value".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    keel_ir::verify_module(&m).unwrap();
    assert!(
        !keel_opt::find_pass("stack-forward")
            .unwrap()
            .run(&mut m.functions[0])
    );
    assert_eq!(loads(&m), 1);
}

#[test]
fn native_fixture_forwards_payload_but_keeps_post_restore_tag_read() {
    let source = include_str!("stack_forward_lifetimes/copy.keel");
    let before = keel_ir::parse(source).unwrap();
    assert_eq!(loads(&before), 2);
    let m = optimize(source);
    assert_eq!(loads(&m), 1);
    let f = &m.functions[0];
    let remaining = f.layout_insts().find_map(|(_, i)| {
        if let InstData::Load { ty, .. } = f.inst_data(i) {
            Some(*ty)
        } else {
            None
        }
    });
    assert_eq!(remaining, Some(keel_ir::Type::I64));
}

#[test]
fn saved_sp_must_remain_an_opaque_restore_mark() {
    for observer in [
        "v11 = ptrtoint.i64 v6",
        "call @effect(v6)",
        "v11 = load.i64 v6",
        "v11 = ptradd v6, v4",
        "store v6, v0",
        "v11 = icmp.eq v6, v0",
    ] {
        let m = optimize(&fixture(&format!(
            "store v1, v3\n v7 = load.i64 v3\n {observer}\n return v7"
        )));
        assert!(loads(&m) >= 1, "{observer}: {m}");
    }
}

#[test]
fn quickjs_integer_sub_crosses_tag_branch_and_preserves_float_read() {
    let source = include_str!("stack_forward_lifetimes/sub.keel");
    let original = keel_ir::parse(source).unwrap();
    assert_eq!(loads(&original), 3);
    let m = optimize(source);
    assert_eq!(loads(&m), 1);
    let f = &m.functions[0];
    assert!(f.layout_insts().any(|(_, i)| matches!(
        f.inst_data(i),
        InstData::Load {
            ty: keel_ir::Type::F64,
            ..
        }
    )));
    assert!(
        f.layout_insts()
            .any(|(_, i)| matches!(f.inst_data(i), InstData::Store { .. }))
    );
}

#[test]
fn subranges_require_unique_dominating_initialization_and_no_observer() {
    let source = include_str!("stack_forward_lifetimes/sub.keel");
    for effect in [
        "store v1, v3",
        "v15 = trunc.i8 v1\n store v15, v3",
        "v15 = ptrtoint.i64 v3",
        "v15 = intrinsic frame_address()",
        "asm \"nop\", \"\"()",
        "v15 = intrinsic stack_save()",
    ] {
        let source = source.replace(
            "brif v2, block2, block1",
            &format!("{effect}\n brif v2, block2, block1"),
        );
        let m = optimize(&source);
        assert!(loads(&m) >= 2, "{effect}: {m}");
    }
    // A syntactically unique writer in only one arm does not initialize a join.
    let source = source
        .replace("store v0, v3", "")
        .replace("block2:\n", "block2:\n store v0, v3\n");
    assert_eq!(loads(&optimize(&source)), 2); // Only ss1's dominated i32 load disappears.
}

#[test]
fn restores_from_external_marks_are_excluded() {
    let m = optimize(&fixture(
        "store v1, v3\n v7 = load.i64 v3\n intrinsic stack_restore(v0)\n return v7",
    ));
    assert_eq!(loads(&m), 1);
}
