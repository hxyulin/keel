//! Ordinary initializers, post-capture aliasing and conditional multi reductions.
use std::{fs, path::Path, process::Command};
fn command(c: &mut Command) -> String {
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "{c:?}\n{}\n{}",
        String::from_utf8_lossy(&o.stdout),
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
fn source() -> String {
    let corpus = include_str!("../../keel-cranelift/kernels/kernels.c");
    let tail = &corpus[corpus.find("struct acc {").unwrap()..];
    format!(
        "typedef unsigned long long u64; typedef long long i64; typedef unsigned int u32;\nvoid *memset(void *, int, unsigned long);\n{tail}\n{}",
        r#"
extern void seed(struct acc *, u64);
extern i64 *seed_alias(struct acc *);
extern void mutate(void);
u64 seeded(const i64 *v,u64 start,u64 n,u64 seed_value) {
 struct acc a; seed(&a,seed_value);
 for(u64 i=start;i<n;i++)acc_add(&a,v[i]);
 return a.lo*3+a.hi+a.count;
}
u64 alias(u64 n) {
 struct acc a; i64 *p=seed_alias(&a);
 for(u64 i=0;i<n;i++)a.lo+=(u64)p[0];
 return a.lo;
}
u64 observed(u64 n) {
 struct acc a; seed(&a,1);
 for(u64 i=0;i<n;i++){a.lo+=1;mutate();}
 return a.lo;
}
"#
    )
}
const HARNESS: &str = r#"
#include <stdint.h>
#include <string.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
typedef unsigned long long u64;typedef long long i64;
struct acc{u64 lo,hi;unsigned count;};static struct acc *captured;
void seed(struct acc *a,u64 x){a->lo=x;a->hi=~x;a->count=(unsigned)x;captured=a;}
i64 *seed_alias(struct acc *a){seed(a,1);return (i64*)&a->lo;}
void mutate(void){captured->lo*=3;}
extern u64 k_stackobj(const i64*,u64);
extern u64 seeded(const i64*,u64,u64,u64);
extern u64 alias(u64);extern u64 observed(u64);
static u64 expected(const i64 *v,u64 start,u64 n,u64 seed_value,int seeded) {
 // Independent unsigned-wide oracle, reduced only at the observable widths.
 unsigned __int128 neg=seeded?seed_value:0,pos=seeded?~seed_value:0;
 u64 count=seeded?(unsigned)seed_value:0;
 for(u64 i=start;i<n;i++){if(v[i]<0)neg+=(u64)v[i];else pos+=(u64)v[i];count++;}
 return (u64)(neg*3+pos)+(unsigned)count;
}
int main(void){
 const i64 edges[]={0,1,-1,INT64_MIN,INT64_MAX,INT64_MIN+1,INT64_MAX-1,4294967296LL,-4294967296LL};
 const u64 seeds[]={0,1,~0ULL,0xffffffffULL,0x80000000ULL,0x123456789abcdef0ULL};
 long page=sysconf(_SC_PAGESIZE);char *p=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(p==MAP_FAILED||mprotect(p+page,page,PROT_NONE))return 1;
 unsigned checks=0;
 for(unsigned pattern=0;pattern<8;pattern++)for(u64 n=0;n<=257;n++){
  i64 *v=(i64*)(p+page)-n;
  for(u64 i=0;i<n;i++){i64 x=edges[(i+pattern)%9];if(pattern==1)x=x==INT64_MIN?INT64_MAX:x<0?-x:x;if(pattern==2)x=x>0?-x:x;if(pattern==3)x=0;v[i]=x;}
  if(k_stackobj(v,n)!=expected(v,0,n,0,0))return 2;checks++;
  for(unsigned s=0;s<6;s++)for(unsigned k=0;k<5;k++){
   const u64 starts[]={0,1,3,n,n+1};u64 start=starts[k];
   if(seeded(v,start,n,seeds[s])!=expected(v,start,n,seeds[s],1)){fprintf(stderr,"seeded pattern=%u n=%llu start=%llu seed=%llx\n",pattern,n,start,seeds[s]);return 3;}checks++;
  }
 }
 if(k_stackobj(0,0)!=0)return 4;
 for(u64 n=0;n<130;n++){
  u64 a=1,b=1;for(u64 i=0;i<n;i++){a+=a;b=(b+1)*3;}
  if(alias(n)!=a||observed(n)!=b)return 5;checks+=2;
 }
 munmap(p,2*page);printf("%u stack-object oracle checks passed\n",checks);return 0;
}
"#;
#[test]
fn stack_object_pipeline_vectorizes_both_native_targets() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stackobj-static");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), source()).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let ir = keel(
            &dir,
            &[
                "cc",
                "-O",
                "--verify-each",
                "--target",
                target,
                "--emit-ir",
                "kernels.c",
            ],
        );
        let module = keel_ir::parse(&ir).unwrap();
        keel_ir::verify_module(&module).unwrap();
        for name in ["k_stackobj", "seeded"] {
            let f = module.functions.iter().find(|f| f.name == name).unwrap();
            assert!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::VectorIcmp { .. })),
                "{f}"
            );
            assert!(
                !f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Store { .. })),
                "{f}"
            );
            let entry = f.entry_block().unwrap();
            assert_eq!(
                f.layout_insts()
                    .filter(|(b, i)| *b == entry
                        && matches!(f.inst_data(*i), keel_ir::InstData::Load { .. }))
                    .count(),
                // Canonical memset supplies zero initializers; an opaque seed
                // call still requires loading all three aggregate fields.
                if name == "k_stackobj" { 0 } else { 3 },
                "{f}"
            );
        }
        for name in ["alias", "observed"] {
            let f = module.functions.iter().find(|f| f.name == name).unwrap();
            assert!(
                f.layout_insts()
                    .any(|(_, i)| matches!(f.inst_data(i), keel_ir::InstData::Store { .. })),
                "{f}"
            );
        }
        keel(
            &dir,
            &[
                "cc",
                "-O",
                "--verify-each",
                "--target",
                target,
                "--emit-object",
                "kernels.c",
                "-o",
                "kernels.o",
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
fn native_initializers_aliases_wrapping_counts_and_guarded_tails() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("stackobj-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), source()).unwrap();
    fs::write(dir.join("driver.c"), HARNESS).unwrap();
    for flags in [
        vec![],
        vec!["--regalloc", "fast"],
        vec!["--isel-tier", "baseline"],
    ] {
        let mut args = vec![
            "cc",
            "-O",
            "--verify-each",
            "--emit-object",
            "kernels.c",
            "-o",
            "kernels.o",
        ];
        args.extend(flags);
        keel(&dir, &args);
        command(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "-fno-vectorize",
            "-fno-slp-vectorize",
            "-ffp-contract=off",
            "driver.c",
            "kernels.o",
            "-o",
            "oracle",
        ]));
        assert!(
            command(&mut Command::new(dir.join("oracle")))
                .contains("64244 stack-object oracle checks passed")
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
