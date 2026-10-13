//! End-to-end tests: compile each `tests/programs/*.keel` to assembly, link
//! it with `clang` (plus a same-named `.c` driver if there is one), run it,
//! and compare with the `; expect:` line, either `exit <code>` or `trap`.
//! `; stdout: <text>` lines, if present, give the expected output line by
//! line.
//!
//! Every program is compiled three times: with the real register set; with
//! two registers of each kind (and class), which spills most values (running the spill
//! code and register/stack mixes in parallel copies on every program); and
//! after the default optimization pipeline, which must not change what the
//! program does.

use keel_codegen::Pools;
use keel_codegen::aarch64::REGISTERS;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// SIGTRAP, raised by `brk`.
const SIGTRAP: i32 = 5;

/// Register sets to compile with, whether to optimize first, and a suffix
/// for the output files.
const CONFIGS: [(Pools, bool, &str); 3] = [
    (REGISTERS, false, ""),
    (
        Pools {
            caller_saved: &[9, 10],
            callee_saved: &[19, 20],
            float_caller_saved: &[19, 20],
            float_callee_saved: &[8, 9],
        },
        false,
        "-tiny",
    ),
    (REGISTERS, true, "-opt"),
];

fn run_program(path: &Path, pools: &Pools, optimize: bool, suffix: &str) -> Result<(), String> {
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

    let mut module = keel_ir::parse(&src).map_err(|e| format!("parse error: {e}"))?;
    keel_ir::verify_module(&module).map_err(|e| format!("verifier: {e:?}"))?;
    if optimize {
        keel_opt::optimize(&mut module).map_err(|e| e.to_string())?;
    }
    let asm = keel_codegen::aarch64::compile_with(&module, pools).map_err(|e| e.to_string())?;

    let stem = format!("{}{suffix}", path.file_stem().unwrap().to_string_lossy());
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("programs");
    std::fs::create_dir_all(&dir).unwrap();
    let asm_path = dir.join(format!("{stem}.s"));
    let exe_path = dir.join(&stem);
    std::fs::write(&asm_path, asm).unwrap();

    let mut clang = Command::new("clang");
    clang.arg("-o").arg(&exe_path).arg(&asm_path);
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

    let out = run_with_timeout(&exe_path)?;
    let got = match (out.status.code(), out.status.signal()) {
        (Some(code), _) => format!("exit {code}"),
        (None, Some(SIGTRAP)) => "trap".to_string(),
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

fn run_with_timeout(exe: &Path) -> Result<Output, String> {
    let mut child = Command::new(exe)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("running {}: {e}", exe.display()))?;
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
                .map(move |(pools, optimize, suffix)| (p, pools, *optimize, suffix))
        })
        .filter_map(|(p, pools, optimize, suffix)| {
            run_program(p, pools, optimize, suffix).err().map(|e| {
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
