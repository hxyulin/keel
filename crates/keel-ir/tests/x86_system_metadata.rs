//! Frozen pre-generation IDs and signatures; no privileged instructions execute.
use keel_ir::{InstData, Intrinsic, MemoryEffects, Type};
use std::collections::HashMap;

fn fixture(source: &str) -> impl Iterator<Item = &str> {
    source.lines().filter(|line| !line.starts_with('#'))
}

#[test]
fn all_numeric_ids_and_inventory_order_stay_stable() {
    let actual: HashMap<_, _> = Intrinsic::ALL
        .iter()
        .map(|id| (format!("{id:?}"), *id as usize))
        .collect();
    let expected: Vec<_> = fixture(include_str!("fixtures/intrinsic-inventory.txt")).collect();
    assert_eq!(Intrinsic::ALL.len(), expected.len());
    assert_eq!(actual.len(), expected.len(), "duplicate inventory entry");
    let mut names = std::collections::HashSet::new();
    for intrinsic in Intrinsic::ALL {
        assert!(
            names.insert(intrinsic.info().name),
            "duplicate intrinsic name"
        );
        assert_eq!(Intrinsic::from_name(intrinsic.info().name), Some(intrinsic));
    }
    for (index, row) in expected.iter().enumerate() {
        let (numeric, inventory) = row.split_once('|').unwrap();
        assert_eq!(actual[numeric], index, "numeric ID for {numeric}");
        assert_eq!(format!("{:?}", Intrinsic::ALL[index]), inventory);
    }
}

fn types(source: &str) -> Vec<Type> {
    if source.is_empty() {
        return vec![];
    }
    source
        .split(',')
        .map(|name| match name {
            "I16" => Type::I16,
            "I32" => Type::I32,
            "I64" => Type::I64,
            "Ptr" => Type::Ptr,
            _ => panic!("unexpected fixture type {name}"),
        })
        .collect()
}

#[test]
fn system_signatures_effects_names_and_target_classification_stay_stable() {
    let rows: Vec<_> = fixture(include_str!("fixtures/x86-system-intrinsics.txt")).collect();
    assert_eq!(rows.len(), 45);
    for row in rows {
        let fields: Vec<_> = row.split('|').collect();
        let id = Intrinsic::from_name(fields[2]).unwrap();
        assert_eq!(id as usize, fields[0].parse::<usize>().unwrap());
        assert_eq!(format!("{id:?}"), fields[1]);
        let info = id.info();
        assert_eq!(info.name, fields[2]);
        assert_eq!(id.to_string(), fields[2]);
        assert_eq!(info.params, types(fields[3]));
        assert_eq!(info.returns, types(fields[4]));
        assert_eq!(info.pure, fields[5].parse::<bool>().unwrap());
        assert_eq!(fields[6], "Barrier");
        assert_eq!(fields[7], "x86_64");
        let inst = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(inst.memory_effects(), MemoryEffects::Barrier);
        assert!(inst.is_pinned());
        assert!(id.is_x86_target());
        // Exercise exported patterns from outside the defining crate.
        assert!(matches!(id, keel_ir::x86_system_intrinsics!(x86_64)));
        assert!(matches!(id, keel_ir::x86_system_intrinsics!(Barrier)));
    }
    for id in Intrinsic::ALL {
        assert_eq!(
            matches!(id, keel_ir::x86_system_intrinsics!(all)),
            (774..=818).contains(&(id as usize)),
            "{id:?}"
        );
    }
}
