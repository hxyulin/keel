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
"#;
const HARNESS: &str = r#"
#include <stdint.h>
typedef struct { unsigned long a,b; } Pair;
extern unsigned long check(unsigned);
extern unsigned long copies(Pair);
static unsigned choose(unsigned x){return (x&1)?x+3:x^17;}
int main(void){
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
