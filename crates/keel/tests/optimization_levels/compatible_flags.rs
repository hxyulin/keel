use super::{invoke, successful};
use std::{fs, path::Path};

#[test]
fn math_errno_is_independent_and_ordered() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("math-errno-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "double sqrt(double); double root(double x) { return sqrt(x); } double cancel(double x) { return x-x; }").unwrap();
    for (flags, has_call, relaxed) in [
        (vec![], true, false),
        (vec!["-fno-math-errno"], false, false),
        (vec!["-fno-math-errno", "-fmath-errno"], true, false),
        (vec!["-ffast-math", "-fmath-errno"], true, true),
        (vec!["-fmath-errno", "-ffast-math"], false, true),
        (vec!["-ffast-math", "-fno-fast-math"], true, false),
        (vec!["-fno-fast-math", "-fno-math-errno"], false, false),
    ] {
        let mut args = vec![
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-ir",
            "input.c",
        ];
        args.extend_from_slice(&flags);
        let text = String::from_utf8(successful(&dir, &args)).unwrap();
        assert_eq!(text.contains("call @sqrt("), has_call, "{flags:?}\n{text}");
        let module = keel_ir::parse(&text).unwrap();
        assert_eq!(
            module
                .functions
                .iter()
                .find(|f| f.name == "cancel")
                .unwrap()
                .attrs
                .fast_math,
            relaxed
        );
    }
    for flags in [
        vec!["-fno-math-errno", "-fno-builtin"],
        vec!["-fno-math-errno", "-fno-builtin-sqrt"],
    ] {
        let mut args = vec![
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-ir",
            "input.c",
        ];
        args.extend(flags);
        assert!(
            String::from_utf8(successful(&dir, &args))
                .unwrap()
                .contains("call @sqrt(")
        );
    }
    let macros = String::from_utf8(successful(
        &dir,
        &["cc", "-E", "-dM", "-ffast-math", "-fmath-errno", "input.c"],
    ))
    .unwrap();
    assert!(!macros.contains("#define __FAST_MATH__"));
}

#[test]
fn invocation_family_flags_control_inlining_and_vectorization() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("family-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "static int helper(int x) { return x*3+1; } int caller(int x) { return helper(x); } void map(unsigned *a, unsigned *b, unsigned long long n) { for(unsigned long long i=0;i<n;i++) b[i]=a[i]+1; }").unwrap();
    let ir = |flags: &[&str]| {
        let mut args = vec![
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-ir",
            "input.c",
        ];
        args.extend_from_slice(flags);
        String::from_utf8(successful(&dir, &args)).unwrap()
    };
    let normal = ir(&["-O3"]);
    assert!(!normal.contains("call @helper("));
    assert!(normal.contains("load.i32x4"), "{normal}");
    let disabled = ir(&[
        "-O3",
        "-fno-inline-functions",
        "-fno-vectorize",
        "-fno-unroll-loops",
    ]);
    assert!(disabled.contains("call @helper("), "{disabled}");
    assert!(!disabled.contains("load.i32x4"), "{disabled}");
    let restored = ir(&[
        "-fno-inline-functions",
        "-finline-functions",
        "-fno-tree-vectorize",
        "-fvectorize",
        "-O3",
    ]);
    assert!(!restored.contains("call @helper("));
    assert!(restored.contains("load.i32x4"));
    let o0 = ir(&[
        "-O0",
        "-finline-functions",
        "-funroll-loops",
        "-ftree-loop-vectorize",
    ]);
    assert!(o0.contains("call @helper("));
    assert!(!o0.contains("load.i32x4"));
    fs::write(dir.join("input.c"), "static int helper(int x) { return x*3+1; } __attribute__((optimize(\"no-inline-functions\"))) int caller(int x) { return helper(x); } __attribute__((optimize(\"no-tree-loop-vectorize\"))) void map(unsigned *a, unsigned *b, unsigned long long n) { for(unsigned long long i=0;i<n;i++) b[i]=a[i]+1; }").unwrap();
    let local = ir(&["-O3", "-finline-functions", "-fvectorize"]);
    assert!(local.contains("call @helper("));
    assert!(!local.contains("load.i32x4"));
}

#[test]
fn local_math_modes_override_global_defaults_and_keep_inline_boundaries() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("local-math-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "double sqrt(double); __attribute__((optimize(\"no-fast-math\"))) static double strict(double x) { return sqrt(x); } __attribute__((optimize(\"fast-math\"))) static double relaxed(double x) { return sqrt(x); } double caller(double x) { return strict(x)+relaxed(x); }").unwrap();
    for flags in [vec![], vec!["-ffast-math"]] {
        let mut args = vec![
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-ir",
            "-O0",
            "input.c",
        ];
        args.extend_from_slice(&flags);
        let text = String::from_utf8(successful(&dir, &args)).unwrap();
        let mut module = keel_ir::parse(&text).unwrap();
        let strict = module
            .functions
            .iter()
            .find(|f| f.name == "strict")
            .unwrap();
        let relaxed = module
            .functions
            .iter()
            .find(|f| f.name == "relaxed")
            .unwrap();
        assert!(!strict.attrs.fast_math);
        assert!(relaxed.attrs.fast_math);
        assert!(strict.to_string().contains("call @sqrt("));
        assert!(!relaxed.to_string().contains("call @sqrt("));
        keel_opt::optimize(&mut module).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let caller = module
            .functions
            .iter()
            .find(|f| f.name == "caller")
            .unwrap()
            .to_string();
        assert!(
            caller.contains(if flags.is_empty() {
                "call @relaxed("
            } else {
                "call @strict("
            }),
            "{caller}"
        );
    }
    fs::write(dir.join("errno.c"), "double sqrt(double); __attribute__((optimize(\"fast-math,math-errno\"))) double root(double x) { return sqrt(x); }").unwrap();
    let errno = String::from_utf8(successful(
        &dir,
        &[
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "-fno-math-errno",
            "--emit-ir",
            "errno.c",
        ],
    ))
    .unwrap();
    assert!(errno.contains("call @sqrt("));
    fs::write(dir.join("required.c"), "__attribute__((always_inline,optimize(\"no-fast-math\"))) static float strict(float x) { return x-x; } float f(float x) { return strict(x); }").unwrap();
    assert!(
        !invoke(&dir, &["cc", "-ffast-math", "--emit-ir", "required.c"])
            .status
            .success()
    );
}

#[test]
fn incompatible_or_unimplemented_flags_are_diagnosed() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("unsupported-compatible-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "int f(void) { return 1; }").unwrap();
    for flags in [
        vec!["-march=native"],
        vec!["-march=x86-64-v4"],
        vec!["-march=x86-64-v2", "--target", "aarch64-unknown-linux-gnu"],
        vec!["-march=x86-64-v2", "--cpu-level", "v3"],
        vec!["--cpu-level", "v3", "-march=x86-64-v2"],
        vec!["-ffp-contract=off"],
        vec!["-fno-slp-vectorize"],
        vec!["--emit-wasm", "-fno-math-errno", "-o", "out.wasm"],
    ] {
        let mut args = vec!["cc", "--target", "x86_64-unknown-linux-gnu", "input.c"];
        args.extend_from_slice(&flags);
        assert!(!invoke(&dir, &args).status.success(), "{flags:?}");
    }
}

#[test]
fn architecture_names_match_supported_cpu_levels() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("architecture-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("input.c"),
        "int f(void) { return __atomic_always_lock_free(16,0); }",
    )
    .unwrap();
    for (architecture, level) in [
        ("-march=x86-64", "v1"),
        ("-march=x86-64-v2", "v2"),
        ("-march=x86-64-v3", "v3"),
    ] {
        let base = [
            "cc",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--emit-ir",
            "input.c",
        ];
        let mut args = base.to_vec();
        args.push(architecture);
        let arch = successful(&dir, &args);
        let mut args = base.to_vec();
        args.extend(["--cpu-level", level]);
        assert_eq!(arch, successful(&dir, &args));
    }
    let args = [
        "cc",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--emit-ir",
        "input.c",
    ];
    let mut repeated = args.to_vec();
    repeated.extend(["-march=x86-64-v3", "-march=x86-64"]);
    let mut baseline = args.to_vec();
    baseline.push("-march=x86-64");
    assert_eq!(successful(&dir, &repeated), successful(&dir, &baseline));
}
