//! End-to-end tests: every `tests/programs/*.c` is preprocessed with
//! both Clang and Keel, compiled by keel (as is and with the default optimization
//! pipeline), linked with clang and run. Its output and exit status must be
//! the same as the program built by clang alone, so the programs print
//! what they compute rather than spelling out expected values.

use std::path::Path;
use std::process::{Command, Output};
#[cfg(target_os = "macos")]
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
    build_with_allocator(
        i_path,
        exe,
        optimize,
        opts,
        keel_codegen::regalloc::AllocationTier::Full,
    )
}

fn build_with_allocator(
    i_path: &Path,
    exe: &Path,
    optimize: bool,
    opts: keel_c::Options,
    allocation: keel_codegen::regalloc::AllocationTier,
) -> Result<(), String> {
    let src = std::fs::read_to_string(i_path).unwrap();
    let mut module =
        keel_c::compile(&src, i_path.to_str().unwrap(), opts).map_err(|e| e.to_string())?;
    keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
    if optimize {
        keel_opt::optimize(&mut module).map_err(|e| e.to_string())?;
    }
    let asm = keel_codegen::compile_with_pipeline(
        &module,
        opts.target,
        &keel_codegen::CodegenPipeline {
            selector: &keel_codegen::SelectionTier::Baseline,
            allocator: &allocation,
        },
    )
    .map_err(|e| e.to_string())?;
    let asm_path = exe.with_extension("s");
    std::fs::write(&asm_path, asm).unwrap();
    let out = Command::new("clang")
        .args(opts.debug_info.then_some("-g"))
        .arg("-o")
        .arg(exe)
        .arg(&asm_path)
        .arg("-lm")
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
    let sysroot = if cfg!(target_os = "macos") {
        let out = run(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]));
        Some(std::path::PathBuf::from(
            String::from_utf8(out.stdout).unwrap().trim(),
        ))
    } else {
        None
    };
    for path in &paths {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let reference = out_dir.join(format!("{stem}-clang"));
        run(Command::new("clang")
            .args(
                cfg!(all(target_arch = "aarch64", target_os = "linux"))
                    .then_some("--rtlib=compiler-rt"),
            )
            .args(["-std=c11", "-w", "-o"])
            .arg(&reference)
            .arg(path)
            .arg("-lm"));
        let want = Command::new(&reference).output().unwrap();
        let i_path = out_dir.join(format!("{stem}.i"));
        run(Command::new("clang")
            .args(["-E", "-std=c11", "-o"])
            .arg(&i_path)
            .arg(path));
        let raw = std::fs::read_to_string(path).unwrap();
        let standalone_path = out_dir.join(format!("{stem}-standalone.i"));
        let standalone = keel_c::preprocess(
            &raw,
            path.to_str().unwrap(),
            &keel_c::PreprocessOptions {
                sysroot: sysroot.clone(),
                ..Default::default()
            },
        )
        .unwrap();
        std::fs::write(&standalone_path, standalone).unwrap();
        for (preprocessor, preprocessed) in [("clang", &i_path), ("native", &standalone_path)] {
            for optimize in [false, true] {
                let name = format!(
                    "{stem} ({preprocessor} pp){}",
                    if optimize { " (-O)" } else { "" }
                );
                let exe = out_dir.join(format!(
                    "{stem}-{preprocessor}{}",
                    if optimize { "-opt" } else { "" }
                ));
                if let Err(e) = build_with_keel(preprocessed, &exe, optimize) {
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
#[cfg(target_os = "macos")]
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
    use keel_codegen::regalloc::AllocationTier::{Fast, Full};
    for (build_, allocation, optimize) in [
        ("clang", Full, false),
        ("keel", Full, false),
        ("keel-opt", Full, true),
        ("keel-fast", Fast, false),
        ("keel-fast-opt", Fast, true),
    ] {
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
            let opts = keel_c::Options {
                debug_info: true,
                target: Default::default(),
            };
            if let Err(e) = build_with_allocator(&i_path, &exe, optimize, opts, allocation) {
                panic!("building vars.c: {e}");
            }
        }
        let mut lldb = Command::new("lldb");
        lldb.arg("--batch").current_dir(&dir);
        for command in commands {
            lldb.arg("-o").arg(command);
        }
        // LLDB reports physical executable paths. cargo manages targets through
        // a symlink, so normalize the resolved per-build directory as well.
        let physical_dir = dir.canonicalize().unwrap();
        let out = run_with_timeout(lldb.arg("vars"))
            .replace(physical_dir.to_str().unwrap(), "DIR")
            .replace(dir.to_str().unwrap(), "DIR");
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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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

/// Pointer updates remain one seq_cst RMW on both targets, with and
/// without optimization, rather than a separate load and store.
#[test]
fn atomic_pointer_updates_are_seq_cst_rmw() {
    let src = "int *post_inc(_Atomic(int *) *p) { return (*p)++; }\n\
               int *pre_inc(_Atomic(int *) *p) { return ++*p; }\n\
               int *post_dec(_Atomic(int *) *p) { return (*p)--; }\n\
               int *pre_dec(_Atomic(int *) *p) { return --*p; }\n";
    for target in keel_ir::Target::NATIVE {
        let mut module = keel_c::compile(
            src,
            "test.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let text = module.to_string();
            assert_eq!(text.matches("atomic_rmw.add seq_cst").count(), 2, "{text}");
            assert_eq!(text.matches("atomic_rmw.sub seq_cst").count(), 2, "{text}");
            assert!(!text.contains("atomic_load"), "{text}");
            assert!(!text.contains("atomic_store"), "{text}");
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

/// Dispatch sites must share both the jump table and the SSA merge for
/// locals changed by different handlers, including backward jumps.
#[test]
fn computed_gotos_share_dispatch() {
    let src = "int run(int n) {\n\
        static void *table[] = { &&add, &&sub, &&done };\n\
        int value = 0;\n\
        goto *table[n % 3];\n\
        add: value += n; n--; goto *table[n % 3];\n\
        sub: value -= n; n--; goto *table[n % 3];\n\
        done: return value; }";
    for debug_info in [false, true] {
        for target in [
            keel_ir::Target::Aarch64AppleDarwin,
            keel_ir::Target::X86_64LinuxGnu,
        ] {
            let module =
                keel_c::compile(src, "dispatch.c", keel_c::Options { debug_info, target }).unwrap();
            keel_ir::verify_module(&module).unwrap();
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                let text = module.to_string();
                assert_eq!(text.matches("switch ").count(), 1, "{text}");
                keel_codegen::compile(&module, target).unwrap();
            }
        }
    }
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

/// GNU inference keeps an outer name visible until the initializer ends.
/// Clang rejects this shadowing case, so check the folded result directly.
#[test]
fn auto_type_initializer_uses_outer_scope() {
    let mut module =
        compile("int f(void) { int x = 40; { __auto_type x = x + 2; return x; } }").unwrap();
    keel_opt::optimize(&mut module).unwrap();
    let text = module.to_string();
    assert!(text.contains("iconst.i32 42"), "{text}");
}

/// An explicit atomic qualifier applies to the inferred type, including
/// pointers, while an atomic initializer is read as its underlying type.
#[test]
fn auto_type_atomic_qualifier_applies_to_inferred_type() {
    let src = "int *f(int *a) { _Atomic __auto_type p = a; return p++; }\n\
               int g(_Atomic(int) *a) { __auto_type x = *a; return x + 1; }";
    for target in keel_ir::Target::NATIVE {
        let mut module = keel_c::compile(
            src,
            "test.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        keel_ir::verify_module(&module).unwrap();
        keel_opt::optimize(&mut module).unwrap();
        keel_codegen::compile(&module, target).unwrap();
        let text = module.to_string();
        assert_eq!(text.matches("atomic_rmw.add seq_cst").count(), 1, "{text}");
        assert_eq!(text.matches("atomic_load.i32 seq_cst").count(), 1, "{text}");
    }
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

/// All fixed-type overflow families preserve the result store on both
/// backends, even when optimization runs.
#[test]
fn overflow_builtins_compile_on_both_targets() {
    let mut src = String::new();
    for (prefix, signed) in [("s", true), ("u", false)] {
        for (suffix, width) in [("", "int"), ("l", "long"), ("ll", "long long")] {
            let ty = if signed {
                width.to_string()
            } else {
                format!("unsigned {width}")
            };
            for operation in ["add", "sub", "mul"] {
                src.push_str(&format!(
                    "_Bool {prefix}{operation}{suffix}({ty} a, {ty} b, {ty} *r) {{ \
                     return __builtin_{prefix}{operation}{suffix}_overflow(a, b, r); }}\n"
                ));
            }
        }
    }
    for target in keel_ir::Target::NATIVE {
        let mut module = keel_c::compile(
            &src,
            "test.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
            let text = module.to_string();
            for operation in ["sadd", "uadd", "ssub", "usub", "smul", "umul"] {
                assert_eq!(
                    text.matches(&format!("{operation}_overflow")).count(),
                    3,
                    "{text}"
                );
            }
            assert_eq!(text.matches("store ").count(), 18, "{text}");
        }
    }
}

/// Erasing hint metadata must preserve calls in every runtime argument.
#[test]
fn hint_argument_calls_survive_optimization() {
    let src = "long value(void); long expected(void); void *pointer(void); unsigned long offset(void);\n\
               long plain(void) { return __builtin_expect(value(), expected()); }\n\
               long probability(void) { return __builtin_expect_with_probability(value(), expected(), 0.75); }\n\
               void *aligned(void) { return __builtin_assume_aligned(pointer(), 8, offset()); }";
    for target in keel_ir::Target::NATIVE {
        let mut module = keel_c::compile(
            src,
            "test.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            keel_codegen::compile(&module, target).unwrap();
            let text = module.to_string();
            for (name, count) in [("value", 2), ("expected", 2), ("pointer", 1), ("offset", 1)] {
                assert_eq!(
                    text.matches(&format!("call @{name}(")).count(),
                    count,
                    "{text}"
                );
            }
        }
    }
}

/// A debugger trap is pinned but permits later instructions and return.
#[test]
fn debugtrap_preserves_continuation_on_both_targets() {
    let src = "int f(void) { __builtin_debugtrap(); return 42; }";
    for target in keel_ir::Target::NATIVE {
        let mut module = keel_c::compile(
            src,
            "test.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let text = module.to_string();
            assert_eq!(text.matches("intrinsic debugtrap()").count(), 1, "{text}");
            assert!(text.contains("return "), "{text}");
            let asm = keel_codegen::compile(&module, target).unwrap();
            let breakpoint = match target {
                keel_ir::Target::Wasm32UnknownUnknown => panic!("native-only test target"),
                keel_ir::Target::Aarch64AppleDarwin | keel_ir::Target::Aarch64LinuxGnu => {
                    "brk\t#61440"
                }
                keel_ir::Target::X86_64LinuxGnu => "int3",
            };
            assert!(asm.contains(breakpoint), "{asm}");
        }
    }
}

/// Aligned debug locations survive optimization and textual IR round trips.
#[test]
fn alignment_and_indirect_debug_locations_round_trip() {
    let src = "_Alignas(64) long global;\n\
               struct S { char c; int x __attribute__((aligned(16))); };\n\
               int f(int value) { _Alignas(32) int x = value; struct S s = { 0 };\n\
               s.x = x; return ((unsigned long)&x & 31) + __alignof__(x) + s.x; }";
    for target in keel_ir::Target::NATIVE {
        let mut module = keel_c::compile(
            src,
            "alignment.c",
            keel_c::Options {
                target,
                debug_info: true,
            },
        )
        .unwrap();
        for optimize in [false, true] {
            if optimize {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            assert!(
                module.functions[0]
                    .debug_vars
                    .iter()
                    .any(|v| v.name == "x" && v.indirect)
            );
            let text = module.to_string();
            assert!(text.contains("align 64"), "{text}");
            let reparsed = keel_ir::parse(&text).unwrap();
            keel_ir::verify_module(&reparsed).unwrap();
            let canonical = reparsed.to_string();
            assert_eq!(keel_ir::parse(&canonical).unwrap().to_string(), canonical);
            keel_codegen::compile(&reparsed, target).unwrap();
        }
        // A location that dereferences a partial pointer must be rejected.
        let func = &mut module.functions[0];
        let too_small = func.create_stack_slot(4, 4);
        func.debug_vars
            .iter_mut()
            .find(|v| v.indirect)
            .unwrap()
            .slot = too_small;
        let errors = keel_ir::verify_module(&module).unwrap_err();
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("pointer-sized stack slot")),
            "{errors:?}"
        );
    }
}

/// Debug metadata must not change the actual addresses or values of
/// over-aligned objects. Linking also checks that dsymutil accepts it.
#[test]
fn aligned_objects_with_debug_info() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/alignment.c");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("alignment-debug");
    std::fs::create_dir_all(&dir).unwrap();
    let reference = dir.join("clang");
    run(Command::new("clang")
        .args(["-std=c11", "-w", "-o"])
        .arg(&reference)
        .arg(&source));
    let expected = Command::new(reference).output().unwrap();
    let input = dir.join("alignment.i");
    run(Command::new("clang")
        .args(["-E", "-std=c11", "-o"])
        .arg(&input)
        .arg(&source));
    for optimize in [false, true] {
        let exe = dir.join(if optimize { "keel-opt-g" } else { "keel-g" });
        build(
            &input,
            &exe,
            optimize,
            keel_c::Options {
                debug_info: true,
                ..Default::default()
            },
        )
        .unwrap();
        let actual = Command::new(exe).output().unwrap();
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
    }
}

/// Unsupported by-value layouts must fail rather than lose their size or
/// SysV MEMORY classification when translated into the IR layout.
#[test]
fn alignment_abi_limits_are_diagnosed() {
    for target in keel_ir::Target::NATIVE {
        for src in [
            "struct __attribute__((aligned(32))) S { int x; }; int f(struct S s) { return s.x; }",
            "struct __attribute__((aligned(32))) S { int x; }; struct S f(void) { return (struct S){ 1 }; }",
        ] {
            let error = keel_c::compile(
                src,
                "test.c",
                keel_c::Options {
                    target,
                    ..Default::default()
                },
            )
            .unwrap_err();
            assert!(
                error
                    .message
                    .contains("aligned above 16 bytes passed by value"),
                "{error}"
            );
        }
    }
    let src =
        "struct S { char c; int x __attribute__((packed)); }; int f(struct S s) { return s.x; }";
    let error = keel_c::compile(
        src,
        "test.c",
        keel_c::Options {
            target: keel_ir::Target::X86_64LinuxGnu,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        error
            .message
            .contains("unaligned fields passed by value on x86-64"),
        "{error}"
    );
}

/// Programs keel-c does not compile get an error at the right place.
#[test]
fn rejected() {
    let cases = [
        (
            "int f(x) { return x; }",
            "1:7: error: old-style parameter needs an explicit type declaration",
        ),
        (
            "_Complex long double z;",
            "1:22: error: `z`: objects of type `_Complex long double` are not supported",
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
            "struct s; void f(_Atomic(struct s *) *p) { (*p)++; }",
            "error: arithmetic on a pointer to an incomplete type",
        ),
        (
            "void f(_Atomic(int (*)(void)) *p) { --*p; }",
            "error: arithmetic on a function pointer",
        ),
        (
            "void f(void) { __auto_type x; }",
            "error: `__auto_type` needs an initializer and a plain identifier",
        ),
        (
            "void f(void) { __auto_type *x = 0; }",
            "error: expected an identifier",
        ),
        (
            "void f(void) { __auto_type x = 1, y = 2; }",
            "error: `__auto_type` declares only one variable",
        ),
        (
            "void f(void) { __auto_type x = x; }",
            "error: `x` is not declared",
        ),
        (
            "void g(void); void f(void) { __auto_type x = g(); }",
            "error: `__auto_type` initializer has an incomplete type",
        ),
        (
            "struct s { unsigned x:3; }; void f(struct s s) { __auto_type x = s.x; }",
            "error: `__auto_type` cannot infer from a bit-field",
        ),
        (
            "void f(__auto_type x) {}",
            "error: `__auto_type` cannot declare a parameter",
        ),
        (
            "struct s { __auto_type x; };",
            "error: `__auto_type` cannot declare a struct member",
        ),
        (
            "int f(void) { return sizeof(__auto_type); }",
            "error: `__auto_type` is only allowed in variable declarations",
        ),
        (
            "typedef __auto_type T;",
            "error: `__auto_type` cannot declare a typedef",
        ),
        (
            "void f(void) { unsigned __auto_type x = 1; }",
            "error: invalid combination of type specifiers",
        ),
        (
            "int f(void) { return __builtin_sadd_overflow(1, 2, 3); }",
            "error: overflow result must be a pointer",
        ),
        (
            "int f(double *r) { return __builtin_sadd_overflow(1, 2, r); }",
            "error: overflow result pointer has an incompatible type",
        ),
        (
            "int f(int *a, int *r) { return __builtin_sadd_overflow(a, 2, r); }",
            "error: overflow operands must be arithmetic values",
        ),
        (
            "long f(int x, double p) { return __builtin_expect_with_probability(x, 1, p); }",
            "error: probability must be a constant floating-point expression",
        ),
        (
            "long f(int x) { return __builtin_expect_with_probability(x, 1, 1.5); }",
            "error: probability must be between 0 and 1",
        ),
        (
            "long f(int x) { return __builtin_expect_with_probability(x, 1, -0.1); }",
            "error: probability must be between 0 and 1",
        ),
        (
            "long f(int x) { return __builtin_expect_with_probability(x, 1, __builtin_nan(\"\")); }",
            "error: probability must be between 0 and 1",
        ),
        (
            "int g(void); long f(int x) { return __builtin_expect_with_probability(x, 1, (g(), 0.5)); }",
            "error: probability must be a constant floating-point expression",
        ),
        (
            "int g(void); long f(int x) { return __builtin_expect_with_probability(x, 1, 0.5 + (g(), 0.0)); }",
            "error: probability must be a constant floating-point expression",
        ),
        (
            "void *f(void *p, int n) { return __builtin_assume_aligned(p, n); }",
            "error: alignment must be a constant integer",
        ),
        (
            "void *f(void *p) { return __builtin_assume_aligned(p, 0); }",
            "error: alignment must be a nonzero power of two",
        ),
        (
            "void *f(void *p) { return __builtin_assume_aligned(p, 3); }",
            "error: alignment must be a nonzero power of two",
        ),
        (
            "void *f(void *p) { return __builtin_assume_aligned(p, 8.0); }",
            "error: alignment must be a constant integer",
        ),
        (
            "void *f(void *p) { return __builtin_assume_aligned(p, 8, p); }",
            "error: alignment offset must be an integer",
        ),
        (
            "void *f(void *p) { return __builtin_assume_aligned(p); }",
            "error: `__builtin_assume_aligned` takes 2 or 3 arguments",
        ),
        (
            "void *f(void *p) { return __builtin_assume_aligned(p, 8, 0, 0); }",
            "error: `__builtin_assume_aligned` takes 2 or 3 arguments",
        ),
        (
            "void *f(void) { return __builtin_assume_aligned(3, 8); }",
            "error: alignment hint needs a pointer",
        ),
        (
            "int g(void); void *f(void *p) { return __builtin_assume_aligned(p, (g(), 8)); }",
            "error: alignment must be a constant integer",
        ),
        (
            "void f(void) { __builtin_debugtrap(1); }",
            "error: builtin takes 0 argument(s)",
        ),
        (
            "struct __attribute__((packed)) S { unsigned x:3; };",
            "error: packed bit-fields requiring partial-container accesses are not supported",
        ),
        (
            "struct __attribute__((packed)) S { unsigned x:31; unsigned y:2; };",
            "error: packed bit-fields requiring cross-container accesses are not supported",
        ),
        (
            "struct __attribute__((packed)) S { char c; _Atomic int x; };",
            "error: unaligned atomic members are not supported",
        ),
        (
            "int x; struct __attribute__((packed)) S { char c; int *p; } s = { 0, &x };",
            "unaligned pointer relocations are not supported",
        ),
        (
            "_Alignas(0) int f(void);",
            "error: function alignment attributes are not supported",
        ),
        (
            "_Alignas(1) int x;",
            "error: `_Alignas` cannot weaken natural alignment",
        ),
        (
            "int x __attribute__((aligned(3)));",
            "error: alignment must be a power of two",
        ),
        (
            "int x __attribute__((aligned(0)));",
            "error: alignment must be a power of two",
        ),
        (
            "_Alignas(-16) int x;",
            "error: alignment must be a power of two",
        ),
        (
            "_Alignas(8192) int x;",
            "error: alignment above 4096 is not supported",
        ),
        (
            "int n; _Alignas(n) int x;",
            "error: alignment must be an integer constant",
        ),
        (
            "_Alignas(16) typedef int I;",
            "error: `_Alignas` cannot apply to a typedef",
        ),
        (
            "typedef int Wide __attribute__((aligned(16))); Wide xs[2];",
            "error: array element size is not a multiple of its alignment",
        ),
        (
            "int f(void) __attribute__((aligned(32)));",
            "error: function alignment attributes are not supported",
        ),
        (
            "struct S { _Alignas(16) unsigned x:3; };",
            "error: `_Alignas` cannot apply to a bit-field",
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

/// Reserved Intel names must survive both assembler implementations, including
/// TLS references in debug info and symbol differences in function sizes.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn intel_symbol_names_with_debug_info_and_gnu_assembler() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs/asm_symbol_names.c");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("intel-symbol-names");
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("names.i");
    run(Command::new("clang")
        .args(["-E", "-std=c11", "-o"])
        .arg(&input)
        .arg(&source));
    for optimize in [false, true] {
        let exe = dir.join(if optimize { "keel-O" } else { "keel" });
        build(
            &input,
            &exe,
            optimize,
            keel_c::Options {
                debug_info: true,
                ..Default::default()
            },
        )
        .unwrap();
        let gas = exe.with_extension("gas");
        run(Command::new("clang")
            .args(["-g", "-fno-integrated-as", "-o"])
            .arg(&gas)
            .arg(exe.with_extension("s")));
        for path in [&exe, &gas] {
            let result = run(&mut Command::new(path));
            assert_eq!(result.stdout, b"10 14 19\n");
        }
    }
}
