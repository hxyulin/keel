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

// Requires the retained canonical preprocessed input, supplied explicitly by
// the coordinator under a correctness lease. This tests real first-pass
// activation rather than rerunning inline on already-optimized textual IR.
#[test]
#[ignore = "requires KEEL_INLINE_RETAINED_QUICKJS and a correctness lease"]
fn retained_quickjs_hot_helper_activation() {
    let path = std::env::var("KEEL_INLINE_RETAINED_QUICKJS").unwrap();
    let source = fs::read_to_string(&path).unwrap();
    let mut module = keel_c::compile(
        &source,
        &path,
        keel_c::Options {
            debug_info: false,
            target: keel_ir::Target::Aarch64AppleDarwin,
        },
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let before: Vec<_> = module
        .functions
        .iter()
        .map(|f| f.layout_insts().count())
        .collect();
    let mut report = keel_opt::OptimizationReport::default();
    keel_opt::run_passes_for_target_with_report(
        &mut module,
        &["inline"],
        keel_opt::VectorTarget::Aarch64Neon,
        &mut report,
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let sites = &report.invocations[0].inline_sites;
    let dup: Vec<_> = sites
        .iter()
        .filter(|s| s.caller == "JS_CallInternal" && s.callee == "JS_DupValue")
        .collect();
    assert_eq!(dup.len(), 62, "expected the retained first-pass workload");
    let inlined = dup.iter().filter(|s| s.outcome == "inlined").count();
    assert!(
        inlined > 26,
        "must improve on the retained baseline's 26 sites: {inlined}"
    );
    assert_eq!(
        sites
            .iter()
            .filter(|s| s.caller == "JS_CallInternal"
                && s.callee == "JS_FreeValue"
                && s.outcome == "recursive_callee")
            .count(),
        58
    );
    let mut total_growth = 0;
    for (f, old) in module.functions.iter().zip(before) {
        let growth = f.layout_insts().count() - old;
        assert!(growth <= 2048, "caller {} grew by {growth}", f.name);
        total_growth += growth;
    }
    assert!(total_growth <= 16384, "module grew by {total_growth}");
    eprintln!("retained JS_CallInternal/JS_DupValue inlined={inlined}/62; growth={total_growth}");
}
