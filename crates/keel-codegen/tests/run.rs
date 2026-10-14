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

#[cfg(target_arch = "aarch64")]
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
    allocation: AllocationTier,
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
    let asm = compile_with_pipeline(
        &module,
        pools,
        &keel_codegen::CodegenPipeline {
            selector: &keel_codegen::OptionsSelector(selection),
            allocator: &allocation,
        },
    )
    .map_err(|e| e.to_string())?;

    let stem = format!("{}{suffix}", path.file_stem().unwrap().to_string_lossy());
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs");
    std::fs::create_dir_all(&dir).unwrap();
    let asm_path = dir.join(format!("{stem}.s"));
    let exe_path = dir.join(&stem);
    std::fs::write(&asm_path, asm).unwrap();

    let cpp_driver = path.with_extension("cpp");
    let mut clang = if cpp_driver.exists() {
        let mut clang = Command::new("clang++");
        clang.arg("-O2").arg(&cpp_driver);
        clang
    } else {
        Command::new("clang")
    };
    clang.arg("-o").arg(&exe_path).arg(&asm_path).arg("-lm");
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

#[test]
fn optimized_regressions_spills() {
    for program in [
        "constant_loop",
        "cfg_forwarding",
        "gvn_compare",
        "strength_reduce",
        "phi_loop",
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
                run_program(&path, &TINY, true, &suffix, selection, allocation).unwrap();
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
            run_program(p, pools, optimize, suffix, selection, allocation)
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
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let lldb = debug_session(
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

/// Compiles `keel`, links it with `driver` built with `-g`, and runs
/// `commands` in lldb. Returns lldb's output.
#[cfg(target_arch = "aarch64")]
fn debug_session(keel: &Path, driver: &Path, ir_lines: bool, commands: &[&str]) -> String {
    let src = std::fs::read_to_string(keel).unwrap();
    let module = if ir_lines {
        keel_ir::parse_with_ir_lines(&src, keel.to_str().unwrap())
    } else {
        keel_ir::parse(&src)
    };
    let module = module.unwrap();
    keel_ir::verify_module(&module).unwrap();
    let asm = keel_codegen::aarch64::compile(&module).unwrap();

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("debug");
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
