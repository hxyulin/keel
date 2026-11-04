//! The registry's per-intrinsic facts agree with each other and with the
//! metadata that the system and query families own.
use keel_ir::{InstData, Intrinsic, MemoryEffects, X86Family};

#[test]
fn every_intrinsic_has_consistent_facts() {
    for id in Intrinsic::ALL {
        let info = id.info();
        let x86 = matches!(id, keel_ir::x86_intrinsics!());
        assert_eq!(id.x86_family().is_some(), x86, "{id:?}: family iff x86");
        assert_eq!(
            info.name.starts_with("x86_"),
            x86,
            "{id:?}: x86 name prefix"
        );
        let data = InstData::Intrinsic { id, args: vec![] };
        assert_eq!(data.memory_effects(), id.memory_effects(), "{id:?}");
        assert_eq!(data.may_trap(), id.may_trap(), "{id:?}");
        if info.pure {
            assert_eq!(
                id.memory_effects(),
                MemoryEffects::None,
                "{id:?}: pure has no effects"
            );
        }
        // Frame observers stay with the inliner, which owns both frames.
        if id.memory_effects() == MemoryEffects::None
            && !id.may_trap()
            && id != Intrinsic::FrameAddress
        {
            assert!(
                id.inlineable(),
                "{id:?}: effect-free, non-trapping, not inlineable"
            );
        }
    }
}

#[test]
fn owned_metadata_families_match_their_lowering_family() {
    for id in Intrinsic::ALL {
        let family = id.x86_family();
        let system = matches!(id, keel_ir::x86_system_intrinsics!(all));
        assert_eq!(family == Some(X86Family::System), system, "{id:?}");
        if matches!(id, keel_ir::x86_query_intrinsics!(all)) {
            let expected = if id.x86_port_io().is_some() {
                X86Family::PortIo
            } else if id.x86_descriptor_query().is_some() {
                X86Family::DescriptorQuery
            } else {
                assert!(id.x86_selector_query().is_some(), "{id:?}");
                X86Family::SelectorQuery
            };
            assert_eq!(family, Some(expected), "{id:?}");
        }
        if system || matches!(id, keel_ir::x86_query_intrinsics!(all)) {
            assert_eq!(id.memory_effects(), MemoryEffects::Barrier, "{id:?}");
            assert!(id.may_trap() && !id.inlineable(), "{id:?}");
        }
    }
}
