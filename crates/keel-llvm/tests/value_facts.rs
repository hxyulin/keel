//! LLVM load-result metadata, including poison masking and scope isolation.
use keel_ir::{InstData, ValueFacts};
use std::path::Path;
const HEADER: &str = "target triple = \"arm64-apple-macosx11.0.0\"\ntarget datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128-Fn32\"\n";
fn importer() -> Option<keel_llvm::import::Importer> {
    let Some(path) = std::env::var_os("KEEL_TEST_LIBLLVM") else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to the trusted LLVM 23 runtime");
        return None;
    };
    // SAFETY: the test environment explicitly selects the trusted LLVM 23 runtime.
    Some(unsafe { keel_llvm::import::Importer::load(Path::new(&path)) }.unwrap())
}
fn translated(importer: &keel_llvm::import::Importer, body: &str) -> keel_ir::Module {
    let text = importer
        .translate(format!("{HEADER}{body}").as_bytes(), Default::default())
        .unwrap();
    keel_ir::parse(&text).unwrap()
}
fn load_facts(module: &keel_ir::Module) -> Vec<(String, Option<ValueFacts>)> {
    module
        .functions
        .iter()
        .flat_map(|f| {
            f.layout_insts()
                .filter(|&(_, i)| matches!(f.inst_data(i), InstData::Load { .. }))
                .map(|(_, i)| (f.name.clone(), f.inst_facts(i).copied()))
        })
        .collect()
}

#[test]
fn defined_load_result_facts_roundtrip_without_becoming_access_flags() {
    let Some(importer) = importer() else {
        return;
    };
    let module = translated(
        &importer,
        "define i64 @integer(ptr %p) {\n\
        %v = load i64, ptr %p, align 8, !range !0, !noundef !1\nret i64 %v\n}\n\
        define ptr @pointer(ptr %p) {\n\
        %v = load ptr, ptr %p, align 8, !align !2, !nonnull !1, !dereferenceable !3, !dereferenceable_or_null !4, !noundef !1\nret ptr %v\n}\n\
        define ptr @nullable(ptr %p) {\n\
        %v = load ptr, ptr %p, align 8, !dereferenceable_or_null !3, !noundef !1\nret ptr %v\n}\n\
        !0 = !{i64 -8, i64 8}\n!1 = !{}\n!2 = !{i64 16}\n!3 = !{i64 32}\n!4 = !{i64 64}\n",
    );
    let facts = load_facts(&module);
    assert_eq!(
        facts[0].1.unwrap().range,
        Some(keel_ir::IntegerRange {
            lower: (-8i64) as u64,
            upper: 8
        })
    );
    let pointer = facts[1].1.unwrap();
    assert_eq!(pointer.align, 16);
    assert!(pointer.nonnull);
    assert_eq!(pointer.readable_bytes, 32);
    assert_eq!(pointer.readable_or_null_bytes, 64);
    assert!(!facts[2].1.unwrap().nonnull);
    assert_eq!(facts[2].1.unwrap().readable_bytes, 0);
    for function in &module.functions {
        assert!(function.attrs.contract.is_unknown());
        for (_, inst) in function.layout_insts() {
            if let InstData::Load { flags, .. } = function.inst_data(inst) {
                assert_eq!(flags.align, 8);
                assert!(!flags.notrap);
            }
        }
    }
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    assert!(
        exported.contains("!noundef") && exported.contains("!range") && exported.contains("!align")
    );
    let text = importer
        .translate(exported.as_bytes(), Default::default())
        .unwrap();
    assert_eq!(load_facts(&keel_ir::parse(&text).unwrap()), facts);
}

#[test]
fn poisonable_metadata_and_union_ranges_are_not_promoted() {
    let Some(importer) = importer() else {
        return;
    };
    let module = translated(
        &importer,
        "define i64 @masked(ptr %p) {\n\
        %v = load i64, ptr %p, !range !0\n%r = select i1 false, i64 %v, i64 73\nret i64 %r\n}\n\
        define ptr @masked_pointer(ptr %p) {\n\
        %v = load ptr, ptr %p, !nonnull !1, !align !2\n%r = select i1 false, ptr %v, ptr null\nret ptr %r\n}\n\
        define i64 @union_range(ptr %p) {\n\
        %v = load i64, ptr %p, !range !3, !noundef !1\nret i64 %v\n}\n\
        !0 = !{i64 0, i64 8}\n!1 = !{}\n!2 = !{i64 16}\n!3 = !{i64 0, i64 4, i64 8, i64 12}\n",
    );
    assert!(load_facts(&module).iter().all(|(_, facts)| facts.is_none()));
}

#[test]
fn load_ranges_keep_bit_width_and_i1_facts_do_not_annotate_raw_bytes() {
    let Some(importer) = importer() else {
        return;
    };
    for width in [8, 16, 32, 64] {
        let ext = if width < 32 { "zeroext " } else { "" };
        let source = format!(
            "define {ext}i{width} @f(ptr %cell) {{\n %v = load i{width}, ptr %cell, !range !0, !noundef !1\nret i{width} %v\n}}\n!0 = !{{i{width} -8, i{width} 8}}\n!1 = !{{}}\n"
        );
        let module = translated(&importer, &source);
        let facts = load_facts(&module);
        assert_eq!(
            facts[0].1.unwrap().range.unwrap().lower,
            (-8i64 as u64) & (u64::MAX >> (64 - width))
        );
        let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
        let text = importer
            .translate(exported.as_bytes(), Default::default())
            .unwrap();
        assert_eq!(load_facts(&keel_ir::parse(&text).unwrap()), facts);
    }
    let module = translated(
        &importer,
        "define i64 @f(ptr %p) { %v = load i1, ptr %p, !range !0, !noundef !1\n%r = zext i1 %v to i64\nret i64 %r\n}\n!0 = !{i1 false, i1 true}\n!1 = !{}\n",
    );
    assert!(load_facts(&module).iter().all(|(_, facts)| facts.is_none()));
}

#[test]
fn facts_do_not_transfer_to_sibling_loads_or_later_memory_accesses() {
    let Some(importer) = importer() else {
        return;
    };
    let module = translated(
        &importer,
        "declare void @unknown()\n\
        define i64 @f(ptr %cell) {\n\
        %p = load ptr, ptr %cell, !dereferenceable !0, !noundef !1\n\
        %q = load ptr, ptr %cell\ncall void @unknown()\n%v = load i64, ptr %p\nret i64 %v\n}\n\
        !0 = !{i64 8}\n!1 = !{}\n",
    );
    let facts = load_facts(&module);
    assert_eq!(facts[0].1.unwrap().readable_bytes, 8);
    assert!(facts[1].1.is_none() && facts[2].1.is_none());
    let function = &module.functions[0];
    assert!(function.attrs.contract.is_unknown());
    for (_, inst) in function.layout_insts() {
        if let InstData::Load { flags, .. } = function.inst_data(inst) {
            assert!(!flags.notrap);
        }
    }
}

#[test]
fn unsupported_instruction_facts_are_diagnosed_and_nullable_extents_are_not_duplicated() {
    let module = keel_ir::parse("func @f(i64) -> i64 {\nblock0(v0: i64):\n v1 = iadd v0, v0 facts(range(0, 8))\n return v1\n}\n").unwrap();
    assert!(
        keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64)
            .unwrap_err()
            .contains("only on scalar loads")
    );
    let Some(importer) = importer() else {
        return;
    };
    let module = keel_ir::parse("func @f(ptr) -> ptr {\nblock0(v0: ptr):\n v1 = load.ptr v0 facts(readable 8, readable_or_null 16)\n return v1\n}\n").unwrap();
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    let text = importer
        .translate(exported.as_bytes(), Default::default())
        .unwrap();
    let facts = load_facts(&keel_ir::parse(&text).unwrap())[0].1.unwrap();
    assert_eq!(facts.readable_or_null_bytes, 16);
    assert_eq!(facts.readable_bytes, 0);
    assert!(!facts.nonnull);
}
