//! C lane intent -> exact SSA reduction -> feature-checked objects/native oracle.
use keel_codegen::{
    CodegenPipeline, SelectionTier, Target,
    regalloc::{AllocationTier, Pools},
    x86_features::{CpuLevel, FeatureSelector, Features},
};
use std::{fs, process::Command};

const SOURCE: &str = r#"
typedef unsigned U __attribute__((vector_size(16)));
extern void observe(void *);
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
    let root = std::env::temp_dir().join(format!("keel-c-lane-reduce-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("oracle.c"), ORACLE).unwrap();
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
                let mut objects = Vec::new();
                for (i, m) in modules.iter().enumerate() {
                    let (text, obj) = if arm {
                        let pools = if spills {
                            &empty
                        } else {
                            &keel_codegen::aarch64::REGISTERS
                        };
                        let text =
                            keel_codegen::aarch64::compile_with_pipeline(m, pools, &pipeline)
                                .unwrap();
                        let p = keel_codegen::aarch64::compile_program_with_pipeline(
                            m, pools, &pipeline,
                        )
                        .unwrap();
                        let obj =
                            keel_asm::assemble_program_object(&p, Default::default()).unwrap();
                        assert_eq!(obj, keel_asm::assemble_object(&text).unwrap());
                        (text, obj)
                    } else {
                        let pools = if spills {
                            &empty
                        } else {
                            &keel_codegen::x86_64::REGISTERS
                        };
                        let text = keel_codegen::x86_64::compile_with_pipeline(m, pools, &pipeline)
                            .unwrap();
                        let p = keel_codegen::x86_64::compile_program_with_pipeline(
                            m, pools, &pipeline,
                        )
                        .unwrap();
                        let obj =
                            keel_asm::x86::assemble_program_object_with_features(&p, features)
                                .unwrap();
                        assert_eq!(
                            obj,
                            keel_asm::x86::assemble_object_with_features(&text, features).unwrap()
                        );
                        (text, obj)
                    };
                    let name = root.join(format!("{profile}-{sel:?}-{spills}-{i}"));
                    fs::write(name.with_extension("s"), text).unwrap();
                    fs::write(name.with_extension("o"), obj).unwrap();
                    objects.push(name.with_extension("o"));
                }
                // Execute baseline x86 on any native Linux x86 host. Higher
                // levels are emission checks here; maintained suite gates native eligibility.
                if profile == 0 && cfg!(all(target_arch = "x86_64", target_os = "linux"))
                    || arm && cfg!(all(target_arch = "aarch64", target_os = "macos"))
                {
                    let exe = root.join("native");
                    command(
                        Command::new("clang")
                            .arg("-O2")
                            .arg(root.join("oracle.c"))
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
