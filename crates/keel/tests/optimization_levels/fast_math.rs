use super::{invoke, successful};
use std::{fs, path::Path};

#[test]
fn fast_math_is_opt_in_roundtrips_and_last_flag_wins() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fast-math-mode");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("in.c"), "float cancel(float x) { return x-x; }\nfloat madd(float x,float y,float z) { return x*y+z; }\nfloat dot(const float *a,const float *b,unsigned long long n) { float s=0; for(unsigned long long i=0;i<n;++i) s+=a[i]*b[i]; return s; }").unwrap();
    for target in ["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"] {
        let ir = |flags: &[&str]| {
            let mut args = vec!["cc", "--target", target, "-O3", "--emit-ir", "in.c"];
            args.extend_from_slice(flags);
            String::from_utf8(successful(&dir, &args)).unwrap()
        };
        let strict = ir(&[]);
        assert!(!strict.contains("fast_math"), "{strict}");
        let fast = ir(&["-ffast-math"]);
        assert!(fast.contains("fast_math"), "{fast}");
        assert!(fast.contains("vfadd"), "{fast}");
        assert!(
            fast.contains(" = fma ") == target.starts_with("aarch64"),
            "{fast}"
        );
        assert_eq!(strict, ir(&["-ffast-math", "-fno-fast-math"]));
        assert_eq!(fast, ir(&["-fno-fast-math", "-ffast-math"]));
        fs::write(dir.join("fast.keel"), &fast).unwrap();
        // Existing IR permissions survive an empty pipeline and compiler invocation.
        let roundtrip =
            String::from_utf8(successful(&dir, &["opt", "-p", "dce", "fast.keel"])).unwrap();
        assert!(roundtrip.contains("fast_math"));
        successful(&dir, &["compile", "--target", target, "fast.keel"]);
    }
    for (flags, expected) in [
        (vec![], false),
        (vec!["-ffast-math"], true),
        (vec!["-ffast-math", "-fno-fast-math"], false),
    ] {
        let mut args = vec!["cc", "-E", "-dM", "in.c"];
        args.extend(flags);
        let macros = String::from_utf8(successful(&dir, &args)).unwrap();
        assert_eq!(macros.contains("#define __FAST_MATH__ 1"), expected);
    }
    assert!(
        !invoke(
            &dir,
            &[
                "cc",
                "--target",
                "wasm32-unknown-unknown",
                "-ffast-math",
                "in.c"
            ]
        )
        .status
        .success()
    );
}
