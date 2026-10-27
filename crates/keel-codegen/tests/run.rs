//! End-to-end tests: compile each `tests/programs/*.keel` to assembly, link
//! it with `clang` (plus a same-named `.c` driver if there is one, or a
//! `.cpp` one, built with `clang++ -O2`), run it,
//! and compare with the `; expect:` line, either `exit <code>` or `trap`.
//! `; stdout: <text>` lines, if present, give the expected output line by
//! line. A `; target: <triple>` line runs the program only on that target,
//! for what the IR leaves to the ABI.
//!
//! Every program is compiled with selection disabled and enabled, each
//! with the real register set; with
//! two registers of each kind (and class), which spills most values (running the spill
//! code and register/stack mixes in parallel copies on every program); and
//! after the default optimization pipeline, which must not change what the
//! program does.
//!
//! `debug_info` checks the debug info in lldb.

#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
use keel_codegen::aarch64::{REGISTERS, compile_linux_with_pipeline as compile_with_pipeline};
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
use keel_codegen::aarch64::{REGISTERS, compile_with_pipeline};
#[cfg(target_arch = "x86_64")]
use keel_codegen::x86_64::{REGISTERS, compile_with_pipeline};
use keel_codegen::{Pools, regalloc::AllocationTier};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// SIGTRAP, raised by `brk`.
const SIGTRAP: i32 = 5;
/// SIGILL, raised by x86's `ud2`.
const SIGILL: i32 = 4;

/// Two registers of each kind, for the "-tiny" configuration.
#[cfg(target_arch = "aarch64")]
const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};
#[cfg(target_arch = "x86_64")]
const TINY: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3, 12],
    float_caller_saved: &[8, 9],
    float_callee_saved: &[],
};

/// Register sets to compile with, whether to optimize first, and a suffix
/// for the output files.
const CONFIGS: [(
    Pools,
    bool,
    &str,
    keel_codegen::SelectionOptions,
    AllocationTier,
); 8] = [
    (
        REGISTERS,
        false,
        "",
        keel_codegen::SelectionOptions {
            trees: false,
            folded_operands: false,
            register_hints: false,
        },
        AllocationTier::Full,
    ),
    (
        TINY,
        false,
        "-tiny",
        keel_codegen::SelectionOptions {
            trees: false,
            folded_operands: false,
            register_hints: false,
        },
        AllocationTier::Full,
    ),
    (
        REGISTERS,
        true,
        "-opt",
        keel_codegen::SelectionOptions {
            trees: false,
            folded_operands: false,
            register_hints: false,
        },
        AllocationTier::Full,
    ),
    (
        REGISTERS,
        false,
        "-select",
        keel_codegen::SelectionOptions::ALL,
        AllocationTier::Full,
    ),
    (
        TINY,
        false,
        "-select-tiny",
        keel_codegen::SelectionOptions::ALL,
        AllocationTier::Full,
    ),
    (
        REGISTERS,
        true,
        "-select-opt",
        keel_codegen::SelectionOptions::ALL,
        AllocationTier::Full,
    ),
    (
        REGISTERS,
        false,
        "-fast",
        keel_codegen::SelectionOptions {
            trees: false,
            folded_operands: false,
            register_hints: false,
        },
        AllocationTier::Fast,
    ),
    (
        TINY,
        true,
        "-fast-select-tiny",
        keel_codegen::SelectionOptions::ALL,
        AllocationTier::Fast,
    ),
];

fn run_program(
    path: &Path,
    pools: &Pools,
    optimize: bool,
    suffix: &str,
    selection: keel_codegen::SelectionOptions,
    allocation: &dyn keel_codegen::regalloc::RegisterAllocator,
) -> Result<(), String> {
    let src = std::fs::read_to_string(path).unwrap();
    let expect = src
        .lines()
        .find_map(|l| l.strip_prefix("; expect: "))
        .ok_or("missing `; expect:` line")?;

    let stdout_lines: Vec<&str> = src
        .lines()
        .filter_map(|l| l.strip_prefix("; stdout:"))
        .map(|l| l.strip_prefix(' ').unwrap_or(l))
        .collect();

    if let Some(target) = src.lines().find_map(|l| l.strip_prefix("; target: "))
        && Some(target) != keel_codegen::Target::host().map(|t| t.triple())
    {
        return Ok(());
    }
    let mut module = keel_ir::parse(&src).map_err(|e| format!("parse error: {e}"))?;
    keel_ir::verify_module(&module).map_err(|e| format!("verifier: {e:?}"))?;
    if optimize {
        if let Some(names) = src
            .lines()
            .find_map(|line| line.strip_prefix("; opt-passes:"))
        {
            let names: Vec<_> = names.split(',').map(str::trim).collect();
            keel_opt::run_passes(&mut module, &names).map_err(|e| e.to_string())?;
        } else {
            keel_opt::optimize(&mut module).map_err(|e| e.to_string())?;
        }
    }
    let selector = keel_codegen::OptionsSelector(selection);
    let features = std::env::var("KEEL_TEST_X86_FEATURES")
        .ok()
        .map(|s| keel_asm_ir::x86::Features::parse(&s))
        .transpose()?
        .unwrap_or_default();
    #[cfg(target_arch = "x86_64")]
    if (features.avx && !std::is_x86_feature_detected!("avx"))
        || (features.bmi1 && !std::is_x86_feature_detected!("bmi1"))
        || (features.bmi2 && !std::is_x86_feature_detected!("bmi2"))
    {
        return Err("requested test x86 ISA is unavailable on this host".into());
    }
    let selector = keel_codegen::x86_features::FeatureSelector {
        inner: &selector,
        features,
    };
    let asm = compile_with_pipeline(
        &module,
        pools,
        &keel_codegen::CodegenPipeline {
            selector: &selector,
            allocator: allocation,
        },
    )
    .map_err(|e| e.to_string())?;

    let stem = format!("{}{suffix}", path.file_stem().unwrap().to_string_lossy());
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs");
    std::fs::create_dir_all(&dir).unwrap();
    let asm_path = dir.join(format!("{stem}.s"));
    let exe_path = dir.join(&stem);
    std::fs::write(&asm_path, &asm).unwrap();
    let link_input = if std::env::var("KEEL_TEST_ASSEMBLER").as_deref() == Ok("native") {
        if keel_codegen::Target::host() != Some(keel_codegen::Target::X86_64LinuxGnu) {
            return Err("KEEL_TEST_ASSEMBLER=native currently requires x86-64 Linux".into());
        }
        let object = keel_asm::x86::assemble_object_with_features(&asm, features)
            .map_err(|e| e.to_string())?;
        let object_path = dir.join(format!("{stem}.o"));
        std::fs::write(&object_path, object).map_err(|e| e.to_string())?;
        object_path
    } else {
        asm_path.clone()
    };

    let cpp_driver = path.with_extension("cpp");
    let mut clang = if cpp_driver.exists() {
        let mut clang = Command::new("clang++");
        clang.arg("-O2").arg(&cpp_driver);
        clang
    } else {
        Command::new("clang")
    };
    clang.arg("-o").arg(&exe_path).arg(&link_input).arg("-lm");
    let driver = path.with_extension("c");
    if driver.exists() {
        clang.arg(&driver);
    }
    let out = clang.output().map_err(|e| format!("running clang: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "clang failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let out = run_with_timeout(&mut Command::new(&exe_path))?;
    let got = match (out.status.code(), out.status.signal()) {
        (Some(code), _) => format!("exit {code}"),
        (None, Some(SIGTRAP | SIGILL)) => "trap".to_string(),
        (None, Some(sig)) => format!("signal {sig}"),
        (None, None) => "unknown status".to_string(),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !stdout_lines.is_empty() {
        let want: String = stdout_lines.iter().map(|l| format!("{l}\n")).collect();
        if stdout != want {
            return Err(format!("stdout was {stdout:?}, want {want:?}"));
        }
    }
    if got != expect {
        return Err(format!(
            "expected `{expect}`, got `{got}`\n{}(assembly: {})",
            String::from_utf8_lossy(&out.stdout),
            asm_path.display()
        ));
    }
    Ok(())
}

/// How long a program may run before it counts as hung, so a miscompiled
/// loop fails the test instead of stalling it.
const TIMEOUT: Duration = Duration::from_secs(10);

fn run_with_timeout(cmd: &mut Command) -> Result<Output, String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("running {}: {e}", cmd.get_program().display()))?;
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > TIMEOUT {
            child.kill().unwrap();
            child.wait().unwrap();
            return Err(format!("still running after {TIMEOUT:?}; killed"));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // The programs print little, so the pipes cannot fill up and block them.
    Ok(child.wait_with_output().unwrap())
}

// Keep focused test names and catch-all exclusions in the same declaration.
// Each fixture runs CONFIGS once, while remaining selectable by its regression name.
macro_rules! focused_programs {
    ($($name:ident: [$($program:literal),+ $(,)?] { $($extra:tt)* })+) => {
        const FOCUSED_PROGRAMS: &[&str] = &[$($($program,)+)+];
        $(
            #[test]
            fn $name() {
                for program in [$($program),+] {
                    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join(format!("tests/programs/{program}.keel"));
                    for (pools, optimize, suffix, selection, allocation) in CONFIGS {
                        run_program(&path, &pools, optimize, suffix, selection, &allocation)
                            .unwrap();
                    }
                }
                $($extra)*
            }
        )+
    };
}

focused_programs! {
    scalar_address_oracle: ["scalar_address"] {}
    folding_duplicate_operands: ["folding_duplicate"] {}
    switch_default_skips_case_landing_pads: ["switch_default_edges"] {}
    option_switch_default_from_callee_preserves_none_and_some: ["option_switch_default"] {}
    compact_frame_storage: ["compact_frame", "dyn_alloca", "unwind", "exceptions", "setjmp"] {
        #[cfg(feature = "experimental-regalloc")]
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/compact_frame.keel");
        #[cfg(feature = "experimental-regalloc")]
        for allocation in [
            AllocationTier::Local,
            AllocationTier::Split,
            AllocationTier::Weighted,
        ] {
            run_program(
                &path,
                &TINY,
                false,
                &format!("-compact-{allocation:?}"),
                keel_codegen::SelectionOptions::ALL,
                &allocation,
            )
            .unwrap();
        }
        #[cfg(feature = "experimental-regalloc")]
        for base in [AllocationTier::Fast, AllocationTier::Full] {
            let allocation = keel_codegen::regalloc::RematerializingAllocator {
                base: &base,
                prefer_spilling: true,
                constants: true,
                stack_addresses: true,
                null_pointers: false,
            };
            run_program(
                &path,
                &TINY,
                false,
                &format!("-compact-remat-{base:?}"),
                keel_codegen::SelectionOptions::ALL,
                &allocation,
            )
            .unwrap();
        }
    }
}

#[test]
fn optimized_regressions_spills() {
    for program in [
        "constant_loop",
        "cfg_forwarding",
        "gvn_compare",
        "strength_reduce",
        "phi_loop",
        "checked_fold",
        "memory_flags",
        "memory_forward",
        "known_bits",
        "canonical_constants",
        "scalar_simplify",
        "branch_ranges",
    ] {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/programs/{program}.keel"));
        for (selection, selection_name) in [
            (keel_codegen::SelectionOptions::default(), "baseline"),
            (keel_codegen::SelectionOptions::ALL, "all"),
        ] {
            for (allocation, allocation_name) in [
                (AllocationTier::Fast, "fast"),
                (AllocationTier::Full, "full"),
            ] {
                let suffix = format!("-opt-regression-tiny-{selection_name}-{allocation_name}");
                run_program(&path, &TINY, true, &suffix, selection, &allocation).unwrap();
            }
        }
    }
}

#[test]
fn programs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
        .filter(|p| {
            !p.file_stem()
                .and_then(|stem| stem.to_str())
                .is_some_and(|stem| FOCUSED_PROGRAMS.contains(&stem))
        })
        .collect();
    paths.sort();
    assert!(!paths.is_empty());

    let failures: Vec<String> = paths
        .iter()
        .flat_map(|p| {
            CONFIGS
                .iter()
                .map(move |(pools, optimize, suffix, selection, allocation)| {
                    (p, pools, *optimize, suffix, *selection, *allocation)
                })
        })
        .filter_map(|(p, pools, optimize, suffix, selection, allocation)| {
            run_program(p, pools, optimize, suffix, selection, &allocation)
                .err()
                .map(|e| {
                    let name = p.file_name().unwrap().to_string_lossy();
                    format!("{name} (config{suffix}): {e}")
                })
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} program(s) failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Breakpoints by function and by line stop where the line table says, and
/// the backtrace reaches the C caller.
#[test]
#[cfg(target_arch = "aarch64")]
fn debug_info() {
    let tiers = vec![AllocationTier::Full, AllocationTier::Fast];
    #[cfg(feature = "experimental-regalloc")]
    let tiers = {
        let mut tiers = tiers;
        tiers.extend([
            AllocationTier::Local,
            AllocationTier::Split,
            AllocationTier::Weighted,
        ]);
        tiers
    };
    for allocation in tiers {
        debug_info_with_allocator(&allocation);
    }
}

#[cfg(target_arch = "aarch64")]
fn debug_info_with_allocator(allocation: &dyn keel_codegen::regalloc::RegisterAllocator) {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let lldb = debug_session(
        allocation,
        &tests.join("debug/calc.keel"),
        &tests.join("debug/main.c"),
        false,
        &[
            "b calc",
            "b calc.rs:3",
            "b nocol",
            "run",
            "frame info",
            "up",
            "frame info",
            "continue",
            "frame info",
            "continue",
            "frame info",
        ],
    );
    // `b calc` stops after the prologue, at the first line of the body,
    // and so does `b nocol`, though its prologue has the same location.
    for want in [
        "`calc at calc.rs:2:13",
        "`main at main.c:5",
        "`calc at calc.rs:3:14",
        "`nocol + ",
        "`nocol at calc.rs:6\n",
    ] {
        assert!(lldb.contains(want), "lldb output lacks `{want}`:\n{lldb}");
    }

    // The IR's own lines, as `keel compile -g` gives them.
    let lldb = debug_session(
        allocation,
        &tests.join("programs/sum.keel"),
        &tests.join("programs/sum.c"),
        true,
        &[
            "b sum",
            "run",
            "frame info",
            "up",
            "frame info",
            "breakpoint delete 1",
            "b sum.keel:13",
            "continue",
            "frame info",
        ],
    );
    for want in [
        "`sum at sum.keel:4:10",
        "`main at sum.c:",
        "`sum at sum.keel:13:10",
    ] {
        assert!(lldb.contains(want), "lldb output lacks `{want}`:\n{lldb}");
    }
}

#[test]
#[cfg(all(target_arch = "aarch64", feature = "experimental-regalloc"))]
fn rematerialization_debug_info() {
    for base in [AllocationTier::Fast, AllocationTier::Full] {
        let allocator = keel_codegen::regalloc::RematerializingAllocator {
            base: &base,
            prefer_spilling: true,
            constants: true,
            stack_addresses: true,
            null_pointers: false,
        };
        debug_info_with_allocator(&allocator);
    }
}

/// Compiles `keel`, links it with `driver` built with `-g`, and runs
/// `commands` in lldb. Returns lldb's output.
#[cfg(target_arch = "aarch64")]
fn debug_session(
    allocation: &dyn keel_codegen::regalloc::RegisterAllocator,
    keel: &Path,
    driver: &Path,
    ir_lines: bool,
    commands: &[&str],
) -> String {
    let src = std::fs::read_to_string(keel).unwrap();
    let module = if ir_lines {
        keel_ir::parse_with_ir_lines(&src, keel.to_str().unwrap())
    } else {
        keel_ir::parse(&src)
    };
    let module = module.unwrap();
    keel_ir::verify_module(&module).unwrap();
    let asm = keel_codegen::aarch64::compile_with_pipeline(
        &module,
        &REGISTERS,
        &keel_codegen::CodegenPipeline {
            selector: &keel_codegen::SelectionTier::Baseline,
            allocator: allocation,
        },
    )
    .unwrap();

    // Baseline and rematerializing debug tests may run concurrently. Each
    // session must own its assembly, executable and dsymutil outputs.
    static NEXT_SESSION: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let session = NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("debug-{}-{session}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let stem = keel.file_stem().unwrap().to_string_lossy();
    let asm_path = dir.join(format!("{stem}.s"));
    let exe_path = dir.join(&*stem);
    std::fs::write(&asm_path, asm).unwrap();
    let out = Command::new("clang")
        .arg("-g")
        .arg("-o")
        .arg(&exe_path)
        .arg(&asm_path)
        .arg(driver)
        .output()
        .unwrap();
    // dsymutil, which `clang -g` runs, warns about debug info it cannot
    // use rather than failing.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && !stderr.contains("warning"),
        "clang failed or warned:\n{stderr}"
    );

    let mut lldb = Command::new("lldb");
    lldb.arg("--batch");
    for command in commands {
        lldb.arg("-o").arg(command);
    }
    let out = run_with_timeout(lldb.arg(&exe_path)).unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Valid IR the aarch64 backend refuses, with the reason it gives.
/// A conditional branch over more than its ±1 MiB reach, as in big
/// interpreter loops, is relaxed into one that reaches; without that the
/// assembler rejects the program ("fixup value out of range"). Here it is
/// the branch back to the head of a loop whose body is 1.2 MB of `add`s,
/// run twice.
#[test]
#[cfg(target_arch = "aarch64")]
fn long_branches() {
    let n = 300_000;
    let mut src = String::from(
        "func @main(i32) -> i32 {\n    ss0 = stack_slot 4, align 4\nblock0(v0: i32):\n    \
         v1 = stack_addr ss0\n    v2 = iconst.i32 2\n    store v2, v1\n    jump block1\n\
         block1:\n",
    );
    for i in 0..n {
        let prev = if i == 0 { 0 } else { i + 2 };
        src.push_str(&format!("    v{} = iadd v{prev}, v0\n", i + 3));
    }
    let v = n + 3;
    src.push_str(&format!(
        "    v{v} = load.i32 v1\n    v{} = iconst.i32 1\n    v{} = isub v{v}, v{}\n    \
         store v{}, v1\n    v{} = iconst.i32 0\n    v{} = icmp.eq v{}, v{}\n    \
         brif v{}, block2, block1\nblock2:\n    v{} = iconst.i32 7\n    return v{}\n}}\n",
        v + 1,
        v + 2,
        v + 1,
        v + 2,
        v + 3,
        v + 4,
        v + 2,
        v + 3,
        v + 4,
        v + 5,
        v + 5,
    ));
    let module = keel_ir::parse(&src).unwrap();
    let asm = keel_codegen::aarch64::compile(&module).unwrap();
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("long-branches");
    std::fs::create_dir_all(&dir).unwrap();
    let (asm_path, exe) = (dir.join("long.s"), dir.join("long"));
    std::fs::write(&asm_path, asm).unwrap();
    let out = Command::new("clang")
        .arg("-o")
        .arg(&exe)
        .arg(&asm_path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let status = run_with_timeout(&mut Command::new(&exe)).unwrap().status;
    assert_eq!(status.code(), Some(7));
}

#[test]
#[cfg(target_arch = "aarch64")]
fn rejected() {
    let cases = [
        (
            "decl @f(i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64
             func @g(i64) -> i64 cc(c) {
             block0(v0: i64):
                 return_call @f(v0, v0, v0, v0, v0, v0, v0, v0, v0)
             }",
            "needs stack or struct arguments, which tail calls do not support",
        ),
        (
            "decl @f(ptr sret {i64, i64})
             func @g(ptr sret {i64, i64}) cc(c) {
             block0(v0: ptr):
                 return_call @f(v0)
             }",
            "needs stack or struct arguments",
        ),
        (
            "decl @f()
             func @g(ptr sret {i64}) cc(c) {
             block0(v0: ptr):
                 return_call @f()
             }",
            "a function returning a struct cannot make tail calls",
        ),
        (
            "func @g(i128) -> i128 cc(c) {
             block0(v0: i128):
                 v1, v2 = umul_overflow v0, v0
                 return v1
             }",
            "umul_overflow on i128 is not supported yet",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"\", \"{x9}\"(v0)
                 return
             }",
            "asm operands can only be in x0–x8, x16, x17, v0–v7 and v16–v18",
        ),
        (
            "func @g() cc(c) {
             block0:
                 asm \"\", \"~{fp}\"()
                 return
             }",
            "x29 is the frame pointer",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 v1 = intrinsic stack_alloc(v0)
                 asm \"\", \"~{x19}\"()
                 return
             }",
            "x19 is the base pointer",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"\", \"{x1},{x1}\"(v0, v0)
                 return
             }",
            "x1 is named by two inputs",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"\", \"{x1},~{x1}\"(v0)
                 return
             }",
            "x1 is both an operand and clobbered",
        ),
        (
            "func @g(f64) -> f64 cc(c) {
             block0(v0: f64):
                 v1 = asm \"fmov %w0, %1\", \"=w,w\"(v0) -> f64
                 return v1
             }",
            "`%w0` does not fit operand 0 (`w`)",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"mov x0, %1\", \"r\"(v0)
                 return
             }",
            "`%1`: the asm has only 1 operand(s)",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"mov x0, %[0\", \"r\"(v0)
                 return
             }",
            "bracketed asm operand must end with `]`",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"mov x0, %[]\", \"r\"(v0)
                 return
             }",
            "`%` must be followed by an operand number",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"mov x0, %[1]0\", \"r\"(v0)
                 return
             }",
            "`%1`: the asm has only 1 operand(s)",
        ),
        (
            "func @g(i64) cc(c) {
             block0(v0: i64):
                 asm \"\", \"r,r,r,r,r,r,r,r,r,r,r,r\"(v0, v0, v0, v0, v0, v0, v0, v0, v0, v0, v0, v0)
                 return
             }",
            "too many asm operands",
        ),
    ];
    for (src, want) in cases {
        let module = keel_ir::parse(src).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let err = keel_codegen::aarch64::compile(&module)
            .unwrap_err()
            .to_string();
        assert!(err.contains(want), "expected `{want}`, got `{err}`");
    }
}

#[path = "support/allocation_edits.rs"]
mod allocation_edits;

#[test]
fn changing_allocation_locations() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    for name in [
        "regalloc_calls",
        "regalloc_loops",
        "regalloc_pressure",
        "phi_loop",
        "folding_duplicate",
        "edge_locations",
        "floats",
        "i128",
        "switch",
        "setjmp",
        "exceptions",
        "inline_asm",
    ] {
        for (selection, suffix) in [
            (keel_codegen::SelectionOptions::default(), "-boundary"),
            (keel_codegen::SelectionOptions::ALL, "-boundary-selected"),
        ] {
            run_program(
                &dir.join(format!("{name}.keel")),
                &REGISTERS,
                false,
                suffix,
                selection,
                &allocation_edits::BoundaryAllocator,
            )
            .unwrap();
        }
    }
    run_program(
        &dir.join("edge_locations.keel"),
        &REGISTERS,
        false,
        "-edge-locations",
        keel_codegen::SelectionOptions::default(),
        &allocation_edits::EdgeAllocator,
    )
    .unwrap();
}

#[test]
#[cfg(feature = "experimental-regalloc")]
fn full_affinity_allocator_programs() {
    experimental_allocator_programs(&AllocationTier::FullAffinity, "full-affinity");
}
#[test]
#[cfg(feature = "experimental-regalloc")]
fn local_allocator_programs() {
    experimental_allocator_programs(&AllocationTier::Local, "local");
}
#[test]
#[cfg(feature = "experimental-regalloc")]
fn split_allocator_programs() {
    experimental_allocator_programs(&AllocationTier::Split, "split");
}
#[test]
#[cfg(feature = "experimental-regalloc")]
fn weighted_allocator_programs() {
    experimental_allocator_programs(&AllocationTier::Weighted, "weighted");
}
#[test]
#[cfg(feature = "experimental-regalloc")]
fn rematerialization_programs() {
    use keel_codegen::regalloc::RematerializingAllocator;
    for tier in [AllocationTier::Fast, AllocationTier::Full] {
        let allocator = RematerializingAllocator {
            base: &tier,
            prefer_spilling: false,
            constants: true,
            stack_addresses: true,
            null_pointers: false,
        };
        experimental_allocator_programs(&allocator, &format!("remat-{tier:?}"));
    }
}
#[test]
#[cfg(feature = "experimental-regalloc")]
fn null_rematerialization_programs() {
    for base in [AllocationTier::Fast, AllocationTier::Full] {
        let allocator = keel_codegen::regalloc::RematerializingAllocator {
            base: &base,
            prefer_spilling: false,
            constants: false,
            stack_addresses: false,
            null_pointers: true,
        };
        for name in ["null_rematerialization", "null_invoke"] {
            let path =
                Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/programs/{name}.keel"));
            for pools in [
                &REGISTERS,
                &Pools {
                    caller_saved: &[],
                    callee_saved: &[],
                    float_caller_saved: &[],
                    float_callee_saved: &[],
                },
            ] {
                run_program(
                    &path,
                    pools,
                    false,
                    &format!("-null-{base:?}"),
                    keel_codegen::SelectionOptions::default(),
                    &allocator,
                )
                .unwrap();
            }
        }
    }
}

#[test]
#[cfg(feature = "experimental-regalloc")]
fn pressure_rematerialization_programs() {
    use keel_codegen::regalloc::RematerializingAllocator;
    for tier in [AllocationTier::Fast, AllocationTier::Full] {
        let allocator = RematerializingAllocator {
            base: &tier,
            prefer_spilling: true,
            constants: true,
            stack_addresses: true,
            null_pointers: false,
        };
        experimental_allocator_programs(&allocator, &format!("pressure-remat-{tier:?}"));
    }
}
#[test]
#[cfg(feature = "experimental-regalloc")]
fn rematerialization_objects_match_text_assembly() {
    let module = keel_ir::parse(include_str!("programs/rematerialization.keel")).unwrap();
    for tier in [AllocationTier::Fast, AllocationTier::Full] {
        for prefer_spilling in [false, true] {
            let allocator = keel_codegen::regalloc::RematerializingAllocator {
                base: &tier,
                prefer_spilling,
                constants: true,
                stack_addresses: true,
                null_pointers: false,
            };
            let pipeline = keel_codegen::CodegenPipeline {
                selector: &keel_codegen::SelectionTier::Baseline,
                allocator: &allocator,
            };
            for arm in [false, true] {
                let (program, text) = if arm {
                    (
                        keel_codegen::aarch64::compile_program_with_pipeline(
                            &module, &TINY_ARM, &pipeline,
                        )
                        .unwrap(),
                        keel_codegen::aarch64::compile_with_pipeline(&module, &TINY_ARM, &pipeline)
                            .unwrap(),
                    )
                } else {
                    (
                        keel_codegen::x86_64::compile_program_with_pipeline(
                            &module, &TINY_X86, &pipeline,
                        )
                        .unwrap(),
                        keel_codegen::x86_64::compile_with_pipeline(&module, &TINY_X86, &pipeline)
                            .unwrap(),
                    )
                };
                assert_eq!(program.stats().text_bytes, 0);
                let (typed, parsed) = if arm {
                    (
                        keel_asm::assemble_program_object(
                            &program,
                            keel_asm::TargetFeatures::default(),
                        )
                        .unwrap(),
                        keel_asm::assemble_object(&text).unwrap(),
                    )
                } else {
                    (
                        keel_asm::x86::assemble_program_object(&program).unwrap(),
                        keel_asm::x86::assemble_object(&text).unwrap(),
                    )
                };
                assert_eq!(
                    typed, parsed,
                    "{tier:?} pressure={prefer_spilling} arm={arm}"
                );
            }
        }
    }
}
#[cfg(feature = "experimental-regalloc")]
const TINY_ARM: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};
#[cfg(feature = "experimental-regalloc")]
const TINY_X86: Pools = Pools {
    caller_saved: &[6, 7],
    callee_saved: &[3, 12],
    float_caller_saved: &[8, 9],
    float_callee_saved: &[],
};
#[test]
#[cfg(feature = "experimental-regalloc")]
fn rematerialization_all_spilled() {
    let pools = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let allocator = keel_codegen::regalloc::RematerializingAllocator {
        base: &AllocationTier::Full,
        prefer_spilling: false,
        constants: true,
        stack_addresses: true,
        null_pointers: false,
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/rematerialization.keel");
    run_program(
        &path,
        &pools,
        false,
        "-all-spilled-remat",
        keel_codegen::SelectionOptions::default(),
        &allocator,
    )
    .unwrap();
}
#[cfg(feature = "experimental-regalloc")]
fn experimental_allocator_programs(
    tier: &dyn keel_codegen::regalloc::RegisterAllocator,
    name: &str,
) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "keel"))
        .collect();
    paths.sort();
    let mut failures = Vec::new();
    for path in &paths {
        for (pools, optimize, suffix, selection) in [
            (
                REGISTERS,
                false,
                "",
                keel_codegen::SelectionOptions::default(),
            ),
            (
                TINY,
                false,
                "-tiny",
                keel_codegen::SelectionOptions::default(),
            ),
            (
                REGISTERS,
                true,
                "-select-opt",
                keel_codegen::SelectionOptions::ALL,
            ),
            (
                TINY,
                true,
                "-select-opt-tiny",
                keel_codegen::SelectionOptions::ALL,
            ),
        ] {
            let suffix = format!("-{name}{suffix}");
            if let Err(error) = run_program(path, &pools, optimize, &suffix, selection, tier) {
                eprintln!("{} ({suffix}): {error}", path.display());
                failures.push(format!("{} ({suffix}): {error}", path.display()));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
