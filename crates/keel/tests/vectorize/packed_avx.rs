//! C loops must reach packed 128-bit arithmetic through both CLI pipelines.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::command as run;
use super::keel;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use std::process::Command;
use std::{fs, path::Path};

const TARGET: &str = "x86_64-unknown-linux-gnu";
const STRICT: [(&str, &str, &str); 2] = [
    ("orderedadd", "(x+y)+y", "(a[i]+b[i])+b[i]"),
    ("muladd", "x*y+x", "a[i]*b[i]+a[i]"),
];
const OPS: [(&str, &str); 4] = [("add", "+"), ("sub", "-"), ("mul", "*"), ("div", "/")];

fn kernels() -> String {
    let mut c = String::new();
    for (ty, suffix) in [("float", "ps"), ("double", "pd")] {
        for (op, symbol) in OPS {
            c += &format!(
                "void {op}{suffix}({ty} *d,const {ty} *a,const {ty} *b,unsigned long n){{for(unsigned long i=0;i<n;i++)d[i]=a[i]{symbol}b[i];}}\n"
            );
        }
        for (op, _, expr) in STRICT {
            c += &format!(
                "void {op}{suffix}({ty} *d,const {ty} *a,const {ty} *b,unsigned long n){{for(unsigned long i=0;i<n;i++)d[i]={expr};}}\n"
            );
        }
    }
    c
}

fn check_assembly(text: &str, avx: bool) {
    let instructions: Vec<_> = text
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    for suffix in ["ps", "pd"] {
        for (op, _) in OPS {
            let mnemonic = format!("{}{op}{suffix}", if avx { "v" } else { "" });
            assert!(
                instructions.contains(&mnemonic.as_str()),
                "missing {mnemonic}\n{text}"
            );
        }
    }
    assert!(!text.contains("ymm") && !text.contains("zmm"), "{text}");
    assert!(!text.contains("fmadd") && !text.contains("fmsub"), "{text}");
    if !avx {
        assert!(
            !instructions.iter().any(|i| i.starts_with('v')),
            "baseline emitted AVX: {text}"
        );
    }
}

fn check_vector_ir(text: &str) {
    for (shape, suffix) in [("f32x4", "ps"), ("f64x2", "pd")] {
        for (op, _) in OPS {
            let function = text
                .split("func @")
                .find(|f| f.starts_with(&format!("{op}{suffix}(")))
                .unwrap();
            assert!(
                function.contains(&format!("load.{shape}"))
                    && function.contains(&format!("vf{op} ")),
                "{function}"
            );
        }
        for (op, _, _) in STRICT {
            let function = text
                .split("func @")
                .find(|f| f.starts_with(&format!("{op}{suffix}(")))
                .unwrap();
            assert!(function.contains(&format!("load.{shape}")), "{function}");
            let arithmetic: Vec<_> = function
                .lines()
                .filter_map(|l| {
                    l.split_once(" = ")
                        .map(|(_, rhs)| rhs.split_whitespace().next().unwrap())
                })
                .filter(|op| op.starts_with("vf"))
                .collect();
            assert_eq!(
                arithmetic,
                if op == "muladd" {
                    vec!["vfmul", "vfadd"]
                } else {
                    vec!["vfadd", "vfadd"]
                },
                "{function}"
            );
        }
    }
}

#[test]
fn c_loops_select_packed_sse2_or_avx_through_cc_and_opt_compile() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("packed-avx-structure");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), kernels()).unwrap();
    let scalar = keel(&dir, &["cc", "--target", TARGET, "--emit-ir", "kernels.c"]);
    fs::write(dir.join("scalar.keel"), scalar).unwrap();
    let vector = keel(&dir, &["opt", "--target", TARGET, "scalar.keel"]);
    check_vector_ir(&vector);
    fs::write(dir.join("vector.keel"), &vector).unwrap();
    let cc_vector = keel(
        &dir,
        &["cc", "--target", TARGET, "-O", "--emit-ir", "kernels.c"],
    );
    check_vector_ir(&cc_vector);
    // Features affect instruction selection, not vector widths or FP transformations.
    assert_eq!(
        cc_vector,
        keel(
            &dir,
            &[
                "cc",
                "--target",
                TARGET,
                "-O",
                "--target-features",
                "avx",
                "--emit-ir",
                "kernels.c"
            ]
        )
    );
    check_assembly(
        &keel(
            &dir,
            &[
                "cc",
                "--target",
                TARGET,
                "-O",
                "--target-features",
                "none",
                "kernels.c",
            ],
        ),
        false,
    );
    for allocation in ["default", "fast"] {
        for selection in ["none", "trees", "folds", "hints", "all"] {
            for avx in [false, true] {
                for (mode, input, optimize) in
                    [("cc", "kernels.c", true), ("compile", "vector.keel", false)]
                {
                    let mut args = vec![mode, "--target", TARGET, "--isel", selection, input];
                    if allocation == "fast" {
                        args.extend(["--regalloc", "fast"]);
                    }
                    if avx {
                        args.extend(["--target-features", "avx"]);
                    }
                    if optimize {
                        args.push("-O");
                    }
                    check_assembly(&keel(&dir, &args), avx);
                    args.extend(["--emit-object", "-o", "kernels.o"]);
                    keel(&dir, &args);
                    // Check decoded object instructions too: scalar vaddss/vaddsd cannot pass.
                    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
                    {
                        let decoded = run(Command::new("objdump").current_dir(&dir).args([
                            "-d",
                            "--no-show-raw-insn",
                            "kernels.o",
                        ]));
                        let assembly = decoded
                            .lines()
                            .filter_map(|l| {
                                l.split_once(':')
                                    .filter(|(a, _)| {
                                        a.trim().chars().all(|c| c.is_ascii_hexdigit())
                                    })
                                    .map(|(_, b)| b)
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        check_assembly(&assembly, avx);
                    }
                }
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_packed_loops_match_ordered_scalar_reference() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("packed-avx-runtime");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), kernels()).unwrap();
    let mut harness = String::from(include_str!("../packed_avx/harness.c"));
    for (ty, suffix) in [("float", "ps"), ("double", "pd")] {
        for (op, symbol) in OPS {
            harness += &format!(
                "extern void {op}{suffix}({ty}*,const {ty}*,const {ty}*,unsigned long);\nCHECK({op}{suffix},{ty},x{symbol}y)\n"
            );
        }
        for (op, expr, _) in STRICT {
            harness += &format!(
                "extern void {op}{suffix}({ty}*,const {ty}*,const {ty}*,unsigned long);\nCHECK({op}{suffix},{ty},{expr})\n"
            );
        }
    }
    harness += "int main(void){\n";
    for suffix in ["ps", "pd"] {
        for (op, _) in OPS {
            harness += &format!("if(test_{op}{suffix}())return __LINE__;\n");
        }
        for (op, _, _) in STRICT {
            harness += &format!("if(test_{op}{suffix}())return __LINE__;\n");
        }
    }
    harness += "return 0;}\n";
    fs::write(dir.join("harness.c"), harness).unwrap();
    let scalar = keel(&dir, &["cc", "--target", TARGET, "--emit-ir", "kernels.c"]);
    fs::write(dir.join("scalar.keel"), scalar).unwrap();
    fs::write(
        dir.join("vector.keel"),
        keel(&dir, &["opt", "--target", TARGET, "scalar.keel"]),
    )
    .unwrap();
    for avx in [false, true] {
        // Rust's detection includes OSXSAVE/XGETBV, not just the CPU AVX bit.
        if avx && !std::is_x86_feature_detected!("avx") {
            continue;
        }
        for allocation in ["default", "fast"] {
            for selection in ["none", "all"] {
                for (mode, input) in [("cc", "kernels.c"), ("compile", "vector.keel")] {
                    let mut args = vec![
                        mode,
                        "--target",
                        TARGET,
                        "-O",
                        "--isel",
                        selection,
                        "--emit-object",
                        input,
                        "-o",
                        "kernels.o",
                    ];
                    if allocation == "fast" {
                        args.extend(["--regalloc", "fast"]);
                    }
                    if avx {
                        args.extend(["--target-features", "avx"]);
                    }
                    keel(&dir, &args);
                    run(Command::new("clang").current_dir(&dir).args([
                        "-O2",
                        "-fno-vectorize",
                        "-fno-slp-vectorize",
                        "-ffp-contract=off",
                        "harness.c",
                        "kernels.o",
                        "-o",
                        "test",
                    ]));
                    run(&mut Command::new(dir.join("test")));
                }
            }
        }
    }
}
