use super::{command as run, keel};
use std::{fs, path::Path, process::Command};

const SOURCE: &str = r#"
const unsigned char *ascii(const unsigned char *p,const unsigned char *end){while(p<end && *p<128)p++;return p;}
unsigned long prefix(const unsigned char *p,unsigned long n){const unsigned char *q=p;while(q<p+n && *q<128)q++;return q-p;}
const unsigned char *vol(const volatile unsigned char *p,const volatile unsigned char *end){while(p<end && *p<128)p++;return (const unsigned char*)p;}
"#;
const HARNESS: &str = r#"
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern const unsigned char *ascii(const unsigned char*,const unsigned char*);
extern unsigned long prefix(const unsigned char*,unsigned long);
int main(void){
 unsigned char data[320];
 for(unsigned align=0;align<32;align++)for(unsigned n=0;n<=160;n++){
  unsigned char *p=data+align;
  for(unsigned stop=0;stop<=n;stop++){
   memset(data,127,sizeof data);if(stop<n)p[stop]=128+(stop%128);
   if(ascii(p,p+n)!=p+stop || prefix(p,n)!=stop)return 1;
  }
 }
 for(unsigned byte=0;byte<256;byte++)for(unsigned lane=0;lane<32;lane++){
  memset(data,127,sizeof data);data[lane]=byte;
  unsigned expected=byte>=128?lane:128;
  if(prefix(data,128)!=expected)return 2;
 }
 // Empty/reversed ranges must not dereference even inaccessible memory.
 if(ascii(0,0)!=0)return 3;
 if(ascii(data+100,data+50)!=data+100)return 4;
 long page=sysconf(_SC_PAGESIZE);
 unsigned char *mem=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANON,-1,0);
 if(mem==MAP_FAILED || mprotect(mem+page,page,PROT_READ|PROT_WRITE))return 5;
 unsigned char *end=mem+2*page;
 // The scalar loop stops before a protected page although its bound is later.
 // Each unaligned prefix must be peeled before any full-width load.
 for(unsigned n=1;n<=31;n++){
  unsigned char *p=end-n;
  for(unsigned stop=0;stop<n;stop++){
   memset(p,127,n);p[stop]=255;
   if(ascii(p,end+32)!=p+stop || prefix(p,n+32)!=stop)return 8;
  }
 }
 for(unsigned n=0;n<=128;n++){
  unsigned char *p=end-n;
  for(unsigned stop=0;stop<=n;stop++){
   memset(p,127,n);if(stop<n)p[stop]=255;
   if(ascii(p,end)!=p+stop || prefix(p,n)!=stop)return 6;
  }
 }
 // Also catch reading before the start of an unaligned bounded range.
 memset(mem+page,127,page);
 if(prefix(mem+page,127)!=127)return 7;
 munmap(mem,3*page);return 0;
}
"#;

#[test]
fn bounded_ascii_scan_emits_packed_loads_and_native_lane_extracts() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ascii-scan-emission");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("scan.c"), SOURCE).unwrap();
    for target in ["aarch64-apple-darwin", "x86_64-unknown-linux-gnu"] {
        let plain = keel(&dir, &["cc", "--target", target, "--emit-ir", "scan.c"]);
        let basic = keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "--opt-tier",
                "basic",
                "--emit-ir",
                "scan.c",
            ],
        );
        assert!(!plain.contains("load.i64x2") && !basic.contains("load.i64x2"));
        let full = keel(
            &dir,
            &["cc", "--target", target, "-O", "--emit-ir", "scan.c"],
        );
        assert_eq!(full.matches("load.i64x2").count(), 2, "{full}");
        let asm = keel(&dir, &["cc", "--target", target, "-O", "scan.c"]);
        assert!(
            asm.contains(if target.starts_with("aarch64") {
                "umov\tx"
            } else {
                "movhlps"
            }),
            "{asm}"
        );
        keel(
            &dir,
            &[
                "cc",
                "--target",
                target,
                "-O",
                "--emit-object",
                "scan.c",
                "-o",
                "scan.o",
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
fn bounded_ascii_scan_executes_every_exit_alignment_and_protected_page_tail() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ascii-scan-runtime");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("scan.c"), SOURCE).unwrap();
    fs::write(dir.join("harness.c"), HARNESS).unwrap();
    for extra in [vec![], vec!["--regalloc", "fast"], vec!["--isel", "all"]] {
        let mut args = vec!["cc", "-O", "--emit-object", "scan.c", "-o", "scan.o"];
        args.extend(extra);
        keel(&dir, &args);
        run(Command::new("clang").current_dir(&dir).args([
            "-O2",
            "harness.c",
            "scan.o",
            "-o",
            "test",
        ]));
        run(&mut Command::new(dir.join("test")));
    }
    fs::remove_dir_all(dir).unwrap();
}
