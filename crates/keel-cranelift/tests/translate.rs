//! Translation, rejection and CLI behaviour, without running generated code.
mod common;
use keel_cranelift::{OptLevel, Options, TargetProfile};
use std::process::Command;

fn options(opt: OptLevel) -> Options {
    Options {
        target: TargetProfile::AppleArm64,
        opt,
        verify_clif: true,
    }
}
fn compile(src: &str) -> Result<Vec<u8>, String> {
    keel_cranelift::compile_object(&keel_ir::parse(src).unwrap(), &options(OptLevel::Speed))
}
fn clif(src: &str) -> String {
    keel_cranelift::translate_to_clif(&keel_ir::parse(src).unwrap(), &options(OptLevel::None))
        .unwrap()
}

#[test]
fn explicit_memcmp_intrinsic_preserves_i32_libcall_result() {
    let src = "func @f(ptr,ptr,i64)->i32 {\nblock0(v0:ptr,v1:ptr,v2:i64):\n v3 = intrinsic memcmp(v0,v1,v2)\n return v3\n}";
    assert!(clif(src).contains("%Memcmp"));
    assert!(!compile(src).unwrap().is_empty());
}

#[test]
fn rejects_unsupported_instructions_with_context() {
    let cases = [
        (
            "v1 = intrinsic stack_alloc(v0)\n v2 = ptrtoint.i64 v1",
            "intrinsic stack_alloc",
        ),
        (
            "v1 = intrinsic frame_address()\n v2 = ptrtoint.i64 v1",
            "intrinsic frame_address",
        ),
        (
            "v3 = inttoptr.ptr v0\n v2 = atomic_load.i64 seq_cst v3",
            "atomics and fences",
        ),
        ("fence seq_cst\n v2 = iconst.i64 0", "atomics and fences"),
        (
            "v1 = uext.i128 v0\n v3 = sdiv v1, v1\n v2 = trunc.i64 v3",
            "i128 sdiv",
        ),
        (
            "v1 = uext.i128 v0\n v3, v4 = umul_overflow v1, v1\n v2 = trunc.i64 v3",
            "i128 umul_overflow",
        ),
    ];
    for (body, expected) in cases {
        let src = format!("func @bad(i64) -> i64 {{\nblock0(v0: i64):\n {body}\n return v2\n}}\n");
        let error = compile(&src).unwrap_err();
        assert!(error.contains("function @bad: block0, inst"), "{error}");
        assert!(error.contains(expected), "{expected}: {error}");
    }
    let tail =
        "decl @f(i64) -> i64\nfunc @bad(i64) -> i64 {\nblock0(v0: i64):\n return_call @f(v0)\n}\n";
    assert!(compile(tail).unwrap_err().contains("guaranteed tail calls"));
}

#[test]
fn rejects_unsupported_abi_and_module_features() {
    let cases = [
        (
            "decl @printf(ptr, ...) -> i32\nfunc @bad(ptr) -> i32 {\nblock0(v0: ptr):\n v1 = call @printf(v0)\n return v1\n}\n",
            "variadic",
        ),
        (
            "decl @agg(ptr byval {i64})\nfunc @bad(ptr) {\nblock0(v0: ptr):\n call @agg(v0)\n return\n}\n",
            "aggregate byval/sret",
        ),
        (
            "func @bad(ptr byval {i64}) {\nblock0(v0: ptr):\n return\n}\n",
            "aggregate byval/sret",
        ),
        (
            "func @bad(i128) -> i64 {\nblock0(v0: i128):\n v1 = trunc.i64 v0\n return v1\n}\n",
            "i128 ABI",
        ),
        (
            "func @bad(i32x4) {\nblock0(v0: i32x4):\n return\n}\n",
            "vector ABI",
        ),
        (
            "func @bad() -> (i64, i64) {\nblock0:\n v0 = iconst.i64 1\n return v0, v0\n}\n",
            "multiple-result",
        ),
        ("data @t = thread_local align 8 { i64 0 }\n", "thread-local"),
        ("file f0 = \"x.c\"\n", "debug information"),
    ];
    for (src, expected) in cases {
        let error = compile(src).unwrap_err();
        assert!(error.contains(expected), "{expected}: {error}");
    }
    // Unreferenced declarations (header prototypes) need no symbol.
    let unused = "decl @printf(ptr, ...) -> i32\ndecl @agg(ptr byval {i64})\nfunc @ok() {\nblock0:\n return\n}\n";
    compile(unused).unwrap();
    let indirect = "func @bad(ptr) -> i32 {\nblock0(v0: ptr):\n v1 = call_indirect v0(v0) : (ptr, ...) -> i32\n return v1\n}\n";
    assert!(compile(indirect).unwrap_err().contains("variadic"));
}

#[test]
fn per_function_support_report() {
    let src = "func @ok(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}\nfunc @bad() -> ptr {\nblock0:\n v0 = intrinsic stack_save()\n return v0\n}\n";
    let report = keel_cranelift::function_support(&keel_ir::parse(src).unwrap());
    assert_eq!(report.len(), 2);
    assert!(report[0].1.is_ok());
    assert!(report[1].1.as_ref().unwrap_err().contains("stack_save"));
}

#[test]
fn clif_shapes_match_keel_semantics() {
    let text = clif(include_str!("fixtures/bridge.keel"));
    for needle in [
        "sdiv",
        "srem",
        "udiv",
        "urem",
        "smulhi",
        "umulhi",
        "rotl",
        "rotr",
        "brif",
        "call_indirect",
        "shuffle",
        "bitselect",
        "extractlane",
        "insertlane",
        "fcvt_from_sint.f64",
        "fcvt_from_uint.f32",
    ] {
        assert!(text.contains(needle), "missing {needle}");
    }
    // Saturating conversions; Keel `ne` is unordered-or-not-equal.
    let core = clif(include_str!("../../keel-llvm/tests/fixtures/core.keel"));
    for needle in [
        "fcvt_to_sint_sat.i32",
        "fcvt_to_uint_sat.i64",
        "fcmp ne",
        "fcmp uno",
        "call fn",
    ] {
        assert!(core.contains(needle), "missing {needle}");
    }
    assert!(!core.contains("fcvt_to_sint.") && !core.contains("fcvt_to_uint."));
    // Volatile accesses are fenced by sequence points and never `notrap`.
    let vol = clif(
        "func @v(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 volatile notrap v0\n return v1\n}\n",
    );
    assert_eq!(vol.matches("sequence_point").count(), 2);
    assert!(!vol.contains("notrap"));
}

#[test]
fn volatile_survives_speed_optimization() {
    if !common::host_ok() {
        return;
    }
    let src = "func @vol(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 volatile v0\n v2 = load.i64 volatile v0\n v3 = isub v1, v2\n store volatile v3, v0\n store volatile v3, v0\n return v3\n}\n\
               func @plain(ptr) -> i64 {\nblock0(v0: ptr):\n v1 = load.i64 align 8 v0\n v2 = load.i64 align 8 v0\n v3 = isub v1, v2\n store align 8 v3, v0\n store align 8 v3, v0\n return v3\n}\n";
    let dir = common::workdir("volatile");
    let obj = common::write(&dir, "vol.o", &compile(src).unwrap());
    let text =
        String::from_utf8(common::run(Command::new("otool").arg("-tv").arg(&obj)).stdout).unwrap();
    let (vol, plain) = text.split_once("_plain:").unwrap();
    let count = |s: &str, op: &str| {
        s.lines()
            .filter(|l| l.contains(&format!("\t{op}\t")))
            .count()
    };
    assert_eq!((count(vol, "ldr"), count(vol, "str")), (2, 2), "{vol}");
    // Negative control: ordinary accesses are merged at opt_level=speed.
    assert!(
        count(plain, "ldr") < 2 && count(plain, "str") < 2,
        "{plain}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn deterministic_objects() {
    let module = keel_ir::parse(include_str!("fixtures/bridge.keel")).unwrap();
    for opt in [OptLevel::None, OptLevel::Speed] {
        let a = keel_cranelift::compile_object(&module, &options(opt)).unwrap();
        let b = keel_cranelift::compile_object(&module, &options(opt)).unwrap();
        assert_eq!(a, b);
        assert_eq!(&a[..4], &0xfeedfacf_u32.to_le_bytes(), "64-bit Mach-O");
        // The CLIF verifier does not change output.
        let unverified = keel_cranelift::compile_object(
            &module,
            &Options {
                verify_clif: false,
                ..options(opt)
            },
        )
        .unwrap();
        assert_eq!(a, unverified);
    }
}

#[test]
fn cli_requires_target_and_preserves_output_on_failure() {
    let exe = env!("CARGO_BIN_EXE_keel-cranelift");
    let dir = common::workdir("cli");
    let input = dir.join("bad.keel");
    let output = dir.join("out.o");
    std::fs::write(&input, "func @bad(i64) -> ptr {\nblock0(v0: i64):\n v1 = intrinsic stack_alloc(v0)\n return v1\n}\n").unwrap();
    std::fs::write(&output, "sentinel").unwrap();
    let missing = Command::new(exe)
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--target"));
    let wrong = Command::new(exe)
        .args(["--target", "x86_64-unknown-linux-gnu"])
        .arg(&input)
        .args(["-o", "-"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&wrong.stderr).contains("unsupported target"));
    let failed = Command::new(exe)
        .args(["--target", "arm64-apple-macosx11.0.0", "--opt", "speed"])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(!failed.status.success());
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(stderr.contains("function @bad: block0, inst0"), "{stderr}");
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "sentinel");
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        2,
        "temporary file left"
    );

    std::fs::write(
        &input,
        "func @ok(i64) -> i64 {\nblock0(v0: i64):\n return v0\n}\n",
    )
    .unwrap();
    common::run(
        Command::new(exe)
            .args(["--target", "arm64-apple-macosx11.0.0"])
            .arg(&input)
            .arg("-o")
            .arg(&output),
    );
    assert_eq!(
        &std::fs::read(&output).unwrap()[..4],
        &0xfeedfacf_u32.to_le_bytes()
    );
    let text = common::run(
        Command::new(exe)
            .args(["--target", "arm64-apple-macosx11.0.0", "--emit", "clif"])
            .arg(&input)
            .args(["-o", "-"]),
    );
    assert!(String::from_utf8_lossy(&text.stdout).contains("apple_aarch64"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lifecycle_metadata_cannot_be_silently_exported() {
    for attr in ["constructor", "destructor"] {
        let src = format!("func @callback() cc(c) {attr} {{\nblock0:\n return\n}}\n");
        assert!(compile(&src).unwrap_err().contains("lifecycle"));
    }
}

#[test]
fn custom_placement_and_retention_cannot_be_silently_exported() {
    for src in [
        "func @f() retain {\nblock0:\n return\n}\n",
        "func @f() section \".custom\" {\nblock0:\n return\n}\n",
        "data @x = retain align 4 { i32 7 }\n",
        "data @x = section \".custom\" align 4 { i32 7 }\n",
        "data @x = thread_local retain section \".tls_custom\" align 4 { i32 7 }\n",
    ] {
        assert!(
            compile(src)
                .unwrap_err()
                .contains("custom sections or linker retention")
        );
    }
}

#[test]
fn f80_values_and_constants_are_explicitly_unsupported() {
    for src in [
        "func @f(f80) -> f80 {\nblock0(v0: f80):\n return v0\n}\n",
        "func @f() {\nblock0:\n v0 = fconst.f80 0x3fff, 0x8000000000000000\n return\n}\n",
    ] {
        assert!(compile(src).unwrap_err().contains("f80"));
    }
}
