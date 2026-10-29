//! C lane intent -> exact SSA reduction -> feature-checked objects/native oracle.
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use std::{fs, process::Command};

const SOURCE: &str = r#"
typedef unsigned U __attribute__((vector_size(16)));
typedef int S __attribute__((vector_size(16)));
extern void observe(void *);
unsigned long PREFIXextended(const void *p) {
    U a; __builtin_memcpy(&a,p,16);
    return a[3]+(a[1]+(a[0]+a[2]));
}
unsigned PREFIXmixed(const void *p) {
    S a; __builtin_memcpy(&a,p,16);
    return (unsigned)a[0]+a[1]+a[2]+a[3];
}
long PREFIXsigned(const void *p) {
    S a; __builtin_memcpy(&a,p,16);
    return (a[0]+a[1])+(a[2]+a[3]);
}
long PREFIXwide(const void *p) {
    S a; __builtin_memcpy(&a,p,16);
    return (long)a[0]+a[1]+a[2]+a[3];
}
unsigned PREFIXordered(void *p) {
    U a; __builtin_memcpy(&a,p,16);
    unsigned s=a[0]+a[1]+a[2]+a[3];
    observe(p);
    unsigned changed; __builtin_memcpy(&changed,p,4);
    return s+changed;
}
unsigned PREFIXsum(const void *p,void *out) {
    U a; __builtin_memcpy(&a,p,16);
    unsigned s=a[0]+a[1]+a[2]+a[3];
    observe(out);
    __builtin_memcpy(out,&a,16);
    return s;
}
"#;
const ORACLE: &str = r#"
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern unsigned before_sum(const void*,void*),after_sum(const void*,void*);
extern unsigned long before_extended(const void*),after_extended(const void*);
extern unsigned before_mixed(const void*),after_mixed(const void*);
extern long before_signed(const void*),after_signed(const void*);
extern long before_wide(const void*),after_wide(const void*);
extern unsigned before_ordered(void*),after_ordered(void*);
static unsigned calls;
void observe(void *p) { calls++; ((unsigned char*)p)[0]^=0x42; }
int main(void) {
 long page=sysconf(_SC_PAGESIZE);
 unsigned char *m=mmap(0,3*page,PROT_NONE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
 if(m==MAP_FAILED||mprotect(m+page,page,PROT_READ|PROT_WRITE))return 90;
 uint32_t seed=17;
 for(unsigned t=0;t<1024;t++) {
  unsigned a[4],sum=0;
  for(unsigned i=0;i<4;i++) { seed=seed*1664525u+1013904223u;
   unsigned edge[]={0,1,0xffffffffu,0x80000000u,0x7fffffffu};
   a[i]=t<625?edge[(t/(i==0?1:i==1?5:i==2?25:125))%5]:seed;
   sum+=a[i];
  }
  // Unsigned modulo must precede zero extension; mixed C signedness converts
  // each operand to unsigned. Widen-before-add must retain signed i64 addition.
  int32_t sa[4];memcpy(sa,a,16);int64_t wide=0;
  for(unsigned i=0;i<4;i++)wide+=sa[i];
  if(before_extended(a)!=(uint64_t)sum||after_extended(a)!=(uint64_t)sum||
     before_mixed(a)!=sum||after_mixed(a)!=sum||
     before_wide(a)!=wide||after_wide(a)!=wide)return 2;
  // Keep every intermediate C signed sum in range, including negative results.
  int32_t safe[4];int64_t signed_sum=0;
  for(unsigned i=0;i<4;i++){safe[i]=sa[i]/8;signed_sum+=safe[i];}
  if(before_signed(safe)!=signed_sum||after_signed(safe)!=signed_sum)return 3;
  for(unsigned arm=0;arm<2;arm++) {
   unsigned char bytes[16];memcpy(bytes,a,16);bytes[0]^=0x42;
   unsigned changed;memcpy(&changed,bytes,4);memcpy(bytes,a,16);
   unsigned n=calls,got=arm?after_ordered(bytes):before_ordered(bytes);
   if(got!=sum+changed||calls!=n+1)return 4;
  }
  for(unsigned pos=0;pos<3;pos++)for(unsigned alias=0;alias<2;alias++) {
   unsigned char *p=pos==0?m+page:pos==1?m+2*page-16:m+page+1;
   for(unsigned arm=0;arm<2;arm++) {
    unsigned char out[18],expected[18];memset(out,0xa5,18);memset(expected,0xa5,18);
    memcpy(p,a,16);memcpy(expected+1,a,16);unsigned char *d=alias?p:out+1;
    unsigned n=calls;unsigned got=arm?after_sum(p,d):before_sum(p,d);
    if(got!=sum||calls!=n+1||memcmp(p,a,16)||memcmp(out,alias?(unsigned char[18]){[0 ... 17]=0xa5}:expected,18))return 1;
   }
  }
 }
 return munmap(m,3*page)!=0;
}
"#;
fn command(c: &mut Command) {
    let o = c.output().unwrap();
    assert!(o.status.success(), "{c:?}: {o:?}");
}

#[test]
fn c_wrapping_sum_effects_footprints_and_objects() {
    let root = std::env::var_os("KEEL_C_LANE_REDUCE_EVIDENCE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-c-lane-reduce-{}", std::process::id()))
        });
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("oracle.c"), ORACLE).unwrap();
    let native = cfg!(any(
        all(target_arch = "aarch64", target_os = "macos"),
        all(target_arch = "x86_64", target_os = "linux")
    ));
    if native {
        command(
            Command::new("clang")
                .args(["-O2", "-c"])
                .arg(root.join("oracle.c"))
                .arg("-o")
                .arg(root.join("oracle.o")),
        );
    }
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    // V3 with AVX/AVX2 disabled exercises the existing legal SSE2 recipe.
    for profile in 0..5 {
        let arm = profile == 4;
        let target = if arm {
            Target::Aarch64AppleDarwin
        } else {
            Target::X86_64LinuxGnu
        };
        let mut features = Features::for_level(match profile {
            0 => CpuLevel::V1,
            1 => CpuLevel::V2,
            _ => CpuLevel::V3,
        });
        if profile == 3 {
            features.avx = false;
            features.avx2 = false;
            features.f16c = false;
            features.fma = false;
        }
        let mut modules = Vec::new();
        for (label, optimize) in [("before_", false), ("after_", true)] {
            let mut m = keel_c::compile(
                &SOURCE.replace("PREFIX", label),
                "sum.c",
                keel_c::Options {
                    target,
                    debug_info: false,
                },
            )
            .unwrap();
            if optimize {
                keel_opt::optimize(&mut m).unwrap();
            }
            keel_ir::verify_module(&m).unwrap();
            assert_eq!(m.to_string().contains("vreduce_add"), optimize);
            fs::write(root.join(format!("{profile}-{label}.ir")), m.to_string()).unwrap();
            modules.push(m);
        }
        for sel in [
            SelectionTier::Baseline,
            SelectionTier::Trees,
            SelectionTier::Full,
        ] {
            for spills in [false, true] {
                let selector = FeatureSelector {
                    inner: &sel,
                    features: if arm { Features::default() } else { features },
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &AllocationTier::Full,
                };
                // keel-codegen vector_reduce_add_x86 checks text, program and
                // chunk parity for vreduce_add; this test compiles the program route.
                let mut objects = Vec::new();
                for (i, m) in modules.iter().enumerate() {
                    let obj = if arm {
                        let pools = if spills {
                            &empty
                        } else {
                            &keel_codegen::aarch64::REGISTERS
                        };
                        let p = keel_codegen::aarch64::compile_program_with_pipeline(
                            m, pools, &pipeline,
                        )
                        .unwrap();
                        keel_asm::assemble_program_object(&p, Default::default()).unwrap()
                    } else {
                        let pools = if spills {
                            &empty
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let p = keel_codegen::x86_64::compile_program_with_pipeline(
                            m, pools, &pipeline,
                        )
                        .unwrap();
                        keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap()
                    };
                    let name = root.join(format!("{profile}-{sel:?}-{spills}-{i}.o"));
                    fs::write(&name, obj).unwrap();
                    objects.push(name);
                }
                // Execute baseline x86 on any native Linux x86 host, with the Full
                // selector. Higher levels and other selectors are emission checks
                // here; maintained suite gates native eligibility.
                if sel == SelectionTier::Full
                    && (profile == 0 && cfg!(all(target_arch = "x86_64", target_os = "linux"))
                        || arm && cfg!(all(target_arch = "aarch64", target_os = "macos")))
                {
                    let exe = root.join("native");
                    command(
                        Command::new("clang")
                            .arg(root.join("oracle.o"))
                            .args(&objects)
                            .arg("-o")
                            .arg(&exe),
                    );
                    command(&mut Command::new(exe));
                }
            }
        }
    }
}

#[test]
fn c_promotions_dynamic_indices_and_distinct_loads_stay_scalar() {
    for (ty, expression) in [
        ("short", "a[0]+a[1]+a[2]+a[3]"),
        ("unsigned short", "a[0]+a[1]+a[2]+a[3]"),
        ("int", "(long)a[0]+a[1]+a[2]+a[3]"),
        ("unsigned", "a[0]+a[1]+a[2]+a[n]"),
        ("unsigned", "a[0]+a[1]+a[2]+a[2]"),
        ("unsigned", "a[0]+a[1]+a[2]+b[3]"),
        ("float", "a[0]+a[1]+a[2]+a[3]"),
    ] {
        let result = if ty == "float" { "float" } else { "long" };
        let source = format!(
            "typedef {ty} V __attribute__((vector_size(16)));\n{result} f(V a,V b,unsigned n) {{ return {expression}; }}"
        );
        let mut m = keel_c::compile(
            &source,
            "negative.c",
            keel_c::Options {
                target: Target::X86_64LinuxGnu,
                debug_info: true,
            },
        )
        .unwrap();
        keel_opt::optimize(&mut m).unwrap();
        keel_ir::verify_module(&m).unwrap();
        assert!(!m.to_string().contains("vreduce_add"), "{source}\n{m}");
    }
}
