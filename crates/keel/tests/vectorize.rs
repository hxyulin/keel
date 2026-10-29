//! End-to-end C -> optimized vector IR -> native object -> Clang harness.
#[path = "vectorize/ascii_scan.rs"]
mod ascii_scan;
#[path = "vectorize/packed_avx.rs"]
mod packed_avx;

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
void copy8(unsigned char *d,const unsigned char *a,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i];}
void copy16(unsigned short *d,const unsigned short *a,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i];}
void signedadd(unsigned *d,const unsigned *a,const unsigned *b,int n){for(int i=0;i<n;i++)d[i]=a[i]+b[i];}
void twomaps(unsigned *d,const unsigned *a,const unsigned *b,unsigned long n){for(unsigned long i=0;i<n;i++)d[i]=a[i]+b[i];for(unsigned long i=0;i<n;i++)d[i]=d[i]^a[i];}
void rangeu(unsigned *d,const unsigned *a,const unsigned *b,unsigned lo,unsigned n){for(unsigned i=lo;i<n;i++)d[i]=a[i]+b[i];}
void ranges(unsigned *d,const unsigned *a,const unsigned *b,int lo,int n){for(int i=lo;i<n;i++)d[i]=a[i]+b[i];}
void range64(float *d,const float *a,const float *b,unsigned long lo,unsigned long n){for(unsigned long i=lo;;i++){if(i>=n)break;d[i]=a[i]*b[i]+a[i];}}
void fixedstart(float *d,const float *a,const float *b,unsigned long n){for(unsigned long i=3;i<n;i++)d[i]=a[i]+b[i];}
void rangels(float *d,const float *a,const float *b,long lo,long n){for(long i=lo;i<n;i++)d[i]=a[i]*b[i]+a[i];}
"#;
const HARNESS: &str = r#"
#include <stdint.h>
#include <string.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>
#define DECL(N,T,C) extern void N(T*,const T*,const T*,C);
DECL(addf,float,unsigned long) DECL(addu,unsigned,unsigned) DECL(addl,unsigned long,unsigned long) DECL(divd,double,unsigned long) DECL(mul32,unsigned,unsigned long)
DECL(signedadd,unsigned,int) DECL(twomaps,unsigned,unsigned long)
extern void scale(float*,float,unsigned long);extern void fill(unsigned*,unsigned,int);extern void copyf(float*,const float*,unsigned long);
extern void copy8(unsigned char*,const unsigned char*,unsigned long);extern void copy16(unsigned short*,const unsigned short*,unsigned long);
extern void rangeu(unsigned*,const unsigned*,const unsigned*,unsigned,unsigned);
extern void ranges(unsigned*,const unsigned*,const unsigned*,int,int);
extern void range64(float*,const float*,const float*,unsigned long,unsigned long);
extern void fixedstart(float*,const float*,const float*,unsigned long);
extern void rangels(float*,const float*,const float*,long,long);
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
 CHECK(signedadd,unsigned,x+y);
 #define RANGE(N,T,L,H,EXPR) do { \
  T actual[512],expected[512]; \
  for(int delta=-8;delta<=8;delta++)for(unsigned alias=0;alias<3;alias++){ \
   for(unsigned j=0;j<512;j++)actual[j]=expected[j]=(T)(j%31+1); \
   int d=64,a=d+delta,b=alias==0?300:alias==1?d:d-delta; \
   for(long i=(L);i<(long)(H);i++){T x=expected[a+i],y=expected[b+i];expected[d+i]=(EXPR);} \
   N(actual+d,actual+a,actual+b,(L),(H));if(memcmp(actual,expected,sizeof actual))return __LINE__; \
  } \
 }while(0)
 for(unsigned n=0;n<=129;n++){
  const unsigned starts[]={0,1,3,17,129,130,0xffffffffu};
  for(unsigned k=0;k<7;k++){unsigned lo=starts[k];RANGE(rangeu,unsigned,lo,n,x+y);RANGE(range64,float,lo,n,x*y+x);}
 }
 for(int n=-3;n<=129;n++){
  const int starts[]={-7,-1,0,1,3,17,129,130,0x7fffffff};
  for(unsigned k=0;k<9;k++){int lo=starts[k];RANGE(ranges,unsigned,lo,n,x+y);RANGE(rangels,float,lo,n,x*y+x);}
 }
 range64(0,0,0,~0ul,129);rangeu(0,0,0,~0u,0);
 rangels(0,0,0,0x7fffffffffffffffl,(-0x7fffffffffffffffl-1));
 for(unsigned n=0;n<=129;n++){
  unsigned d[140],a[140],b[140];for(unsigned i=0;i<140;i++){d[i]=17;a[i]=i*123456789u;b[i]=~a[i];}
  twomaps(d,a,b,n);for(unsigned i=0;i<140;i++)if(d[i]!=(i<n?((a[i]+b[i])^a[i]):17))return 38;
  signedadd(d,a,b,-1);for(unsigned i=0;i<140;i++)if(d[i]!=(i<n?((a[i]+b[i])^a[i]):17))return 39;
 }
 // Narrow copies exercise every tail and exact/partial alias at both lane widths.
 for(unsigned n=0;n<=129;n++)for(int delta=-17;delta<=17;delta++){
  unsigned char d8[320],e8[320];unsigned short d16[320],e16[320];
  for(unsigned i=0;i<320;i++){d8[i]=e8[i]=(unsigned char)(i*37);d16[i]=e16[i]=(unsigned short)(i*12345);}
  for(unsigned i=0;i<n;i++){e8[64+i]=e8[64+delta+i];e16[64+i]=e16[64+delta+i];}
  copy8(d8+64,d8+64+delta,n);copy16(d16+64,d16+64+delta,n);
  if(memcmp(d8,e8,sizeof d8)||memcmp(d16,e16,sizeof d16))return 40;
 }
 // Copies must preserve payload bits, including NaNs, infinities and signed zero.
 {uint32_t bits[137];float src[137],out[137];const uint32_t patterns[]={0,0x80000000u,0x7f800000u,0xff800000u,0x7fc12345u,0x7f812345u,1};
  for(unsigned i=0;i<137;i++)bits[i]=patterns[i%7];memcpy(src,bits,sizeof bits);copyf(out,src,137);if(memcmp(bits,out,sizeof bits))return 41;}
 float a[4097];for(unsigned i=0;i<4097;i++)a[i]=(float)(i%17);scale(a,3.0f,4097);for(unsigned i=0;i<4097;i++)if(a[i]!=(float)(i%17)*3.0f)return 31;
 unsigned u[100];for(unsigned i=0;i<100;i++)u[i]=9;fill(u,17,-1);for(unsigned i=0;i<100;i++)if(u[i]!=9)return 32;fill(u,17,99);for(unsigned i=0;i<100;i++)if(u[i]!=(i==99?9:17))return 33;
 // End exactly at a protected page: the vector tail must never overread.
 long page=sysconf(_SC_PAGESIZE);char *p=mmap(0,6*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0);if(p==MAP_FAILED)return 34;
 for(unsigned j=1;j<6;j+=2)if(mprotect(p+j*page,page,PROT_NONE))return 35;
 for(unsigned n=0;n<80;n++){float *d=(float*)(p+page)-n,*x=(float*)(p+3*page)-n,*y=(float*)(p+5*page)-n;for(unsigned i=0;i<n;i++){x[i]=(float)i;y[i]=2;}addf(d,x,y,n);for(unsigned i=0;i<n;i++)if(d[i]!=(float)i+2)return 36;copyf(d,x,n);if(memcmp(d,x,4*n))return 37;}
 for(unsigned n=0;n<80;n++)for(unsigned lo=0;lo<7;lo++){
  float *d=(float*)(p+page)-n,*x=(float*)(p+3*page)-n,*y=(float*)(p+5*page)-n;
  for(unsigned i=0;i<n;i++){d[i]=-1;x[i]=(float)i;y[i]=2;}
  range64(d,x,y,lo,n);for(unsigned i=0;i<n;i++)if(d[i]!=(i<lo?-1:(float)i*3))return 42;
  fixedstart(d,x,y,n);for(unsigned i=3;i<n;i++)if(d[i]!=(float)i+2)return 43;
 }
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
        for shape in ["i8x16", "i16x8", "i32x4", "i64x2", "f32x4", "f64x2"] {
            assert!(full.contains(&format!("load.{shape}")), "missing {shape}");
        }
        for name in ["rangeu", "ranges", "range64", "fixedstart", "rangels"] {
            let function = full
                .split("func @")
                .find(|f| f.starts_with(&format!("{name}(")))
                .unwrap();
            assert!(
                function.contains("load.i32x4") || function.contains("load.f32x4"),
                "{function}"
            );
        }
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
    command(Command::new("clang").current_dir(&dir).args([
        "-O2",
        "-fno-vectorize",
        "-fno-slp-vectorize",
        "-ffp-contract=off",
        "-c",
        "harness.c",
        "-o",
        "harness.o",
    ]));
    for extra in [vec![], vec!["--regalloc", "fast"], vec!["--isel", "all"]] {
        let mut flags = vec!["cc", "-O", "--emit-object", "kernels.c", "-o", "kernels.o"];
        flags.extend(extra);
        keel(&dir, &flags);
        command(Command::new("clang").current_dir(&dir).args([
            "harness.o",
            "kernels.o",
            "-o",
            "test",
        ]));
        command(&mut Command::new(dir.join("test")));
    }
    fs::remove_dir_all(dir).unwrap();
}
