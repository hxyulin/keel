//! Memory/pointer contract transport and definedness boundaries.
use keel_ir::{InstData, MemoryAccess as A, MemorySummary};
use std::{path::PathBuf, process::Command};
const HEADER: &str = "target triple = \"arm64-apple-macosx11.0.0\"\ntarget datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128-Fn32\"\n";
fn importer() -> Option<keel_llvm::import::Importer> {
    let path = std::env::var_os("KEEL_TEST_LIBLLVM")?;
    // SAFETY: the test environment supplies the trusted LLVM 23 runtime.
    Some(unsafe { keel_llvm::import::Importer::load(&PathBuf::from(path)) }.unwrap())
}
fn translated(i: &keel_llvm::import::Importer, body: &str) -> keel_ir::Module {
    let text = i
        .translate(format!("{HEADER}{body}").as_bytes(), Default::default())
        .unwrap();
    keel_ir::parse(&text).unwrap()
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
    assert!(safe.parameters[0].1.nonnull && safe.parameters[0].1.no_capture);
    assert_eq!(safe.parameters[1].1.readable_bytes, 32);
    assert_eq!(safe.parameters[1].1.align, 8);
    assert!(safe.parameters[1].1.nonnull);
    assert!(module.decls[1].attrs.contract.parameters.is_empty());
    let result = &module.decls[2].attrs.contract.returns[0].1;
    assert_eq!(result.align, 8);
    assert!(result.nonnull && !result.no_capture);
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
    assert!(b.parameters[0].1.no_capture);
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
fn noalias_is_definition_only_and_does_not_invent_pointer_facts() {
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
    assert_eq!(module.decls[0].attrs.contract, Default::default());
    assert_eq!(module.functions[0].restrict_params, [0, 1]);
    assert_eq!(module.functions[0].attrs.contract, Default::default());
    assert!(module.functions[1].restrict_params.is_empty());
    assert!(
        module.functions[1]
            .layout_insts()
            .all(|(_, inst)| module.functions[1].call_contract(inst).is_none())
    );
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
