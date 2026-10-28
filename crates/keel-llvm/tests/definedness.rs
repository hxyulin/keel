//! Defined LLVM executions must not acquire traps or stronger pointer promises.
use keel_llvm::import::Importer;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn importer() -> Option<Importer> {
    let Some(path) = std::env::var_os("KEEL_TEST_LIBLLVM") else {
        eprintln!("skipped: set KEEL_TEST_LIBLLVM to the trusted LLVM 23 library");
        return None;
    };
    // SAFETY: the test environment explicitly supplies the trusted LLVM 23 runtime.
    Some(unsafe { Importer::load(Path::new(&path)) }.expect("load test LLVM 23"))
}

fn executable_host() -> bool {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        true
    } else {
        eprintln!("skipped execution: definedness fixtures require Apple arm64");
        false
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/definedness")
        .join(name)
}

fn run(command: &mut Command) -> Output {
    let output = command.output().expect("run fixture command");
    assert!(
        output.status.success(),
        "{command:?}: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn execute_llvm(dir: &Path, label: &str, source: &Path, driver: &Path, expected: &[u8]) {
    for opt in ["-O0", "-O2"] {
        let executable = dir.join(format!("{label}{opt}"));
        run(Command::new("clang")
            .args([
                "-target",
                "arm64-apple-macosx11.0.0",
                "-Wno-override-module",
                opt,
            ])
            .arg(source)
            .arg(driver)
            .arg("-o")
            .arg(&executable));
        assert_eq!(
            run(&mut Command::new(executable)).stdout,
            expected,
            "{label} {opt}"
        );
    }
}

#[test]
fn explicit_scalar_undefined_values_are_concrete_and_freeze_keeps_inputs() {
    let Some(importer) = importer() else {
        return;
    };
    let header = "target triple = \"arm64-apple-macosx11.0.0\"\ntarget datalayout = \"e-m:o-i64:64-i128:128-n32:64-S128-Fn32\"\n";
    for ty in ["i1", "i8", "i16", "i32", "i64", "float", "double", "ptr"] {
        for value in ["poison", "undef"] {
            let ret = if matches!(ty, "i1" | "i8" | "i16") {
                format!("zeroext {ty}")
            } else {
                ty.into()
            };
            let param = if matches!(ty, "i1" | "i8" | "i16") {
                format!("{ty} zeroext")
            } else {
                ty.into()
            };
            let source = format!(
                "{header}define {ret} @raw() {{ ret {ty} {value} }}\n\
                 define {ret} @frozen() {{ %r = freeze {ty} {value}\nret {ty} %r }}\n\
                 define {ret} @identity({param} %x) {{ %r = freeze {ty} %x\nret {ty} %r }}\n"
            );
            let text = importer
                .translate(source.as_bytes(), Default::default())
                .unwrap_or_else(|e| panic!("{ty} {value}: {e}"));
            let module = keel_ir::parse(&text).unwrap();
            keel_ir::verify_module(&module).unwrap();
            assert!(!text.contains("trap"), "{ty} {value}: {text}");
            let identity = &module.functions[2];
            let entry = identity.entry_block().unwrap();
            let parameter = identity.block_params(entry)[0];
            let last = *identity.block_insts(entry).last().unwrap();
            let keel_ir::InstData::Return { args } = identity.inst_data(last) else {
                panic!("freeze identity must return its concrete input: {text}");
            };
            assert_eq!(identity.resolve(args[0]), parameter, "{ty}: {text}");
        }
    }
    for body in [
        "define <2 x i64> @f() { %r = freeze <2 x i64> poison\nret <2 x i64> %r }",
        "define [2 x i64] @f() { %r = freeze [2 x i64] undef\nret [2 x i64] %r }",
        "@g = global i64 undef",
    ] {
        assert!(
            importer
                .translate(format!("{header}{body}").as_bytes(), Default::default())
                .is_err(),
            "scalar refinement must not widen aggregate/global support: {body}"
        );
    }
}

#[test]
fn scalar_refinement_matches_allowed_outcomes_and_freeze_stability() {
    let Some(importer) = importer() else {
        return;
    };
    let text = importer
        .translate(
            include_bytes!("fixtures/definedness/refinement.ll"),
            Default::default(),
        )
        .unwrap();
    let module = keel_ir::parse(&text).unwrap();
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    assert!(!exported.contains("freeze"));
    if !executable_host() {
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "keel-definedness-refinement-{}",
        std::process::id()
    ));
    fs::create_dir(&dir).unwrap();
    let roundtrip = dir.join("roundtrip.ll");
    fs::write(&roundtrip, exported).unwrap();
    let driver = fixture("refinement_driver.c");
    execute_llvm(
        &dir,
        "source",
        &fixture("refinement.ll"),
        &driver,
        b"refinement-ok\n",
    );
    execute_llvm(&dir, "roundtrip", &roundtrip, &driver, b"refinement-ok\n");
    if let Some(keel) = std::env::var_os("KEEL_TEST_KEEL") {
        let imported = dir.join("refinement.keel");
        fs::write(&imported, text).unwrap();
        for tier in ["none", "fast", "full"] {
            let object = dir.join(format!("{tier}.o"));
            run(Command::new(&keel)
                .args(["compile", "--opt-tier", tier, "--emit-object"])
                .arg(&imported)
                .arg("-o")
                .arg(&object));
            let executable = dir.join(tier);
            run(Command::new("clang")
                .arg(&driver)
                .arg(object)
                .arg("-o")
                .arg(&executable));
            assert_eq!(
                run(&mut Command::new(executable)).stdout,
                b"refinement-ok\n",
                "native {tier}"
            );
        }
    } else {
        eprintln!("skipped native refinement execution: set KEEL_TEST_KEEL");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn masked_poison_selects_and_phi_edges_match_defined_c_oracle() {
    let Some(importer) = importer() else {
        return;
    };
    let text = importer
        .translate(
            include_bytes!("fixtures/definedness/masked.ll"),
            Default::default(),
        )
        .unwrap();
    let module = keel_ir::parse(&text).unwrap();
    let exported = keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).unwrap();
    assert!(
        !exported.contains(" nonnull"),
        "masked nonnull is not an unconditional pointer fact: {exported}"
    );
    assert!(
        !exported.contains("add nsw"),
        "masked overflow must not become a no-overflow promise"
    );
    if !executable_host() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("keel-definedness-masked-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let roundtrip = dir.join("roundtrip.ll");
    fs::write(&roundtrip, exported).unwrap();
    let driver = fixture("masked_driver.c");
    execute_llvm(
        &dir,
        "source",
        &fixture("masked.ll"),
        &driver,
        b"definedness-ok\n",
    );
    execute_llvm(&dir, "roundtrip", &roundtrip, &driver, b"definedness-ok\n");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn masked_dividend_refinement_totalizes_signed_overflow() {
    let Some(importer) = importer() else {
        return;
    };
    let text = importer
        .translate(
            include_bytes!("fixtures/definedness/division.ll"),
            Default::default(),
        )
        .unwrap();
    let module = keel_ir::parse(&text).unwrap();
    let function = &module.functions[0];
    let divisor = function
        .layout_insts()
        .find_map(|(_, inst)| match function.inst_data(inst) {
            keel_ir::InstData::Binary {
                op: keel_ir::BinaryOp::Sdiv,
                args,
            } => Some(args[1]),
            _ => None,
        })
        .expect("fixture retains signed division");
    let keel_ir::ValueDef::Result(select, _) = function.value_def(divisor) else {
        panic!("divisor must come from overflow totalization");
    };
    let keel_ir::InstData::Select { args } = function.inst_data(select) else {
        panic!("overflow pair must select a safe divisor");
    };
    let keel_ir::ValueDef::Result(one, _) = function.value_def(args[1]) else {
        panic!("safe divisor must be a constant");
    };
    assert!(
        matches!(
            function.inst_data(one),
            keel_ir::InstData::Iconst { imm: 1, .. }
        ),
        "overflow pair must use divisor one"
    );
    assert!(
        keel_llvm::export(&module, keel_llvm::TargetProfile::AppleArm64).is_err(),
        "LLVM division export remains outside the supported subset"
    );
    if !executable_host() {
        return;
    }
    let Some(keel) = std::env::var_os("KEEL_TEST_KEEL") else {
        eprintln!(
            "skipped native division execution: set KEEL_TEST_KEEL to a built Keel executable"
        );
        return;
    };
    let dir =
        std::env::temp_dir().join(format!("keel-definedness-division-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let imported = dir.join("division.keel");
    fs::write(&imported, text).unwrap();
    let driver = fixture("division_driver.c");
    execute_llvm(
        &dir,
        "source",
        &fixture("division.ll"),
        &driver,
        b"division-definedness-ok\n",
    );
    for tier in ["none", "fast", "full"] {
        let object = dir.join(format!("{tier}.o"));
        run(Command::new(&keel)
            .args(["compile", "--opt-tier", tier, "--emit-object"])
            .arg(&imported)
            .arg("-o")
            .arg(&object));
        let executable = dir.join(tier);
        run(Command::new("clang")
            .arg(&driver)
            .arg(object)
            .arg("-o")
            .arg(&executable));
        assert_eq!(
            run(&mut Command::new(executable)).stdout,
            b"division-definedness-ok\n",
            "native {tier}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
