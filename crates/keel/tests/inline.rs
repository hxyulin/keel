//! Native C ABI and observable behavior after module-level inlining.
use std::{fs, path::Path, process::Command};
const SOURCE: &str = r#"
typedef struct { unsigned long a, b; } Pair;
static Pair make(unsigned x) { Pair p; p.a=x; p.b=7; return p; }
static Pair modify(Pair p, Pair *original) { original->a=99; p.b+=p.a; return p; }
static unsigned choose(unsigned x, unsigned *counter) { (*counter)++; if(x&1)return x+3; return x^17; }
static unsigned long partial(unsigned long x,unsigned short y) {
 union { unsigned long n; unsigned char b[8]; } u; u.n=x; u.b[2]=(unsigned char)y;u.b[3]=(unsigned char)(y>>8);return u.n;
}
static unsigned long local(unsigned x) { volatile unsigned y=x; return y; }
unsigned long check(unsigned x) {
 Pair p=make(x); Pair r=modify(p,&p); unsigned count=0;
 unsigned a=choose(x,&count),b=choose(x+1,&count);
 return r.a+r.b+p.a+count+a+b+partial(0x123456789abcdef0ul,(unsigned short)x)+local(x);
}
unsigned long copies(Pair p) { Pair r=modify(p,&p);return r.a+3*r.b+5*p.a+7*p.b; }
static inline Pair dup_value(Pair p) {
 if ((unsigned)p.b >= 4294967285u) (*(unsigned *)p.a)++;
 return p;
}
unsigned long dup_check(unsigned tag, unsigned initial) {
 unsigned count=initial;
 Pair p={(unsigned long)&count,tag}, r;
 r=dup_value(p); r=dup_value(r); r=dup_value(r); r=dup_value(r);
 return count+r.b+(r.a==(unsigned long)&count);
}
"#;
const HARNESS: &str = r#"
#include <stdint.h>
typedef struct { unsigned long a,b; } Pair;
extern unsigned long check(unsigned);
extern unsigned long copies(Pair);
extern unsigned long dup_check(unsigned, unsigned);
static unsigned choose(unsigned x){return (x&1)?x+3:x^17;}
int main(void){
 unsigned tags[]={0,3,4294967284u,4294967285u,4294967295u};
 unsigned initials[]={0,4294967294u};
 for(unsigned i=0;i<5;i++)for(unsigned j=0;j<2;j++){
  unsigned expected=initials[j]+(tags[i]>=4294967285u?4u:0u);
  if(dup_check(tags[i],initials[j])!=(unsigned long)expected+tags[i]+1ul)return 3;
 }
 for(unsigned i=0;i<10000;i++){
  unsigned x=i*393241u;
  unsigned long partial=(0x123456789abcdef0ul&~0xffff0000ul)|((unsigned long)(unsigned short)x<<16);
  unsigned long expect=(unsigned long)x+(7ul+x)+99+2+choose(x)+choose(x+1)+partial+x;
  if(check(x)!=expect)return 1;
  Pair p={x,7};if(copies(p)!=(unsigned long)x+3*(7ul+x)+5*99+7*7)return 2;
 }
 return 0;
}
"#;
fn command(c: &mut Command) -> String {
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
    command(
        Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(dir)
            .args(args),
    )
}
#[test]
fn full_optimization_inlines_private_helpers_and_keeps_other_tiers() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inline-emission");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let basic = keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "--opt-tier",
                "basic",
                "--emit-ir",
                "source.c",
            ],
        );
        assert!(basic.contains("call @make("));
        let full = keel(
            &dir,
            &["cc", "--target", target, "-O", "--emit-ir", "source.c"],
        );
        assert!(!full.contains("call @make("), "{full}");
        assert!(!full.contains("call @choose("), "{full}");
        assert!(!full.contains("call @modify("), "{full}");
        keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O",
                "--emit-object",
                "source.c",
                "-o",
                "test.o",
            ],
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn native_inline_preserves_aggregate_copies_side_effects_and_partial_stores() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inline-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("source.c"), SOURCE).unwrap();
    fs::write(dir.join("harness.c"), HARNESS).unwrap();
    // Run the identical observable oracle against an independent C compiler.
    command(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-fno-strict-aliasing",
        "source.c",
        "harness.c",
        "-o",
        "reference",
    ]));
    command(&mut Command::new(dir.join("reference")));
    for mode in [vec![], vec!["--regalloc", "fast"], vec!["--isel", "all"]] {
        let mut args = vec!["cc", "-O", "--emit-object", "source.c", "-o", "source.o"];
        args.extend(mode);
        keel(&dir, &args);
        command(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "harness.c",
            "source.o",
            "-o",
            "test",
        ]));
        command(&mut Command::new(dir.join("test")));
    }
    fs::remove_dir_all(dir).unwrap();
}

/// Two units, each with a `static helper`, where `main` calls the other
/// unit's `twice`.
const UNIT_A: &str = "static int helper(int x) { return x * 3; }\n\
int twice(int);\n\
int main(void) { return twice(helper(7)) == 42 ? 0 : 1; }\n";
const UNIT_B: &str = "static int helper(int x) { return x + 1; }\n\
int twice(int x) { return 2 * helper(x) - 2; }\n";

#[test]
fn compile_merges_units_and_inlines_across_them() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inline-whole-program");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.c"), UNIT_A).unwrap();
    fs::write(dir.join("b.c"), UNIT_B).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        for unit in ["a", "b"] {
            let (c, ir) = (format!("{unit}.c"), format!("{unit}-{target}.keel"));
            keel(
                &dir,
                &[
                    "cc",
                    "--target",
                    target,
                    "--opt-tier",
                    "none",
                    "--emit-ir",
                    &c,
                    "-o",
                    &ir,
                ],
            );
        }
        let (a, b) = (format!("a-{target}.keel"), format!("b-{target}.keel"));
        let separate = keel(&dir, &["compile", "--target", target, "-O", &a]);
        assert!(separate.contains("twice"), "{separate}");
        let asm = format!("whole-{target}.s");
        keel(
            &dir,
            &[
                "compile",
                "--target",
                target,
                "-O",
                "--internalize",
                "main",
                &a,
                &b,
                "-o",
                &asm,
            ],
        );
        let lto = [
            "cc",
            "--target",
            target,
            "-O",
            "-flto",
            "--internalize",
            "main",
        ];
        let from_c = keel(&dir, &[&lto[..], &["a.c", "b.c"]].concat());
        // `twice` became local, was inlined into `main` and then removed.
        for whole in [fs::read_to_string(dir.join(&asm)).unwrap(), from_c] {
            assert!(!whole.contains("twice"), "{whole}");
            assert!(whole.contains("main"), "{whole}");
        }
    }
    for (args, error) in [
        (&["cc", "a.c", "b.c"][..], "needs `-flto`"),
        (
            &["cc", "--internalize", "main", "a.c"],
            "`--internalize` needs `-flto`",
        ),
        (&["cc", "-flto", "-E", "a.c", "b.c"], "does not preprocess"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains(error),
            "{args:?}: {stderr}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
#[test]
fn merged_units_keep_their_own_static_helpers_natively() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("inline-whole-program-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.c"), UNIT_A).unwrap();
    fs::write(dir.join("b.c"), UNIT_B).unwrap();
    for unit in ["a", "b"] {
        let (c, ir) = (format!("{unit}.c"), format!("{unit}.keel"));
        keel(
            &dir,
            &["cc", "--opt-tier", "none", "--emit-ir", &c, "-o", &ir],
        );
    }
    let merge = |tier: &str, output: &[&str]| {
        let args = [
            &["compile", tier, "--internalize", "main", "a.keel", "b.keel"][..],
            output,
        ];
        keel(&dir, &args.concat())
    };
    // Without optimization both helpers survive under their renamed names.
    let asm = merge("-O0", &[]);
    assert!(
        asm.contains("helper.0") && asm.contains("helper.1"),
        "{asm}"
    );
    for tier in ["-O0", "-O"] {
        merge(tier, &["--emit-object", "-o", "whole.o"]);
        command(
            Command::new("clang")
                .current_dir(&dir)
                .args(["whole.o", "-o", "whole"]),
        );
        command(&mut Command::new(dir.join("whole")));
    }
    let lto = [
        "cc",
        "-O",
        "-flto",
        "--internalize",
        "main",
        "--emit-object",
    ];
    keel(&dir, &[&lto[..], &["a.c", "b.c", "-o", "lto.o"]].concat());
    command(
        Command::new("clang")
            .current_dir(&dir)
            .args(["lto.o", "-o", "lto"]),
    );
    command(&mut Command::new(dir.join("lto")));
    fs::remove_dir_all(dir).unwrap();
}
