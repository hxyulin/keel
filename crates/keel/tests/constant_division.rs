//! Constant reciprocal division through the real C frontend and both backends.
use std::{fs, path::Path, process::Command};

const DIVISORS: &[u64] = &[1, 2, 3, 5, 7, 10, 31, 37, 63, 127, 255, 641];

fn run(c: &mut Command) -> String {
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "{c:?}: {}\n{}",
        o.status,
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}

fn keel(dir: &Path, args: &[&str]) -> String {
    run(Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args(args))
}

fn sources() -> (String, String) {
    let mut source = String::new();
    let mut harness = String::from("int main(void) { unsigned long long state=123456789;\n");
    let mut declarations = String::new();
    for (ty, width) in [("unsigned", 32), ("unsigned long long", 64)] {
        let suffix = if width == 32 { "u" } else { "ull" };
        let max = u64::MAX >> (64 - width);
        let mut divisors = DIVISORS.to_vec();
        divisors.extend([max >> 1, (max >> 1) + 1, (max >> 1) + 2, max]);
        for (index, d) in divisors.into_iter().enumerate() {
            let name = format!("f{width}_{index}");
            source += &format!(
                "{ty} {name}({ty} n, {ty} *r) {{ *r=n%{d}{suffix}; return n/{d}{suffix}; }}\n"
            );
            declarations += &format!("extern {ty} {name}({ty}, {ty}*);\n");
            harness += &format!(
                "{{ volatile {ty} d={d}ull; {ty} inputs[]={{0,1,({ty})(d-1),d,({ty})(d+1),({ty})-1,({ty})(({ty})-1/d*d)}}; for(unsigned i=0;i<2000;i++) {{ state^=state<<13;state^=state>>7;state^=state<<17; {ty} n=i<7?inputs[i]:({ty})state,r; {ty} q={name}(n,&r); if(q!=n/d || r!=n%d)return 1; }} }}\n"
            );
        }
    }
    harness += "return 0; }\n";
    (source, declarations + &harness)
}

#[test]
fn reciprocal_division_emits_both_backends_in_all_tiers_and_selectors() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("constant-division-emission");
    fs::create_dir_all(&dir).unwrap();
    let (source, _) = sources();
    fs::write(dir.join("division.c"), source).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for tier in ["none", "basic", "full"] {
            let ir = keel(
                &dir,
                &[
                    "cc",
                    "--target",
                    target,
                    "--opt-tier",
                    tier,
                    "--emit-ir",
                    "division.c",
                ],
            );
            if tier == "none" {
                assert!(ir.contains("udiv") && ir.contains("urem"));
            } else {
                assert!(!ir.contains("udiv") && !ir.contains("urem"), "{ir}");
                assert!(ir.contains("umulhi"));
            }
            for selector in ["none", "trees", "folds", "hints", "all"] {
                let asm = keel(
                    &dir,
                    &[
                        "cc",
                        "--target",
                        target,
                        "--opt-tier",
                        tier,
                        "--isel",
                        selector,
                        "division.c",
                    ],
                );
                if tier != "none" {
                    assert!(
                        !asm.lines().any(|l| matches!(
                            l.split_whitespace().next(),
                            Some("udiv" | "div" | "divq" | "divl")
                        )),
                        "{asm}"
                    );
                }
            }
        }
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn reciprocal_division_executes_against_independent_dynamic_division() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("constant-division-runtime");
    fs::create_dir_all(&dir).unwrap();
    let (source, harness) = sources();
    fs::write(dir.join("division.c"), source).unwrap();
    fs::write(dir.join("harness.c"), harness).unwrap();
    for tier in ["none", "basic", "full"] {
        for selector in ["none", "trees", "folds", "hints", "all"] {
            keel(
                &dir,
                &[
                    "cc",
                    "--opt-tier",
                    tier,
                    "--isel",
                    selector,
                    "--emit-object",
                    "division.c",
                    "-o",
                    "division.o",
                ],
            );
            run(Command::new("clang").current_dir(&dir).args([
                "-O2",
                "harness.c",
                "division.o",
                "-o",
                "check",
            ]));
            run(&mut Command::new(dir.join("check")));
        }
    }
}
