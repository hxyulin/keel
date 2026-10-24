//! The generic contract is independent of the fixed-type overflow IR operation.
use keel_ir::{InstData, Target};
use std::path::Path;
use std::process::Command;

const TYPES: &[&str] = &[
    "signed char",
    "unsigned char",
    "short",
    "unsigned short",
    "int",
    "unsigned",
    "long",
    "unsigned long",
    "long long",
    "unsigned long long",
    "__int128",
    "unsigned __int128",
];

#[test]
fn generic_overflow_types_codegen_and_volatile_store() {
    let mut source = String::new();
    // Every input/result width and signedness, including i128 products. Rotate
    // the destination to exercise both widening and narrowing without creating
    // thousands of near-identical functions in the frontend regression suite.
    for (i, a) in TYPES.iter().enumerate() {
        for (j, b) in TYPES.iter().enumerate() {
            let r = TYPES[(i + j) % TYPES.len()];
            for op in ["add", "sub", "mul"] {
                source.push_str(&format!("_Bool {op}_{i}_{j}({a} a, {b} b, volatile {r} *r) {{ return __builtin_{op}_overflow(a,b,r); }}\n"));
            }
        }
    }
    for target in Target::NATIVE {
        let mut module = keel_c::compile(
            &source,
            "generic.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        for optimized in [false, true] {
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let stores = module
                .functions
                .iter()
                .flat_map(|f| f.layout_insts().map(move |(_, i)| f.inst_data(i)))
                .filter(|i| matches!(i, InstData::Store { flags, .. } if flags.volatile))
                .count();
            assert_eq!(stores, TYPES.len() * TYPES.len() * 3);
            keel_codegen::compile(&module, target).unwrap();
        }
    }
}

#[test]
fn generic_overflow_diagnostics() {
    for source in [
        "int f(int *p) { return __builtin_add_overflow(p,1,p); }",
        "int f(int *p) { return __builtin_mul_overflow(1,2.0,p); }",
        "int f(void) { return __builtin_sub_overflow(1,2,0); }",
        "int f(void *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(_Bool *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(double *p) { return __builtin_add_overflow(1,2,p); }",
        "enum E { A }; int f(enum E *p) { return __builtin_add_overflow(1,2,p); }",
        "enum E { A }; typedef enum E E; int f(volatile E *p) { return __builtin_add_overflow(1,2,p); }",
        "typedef const int C; int f(volatile C *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(const int *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(_Atomic(int) *p) { return __builtin_add_overflow(1,2,p); }",
        "int f(int *p) { return __builtin_add_overflow(1,p); }",
    ] {
        let error = keel_c::compile(source, "invalid-generic.c", Default::default())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("invalid-generic.c:1:")
                && (error.contains("generic overflow") || error.contains("builtin takes 3")),
            "{source}\n{error}"
        );
    }
    for source in [
        "enum E { A }; int f(int *p) { return __builtin_add_overflow(A,(_Bool)1,p); }",
        "typedef volatile unsigned long V; int f(V *p) { return __builtin_mul_overflow(1,2,p); }",
    ] {
        keel_c::compile(source, "valid-generic.c", Default::default()).unwrap();
    }
}

fn checked(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn generic_overflow_differential_clang() {
    differential_clang("generic_overflow");
}

#[test]
fn fixed_overflow_differential_clang() {
    differential_clang("overflow_builtins");
}

fn differential_clang(stem: &str) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/programs/{stem}.c"));
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{stem}-differential"));
    std::fs::create_dir_all(&dir).unwrap();
    let reference = dir.join("clang");
    checked(
        Command::new("clang")
            .args(["-std=c11", "-O0", "-o"])
            .arg(&reference)
            .arg(&source),
    );
    let want = checked(&mut Command::new(&reference));
    let reference_opt = dir.join("clang-opt");
    checked(
        Command::new("clang")
            .args(["-std=c11", "-O2", "-o"])
            .arg(&reference_opt)
            .arg(&source),
    );
    assert_eq!(
        checked(&mut Command::new(&reference_opt)).stdout,
        want.stdout
    );
    let preprocessed = checked(Command::new("clang").args(["-E", "-std=c11"]).arg(&source));
    let src = String::from_utf8(preprocessed.stdout).unwrap();
    let native_src = keel_c::preprocess(
        &std::fs::read_to_string(&source).unwrap(),
        source.to_str().unwrap(),
        &keel_c::PreprocessOptions {
            sysroot: if cfg!(target_os = "macos") {
                Some(
                    String::from_utf8(
                        checked(Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]))
                            .stdout,
                    )
                    .unwrap()
                    .trim()
                    .into(),
                )
            } else {
                None
            },
            ..Default::default()
        },
    )
    .unwrap();
    for (pp, src) in [("clang", src), ("native", native_src)] {
        for optimized in [false, true] {
            let mut module =
                keel_c::compile(&src, source.to_str().unwrap(), Default::default()).unwrap();
            keel_ir::verify_module(&module).unwrap();
            if optimized {
                keel_opt::optimize(&mut module).unwrap();
            }
            keel_ir::verify_module(&module).unwrap();
            let asm = keel_codegen::compile(&module, keel_c::Options::default().target).unwrap();
            let assembly = dir.join(format!("{pp}-{optimized}.s"));
            let executable = dir.join(format!("{pp}-{optimized}"));
            std::fs::write(&assembly, asm).unwrap();
            checked(
                Command::new("clang")
                    .arg(&assembly)
                    .arg("-o")
                    .arg(&executable),
            );
            let got = checked(&mut Command::new(&executable));
            assert_eq!(got.stdout, want.stdout, "{pp} optimized={optimized}");
        }
    }
}
