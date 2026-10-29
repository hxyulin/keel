//! By-value records reach the IR in a layout whose leaves give each SysV
//! eightbyte the class GCC and Clang give it.
use keel_ir::{Module, Purpose, Target};

fn param_layout(source: &str, target: Target) -> String {
    let module: Module = keel_c::compile(
        source,
        "layout.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let f = module.functions.iter().find(|f| f.name == "f").unwrap();
    let Purpose::ByVal(layout) = &f.sig.params[0].purpose else {
        panic!("not byval: {:?}", f.sig.params[0]);
    };
    layout.to_string()
}

#[test]
fn x86_layouts_follow_eightbyte_classes() {
    for (record, layout) in [
        ("struct { double d; long l; }", "{f64, i64}"),
        ("struct { long l; double d; }", "{i64, f64}"),
        ("struct { int a, b; double d; }", "{i64, f64}"),
        ("struct { float a, b; int c; }", "{[2 x f32], i32}"),
        ("struct { char c; double d; }", "{i64, f64}"),
        ("struct { float a; double d; }", "{f64, f64}"),
        ("struct { struct { float x, y; } p; long n; }", "{f64, i64}"),
        ("struct { float f[2]; double d; }", "{f64, f64}"),
        ("struct { double d; int x : 5; }", "{f64, i64}"),
        (
            "struct { union { double d; float f[2]; } u; long n; }",
            "{f64, i64}",
        ),
        ("struct { _Alignas(16) double d; }", "{[0 x i128], f64}"),
        ("struct { _Alignas(16) long l; }", "{[0 x i128], i64}"),
        // INTEGER only, an HFA, and MEMORY keep the old layouts.
        ("struct { float a; int b; }", "[2 x i32]"),
        ("union { double d; long l; }", "[1 x i64]"),
        ("struct { double a, b; }", "[2 x f64]"),
        ("struct { float a, b, c; }", "[3 x f32]"),
        ("struct { double a; long b, c; }", "[3 x i64]"),
    ] {
        let source = format!("typedef {record} R; long f(R r) {{ return 0; }}");
        assert_eq!(
            param_layout(&source, Target::X86_64LinuxGnu),
            layout,
            "{record}"
        );
    }
    // AAPCS64 classifies by size and HFA alone.
    let source = "struct R { double d; long l; }; long f(struct R r) { return 0; }";
    assert_eq!(param_layout(source, Target::Aarch64LinuxGnu), "[2 x i64]");
}

#[test]
fn x86_floats_under_four_byte_alignment_fail_closed() {
    let error = keel_c::compile(
        "struct __attribute__((packed)) R { double d; float f; }; long f(struct R r) { return 0; }",
        "packed.c",
        keel_c::Options {
            target: Target::X86_64LinuxGnu,
            debug_info: false,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("4-byte alignment"), "{error}");
}

/// Each side of every call is built once by Keel and once by GCC or Clang.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_calls_match_gnu_and_clang() {
    use std::{fs, path::Path, process::Command};
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("x86-sysv-aggregates");
    fs::create_dir_all(&dir).unwrap();
    for (name, text) in [
        (
            "x86_sysv_aggregates.h",
            include_str!("fixtures/x86_sysv_aggregates.h"),
        ),
        (
            "callee.c",
            include_str!("fixtures/x86_sysv_aggregates_callee.c"),
        ),
        (
            "main.c",
            include_str!("fixtures/x86_sysv_aggregates_runner.c"),
        ),
    ] {
        fs::write(dir.join(name), text).unwrap();
    }
    let keel = |source: &str, optimize: bool| {
        let path = dir.join(source);
        let text = keel_c::preprocess(
            &fs::read_to_string(&path).unwrap(),
            path.to_str().unwrap(),
            &keel_c::PreprocessOptions {
                target: Target::X86_64LinuxGnu,
                ..Default::default()
            },
        )
        .unwrap();
        let mut module = keel_c::compile(
            &text,
            source,
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: false,
            },
        )
        .unwrap();
        if optimize {
            keel_opt::optimize(&mut module).unwrap();
        }
        let program = keel_codegen::compile_program(&module, Target::X86_64LinuxGnu).unwrap();
        let object = dir.join(source).with_extension("o");
        fs::write(
            &object,
            keel_asm::x86::assemble_program_object(&program).unwrap(),
        )
        .unwrap();
        object
    };
    for optimize in [false, true] {
        let callee = keel("callee.c", optimize);
        let main = keel("main.c", optimize);
        for compiler in ["gcc", "clang"] {
            for (keel_object, foreign) in [(&callee, "main.c"), (&main, "callee.c")] {
                let result = Command::new(compiler)
                    .args(["-O2", "-no-pie"])
                    .arg(keel_object)
                    .arg(dir.join(foreign))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{compiler}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let result = Command::new(dir.join("run")).output().unwrap();
                assert!(
                    result.status.success(),
                    "{compiler} {foreign} opt={optimize}: {}",
                    String::from_utf8_lossy(&result.stdout)
                );
            }
        }
    }
}
