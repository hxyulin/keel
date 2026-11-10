//! Native ABI oracles for the caller-save adapter selected by -O2 and -Os.
use std::{fs, path::Path, process::Command};

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn caller_saves_preserve_live_results_scratch_sequences_and_abi_operands() {
    if !(cfg!(all(target_arch = "aarch64", target_os = "macos"))
        || cfg!(all(target_arch = "x86_64", target_os = "linux")))
    {
        eprintln!("SKIP: caller-save execution requires native AArch64 macOS or x86-64 Linux");
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("caller-save-abi");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("kernels.c"), SOURCE).unwrap();
    fs::write(dir.join("oracle.c"), ORACLE).unwrap();
    let mut reference = Command::new("clang");
    reference
        .current_dir(&dir)
        .args(["-O2", "-c", "kernels.c", "-o", "reference.o"]);
    for name in [
        "bank_entry",
        "bank_indirect",
        "bank_pair",
        "bank_sret",
        "wide_math",
        "wide_atomic",
        "wide_float",
        "cold_call",
    ] {
        reference.arg(format!("-D{name}=reference_{name}"));
    }
    run(&mut reference);
    run(Command::new("clang")
        .current_dir(&dir)
        .args(["-O2", "-c", "oracle.c", "-o", "oracle.o"]));
    for level in ["-O2", "-Os"] {
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(["cc", level, "kernels.c", "-o", "kernels.s"]));
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(["asm", "kernels.s", "-o", "text.o"]));
        run(Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args(["cc", level, "--emit-object", "kernels.c", "-o", "direct.o"]));
        let text = |name: &str| {
            use object::{Object, ObjectSection};
            let bytes = fs::read(dir.join(name)).unwrap();
            let object = object::File::parse(bytes.as_slice()).unwrap();
            object
                .sections()
                .find(|s| matches!(s.name(), Ok(".text" | "__text")))
                .unwrap()
                .data()
                .unwrap()
                .to_vec()
        };
        assert_eq!(
            text("direct.o"),
            text("text.o"),
            "{level}: native output routes disagree"
        );
        // The two routes are byte-identical, so execute only the direct image.
        run(Command::new("clang").current_dir(&dir).args([
            "direct.o",
            "reference.o",
            "oracle.o",
            "-o",
            "run",
        ]));
        run(&mut Command::new(dir.join("run")));
    }
    fs::remove_dir_all(dir).unwrap();
}

const SOURCE: &str = r#"
#include <stddef.h>
#include <stdint.h>
typedef struct { uint64_t a,b; } Pair;
typedef uint64_t (*Fn)(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
extern uint64_t helper12(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
extern Pair helper_pair(Pair,Pair,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
uint64_t bank_entry(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l) {
 uint64_t r=helper12(a,b,c,d,e,f,g,h,i,j,k,l);
 return (r^a)+b+c+d+e+f+g+h+i+j+k+l;
}
uint64_t bank_indirect(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l,Fn fn) {
 return fn(a,b,c,d,e,f,g,h,i,j,k,l)^a;
}
Pair bank_pair(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g) {
 Pair p={a,b},q={c,d};
 return helper_pair(p,q,a,b,c,d,e,f,g);
}
#include <stdint.h>
uint64_t wide_math(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l) {
 unsigned __int128 x=((unsigned __int128)a<<64)|b;
 unsigned __int128 y=((unsigned __int128)c<<64)|d;
 for(unsigned n=0;n<9;n++) {
  x=(x*y+e) ^ (x>>((f+n)%127));
  if(n&1) x /= (y|1);
  if(x<y) x+=((unsigned __int128)g<<64)|h;
  else x^=((unsigned __int128)i<<64)|j;
  y=(y<<((k+n)%127))^x^l;
  a=(a^b)+c; b=(b^c)+d; c=(c^d)+e; d=(d^e)+f;
  e=(e^f)+g; f=(f^g)+h; g=(g^h)+i; h=(h^i)+j;
  i=(i^j)+k; j=(j^k)+l; k=(k^l)+a; l=(l^a)+b;
 }
 return (uint64_t)x+(uint64_t)(x>>64)+(uint64_t)y+(uint64_t)(y>>64)+a+b+c+d+e+f+g+h+i+j+k+l;
}
uint64_t wide_atomic(uint64_t *slot,uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l) {
 for(unsigned n=0;n<5;n++) {
  uint64_t prev=__atomic_fetch_add(slot,a+b,__ATOMIC_SEQ_CST);
  uint64_t expected=prev+a+b;
  __atomic_compare_exchange_n(slot,&expected,c+d,0,__ATOMIC_SEQ_CST,__ATOMIC_SEQ_CST);
  a=(a^b)+c; b=(b^c)+d; c=(c^d)+e; d=(d^e)+f;
  e=(e^f)+g; f=(f^g)+h; g=(g^h)+i; h=(h^i)+j;
  i=(i^j)+k; j=(j^k)+l; k=(k^l)+a; l=(l^a)+b;
 }
 return *slot+a+b+c+d+e+f+g+h+i+j+k+l;
}

extern uint64_t cold_return(void) __attribute__((cold));
uint64_t cold_call(volatile const uint64_t *p, size_t n, uint64_t seed) {
  uint64_t a0=seed+1, a1=seed+2, a2=seed+3, a3=seed+4, a4=seed+5, a5=seed+6, a6=seed+7, a7=seed+8, a8=seed+9, a9=seed+10, a10=seed+11, a11=seed+12;
  for (size_t i=0;i<n;++i) {
    uint64_t x=p[i];
    if (x==0) a0 += cold_return();
    if (x&1) {
      a0 += (a11 ^ (x+0));
      a1 += (a0 ^ (x+1));
      a2 += (a1 ^ (x+2));
      a3 += (a2 ^ (x+3));
      a4 += (a3 ^ (x+4));
      a5 += (a4 ^ (x+5));
      a6 += (a5 ^ (x+6));
      a7 += (a6 ^ (x+7));
      a8 += (a7 ^ (x+8));
      a9 += (a8 ^ (x+9));
      a10 += (a9 ^ (x+10));
      a11 += (a10 ^ (x+11));
    } else {
      a0 ^= (a11 + (x+0));
      a1 ^= (a0 + (x+1));
      a2 ^= (a1 + (x+2));
      a3 ^= (a2 + (x+3));
      a4 ^= (a3 + (x+4));
      a5 ^= (a4 + (x+5));
      a6 ^= (a5 + (x+6));
      a7 ^= (a6 + (x+7));
      a8 ^= (a7 + (x+8));
      a9 ^= (a8 + (x+9));
      a10 ^= (a9 + (x+10));
      a11 ^= (a10 + (x+11));
    }
  }
  return a0 ^ a1 ^ a2 ^ a3 ^ a4 ^ a5 ^ a6 ^ a7 ^ a8 ^ a9 ^ a10 ^ a11;
}


typedef struct { uint64_t a,b,c; } Triple;
extern Triple helper_triple(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
Triple bank_sret(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l) { Triple r=helper_triple(a,b,c,d,e,f,g,h,i,j,k,l);r.a^=a;r.b+=l;return r;}

uint64_t wide_float(double f,uint64_t a,uint64_t b,uint64_t c,uint64_t d) {
 unsigned __int128 x=((unsigned __int128)a<<64)|b;
 unsigned __int128 y=(((unsigned __int128)c<<64)|d)|1;
 union { double value; uint64_t bits; } r;
 r.value=f+(double)(uint64_t)(x/y);
 return r.bits^a^b^c^d;
}
"#;

const ORACLE: &str = r#"
#include <stdint.h>
#include <stddef.h>
typedef struct { uint64_t a,b; } Pair;
typedef uint64_t (*Fn)(uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t,uint64_t);
uint64_t bank_entry(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
uint64_t reference_bank_entry(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
uint64_t bank_indirect(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l,Fn fn);
uint64_t reference_bank_indirect(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l,Fn fn);
Pair bank_pair(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g);
Pair reference_bank_pair(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g);
void cold_hook(void) {

#if defined(__aarch64__)
 __asm__ volatile(
  "mov x0,#-1\n mov x1,#-1\n mov x2,#-1\n mov x3,#-1\n"
  "mov x4,#-1\n mov x5,#-1\n mov x6,#-1\n mov x7,#-1\n"
  "mov x8,#-1\n mov x9,#-1\n mov x10,#-1\n mov x11,#-1\n"
  "mov x12,#-1\n mov x13,#-1\n mov x14,#-1\n mov x15,#-1\n"
  "mov x16,#-1\n mov x17,#-1"
  ::: "x0","x1","x2","x3","x4","x5","x6","x7","x8","x9","x10","x11","x12","x13","x14","x15","x16","x17","cc","memory");
#elif defined(__x86_64__)
 __asm__ volatile(
  "xor %%rax,%%rax\n xor %%rcx,%%rcx\n xor %%rdx,%%rdx\n"
  "xor %%rsi,%%rsi\n xor %%rdi,%%rdi\n xor %%r8,%%r8\n"
  "xor %%r9,%%r9\n xor %%r10,%%r10\n xor %%r11,%%r11"
  ::: "rax","rcx","rdx","rsi","rdi","r8","r9","r10","r11","cc","memory");
#endif
}

uint64_t helper12(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l) {
 uint64_t result=a+3*b+5*c+7*d+11*e+13*f+17*g+19*h+23*i+29*j+31*k+37*l;
 cold_hook(); return result;
}
Pair helper_pair(Pair p,Pair q,uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g) {
 Pair r={p.a+q.b+a+c+e+g,p.b+q.a+b+d+f}; cold_hook(); return r;
}
uint64_t wide_math(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
uint64_t wide_atomic(uint64_t *,uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
uint64_t cold_call(volatile const uint64_t *,size_t,uint64_t);
uint64_t reference_wide_math(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
uint64_t reference_wide_atomic(uint64_t *,uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
uint64_t reference_cold_call(volatile const uint64_t *,size_t,uint64_t);
typedef struct { uint64_t a,b,c; } Triple;
Triple bank_sret(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
Triple reference_bank_sret(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l);
Triple helper_triple(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f,uint64_t g,uint64_t h,uint64_t i,uint64_t j,uint64_t k,uint64_t l) { Triple r={a+c+e+g+i+k,b+d+f+h+j+l,a^b^k^l};cold_hook();return r;}
uint64_t cold_return(void) { cold_hook();return UINT64_C(0x12345678abcdef01); }
uint64_t wide_float(double,uint64_t,uint64_t,uint64_t,uint64_t);
uint64_t reference_wide_float(double,uint64_t,uint64_t,uint64_t,uint64_t);
int main(void) {
 uint64_t v[12];
 for (unsigned round=0;round<24;++round) {
  for(unsigned i=0;i<12;++i) v[i]=UINT64_C(0xfedcba9876543210)*(round+i)^((uint64_t)round<<i);
  if(bank_entry(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11])!=reference_bank_entry(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11])) return 1;
  if(bank_indirect(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11],helper12)!=reference_bank_indirect(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11],helper12)) return 2;
  if(wide_float((double)round+0.5,v[0],v[1],v[2],v[3])!=reference_wide_float((double)round+0.5,v[0],v[1],v[2],v[3])) return 8;
  if(wide_math(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11])!=reference_wide_math(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11])) return 4;
  uint64_t x=v[0],y=v[0];
  if(wide_atomic(&x,v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11])!=reference_wide_atomic(&y,v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11]) || x!=y) return 5;
  v[round%12]=0;
  if(round&1) v[0]=0;
  if(cold_call(v,round%13,round)!=reference_cold_call(v,round%13,round)) return 6;
  Triple r=bank_sret(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11]),s=reference_bank_sret(v[0],v[1],v[2],v[3],v[4],v[5],v[6],v[7],v[8],v[9],v[10],v[11]);
  if(r.a!=s.a || r.b!=s.b || r.c!=s.c) return 7;
  Pair a=bank_pair(v[0],v[1],v[2],v[3],v[4],v[5],v[6]),b=reference_bank_pair(v[0],v[1],v[2],v[3],v[4],v[5],v[6]);
  if(a.a!=b.a || a.b!=b.b) return 3;
 }
 return 0;
}
"#;
