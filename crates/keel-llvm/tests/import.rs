//! Characterization tests for the Python LLVM importer contract.
use std::{path::PathBuf, process::Command};
const HEADER: &str = "target triple = \"arm64-apple-macosx11.0.0\"\ntarget datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128-Fn32\"\n";
fn importer() -> Option<keel_llvm::import::Importer> {
    let path = std::env::var_os("KEEL_TEST_LIBLLVM")?;
    // SAFETY: the test environment supplies the trusted LLVM 23 runtime.
    Some(
        unsafe { keel_llvm::import::Importer::load(&PathBuf::from(path)) }
            .expect("load trusted test LLVM"),
    )
}
#[test]
fn scalar_flags_and_boolean_refinement() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let src =
        format!("{HEADER}define i64 @f(i64 %a, i64 %b) {{ %r = add nsw i64 %a, %b\nret i64 %r }}");
    let text = i.translate(src.as_bytes(), Default::default()).unwrap();
    assert!(text.contains("iadd"));
    assert!(!text.contains("nsw"));
    for body in [
        "define i64 @f() { ret i64 poison }",
        "define i64 @f() { %r = freeze i64 poison\nret i64 %r }",
        "define void @f(i64 %n) { %p = alloca i64, i64 %n\nret void }",
    ] {
        assert!(
            i.translate(format!("{HEADER}{body}").as_bytes(), Default::default())
                .is_err(),
            "{body}"
        );
    }
}
#[test]
fn fixture_output_matches_python_reference() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = [
        "crates/keel-cranelift/tools/ir-fixtures/scalar.ll",
        "suite/llvm_import_linkage_fixtures/available.ll",
    ];
    for path in paths {
        let source = std::fs::read(root.join(path)).unwrap();
        for localize in [false, true] {
            let script = "import sys;sys.path.insert(0,sys.argv[1]);from llvm_import import LLVM,import_module;print(import_module(LLVM(sys.argv[2]),sys.stdin.buffer.read(),localize_odr=sys.argv[3]=='true'),end='')";
            let mut child = Command::new("python3")
                .args(["-c", script])
                .arg(root.join("suite"))
                .arg(std::env::var_os("KEEL_TEST_LIBLLVM").unwrap())
                .arg(localize.to_string())
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(&source).unwrap();
            }
            let python = child.wait_with_output().unwrap();
            let rust = i.translate(
                &source,
                keel_llvm::import::Options {
                    localize_odr_functions: localize,
                },
            );
            assert_eq!(
                rust.is_ok(),
                python.status.success(),
                "{path} localize={localize}: Rust {rust:?}; Python {}",
                String::from_utf8_lossy(&python.stderr)
            );
            if let Ok(text) = rust {
                let a = keel_ir::parse(&text).unwrap().to_string();
                let b = keel_ir::parse(&String::from_utf8(python.stdout).unwrap())
                    .unwrap()
                    .to_string();
                assert_eq!(a, b, "{path}");
            }
        }
    }
}

#[test]
fn memory_alignment_and_volatility_survive_import() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let source = format!(
        "{HEADER}define i32 @aligned(ptr %p) {{\n\
         %a = load i32, ptr %p, align 32\n\
         store volatile i32 %a, ptr %p, align 16\n\
         %b = load volatile i32, ptr %p, align 1\n\
         store i32 %b, ptr %p, align 1\n\
         %c = load i32, ptr %p\n\
         ret i32 %c\n}}"
    );
    let text = i.translate(source.as_bytes(), Default::default()).unwrap();
    let module = keel_ir::parse(&text).unwrap();
    let f = &module.functions[0];
    let flags = f
        .layout_insts()
        .filter_map(|(_, inst)| match f.inst_data(inst) {
            keel_ir::InstData::Load { flags, .. } | keel_ir::InstData::Store { flags, .. } => {
                Some((flags.align, flags.volatile, flags.notrap))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        flags,
        [
            (32, false, false),
            (16, true, false),
            (1, true, false),
            (1, false, false),
            (4, false, false)
        ],
        "explicit alignment, LLVM's ABI-default alignment and volatility must survive independently"
    );
    let llvm = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    assert!(llvm.contains("load i32, ptr %v0, align 32"), "{llvm}");
    assert!(llvm.contains("store volatile i32"), "{llvm}");
    assert!(llvm.contains("align 16"), "{llvm}");
}

#[test]
fn function_and_declaration_attributes_survive_import() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let source = format!(
        "{HEADER}\
         declare void @external() noinline nounwind cold\n\
         define i32 @preferred() #0 {{ ret i32 3 }}\n\
         define void @never() noreturn nounwind {{ unreachable }}\n\
         define i32 @plain() {{ ret i32 4 }}\n\
         define i32 @unoptimized(i32 %x) noinline optnone {{ ret i32 %x }}\n\
         attributes #0 = {{ alwaysinline inlinehint nounwind hot }}\n"
    );
    let text = i.translate(source.as_bytes(), Default::default()).unwrap();
    let module = keel_ir::parse(&text).unwrap();
    let external = &module
        .decls
        .iter()
        .find(|d| d.name == "external")
        .unwrap()
        .attrs;
    assert!(
        external.noinline && external.nounwind && external.cold,
        "{external:?}"
    );
    let preferred = &module
        .functions
        .iter()
        .find(|f| f.name == "preferred")
        .unwrap()
        .attrs;
    assert!(
        preferred.alwaysinline && preferred.inlinehint && preferred.nounwind && preferred.hot,
        "{preferred:?}"
    );
    let never = &module
        .functions
        .iter()
        .find(|f| f.name == "never")
        .unwrap()
        .attrs;
    assert!(never.noreturn && never.nounwind, "{never:?}");
    let plain = &module
        .functions
        .iter()
        .find(|f| f.name == "plain")
        .unwrap()
        .attrs;
    assert_eq!(*plain, keel_ir::FunctionAttrs::default());
    let unoptimized = &module
        .functions
        .iter()
        .find(|f| f.name == "unoptimized")
        .unwrap()
        .attrs;
    assert!(
        unoptimized.optnone && unoptimized.noinline,
        "{unoptimized:?}"
    );
}

#[test]
fn call_site_attributes_do_not_become_callee_promises() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let source = format!(
        "{HEADER}declare void @callee()\n\
         define void @caller() {{ call void @callee() nounwind\n ret void }}"
    );
    let text = i.translate(source.as_bytes(), Default::default()).unwrap();
    let module = keel_ir::parse(&text).unwrap();
    let callee = module.decls.iter().find(|d| d.name == "callee").unwrap();
    assert!(
        !callee.attrs.nounwind,
        "call-site nounwind must not strengthen other calls"
    );
    assert!(
        !module.functions[0].attrs.nounwind,
        "call-site nounwind must not become a caller promise"
    );
}

#[test]
fn contradictory_frequency_hints_do_not_reject_valid_llvm() {
    let Some(i) = importer() else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to LLVM 23");
        return;
    };
    let source = format!("{HEADER}define void @mixed() cold hot {{ ret void }}");
    let text = i.translate(source.as_bytes(), Default::default()).unwrap();
    let module = keel_ir::parse(&text).unwrap();
    assert!(!module.functions[0].attrs.cold && !module.functions[0].attrs.hot);
}
