//! End-to-end C -> optimized vector IR -> native object -> Clang harness.
use std::{fs, path::Path, process::Command};
const KERNELS: &str = r#"
void addf(float *d,const float *a,const float *b,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i]+b[i];}
void addu(unsigned *d,const unsigned *a,const unsigned *b,unsigned n){for(unsigned i=0;i<n;i++)d[i]=a[i]+b[i];}
void addl(unsigned long *d,const unsigned long *a,const unsigned long *b,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=(a[i]+b[i])^a[i];}
void divd(double *d,const double *a,const double *b,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i]/b[i];}
void scale(float *a,float x,unsigned long n){for(unsigned long i=0;i<n;i++)a[i]=a[i]*x;}
void fill(unsigned *a,unsigned x,int n){for(int i=0;i<n;i++)a[i]=x;}
void mul32(unsigned *d,const unsigned *a,const unsigned *b,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i]*b[i];}
void copyf(float *d,const float *a,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i];}
"#;
const HARNESS: &str = r#"
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#define DECL(N,T,C) extern void N(T*,const T*,const T*,C);
DECL(addf,float,unsigned long) DECL(addu,unsigned,unsigned) DECL(addl,unsigned long,unsigned long) DECL(divd,double,unsigned long) DECL(mul32,unsigned,unsigned long)
extern void scale(float*,float,unsigned long);extern void fill(unsigned*,unsigned,int);extern void copyf(float*,const float*,unsigned long);
#define CHECK(N,T,EXPR) do { \
 T actual[1536],expected[1536]; \
 for(unsigned n=0;n<=129;n++) for(int delta=-8;delta<=8;delta++) for(unsigned alias=0;alias<3;alias++){ \
  for(unsigned j=0;j<1536;j++) actual[j]=expected[j]=(T)(j%31+1); \
  unsigned d=128,a=d+delta,b=alias==0?768:alias==1?d:d-delta; \
  for(unsigned i=0;i<n;i++){T x=expected[a+i],y=expected[b+i];expected[d+i]=(EXPR);} \
  N(actual+d,actual+a,actual+b,n); if(memcmp(actual,expected,sizeof actual))return __LINE__; \
 } } while(0)
int main(void){
 CHECK(addf,float,x+y);CHECK(addu,unsigned,x+y);CHECK(addl,unsigned long,(x+y)^x);CHECK(divd,double,x/y);CHECK(mul32,unsigned,x*y);
 float a[4097];for(unsigned i=0;i<4097;i++)a[i]=(float)(i%17);scale(a,3.0f,4097);for(unsigned i=0;i<4097;i++)if(a[i]!=(float)(i%17)*3.0f)return 31;
 unsigned u[100];for(unsigned i=0;i<100;i++)u[i]=9;fill(u,17,-1);for(unsigned i=0;i<100;i++)if(u[i]!=9)return 32;fill(u,17,99);for(unsigned i=0;i<100;i++)if(u[i]!=(i==99?9:17))return 33;
 // End exactly at a protected page: the vector tail must never overread.
 long page=sysconf(_SC_PAGESIZE);char *p=mmap(0,6*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED)return 34;
 for(unsigned j=1;j<6;j+=2)if(mprotect(p+j*page,page,PROT_NONE))return 35;
 for(unsigned n=0;n<80;n++){float *d=(float*)(p+page)-n,*x=(float*)(p+3*page)-n,*y=(float*)(p+5*page)-n;for(unsigned i=0;i<n;i++){x[i]=(float)i;y[i]=2;}addf(d,x,y,n);for(unsigned i=0;i<n;i++)if(d[i]!=(float)i+2)return 36;copyf(d,x,n);if(memcmp(d,x,4*n))return 37;}
 munmap(p,6*page);return 0;
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
fn frontend_vectorization_is_target_aware_and_opt_in_with_full_optimization() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vectorize-emission");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), KERNELS).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let plain = keel(&dir, &["cc", "--target", target, "--emit-ir", "kernels.c"]);
        assert!(!plain.contains("vfadd"));
        fs::write(dir.join("scalar.keel"), &plain).unwrap();
        let via_opt = keel(&dir, &["opt", "--target", target, "scalar.keel"]);
        assert!(via_opt.contains("vfadd"));
        assert_eq!(via_opt.contains("vimul"), target.starts_with("aarch64"));
        let basic = keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "--opt-tier",
                "basic",
                "--emit-ir",
                "kernels.c",
            ],
        );
        assert!(!basic.contains("vfadd"));
        let full = keel(
            &dir,
            &["cc", "--target", target, "-O", "--emit-ir", "kernels.c"],
        );
        assert!(
            full.contains("vfadd")
                && full.contains("vfdiv")
                && full.contains("viadd")
                && full.contains("vsplat"),
            "{full}"
        );
        assert_eq!(full.contains("vimul"), target.starts_with("aarch64"));
        let object = if target.starts_with("aarch64") {
            "arm.o"
        } else {
            "x86.o"
        };
        keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O",
                "--emit-object",
                "kernels.c",
                "-o",
                object,
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
fn native_vectorized_c_preserves_overlap_tails_signed_counts_and_page_boundaries() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vectorize-runtime");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), KERNELS).unwrap();
    fs::write(dir.join("harness.c"), HARNESS).unwrap();
    for extra in [vec![], vec!["--regalloc", "fast"], vec!["--isel", "all"]] {
        let mut flags = vec!["cc", "-O", "--emit-object", "kernels.c", "-o", "kernels.o"];
        flags.extend(extra);
        keel(&dir, &flags);
        command(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "-fno-vectorize",
            "-fno-slp-vectorize",
            "-ffp-contract=off",
            "harness.c",
            "kernels.o",
            "-o",
            "test",
        ]));
        command(&mut Command::new(dir.join("test")));
    }
    fs::remove_dir_all(dir).unwrap();
}
