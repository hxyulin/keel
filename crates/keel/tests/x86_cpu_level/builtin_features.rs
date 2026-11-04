//! Unoptimized C reports a builtin call that lacks its features at the call;
//! optimized C leaves the check to code generation, after dead code is gone.
use super::keel;
use std::fs;

#[test]
fn unoptimized_c_reports_missing_builtin_features_at_the_call() {
    let dir = std::env::temp_dir().join(format!("keel-builtin-features-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let missing = dir.join("missing.c");
    fs::write(
        &missing,
        "#include <immintrin.h>\n__m512i f(__m512i a) {\n  return _mm512_slli_epi32(a, 3);\n}\n",
    )
    .unwrap();
    let dead = dir.join("dead.c");
    fs::write(
        &dead,
        "#include <immintrin.h>\nint f(__m512i *p) { if (0) *p = _mm512_slli_epi32(*p, 3); return 0; }\n",
    )
    .unwrap();
    let run = |source: &std::path::Path, args: &[&str]| {
        keel()
            .args(["cc", "--target", "x86_64-unknown-linux-gnu"])
            .args(args)
            .arg(source)
            .arg("-o")
            .arg(dir.join("out.s"))
            .output()
            .unwrap()
    };
    let output = run(&missing, &["-O0"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("missing.c:3:"), "{stderr}");
    assert!(
        stderr.contains("requires target feature `avx512f`, which `f` does not enable"),
        "{stderr}"
    );
    assert!(
        run(&missing, &["-O0", "--target-features", "+avx512f"])
            .status
            .success()
    );
    // The optimizer removes the dead call before code generation checks it.
    assert!(run(&dead, &["-O2"]).status.success());
    let _ = fs::remove_dir_all(&dir);
}
