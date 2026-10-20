use keel_ir::InstData as D;
const FIXTURE: &str = include_str!("stack_cache/fields.keel");
fn apply(text: &str) -> keel_ir::Module {
    let mut m = keel_ir::parse(text).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["stack-cache"]).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_ir::verify_module(&keel_ir::parse(&m.to_string()).unwrap()).unwrap();
    m
}
#[test]
fn caches_post_capture_loop_reads_but_preserves_stores_and_alias_reads() {
    let m = apply(FIXTURE);
    for f in &m.functions {
        let loads: Vec<_> = f
            .layout_insts()
            .filter_map(|(b, i)| match f.inst_data(i) {
                D::Load { flags, .. } => Some((b, flags.notrap)),
                _ => None,
            })
            .collect();
        assert_eq!(loads.len(), 2, "{m}"); // one seeded field, one arbitrary alias
        assert_eq!(loads.iter().filter(|&&(_, n)| n).count(), 1);
        assert_eq!(
            f.layout_insts()
                .filter(|&(_, i)| matches!(f.inst_data(i), D::Store { .. }))
                .count(),
            1
        );
        assert_eq!(
            f.layout_insts()
                .filter(|&(_, i)| matches!(f.inst_data(i), D::Call { .. }))
                .count(),
            1
        );
        let entry = f.entry_block().unwrap();
        let list = f.block_insts(entry);
        let call = list
            .iter()
            .position(|&i| matches!(f.inst_data(i), D::Call { .. }))
            .unwrap();
        let load = list
            .iter()
            .position(|&i| matches!(f.inst_data(i), D::Load { .. }))
            .unwrap();
        assert!(load > call);
    }
    let mut again = m.clone();
    keel_opt::run_passes(&mut again, &["stack-cache"]).unwrap();
    assert_eq!(m.to_string(), again.to_string());
}
#[test]
fn excludes_writers_barriers_and_frame_observers() {
    for text in [
        FIXTURE.replace("store v10, v2", "store v10, v3"),
        FIXTURE.replace("store v10, v2", "store volatile v10, v2"),
        FIXTURE.replace(" v9 = iconst.", " fence acquire\n v9 = iconst."),
        FIXTURE.replace(
            " v9 = iconst.",
            " v20 = intrinsic frame_address()\n v9 = iconst.",
        ),
        FIXTURE.replace(" v9 = iconst.", " v20 = call @init8(v2, v1)\n v9 = iconst."),
        FIXTURE.replace(" -> ptr\nfunc", " -> ptr returns_twice\nfunc"),
    ] {
        let before = keel_ir::parse(&text).unwrap();
        keel_ir::verify_module(&before).unwrap();
        assert_eq!(before.to_string(), apply(&text).to_string(), "{text}");
    }
}
#[test]
fn overlap_and_partial_fields_remain_in_memory() {
    let text = FIXTURE.replace(
        " store v10, v2",
        " store v10, v2\n v20 = iconst.i8 42\n store v20, v2",
    );
    let m = apply(&text);
    for f in m.functions.iter().skip(1) {
        assert_eq!(
            f.layout_insts()
                .filter(|&(_, i)| matches!(f.inst_data(i), D::Load { .. }))
                .count(),
            3
        );
    }
}
#[test]
fn debug_homes_are_preserved() {
    use keel_ir::{DebugType, DebugTypeData, DebugVar, Encoding, EntityRef, StackSlot};
    let mut m = keel_ir::parse(FIXTURE).unwrap();
    m.debug_types.push(DebugTypeData::Base {
        name: "word".into(),
        encoding: Encoding::Unsigned,
        size: 8,
    });
    m.functions[0].debug_vars.push(DebugVar {
        name: "home".into(),
        ty: DebugType::new(0),
        slot: StackSlot::new(0),
        indirect: false,
        param: false,
        scope: None,
    });
    let before = m.functions[0].to_string();
    keel_opt::run_passes(&mut m, &["stack-cache"]).unwrap();
    assert_eq!(before, m.functions[0].to_string());
}
#[test]
fn duplicate_edges_receive_every_cached_argument() {
    let text = FIXTURE
        .replace(
            "jump block1(v4, v4)",
            "brif v4, block1(v4, v4), block1(v4, v4)",
        )
        .replace(
            "jump block1(v13, v11)",
            "brif v7, block1(v13, v11), block1(v13, v11)",
        );
    let m = apply(&text);
    for f in &m.functions {
        assert_eq!(
            f.layout_insts()
                .filter(|&(_, i)| matches!(f.inst_data(i), D::Load { .. }))
                .count(),
            2
        );
    }
}
#[test]
fn unknown_initializers_are_read_and_out_of_bounds_writes_disable_caching() {
    let m = apply(FIXTURE);
    for f in &m.functions {
        let entry = f.entry_block().unwrap();
        assert!(
            f.block_insts(entry)
                .iter()
                .any(|&i| matches!(f.inst_data(i),D::Load{flags,..} if flags.notrap))
        );
    }
    // A partial fixed-slot read is safe; a partial or unknown *write* in the
    // suffix invalidates the exact field. No byte reinterpretation is inferred.
    let text = FIXTURE.replace(
        "store v10, v2",
        "store v10, v2\n v20 = iconst.i64 -1\n v21 = ptradd v2, v20\n store v10, v21",
    );
    let before = keel_ir::parse(&text).unwrap();
    assert_eq!(before.to_string(), apply(&text).to_string());
}
