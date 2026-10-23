//! Executable differential tests: the same Keel IR is compiled by the native
//! backend (baseline and full selection) and by Cranelift (none and speed),
//! linked with the same Clang-compiled C driver and run on this host.
mod common;
use common::*;
use std::os::unix::process::ExitStatusExt;
use std::process::Command;

/// Run every configuration of `module` with `driver`; all outputs must agree.
fn agree(
    name: &str,
    module: &keel_ir::Module,
    driver: &str,
    cflags: &[&str],
    args: &[&str],
) -> Vec<u8> {
    let dir = workdir(name);
    let mut reference: Option<(String, Vec<u8>)> = None;
    for config in CONFIGS {
        let obj = write(&dir, &format!("{config}.o"), &object(module, config));
        let exe = link(&dir, config, &fixture(driver), cflags, &[&obj]);
        let out = run(Command::new(&exe).args(args)).stdout;
        assert!(!out.is_empty());
        match &reference {
            None => reference = Some((config.into(), out)),
            Some((first, expected)) => assert!(
                *expected == out,
                "{name}: {config} output differs from {first}; see {}",
                dir.display()
            ),
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
    reference.unwrap().1
}

#[test]
fn keel_llvm_core_fixture() {
    if !host_ok() {
        return;
    }
    let module = parse(&fixture("../keel-llvm/tests/fixtures/core.keel"));
    let out = agree(
        "core",
        &module,
        "../keel-llvm/tests/fixtures/driver.c",
        &["-O2"],
        &[],
    );
    assert_eq!(out.iter().filter(|&&b| b == b'\n').count(), 1448);
}

#[test]
fn bridge_fixture_and_traps() {
    if !host_ok() {
        return;
    }
    let module = parse(&fixture("tests/fixtures/bridge.keel"));
    let out = agree(
        "bridge",
        &module,
        "tests/fixtures/bridge_driver.c",
        &["-O1"],
        &[],
    );
    assert!(out.iter().filter(|&&b| b == b'\n').count() > 10_000);

    // Division by zero, sdiv MIN/-1 (64- and 8-bit) and `trap` must stop
    // the program. Keel uses `brk` (SIGTRAP), Cranelift `udf` (SIGILL).
    let dir = workdir("bridge-traps");
    for config in CONFIGS {
        let obj = write(&dir, &format!("{config}.o"), &object(&module, config));
        let exe = link(
            &dir,
            config,
            &fixture("tests/fixtures/bridge_driver.c"),
            &["-O1"],
            &[&obj],
        );
        for case in [
            "sdiv0", "udiv0", "srem0", "urem0", "sdivmin", "sdiv8min", "trap",
        ] {
            let status = Command::new(&exe)
                .args(["trap", case])
                .output()
                .unwrap()
                .status;
            let signal = status.signal();
            assert!(signal.is_some(), "{config} {case}: {status}");
            if config.starts_with("clif") {
                assert_eq!(signal, Some(libc_sigill()), "{config} {case}");
            }
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

fn libc_sigill() -> i32 {
    4
}

/// Split a module: Cranelift compiles the supported functions, the native
/// backend the rest. Linking both exercises calls across the two backends.
fn split(m: &keel_ir::Module, native_names: &[&str]) -> (keel_ir::Module, keel_ir::Module) {
    let mut clif = m.clone();
    clif.functions
        .retain(|f| !native_names.contains(&f.name.as_str()));
    let mut native = keel_ir::Module {
        decls: m.decls.clone(),
        ..Default::default()
    };
    for f in m
        .functions
        .iter()
        .filter(|f| native_names.contains(&f.name.as_str()))
    {
        assert!(!f.local, "cannot split local @{}", f.name);
        clif.decls.push(keel_ir::FuncDecl {
            attrs: f.attrs,
            name: f.name.clone(),
            sig: f.sig.clone(),
        });
        for (_, callee) in f.callees() {
            if let Some(def) = m.functions.iter().find(|g| g.name == callee.name) {
                assert!(!def.local, "@{} needs local @{}", f.name, def.name);
                if !native.decls.iter().any(|d| d.name == def.name) {
                    native.decls.push(callee.clone());
                }
            }
        }
        for (_, data) in f.data_symbols() {
            let object = m.data_object(data).unwrap();
            assert!(!object.local, "@{} needs local data @{data}", f.name);
            if !native.data.iter().any(|d| d.name == data) {
                native.data.push(keel_ir::DataObject {
                    name: data.into(),
                    thread_local: false,
                    local: false,
                    weak: false,
                    weak_odr: false,
                    hidden: false,
                    retain: false,
                    section: None,
                    init: None,
                });
            }
        }
        native.functions.push(f.clone());
    }
    keel_ir::verify_module(&clif).unwrap();
    keel_ir::verify_module(&native).unwrap();
    (clif, native)
}

#[test]
fn keel_llvm_extended_fixture_mixed_objects() {
    if !host_ok() {
        return;
    }
    let module = parse(&fixture("../keel-llvm/tests/fixtures/extended.keel"));
    let unsupported: Vec<String> = keel_cranelift::function_support(&module)
        .into_iter()
        .filter_map(|(name, r)| r.err().map(|_| name))
        .collect();
    // Dynamic stack allocation is the fixture's only unsupported feature.
    assert_eq!(unsupported, ["dynamic"]);
    let names: Vec<&str> = unsupported.iter().map(String::as_str).collect();
    let (clif_part, native_part) = split(&module, &names);
    let dir = workdir("extended");
    let driver = fixture("../keel-llvm/tests/fixtures/extended_driver.c");
    let reference = {
        let obj = write(&dir, "native.o", &native(&module, false));
        let exe = link(&dir, "native", &driver, &["-O2"], &[&obj]);
        run(&mut Command::new(exe)).stdout
    };
    assert!(!reference.is_empty());
    let rest = write(&dir, "rest.o", &native(&native_part, false));
    for opt in ["clif-none", "clif-speed"] {
        let obj = write(&dir, &format!("{opt}.o"), &object(&clif_part, opt));
        let exe = link(&dir, opt, &driver, &["-O2"], &[&obj, &rest]);
        let out = run(&mut Command::new(exe)).stdout;
        assert!(
            out == reference,
            "{opt} mixed output differs; see {}",
            dir.display()
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// kernels.c through the C frontend, unoptimized and with Keel's -O IR
/// pipeline, against independent Clang -O0 and -O2 builds of the same C.
#[test]
fn c_kernels_raw_and_optimized_ir() {
    if !host_ok() {
        return;
    }
    let path = fixture("kernels/kernels.c");
    let src = std::fs::read_to_string(&path).unwrap();
    let target = keel_ir::Target::Aarch64AppleDarwin;
    let pre = keel_c::preprocess(
        &src,
        path.to_str().unwrap(),
        &keel_c::PreprocessOptions {
            include_dirs: vec![],
            system_include_dirs: vec![],
            sysroot: None,
            darwin_deployment: None,
            defines: vec![],
            target,
            dump_macros: false,
        },
    )
    .unwrap();
    let raw = keel_c::compile(
        &pre,
        path.to_str().unwrap(),
        keel_c::Options {
            debug_info: false,
            target,
        },
    )
    .unwrap();
    keel_ir::verify_module(&raw).unwrap();
    let mut optimized = raw.clone();
    // Exactly what `keel cc -O --target aarch64-apple-darwin` runs.
    keel_opt::run_passes_for_target(
        &mut optimized,
        keel_opt::DEFAULT_PIPELINE,
        keel_opt::VectorTarget::Aarch64Neon,
    )
    .unwrap();

    let dir = workdir("kernels");
    let driver_c = fixture("kernels/driver.c");
    let driver = dir.join("driver.o");
    run(Command::new("clang")
        .args(["-O2", "-ffp-contract=off", "-c"])
        .arg(&driver_c)
        .arg("-o")
        .arg(&driver));
    let check = |exe: &std::path::Path| run(Command::new(exe).arg("check")).stdout;
    let mut oracle = None;
    for level in ["-O0", "-O2"] {
        let obj = dir.join(format!("clang{level}.o"));
        run(Command::new("clang")
            .args([level, "-ffp-contract=off", "-c"])
            .arg(&path)
            .arg("-o")
            .arg(&obj));
        let exe = dir.join(format!("clang{level}"));
        run(Command::new("clang")
            .arg(&driver)
            .arg(&obj)
            .arg("-o")
            .arg(&exe));
        let out = check(&exe);
        match &oracle {
            None => oracle = Some(out),
            Some(o) => assert!(*o == out, "clang -O0 and -O2 disagree"),
        }
    }
    let oracle = oracle.unwrap();
    assert!(oracle.iter().filter(|&&b| b == b'\n').count() > 700);
    for (ir, module) in [("raw", &raw), ("O", &optimized)] {
        for config in CONFIGS {
            let name = format!("{ir}-{config}");
            let obj = write(&dir, &format!("{name}.o"), &object(module, config));
            let exe = dir.join(&name);
            run(Command::new("clang")
                .arg(&driver)
                .arg(&obj)
                .arg("-o")
                .arg(&exe));
            assert!(
                check(&exe) == oracle,
                "{name} disagrees with Clang; see {}",
                dir.display()
            );
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
