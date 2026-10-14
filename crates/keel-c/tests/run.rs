//! End-to-end tests: every `tests/programs/*.c` is preprocessed with
//! `clang -E`, compiled by keel (as is and with the default optimization
//! pipeline), linked with clang and run. Its output and exit status must be
//! the same as the program built by clang alone, so the programs print
//! what they compute rather than spelling out expected values.

use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn run(cmd: &mut Command) -> Output {
    let out = cmd.output().expect("running a command");
    assert!(
        out.status.success(),
        "{cmd:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// Compiles preprocessed C at `i_path` to an executable at `exe`.
fn build_with_keel(i_path: &Path, exe: &Path, optimize: bool) -> Result<(), String> {
    build(i_path, exe, optimize, keel_c::Options::default())
}

fn build(i_path: &Path, exe: &Path, optimize: bool, opts: keel_c::Options) -> Result<(), String> {
    let src = std::fs::read_to_string(i_path).unwrap();
    let mut module =
        keel_c::compile(&src, i_path.to_str().unwrap(), opts).map_err(|e| e.to_string())?;
    keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
    if optimize {
        keel_opt::optimize(&mut module).map_err(|e| e.to_string())?;
    }
    let asm = keel_codegen::aarch64::compile(&module).map_err(|e| e.to_string())?;
    let asm_path = exe.with_extension("s");
    std::fs::write(&asm_path, asm).unwrap();
    let out = Command::new("clang")
        .args(opts.debug_info.then_some("-g"))
        .arg("-o")
        .arg(exe)
        .arg(&asm_path)
        .output()
        .unwrap();
    // dsymutil warns, rather than fails, on debug info it cannot read.
    if !out.status.success() || String::from_utf8_lossy(&out.stderr).contains("warning") {
        return Err(format!("clang: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(())
}

#[test]
fn programs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-programs");
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "c"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty());
    let mut failures = Vec::new();
    for path in &paths {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let reference = out_dir.join(format!("{stem}-clang"));
        run(Command::new("clang")
            .args(["-std=c11", "-w", "-o"])
            .arg(&reference)
            .arg(path));
        let want = Command::new(&reference).output().unwrap();
        let i_path = out_dir.join(format!("{stem}.i"));
        run(Command::new("clang")
            .args(["-E", "-std=c11", "-o"])
            .arg(&i_path)
            .arg(path));
        for optimize in [false, true] {
            let name = format!("{stem}{}", if optimize { " (-O)" } else { "" });
            let exe = out_dir.join(format!("{stem}{}", if optimize { "-opt" } else { "" }));
            if let Err(e) = build_with_keel(&i_path, &exe, optimize) {
                failures.push(format!("{name}: {e}"));
                continue;
            }
            let got = Command::new(&exe).output().unwrap();
            if got.status.code() != want.status.code() || got.stdout != want.stdout {
                failures.push(format!(
                    "{name}: exit {:?}, expected {:?}\n--- keel:\n{}--- clang:\n{}",
                    got.status.code(),
                    want.status.code(),
                    String::from_utf8_lossy(&got.stdout),
                    String::from_utf8_lossy(&want.stdout),
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// With `-g`, lldb shows `tests/debug/vars.c` as it does when clang -O0
/// builds it: the same variables in each scope, with the same types and
/// values; changes to them take effect; functions can be called. The
/// same session runs on both builds and its output is compared, less what
/// differs between any two builds (addresses, columns). Also with -O,
/// which leaves stack slots alone.
#[test]
fn debugger() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/debug/vars.c");
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-debug");
    let i_path = out_dir.join("vars.i");
    let commands = [
        "b stop",
        "b area",
        "b sum",
        "run",
        "up",
        "frame variable",
        "p *a.next",
        "p arr[1] + *ptr",
        "p counter + scale",
        "p per_thread",
        "expr d = 10.5",
        "continue",
        "frame variable n",
        "expr total = 100",
        "continue",
        "up",
        // In the loop: `i` and `sq` are in scope.
        "frame variable",
        "continue",
        "up",
        // In the block after it: the inner `total`, not `i`.
        "frame variable",
        "p total",
        "finish",
        "continue",
        "frame variable p k",
        "p sum(4)",
        "continue",
    ];
    let mut sessions = Vec::new();
    for build_ in ["clang", "keel", "keel-opt"] {
        let dir = out_dir.join(build_);
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("vars");
        if build_ == "clang" {
            run(Command::new("clang")
                .args(["-g", "-O0", "-w", "-std=c11", "-o"])
                .arg(&exe)
                .arg(&path));
            run(Command::new("clang")
                .args(["-E", "-std=c11", "-o"])
                .arg(&i_path)
                .arg(&path));
        } else {
            let opts = keel_c::Options { debug_info: true };
            if let Err(e) = build(&i_path, &exe, build_ == "keel-opt", opts) {
                panic!("building vars.c: {e}");
            }
        }
        let mut lldb = Command::new("lldb");
        lldb.arg("--batch").current_dir(&dir);
        for command in commands {
            lldb.arg("-o").arg(command);
        }
        let out = run_with_timeout(lldb.arg("vars")).replace(dir.to_str().unwrap(), "DIR");
        sessions.push((build_, normalize(&out)));
    }
    let (_, clang) = &sessions[0];
    // So that the comparison is not between two sessions that both fail.
    for want in [
        "(point_t) p = (x = 3, y = 4)",
        "(color) c = GREEN",
        "(size_t) len = 16",
        "(const char *) greeting = 0x",
        "next = NULL",
        "(int) 8\n",
        "(int) sq = 4",
        "(int) total = 99",
        "Return value: (int) $",
        "(int) 6\n",
        // 21 + 24 + 10 + 5 + 1 + 16 + 1 + 8, with `d` changed.
        "exited with status = 86",
    ] {
        assert!(
            clang.contains(want),
            "clang's session lacks `{want}`:\n{clang}"
        );
    }
    for (build_, session) in &sessions[1..] {
        if session != clang {
            let diff: Vec<String> = clang
                .lines()
                .zip(session.lines())
                .filter(|(a, b)| a != b)
                .map(|(a, b)| format!("clang: {a}\n{build_}: {b}"))
                .collect();
            panic!(
                "lldb on the {build_} build differs from clang's:\n{}\n\n{session}",
                diff.join("\n")
            );
        }
    }
}

/// lldb output without what differs between two correct builds:
/// addresses, process ids, offsets into functions (where a breakpoint
/// goes after the prologue), columns and the column markers (`^`) under
/// source lines.
fn normalize(out: &str) -> String {
    let mut lines = Vec::new();
    for line in out.lines() {
        if line.trim() == "^" {
            continue;
        }
        let mut s = String::new();
        let mut rest = line;
        while let Some(c) = rest.chars().next() {
            if let Some(after) = rest.strip_prefix("0x") {
                s.push_str("0x");
                rest = after.trim_start_matches(|c: char| c.is_ascii_hexdigit());
            } else if let Some(after) = rest.strip_prefix("Process ") {
                s.push_str("Process N");
                rest = after.trim_start_matches(|c: char| c.is_ascii_digit());
            } else if let Some(after) = rest.strip_prefix(" + ")
                && after.starts_with(|c: char| c.is_ascii_digit())
            {
                rest = after.trim_start_matches(|c: char| c.is_ascii_digit());
            } else if let Some(after) = rest.strip_prefix(".c:") {
                // `vars.c:12:5`: keep the line.
                let line_end = after
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(after.len());
                s.push_str(".c:");
                s.push_str(&after[..line_end]);
                rest = &after[line_end..];
                if let Some(col) = rest.strip_prefix(':') {
                    rest = col.trim_start_matches(|c: char| c.is_ascii_digit());
                }
            } else {
                s.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        lines.push(s);
    }
    lines.join("\n")
}

/// Runs `cmd` for at most a minute, giving its stdout and stderr.
fn run_with_timeout(cmd: &mut Command) -> String {
    use std::process::Stdio;
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("running lldb");
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            panic!("{cmd:?} still running after a minute");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr)
}

fn compile(src: &str) -> Result<keel_ir::Module, String> {
    keel_c::compile(src, "test.c", keel_c::Options::default()).map_err(|e| e.to_string())
}

/// Locals live in SSA values unless their address is taken; arrays and
/// address-taken scalars get stack slots.
#[test]
fn locals_become_ssa_values() {
    let module = compile(
        "int sum(int n) { int s = 0; for (int i = 0; i < n; i++) s += i; return s; }\n\
         void g(int *);\n\
         int f(void) { int x = 1; int a[2] = { 0 }; g(&x); g(a); return x; }\n",
    )
    .unwrap();
    let text = module.to_string();
    let sum = text.split("func @f").next().unwrap();
    assert!(!sum.contains("stack_slot"), "{sum}");
    // The loop header takes `i` and `s` as block parameters.
    let header = sum.lines().find(|l| l.starts_with("block1(")).unwrap();
    assert_eq!(header.matches(": i32").count(), 2, "{sum}");
    let f = text.split("func @f").nth(1).unwrap();
    assert_eq!(f.matches("stack_slot").count(), 2, "{f}");
}

/// A local declared in a loop without an initializer is a new object each
/// iteration: what it held last time round is not carried over, so it is
/// not a parameter of the loop header (it was, live through the whole
/// loop, before declarations started a new value).
#[test]
fn uninitialized_locals_do_not_live_across_iterations() {
    let module = compile(
        "double f(int n, double *a) {\n\
         double s = 0;\n\
         for (int i = 0; i < n; i++) { double x; if (a[i] > 0) x = a[i]; s += x; }\n\
         return s; }\n",
    )
    .unwrap();
    let text = module.to_string();
    let header = text.lines().find(|l| l.starts_with("block1(")).unwrap();
    assert_eq!(header.matches(": f64").count(), 1, "only `s`:\n{text}");
}

/// Only what is used is emitted: unused `static` and `inline` functions
/// and unused declarations leave nothing behind.
#[test]
fn unused_internal_definitions_are_dropped() {
    let module = compile(
        "int puts(const char *);\nint unused_decl(void);\n\
         static int helper(void) { return 1; }\n\
         inline int inl(void) { return 2; }\n\
         static int used(void) { return puts(\"x\"); }\n\
         int main(void) { return used(); }\n",
    )
    .unwrap();
    let names: Vec<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["main", "used"]);
    let decls: Vec<&str> = module.decls.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(decls, ["puts"]);
}

/// Programs keel-c does not compile get an error at the right place.
#[test]
fn rejected() {
    let cases = [
        (
            "int f(int n) { int a[n]; return 0; }",
            "1:22: error: variable-length arrays",
        ),
        (
            "int f(x) int x; { return x; }",
            "1:7: error: old-style parameter lists",
        ),
        (
            "_Complex double z;",
            "1:1: error: `_Complex` is not supported",
        ),
        (
            "int f(void) { return g(); }",
            "1:22: error: `g` is not declared",
        ),
        (
            "struct s; int f(struct s *p) { return p->x; }",
            "error: member of an incomplete struct",
        ),
        (
            "int x = 1; int y = x;",
            "1:20: error: initializer is not a constant",
        ),
        (
            "int a[2] = { 1, 2, 3 };",
            "1:20: error: too many initializers",
        ),
        (
            "int f(int x) { switch (x) { case 1: case 1: return 0; } }",
            "error: duplicate case 1",
        ),
        (
            "int f(void) { return 1 + ; }",
            "1:26: error: expected an expression, found `;`",
        ),
        (
            "struct s { _Atomic double d; };",
            "1:12: error: `_Atomic` is only supported on integers and pointers",
        ),
    ];
    let mut failures = Vec::new();
    for (src, want) in cases {
        match compile(src) {
            Ok(_) => failures.push(format!("`{src}` compiled")),
            Err(e) if !e.contains(want) => failures.push(format!("`{src}`: {e}")),
            Err(_) => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
