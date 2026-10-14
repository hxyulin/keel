//! End-to-end tests: every `tests/programs/*.c` is preprocessed with
//! `clang -E`, compiled by keel (as is and with the default optimization
//! pipeline), linked with clang and run. Its output and exit status must be
//! the same as the program built by clang alone, so the programs print
//! what they compute rather than spelling out expected values.

use std::path::Path;
use std::process::{Command, Output};

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
    let src = std::fs::read_to_string(i_path).unwrap();
    let mut module = keel_c::compile(&src, i_path.to_str().unwrap(), keel_c::Options::default())
        .map_err(|e| e.to_string())?;
    keel_ir::verify_module(&module).map_err(|e| format!("invalid IR: {e:?}"))?;
    if optimize {
        keel_opt::optimize(&mut module).map_err(|e| e.to_string())?;
    }
    let asm = keel_codegen::aarch64::compile(&module).map_err(|e| e.to_string())?;
    let asm_path = exe.with_extension("s");
    std::fs::write(&asm_path, asm).unwrap();
    let out = Command::new("clang")
        .arg("-o")
        .arg(exe)
        .arg(&asm_path)
        .output()
        .unwrap();
    if !out.status.success() {
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
            "void f(void) { void *p = 0; goto *p; }",
            "error: computed goto is not supported",
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
