use keel_ir::{InstData, Target};

#[test]
fn unsigned_checked_proofs_and_fallbacks() {
    for target in Target::ALL {
        for op in ["memcpy", "memmove", "memset"] {
            for (length, bound, safe) in [
                ("8", "8", true),
                ("8", "-1UL", true),
                ("n", "-1UL", true),
                ("0", "0", true),
                ("8", "7", false),
                ("n", "8", false),
                ("-1UL", "8", false),
                ("8", "b", false),
                ("8", "__builtin_object_size(d,0)", true),
                ("-1UL", "-2UL", false),
                ("-2UL", "-2UL", true),
            ] {
                let value = if op == "memset" { "511" } else { "s" };
                let source = format!(
                    "void *f(void *d, const void *s, unsigned long n, unsigned long b) {{ return __builtin___{op}_chk(d,{value},{length},{bound}); }}"
                );
                let mut m = keel_c::compile(
                    &source,
                    "safe.c",
                    keel_c::Options {
                        target,
                        ..Default::default()
                    },
                )
                .unwrap();
                let f = &m.functions[0];
                assert_eq!(
                    f.layout_insts()
                        .any(|(_, i)| matches!(f.inst_data(i), InstData::Intrinsic { .. })),
                    safe,
                    "{source}"
                );
                keel_ir::verify_module(&m).unwrap();
                keel_codegen::compile(&m, target).unwrap();
                keel_opt::optimize(&mut m).unwrap();
                keel_ir::verify_module(&m).unwrap();
                keel_codegen::compile(&m, target).unwrap();
            }
        }
    }
}

#[test]
fn zero_checked_keeps_argument_effects_and_volatile_reads() {
    for target in Target::ALL {
        let source = "void *d(void); const void *s(void); unsigned long effect(void); void *f(volatile unsigned long *p) { return __builtin___memmove_chk(d(),s(),(effect(),0),(*p,8)); }";
        let mut m = keel_c::compile(
            source,
            "effects.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        keel_opt::optimize(&mut m).unwrap();
        let f = m.functions.iter().find(|f| f.name == "f").unwrap();
        let calls: Vec<_> = f
            .layout_insts()
            .filter_map(|(_, i)| match f.inst_data(i) {
                InstData::Call { func, .. } => Some(f.callee(*func).name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(calls, ["d", "s", "effect"]);
        assert_eq!(
            f.layout_insts()
                .filter(
                    |(_, i)| matches!(f.inst_data(*i),InstData::Load{flags,..} if flags.volatile)
                )
                .count(),
            1
        );
        keel_codegen::compile(&m, target).unwrap();
    }
}

// The enclosing cargo invocation must hold suite/fast_opt_locked.py's lease.
#[test]
fn native_checked_exact_footprints_and_both_target_objects() {
    use std::{fs, process::Command};
    fn run(c: &mut Command) {
        let o = c.output().unwrap();
        assert!(
            o.status.success(),
            "{c:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("checked-safe-native");
    fs::create_dir_all(&dir).unwrap();
    let mut source = String::new();
    let mut declarations = String::new();
    let mut checks = String::new();
    for n in 0..=32 {
        for op in ["memcpy", "memmove", "memset"] {
            let value = if op == "memset" { "fill" } else { "s" };
            source.push_str(&format!("void *k{op}{n}(void *d,const void *s,int fill) {{ return __builtin___{op}_chk(d,{value},{n},-1UL); }}\n"));
            declarations.push_str(&format!("void *k{op}{n}(void*,const void*,int);\n"));
            checks.push_str(&format!(
                "check(k{op}{n},{n},{});\n",
                i32::from(op == "memset")
            ));
        }
    }
    source.push_str("void vf(volatile double *p,double x) { *p=x; }\n");
    for target in Target::ALL {
        let mut m = keel_c::compile(
            &source,
            "checked-native.c",
            keel_c::Options {
                target,
                ..Default::default()
            },
        )
        .unwrap();
        keel_opt::optimize(&mut m).unwrap();
        keel_ir::verify_module(&m).unwrap();
        let asm = keel_codegen::compile(&m, target).unwrap();
        assert!(
            !asm.contains("__memcpy_chk")
                && !asm.contains("__memmove_chk")
                && !asm.contains("__memset_chk")
        );
        let path = dir.join(format!("{}.s", target.triple()));
        fs::write(&path, asm).unwrap();
        run(Command::new("clang")
            .args(["-target", target.triple(), "-c"])
            .arg(&path)
            .arg("-o")
            .arg(path.with_extension("o")));
    }
    let driver = format!(
        r#"
#include <sys/mman.h>
#include <unistd.h>
#include <stdlib.h>
#include <string.h>
{declarations}
void vf(volatile double*,double);
typedef void *(*Fn)(void*,const void*,int);
static unsigned char *area(long p) {{
 unsigned char *a=mmap(0,p*3,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(a==MAP_FAILED||mprotect(a+p,p,PROT_READ|PROT_WRITE))abort();
 return a;
}}
static void check(Fn f,int n,int set) {{
 long p=sysconf(_SC_PAGESIZE); unsigned char *a=area(p),*b=area(p);
 int fills[]={{0,127,128,255,256,511,-1,(-2147483647-1),2147483647}};
 for(int end=0;end<2;end++) for(int k=0;k<9;k++) {{
  memset(a+p,19,p);memset(b+p,73,p);
  unsigned char *d=a+p+(end?p-n:0),*s=b+p+(end?p-n:0);
  if(f(d,s,fills[k])!=d)abort();
  for(int i=0;i<n;i++)if(d[i]!=(set?(unsigned char)fills[k]:73))abort();
  if(n<p && (end?d[-1]:d[n])!=19)abort();
 }}
 for(int da=0;da<8;da++) for(int sa=0;sa<8;sa++) {{
  memset(a+p,19,p);memset(b+p,73,p);
  f(a+p+16+da,b+p+16+sa,511);
  for(int i=0;i<64;i++)if(a[p+i]!=(i>=16+da&&i<16+da+n?(set?255:73):19))abort();
 }}
 munmap(a,p*3);munmap(b,p*3);
}}
int main(void) {{ {checks} volatile double d=0;vf(&d,-0.0);unsigned long bits;memcpy(&bits,(const void*)&d,8);return bits!=0x8000000000000000UL; }}
"#
    );
    let driver_path = dir.join("driver.c");
    fs::write(&driver_path, driver).unwrap();
    let obj = dir.join(format!("{}.o", keel_c::Options::default().target.triple()));
    let exe = dir.join("check");
    run(Command::new("clang")
        .arg(&driver_path)
        .arg(obj)
        .arg("-o")
        .arg(&exe));
    run(&mut Command::new(exe));
}
