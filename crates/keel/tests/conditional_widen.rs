//! Exercise permanent-corpus C shapes through the Full frontend pipeline.
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};
fn run(cmd: &mut Command) {
    let o = cmd.output().unwrap();
    assert!(
        o.status.success(),
        "{cmd:?}: {}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
}
fn text(path: &Path) -> Vec<u8> {
    let data = fs::read(path).unwrap();
    let f = object::File::parse(data.as_slice()).unwrap();
    f.sections()
        .find(|s| matches!(s.name(), Ok(".text" | "__text")))
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn full_frontend_conditional_widening_preserves_corpus_shapes_and_object_routes() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("conditional-widen-frontend");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("kernels.c"),
        include_str!("../../../suite/runtime-kernels/kernels.c"),
    )
    .unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let stem = target;
        let cli = |args: &[&str]| {
            run(Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args(args));
        };
        cli(&[
            "cc",
            "--verify-each",
            "--opt-tier",
            "full",
            "--target",
            target,
            "--emit-ir",
            "kernels.c",
            "-o",
            &format!("{stem}.keel"),
        ]);
        let ir = fs::read_to_string(dir.join(format!("{stem}.keel"))).unwrap();
        if target.starts_with("aarch64") {
            let m = keel_ir::parse(&ir).unwrap();
            for name in ["rk_positive32", "rk_balance32"] {
                let f = m.functions.iter().find(|f| f.name == name).unwrap();
                assert!(f.to_string().contains("widen_low.i64x2"), "{f}");
            }
        }
        for isel in ["none", "all"] {
            let out = format!("{stem}-{isel}");
            let input = format!("{stem}.keel");
            cli(&[
                "compile",
                "--verify-each",
                "--target",
                target,
                "--isel",
                isel,
                "--emit-object",
                &input,
                "-o",
                &format!("{out}.o"),
            ]);
            cli(&[
                "compile",
                "--verify-each",
                "--target",
                target,
                "--isel",
                isel,
                &input,
                "-o",
                &format!("{out}.s"),
            ]);
            cli(&[
                "asm",
                "--target",
                target,
                &format!("{out}.s"),
                "-o",
                &format!("{out}-text.o"),
            ]);
            assert_eq!(
                text(&dir.join(format!("{out}.o"))),
                text(&dir.join(format!("{out}-text.o")))
            );
        }
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    {
        // Reuse the protected-page oracle with ABI adapters for the two corpus
        // signatures. Nonzero starts/seeds are covered by the IR native gate.
        let oracle = include_str!("../../keel-opt/tests/conditional_widen/oracle.c");
        let adapters = "\nextern uint64_t rk_positive32(const void*,const void*,const void*,uint64_t);\nextern uint64_t rk_balance32(const void*,const void*,const void*,uint64_t);\nuint64_t positive(const void *a,uint64_t n,uint64_t start,uint64_t seed){return seed+(start<n?rk_positive32((const char*)a+4*start,0,0,n-start):0);}\nuint64_t balance(const void *a,uint64_t n,uint64_t start,uint64_t seed){return seed+(start<n?rk_balance32((const char*)a+4*start,0,0,n-start):0);}\n";
        fs::write(dir.join("oracle.c"), oracle.to_owned() + adapters).unwrap();
        for isel in ["none", "all"] {
            run(Command::new("clang").current_dir(&dir).args([
                "-O2",
                "-DORACLE_CASES=2",
                "oracle.c",
                &format!("aarch64-apple-darwin-{isel}.o"),
                "-o",
                "test",
            ]));
            run(&mut Command::new(dir.join("test")));
        }
    }
}
