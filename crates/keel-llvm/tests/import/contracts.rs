//! Memory/pointer contract transport and definedness boundaries.
use super::translated;
use keel_ir::{InstData, MemoryAccess as A, MemorySummary};
use std::{path::PathBuf, process::Command};
fn importer() -> Option<keel_llvm::import::Importer> {
    let path = std::env::var_os("KEEL_TEST_LIBLLVM")?;
    // SAFETY: the test environment supplies the trusted LLVM 23 runtime.
    Some(unsafe { keel_llvm::import::Importer::load(&PathBuf::from(path)) }.unwrap())
}

#[test]
fn capture_components_locations_and_return_noalias_survive_roundtrip() {
    let Some(importer) = importer() else {
        return;
    };
    let module = translated(
        &importer,
        "declare noalias ptr @allocate(ptr captures(ret: address, provenance))\n\
        declare ptr @address(ptr captures(address))\n\
        declare ptr @read(ptr captures(address_is_null, read_provenance))\n\
        declare ptr @conditional(ptr noundef dereferenceable_or_null(32))\n\
        declare ptr @poisonable(ptr dereferenceable_or_null(32))\n",
    );
    let allocation = &module.decls[0].attrs.contract;
    assert!(allocation.returns[0].1.return_noalias);
    assert!(!allocation.returns[0].1.nonnull);
    assert_eq!(allocation.returns[0].1.readable_bytes, 0);
    assert_eq!(
        allocation.parameters[0].1.capture.other,
        keel_ir::CaptureComponents::NONE
    );
    assert_eq!(
        allocation.parameters[0].1.capture.returned,
        keel_ir::CaptureComponents::default()
    );
    let address = module.decls[1].attrs.contract.parameters[0].1.capture;
    assert_eq!(address.other.address, keel_ir::AddressCapture::Address);
    assert_eq!(address.other.provenance, keel_ir::ProvenanceCapture::None);
    let read = module.decls[2].attrs.contract.parameters[0].1.capture;
    assert_eq!(read.other.address, keel_ir::AddressCapture::Nullness);
    assert_eq!(read.other.provenance, keel_ir::ProvenanceCapture::Read);
    let conditional = &module.decls[3].attrs.contract.parameters[0].1;
    assert_eq!(conditional.readable_or_null_bytes, 32);
    assert_eq!(conditional.readable_bytes, 0);
    assert!(!conditional.nonnull);
    assert!(module.decls[4].attrs.contract.parameters.is_empty());
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    let imported = importer
        .translate(exported.as_bytes(), Default::default())
        .unwrap();
    let roundtrip = keel_ir::parse(&imported).unwrap();
    for before in &module.decls {
        let after = roundtrip
            .decls
            .iter()
            .find(|d| d.name == before.name)
            .unwrap();
        assert_eq!(
            before.attrs.contract, after.attrs.contract,
            "{}: {exported}",
            before.name
        );
    }
}

#[test]
fn defined_ranges_keep_function_and_call_scopes_without_promoting_poison() {
    let Some(importer) = importer() else {
        return;
    };
    let module = translated(
        &importer,
        "declare noundef range(i32 -8, 8) i32 @r(i32 noundef range(i32 0, 10), i32 range(i32 0, 4))\n\
        define i32 @caller(i32 %x, ptr %fn) {\n\
          %a = call noundef range(i32 2, 5) i32 @r(i32 noundef range(i32 1, 6) %x, i32 0)\n\
          %b = call noundef range(i32 7, 9) i32 %fn(i32 noundef range(i32 3, 8) %x)\n\
          %s = add i32 %a, %b\nret i32 %s\n}\n\
        define range(i32 1, 8) i32 @poisonable(i32 range(i32 2, 9) %x) { ret i32 %x }\n",
    );
    let declared = &module.decls[0].attrs.contract;
    assert_eq!(
        declared.return_ranges,
        [(
            0,
            keel_ir::IntegerRange {
                lower: 0xfffffff8,
                upper: 8
            }
        )]
    );
    assert_eq!(
        declared.parameter_ranges,
        [(
            0,
            keel_ir::IntegerRange {
                lower: 0,
                upper: 10
            }
        )]
    );
    let caller = &module.functions[0];
    assert!(caller.attrs.contract.is_unknown());
    let calls: Vec<_> = caller
        .layout_insts()
        .filter_map(|(_, i)| caller.call_contract(i))
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].parameter_ranges[0].1.lower, 1);
    assert_eq!(calls[0].return_ranges[0].1.lower, 2);
    assert_eq!(calls[1].parameter_ranges[0].1.lower, 3);
    assert_eq!(calls[1].return_ranges[0].1.lower, 7);
    assert!(
        module.functions[1].attrs.contract.is_unknown(),
        "poisonable ranges need definedness"
    );
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    let imported = importer
        .translate(exported.as_bytes(), Default::default())
        .unwrap();
    let roundtrip = keel_ir::parse(&imported).unwrap();
    assert_eq!(roundtrip.decls[0].attrs.contract, *declared);
    let sites: Vec<_> = roundtrip.functions[0]
        .layout_insts()
        .filter_map(|(_, i)| roundtrip.functions[0].call_contract(i))
        .collect();
    assert_eq!(sites, calls);
}

#[test]
fn conjunctive_ranges_are_retained_and_unencodable_llvm_export_is_rejected() {
    let module =
        keel_ir::parse("decl @f(i32) contract(range_param(0, 0, 8), range_param(0, 4, 12))\n")
            .unwrap();
    keel_ir::verify_module(&module).unwrap();
    assert!(
        keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64)
            .unwrap_err()
            .contains("conjunctive")
    );
}

#[test]
fn range_widths_and_quoted_parameter_names_keep_the_correct_bounds() {
    let Some(importer) = importer() else {
        return;
    };
    for width in [8, 16, 32, 64] {
        let ext = if width < 32 { "zeroext " } else { "" };
        let source = format!(
            "declare {ext}noundef range(i{width} -8, 8) i{width} @f(i{width} noundef range(i{width} -4, 4) %\"range(i{width} 0, 2)\")\n"
        );
        let module = translated(&importer, &source);
        let mask = u64::MAX >> (64 - width);
        let contract = &module.decls[0].attrs.contract;
        assert_eq!(contract.parameter_ranges[0].1.lower, (-4i64 as u64) & mask);
        assert_eq!(contract.return_ranges[0].1.lower, (-8i64 as u64) & mask);
        let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
        let text = importer
            .translate(exported.as_bytes(), Default::default())
            .unwrap();
        assert_eq!(
            keel_ir::parse(&text).unwrap().decls[0].attrs.contract,
            *contract
        );
    }
}

#[test]
fn allocation_and_capture_facts_on_one_indirect_call_do_not_affect_another() {
    let Some(importer) = importer() else {
        return;
    };
    let module = translated(
        &importer,
        "define ptr @indirect(ptr %fn, ptr %p) {\n\
        %a = call noalias ptr %fn(ptr captures(address) %p)\n\
        %b = call ptr %fn(ptr %p)\nret ptr %a\n}\n",
    );
    let function = &module.functions[0];
    let calls: Vec<_> = function
        .layout_insts()
        .filter_map(|(_, i)| {
            matches!(function.inst_data(i), InstData::CallIndirect { .. }).then_some(i)
        })
        .collect();
    let first = function.call_contract(calls[0]).unwrap();
    assert!(first.returns[0].1.return_noalias);
    assert_eq!(
        first.parameters[0].1.capture.other.provenance,
        keel_ir::ProvenanceCapture::None
    );
    assert!(function.call_contract(calls[1]).is_none());
    assert!(function.attrs.contract.is_unknown());
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    let text = importer
        .translate(exported.as_bytes(), Default::default())
        .unwrap();
    let again = keel_ir::parse(&text).unwrap();
    let sites: Vec<_> = again.functions[0]
        .layout_insts()
        .filter_map(|(_, i)| again.functions[0].call_contract(i))
        .collect();
    assert_eq!(sites, [first]);
}
#[test]
fn llvm23_memory_encoding_and_conservative_regions() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "declare void @none() memory(none) nofree nosync nocallback\n\
        declare void @argument(ptr) memory(argmem: read)\n\
        declare void @hidden() memory(inaccessiblemem: write)\n\
        declare void @errno_only() memory(errnomem: read)\n\
        declare void @other_only() memory(readwrite, argmem: none, inaccessiblemem: none, errnomem: none, target_mem0: none, target_mem1: none)\n\
        declare void @target_only() memory(target_mem0: write)\n\
        declare void @unknown()\n",
    );
    let contracts: Vec<_> = module
        .decls
        .iter()
        .map(|d| d.attrs.contract.clone())
        .collect();
    assert_eq!(
        contracts[0].memory,
        MemorySummary {
            argmem: A::None,
            other: A::None,
            inaccessible: A::None
        }
    );
    assert!(contracts[0].nofree && contracts[0].nosync && contracts[0].nocallback);
    assert!(
        !contracts[0].preserves_access,
        "LLVM effects must not assert Keel access stability"
    );
    assert_eq!(
        contracts[1].memory,
        MemorySummary {
            argmem: A::Read,
            other: A::None,
            inaccessible: A::None
        }
    );
    assert_eq!(
        contracts[2].memory,
        MemorySummary {
            argmem: A::None,
            other: A::None,
            inaccessible: A::Write
        }
    );
    assert_eq!(
        contracts[3].memory,
        MemorySummary {
            argmem: A::None,
            other: A::Read,
            inaccessible: A::None
        }
    );
    assert_eq!(
        contracts[4].memory,
        MemorySummary {
            argmem: A::None,
            other: A::ReadWrite,
            inaccessible: A::None
        }
    );
    assert_eq!(
        contracts[5].memory,
        MemorySummary {
            argmem: A::None,
            other: A::None,
            inaccessible: A::Write
        }
    );
    assert_eq!(contracts[6], Default::default());
}
#[test]
fn pointer_promises_require_definedness_and_keep_capture_separate() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "declare ptr @safe(ptr noundef nonnull align 16 captures(none), ptr dereferenceable(32) align 8)\n\
        declare ptr @poisonable(ptr nonnull align 16, ptr captures(address), ptr dereferenceable_or_null(32))\n\
        declare noundef nonnull align 8 ptr @result()\n",
    );
    let safe = &module.decls[0].attrs.contract;
    assert_eq!(safe.parameters.len(), 2);
    assert_eq!(safe.parameters[0].0, 0);
    assert_eq!(safe.parameters[0].1.align, 16);
    assert!(
        safe.parameters[0].1.nonnull
            && safe.parameters[0].1.capture == keel_ir::CaptureContract::NONE
    );
    assert_eq!(safe.parameters[1].1.readable_bytes, 32);
    assert_eq!(safe.parameters[1].1.align, 8);
    assert!(safe.parameters[1].1.nonnull);
    assert_eq!(module.decls[1].attrs.contract.parameters.len(), 1);
    assert_eq!(module.decls[1].attrs.contract.parameters[0].0, 1);
    let result = &module.decls[2].attrs.contract.returns[0].1;
    assert_eq!(result.align, 8);
    assert!(result.nonnull && result.capture == keel_ir::CaptureContract::default());
}
#[test]
fn direct_and_indirect_site_contracts_do_not_escape_the_call() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "declare ptr @callee(ptr)\n\
        define ptr @caller(ptr %p, ptr %fp) {\n\
        %a = call noundef nonnull ptr @callee(ptr noundef nonnull align 8 %p) memory(none) nofree nosync nocallback\n\
        %b = call ptr %fp(ptr captures(none) %a) memory(argmem: read) nofree nosync nocallback\n\
        ret ptr %b\n}\n",
    );
    assert_eq!(module.decls[0].attrs.contract, Default::default());
    let caller = &module.functions[0];
    assert_eq!(caller.attrs.contract, Default::default());
    let calls: Vec<_> = caller
        .layout_insts()
        .filter_map(|(_, inst)| {
            matches!(
                caller.inst_data(inst),
                InstData::Call { .. } | InstData::CallIndirect { .. }
            )
            .then_some(inst)
        })
        .collect();
    let a = caller.call_contract(calls[0]).unwrap();
    let b = caller.call_contract(calls[1]).unwrap();
    assert_eq!(a.memory.argmem, A::None);
    assert!(a.parameters[0].1.nonnull && a.returns[0].1.nonnull);
    assert_eq!(b.memory.argmem, A::Read);
    assert!(b.parameters[0].1.capture == keel_ir::CaptureContract::NONE);
    assert!(b.returns.is_empty());
    let llvm = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    let again = i.translate(llvm.as_bytes(), Default::default()).unwrap();
    let again = keel_ir::parse(&again).unwrap();
    let f = &again.functions[0];
    let sites: Vec<_> = f
        .layout_insts()
        .filter_map(|(_, inst)| f.call_contract(inst))
        .collect();
    assert_eq!(sites, [a, b]);
    let dir = std::env::temp_dir().join(format!("keel-contract-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test.ll");
    std::fs::write(&path, llvm).unwrap();
    let out = Command::new("clang")
        .args(["-Wno-override-module", "-c"])
        .arg(&path)
        .arg("-o")
        .arg(dir.join("test.o"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn masked_nonnull_pointer_and_overflow_do_not_become_preconditions() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "define i64 @masked(ptr nonnull %p, i64 %take, i64 %a, i64 %b) {\n\
        %c = icmp ne i64 %take, 0\n\
        %chosen = select i1 %c, ptr %p, ptr null\n\
        %isnull = icmp eq ptr %chosen, null\n\
        %sum = add nsw i64 %a, %b\n\
        %result = select i1 %isnull, i64 7, i64 %sum\n\
        ret i64 %result\n}\n",
    );
    assert_eq!(module.functions[0].attrs.contract, Default::default());
    let text = module.to_string();
    assert!(!text.contains("nonnull") && !text.contains("nsw"), "{text}");
}

#[test]
fn parameter_noalias_is_definition_scoped_and_return_noalias_is_distinct() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "declare noalias ptr @external(ptr noalias)\n\
        define i64 @read_only(ptr noalias %a, ptr noalias %b) {\n\
        %x = load i64, ptr %a\n %y = load i64, ptr %b\n %r = add i64 %x, %y\n ret i64 %r\n}\n\
        define ptr @caller(ptr %p) { %r = call noalias ptr @external(ptr noalias %p)\n ret ptr %r }\n",
    );
    assert!(module.decls[0].attrs.contract.parameters.is_empty());
    assert!(module.decls[0].attrs.contract.returns[0].1.return_noalias);
    assert_eq!(module.functions[0].restrict_params, [0, 1]);
    assert_eq!(module.functions[0].attrs.contract, Default::default());
    assert!(module.functions[1].restrict_params.is_empty());
    let site = module.functions[1]
        .layout_insts()
        .find_map(|(_, inst)| module.functions[1].call_contract(inst))
        .unwrap();
    assert!(site.parameters.is_empty());
    assert!(site.returns[0].1.return_noalias);
}

#[test]
fn capture_component_cross_product_roundtrips_without_strengthening() {
    use keel_ir::{
        AddressCapture as A, CaptureComponents, CaptureContract, PointerContract,
        ProvenanceCapture as P,
    };
    let Some(importer) = importer() else {
        return;
    };
    let components: Vec<_> = [A::None, A::Nullness, A::Address]
        .into_iter()
        .flat_map(|address| {
            [P::None, P::Read, P::ReadWrite]
                .into_iter()
                .map(move |provenance| CaptureComponents {
                    address,
                    provenance,
                })
        })
        .collect();
    for &other in &components {
        for &returned in &components {
            let mut module = keel_ir::parse("decl @f(ptr) -> ptr\n").unwrap();
            let capture = CaptureContract { other, returned };
            module.decls[0].attrs.contract.parameters.push((
                0,
                PointerContract {
                    capture,
                    ..Default::default()
                },
            ));
            let exported =
                keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
            let text = importer
                .translate(exported.as_bytes(), Default::default())
                .unwrap_or_else(|e| panic!("{capture:?}: {e}\n{exported}"));
            let roundtrip = keel_ir::parse(&text).unwrap();
            let actual = roundtrip.decls[0]
                .attrs
                .contract
                .parameters
                .first()
                .map(|(_, p)| p.capture)
                .unwrap_or_default();
            assert_eq!(actual, capture, "{exported}");
        }
    }
}

#[test]
fn direct_fixed_stack_lifetimes_survive_import_and_export() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "declare void @llvm.lifetime.start.p0(ptr)\n\
        declare void @llvm.lifetime.end.p0(ptr)\n\
        define i64 @stack(i64 %x) { %p = alloca i64\n\
        call void @llvm.lifetime.start.p0(ptr %p)\n\
        store i64 %x, ptr %p\n %y = load i64, ptr %p\n\
        call void @llvm.lifetime.end.p0(ptr %p)\n ret i64 %y\n}\n",
    );
    let text = module.to_string();
    assert!(
        text.contains("intrinsic lifetime_start(") && text.contains("intrinsic lifetime_end("),
        "{text}"
    );
    let llvm = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    assert!(
        llvm.contains("@llvm.lifetime.start.p0(i64 -1, ptr"),
        "{llvm}"
    );
    let again = i.translate(llvm.as_bytes(), Default::default()).unwrap();
    assert!(
        again.contains("intrinsic lifetime_start(") && again.contains("intrinsic lifetime_end("),
        "{again}"
    );
}

#[test]
fn readable_pointer_does_not_invent_nonnull_and_local_nocallback_is_not_assumed() {
    let module = keel_ir::parse("decl @readable(ptr) contract(param(0, readable 16))\n").unwrap();
    let llvm = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    assert!(
        llvm.contains("dereferenceable_or_null(16)") && !llvm.contains(" nonnull"),
        "{llvm}"
    );
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM");
        return;
    };
    let module = translated(
        &i,
        "define void @local() nocallback { ret void }\n\
        define void @caller(ptr %fn) { call void @local() nocallback\n call void %fn() nocallback\n ret void }\n",
    );
    assert!(!module.functions[0].attrs.contract.nocallback);
    let f = &module.functions[1];
    assert!(
        f.layout_insts()
            .all(|(_, inst)| f.call_contract(inst).is_none())
    );
}
