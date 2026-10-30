//! Frozen pre-generation IDs and signatures; no privileged instructions execute.
#[path = "x86_system_metadata/x86_query_metadata.rs"]
mod x86_query_metadata;

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
    assert_eq!(Intrinsic::ALL.len(), expected.len() + 122);
    assert_eq!(
        &Intrinsic::ALL[expected.len()..],
        &[
            Intrinsic::LifetimeStart,
            Intrinsic::LifetimeEnd,
            Intrinsic::X86CpuXcr0,
            Intrinsic::X86Sha256msg1,
            Intrinsic::X86Sha256msg2,
            Intrinsic::X86Sha256rnds2,
            Intrinsic::X86Vpclmulqdq256,
            Intrinsic::X86Vpclmulqdq512,
            Intrinsic::X86Avx512Cmpd,
            Intrinsic::X86Avx512Cmpq,
            Intrinsic::X86Avx512Cmpud,
            Intrinsic::X86Avx512Cmpuq,
            Intrinsic::X86Avx512MaskAddd,
            Intrinsic::X86Avx512MaskAddq,
            Intrinsic::X86Avx512MaskSubd,
            Intrinsic::X86Avx512MaskSubq,
            Intrinsic::X86Avx512MaskLoadd,
            Intrinsic::X86Avx512MaskLoadq,
            Intrinsic::X86Avx512MaskStored,
            Intrinsic::X86Avx512MaskStoreq,
            Intrinsic::X86Avx512Cmpps,
            Intrinsic::X86Avx512Cmppd,
            Intrinsic::X86Avx512MaskAddps,
            Intrinsic::X86Avx512MaskAddpd,
            Intrinsic::X86Avx512MaskSubps,
            Intrinsic::X86Avx512MaskSubpd,
            Intrinsic::X86Avx512MaskMulps,
            Intrinsic::X86Avx512MaskMulpd,
            Intrinsic::X86Avx512MaskDivps,
            Intrinsic::X86Avx512MaskDivpd,
            Intrinsic::X86Avx512MaskSqrtps,
            Intrinsic::X86Avx512MaskSqrtpd,
            Intrinsic::X86Avx512MaskLoadps,
            Intrinsic::X86Avx512MaskLoadpd,
            Intrinsic::X86Avx512MaskStoreps,
            Intrinsic::X86Avx512MaskStorepd,
            Intrinsic::X86Avx512Vpgatherdd,
            Intrinsic::X86Avx512Vpgatherdq,
            Intrinsic::X86Avx512Vpgatherqd,
            Intrinsic::X86Avx512Vpgatherqq,
            Intrinsic::X86Avx512Vgatherdps,
            Intrinsic::X86Avx512Vgatherdpd,
            Intrinsic::X86Avx512Vgatherqps,
            Intrinsic::X86Avx512Vgatherqpd,
            Intrinsic::X86Avx512Vpscatterdd,
            Intrinsic::X86Avx512Vpscatterdq,
            Intrinsic::X86Avx512Vpscatterqd,
            Intrinsic::X86Avx512Vpscatterqq,
            Intrinsic::X86Avx512Vscatterdps,
            Intrinsic::X86Avx512Vscatterdpd,
            Intrinsic::X86Avx512Vscatterqps,
            Intrinsic::X86Avx512Vscatterqpd,
            Intrinsic::X86Avx512Compressd,
            Intrinsic::X86Avx512Compressq,
            Intrinsic::X86Avx512Compressps,
            Intrinsic::X86Avx512Compresspd,
            Intrinsic::X86Avx512Expandd,
            Intrinsic::X86Avx512Expandq,
            Intrinsic::X86Avx512Expandps,
            Intrinsic::X86Avx512Expandpd,
            Intrinsic::X86Avx512CompressStored,
            Intrinsic::X86Avx512CompressStoreq,
            Intrinsic::X86Avx512CompressStoreps,
            Intrinsic::X86Avx512CompressStorepd,
            Intrinsic::X86Avx512ExpandLoadd,
            Intrinsic::X86Avx512ExpandLoadq,
            Intrinsic::X86Avx512ExpandLoadps,
            Intrinsic::X86Avx512ExpandLoadpd,
            Intrinsic::X86Avx512Vcvtdq2ps,
            Intrinsic::X86Avx512Vcvtudq2ps,
            Intrinsic::X86Avx512Vcvtdq2pd,
            Intrinsic::X86Avx512Vcvtudq2pd,
            Intrinsic::X86Avx512Vcvtps2dq,
            Intrinsic::X86Avx512Vcvtps2udq,
            Intrinsic::X86Avx512Vcvttps2dq,
            Intrinsic::X86Avx512Vcvttps2udq,
            Intrinsic::X86Avx512Vcvtpd2dq,
            Intrinsic::X86Avx512Vcvtpd2udq,
            Intrinsic::X86Avx512Vcvttpd2dq,
            Intrinsic::X86Avx512Vcvttpd2udq,
            Intrinsic::X86Avx512Vcvtps2pd,
            Intrinsic::X86Avx512Vcvtpd2ps,
            Intrinsic::X86Avx512Vcvtph2ps,
            Intrinsic::X86Avx512Vcvtps2ph,
            Intrinsic::X86Avx512Getexpps,
            Intrinsic::X86Avx512Getexppd,
            Intrinsic::X86Avx512Getmantps,
            Intrinsic::X86Avx512Getmantpd,
            Intrinsic::X86Avx512Scalefps,
            Intrinsic::X86Avx512Scalefpd,
            Intrinsic::X86Avx512Rcp14ps,
            Intrinsic::X86Avx512Rcp14pd,
            Intrinsic::X86Avx512Rsqrt14ps,
            Intrinsic::X86Avx512Rsqrt14pd,
            Intrinsic::X86Avx512Roundscaleps,
            Intrinsic::X86Avx512Roundscalepd,
            Intrinsic::X86Avx512Fixupimmps,
            Intrinsic::X86Avx512Fixupimmpd,
            Intrinsic::X86Avx512MaskTernlogd,
            Intrinsic::X86Avx512MaskTernlogq,
            Intrinsic::X86Avx512MaskRold,
            Intrinsic::X86Avx512MaskRolq,
            Intrinsic::X86Avx512MaskRord,
            Intrinsic::X86Avx512MaskRorq,
            Intrinsic::X86Avx512MaskRolvd,
            Intrinsic::X86Avx512MaskRolvq,
            Intrinsic::X86Avx512MaskRorvd,
            Intrinsic::X86Avx512MaskRorvq,
            Intrinsic::X86Avx512MaskSllvd,
            Intrinsic::X86Avx512MaskSllvq,
            Intrinsic::X86Avx512MaskSrlvd,
            Intrinsic::X86Avx512MaskSrlvq,
            Intrinsic::X86Avx512MaskSravd,
            Intrinsic::X86Avx512MaskSravq,
            Intrinsic::X86Avx512Testd,
            Intrinsic::X86Avx512Testq,
            Intrinsic::X86Avx512Testnd,
            Intrinsic::X86Avx512Testnq,
            Intrinsic::X86Avx512MaskAbsd,
            Intrinsic::X86Avx512MaskAbsq,
            Intrinsic::X86Avx512MaskMuldq,
            Intrinsic::X86Avx512MaskMuludq,
        ]
    );
    assert_eq!(
        actual.len(),
        Intrinsic::ALL.len(),
        "duplicate inventory entry"
    );
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

#[test]
fn generated_helper_mappings_match_the_frozen_public_api() {
    let expected: HashMap<_, _> = fixture(include_str!("fixtures/x86-system-helper-mappings.txt"))
        .map(|row| {
            let fields: Vec<_> = row.split('|').collect();
            (fields[0], fields)
        })
        .collect();
    assert_eq!(expected.len(), 35);
    for id in Intrinsic::ALL {
        let name = format!("{id:?}");
        match expected.get(name.as_str()) {
            Some(fields) if fields[1] == "register" => {
                assert_eq!(
                    id.x86_system_register(),
                    Some((fields[2], fields[3].parse().unwrap()))
                );
                assert_eq!(id.x86_system_write(), None);
            }
            Some(fields) => {
                assert_eq!(fields[1], "unary");
                assert_eq!(
                    id.x86_system_write(),
                    Some((
                        fields[2],
                        fields[3].parse().unwrap(),
                        fields[4].parse().unwrap()
                    ))
                );
                assert_eq!(id.x86_system_register(), None);
            }
            None => {
                assert_eq!(id.x86_system_register(), None, "{name}");
                assert_eq!(id.x86_system_write(), None, "{name}");
            }
        }
    }
}

#[test]
fn generated_x86_pattern_matches_independent_target_classification() {
    for intrinsic in Intrinsic::ALL {
        assert_eq!(
            matches!(intrinsic, keel_ir::x86_intrinsics!()),
            intrinsic.is_x86_target(),
            "{intrinsic:?}"
        );
    }
}
