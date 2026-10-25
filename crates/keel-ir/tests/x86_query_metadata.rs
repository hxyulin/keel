//! Independent metadata snapshot captured before generation at 50b85e31.
use keel_ir::{InstData, Intrinsic};
use std::collections::HashMap;

#[test]
fn query_and_scalar_io_metadata_matches_the_frozen_contract() {
    let rows: HashMap<_, _> = include_str!("fixtures/x86-query-bindings.txt")
        .lines()
        .filter(|s| !s.starts_with('#'))
        .map(|row| (row.split('|').nth(2).unwrap(), row))
        .collect();
    assert_eq!(rows.len(), 42);
    let mut seen = 0;
    for id in Intrinsic::ALL {
        let generated = matches!(id, keel_ir::x86_query_intrinsics!(all));
        if let Some(expected) = rows.get(id.info().name) {
            let info = id.info();
            let inst = InstData::Intrinsic { id, args: vec![] };
            let actual = format!(
                "{}|{:?}|{}|{:?}|{:?}|{}|{:?}|{}|{:?}|{:?}|{:?}",
                id as usize,
                id,
                info.name,
                info.params,
                info.returns,
                info.pure,
                inst.memory_effects(),
                id.is_x86_target(),
                id.x86_descriptor_query(),
                id.x86_selector_query(),
                id.x86_port_io()
            );
            assert_eq!(&actual, expected);
            assert!(generated);
            assert!(matches!(id, keel_ir::x86_query_intrinsics!(Barrier)));
            assert!(matches!(id, keel_ir::x86_query_intrinsics!(x86_64)));
            assert!(inst.is_pinned());
            seen += 1;
        } else {
            assert!(!generated, "{id:?}");
            assert!(id.x86_descriptor_query().is_none(), "{id:?}");
            assert!(id.x86_selector_query().is_none(), "{id:?}");
            assert!(id.x86_port_io().is_none(), "{id:?}");
        }
    }
    assert_eq!(seen, 42);
}
