use keel_asm_ir::x86::Features;
use keel_codegen::x86_features::FeatureSelector;
use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, SelectionTier, Target,
    regalloc::{Allocation, AllocationInput, AllocationTier, RegisterAllocator},
};

struct Spilled;
impl RegisterAllocator for Spilled {
    fn name(&self) -> &'static str {
        "fabs-spilled"
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let pools = keel_codegen::Pools {
            caller_saved: &[],
            callee_saved: &[],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        AllocationTier::Full.allocate(&AllocationInput {
            func: input.func,
            pools: &pools,
            selection: input.selection,
            is_call: input.is_call,
            returns_twice: input.returns_twice,
        })
    }
}
fn fixture() -> keel_ir::Module {
    let mut source = String::new();
    for bits in [32, 64] {
        source += &format!(
            "func @abs{bits}(f{bits},ptr) -> f{bits} weak_odr {{\nentry(v0:f{bits},v1:ptr):\n v2 = fabs v0\n store v0,v1\n v3 = fabs v2\n return v3\n}}\n"
        );
    }
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    module
}

#[test]
fn fabs_masks_preserve_transport_and_size_policy() {
    let module = fixture();
    for avx in [false, true] {
        let features = Features {
            avx,
            xsave: avx,
            osxsave: avx,
            ..Features::default()
        };
        for selection in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocation in [
                &AllocationTier::Fast as &dyn RegisterAllocator,
                &AllocationTier::Full,
                &Spilled,
            ] {
                let selector = FeatureSelector {
                    inner: &selection,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: allocation,
                };
                for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                    let mut emission = EmissionOptions::default();
                    emission.goal = goal;
                    emission.function_sections = true;
                    let request = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                        .emission(emission);
                    let text = request.text(&module).unwrap();
                    let program = request.program(&module).unwrap();
                    let expected =
                        keel_asm::x86::assemble_object_with_features(&text, features).unwrap();
                    assert_eq!(
                        expected,
                        keel_asm::x86::assemble_program_object_with_features(&program, features)
                            .unwrap()
                    );
                    let mut chunks = keel_asm_ir::Program::default();
                    request
                        .program_chunks(&module, &mut |p| chunks.items.extend(p.items))
                        .unwrap();
                    assert_eq!(
                        expected,
                        keel_asm::x86::assemble_program_object_with_features(&chunks, features)
                            .unwrap()
                    );
                    assert_eq!(
                        text.contains(if avx { "\tvandps\t" } else { "\tandps\t" }),
                        goal == CodegenGoal::Speed,
                        "{text}"
                    );
                    assert_eq!(
                        text.contains(if avx { "\tvandpd\t" } else { "\tandpd\t" }),
                        goal == CodegenGoal::Speed,
                        "{text}"
                    );
                    for bits in [32, 64] {
                        assert_eq!(
                            text.matches(&format!(".Labs{bits}_fabs{bits}:")).count(),
                            usize::from(goal == CodegenGoal::Speed),
                            "{text}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn native_fabs_preserves_bits_and_mxcsr_with_live_inputs_and_spills() {
    if !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        eprintln!("SKIP: scalar fabs oracle requires x86-64 Linux");
        return;
    }
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-fabs-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("driver.c"),r#"
#include <stdint.h>
#include <string.h>
#include <xmmintrin.h>
extern float abs32(float, float *);
extern double abs64(double, double *);
int main(void) {
 const uint64_t d[]={0,1,0x000fffffffffffffULL,0x0010000000000000ULL,0x3ff0000000000000ULL,0x7fefffffffffffffULL,0x7ff0000000000000ULL,0x7ff8000000001234ULL,0x7ff0000000001234ULL};
 const uint32_t f[]={0,1,0x007fffff,0x00800000,0x3f800000,0x7f7fffff,0x7f800000,0x7fc01234,0x7f801234};
 unsigned saved=_mm_getcsr();
 for(int mode=0;mode<4;++mode) for(int sign=0;sign<2;++sign) {
  unsigned state=0x1f80U|(mode&1?0x8040U:0)|(mode&2?0x21U:0);
  for(unsigned i=0;i<sizeof(d)/sizeof(d[0]);++i) {
   uint64_t bits=d[i]|((uint64_t)sign<<63),got,kept; double x,out;
   memcpy(&x,&bits,8);_mm_setcsr(state);double y=abs64(x,&out);
   unsigned flags=_mm_getcsr();memcpy(&got,&y,8);memcpy(&kept,&out,8);
   if(got!=d[i]||kept!=bits||flags!=state)return 1;
  }
  for(unsigned i=0;i<sizeof(f)/sizeof(f[0]);++i) {
   uint32_t bits=f[i]|((uint32_t)sign<<31),got,kept; float x,out;
   memcpy(&x,&bits,4);_mm_setcsr(state);float y=abs32(x,&out);
   unsigned flags=_mm_getcsr();memcpy(&got,&y,4);memcpy(&kept,&out,4);
   if(got!=f[i]||kept!=bits||flags!=state)return 2;
  }
 }
 _mm_setcsr(saved);return 0;
}
"#).unwrap();
    let module = fixture();
    for avx in [false, true] {
        #[cfg(target_arch = "x86_64")]
        if avx && !std::is_x86_feature_detected!("avx") {
            continue;
        }
        let features = Features {
            avx,
            xsave: avx,
            osxsave: avx,
            ..Features::default()
        };
        for allocation in [&AllocationTier::Full as &dyn RegisterAllocator, &Spilled] {
            for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                let selector = FeatureSelector {
                    inner: &SelectionTier::Full,
                    features,
                };
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: allocation,
                };
                let mut emission = EmissionOptions::default();
                emission.goal = goal;
                emission.function_sections = true;
                let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(emission)
                    .program(&module)
                    .unwrap();
                fs::write(
                    dir.join("code.o"),
                    keel_asm::x86::assemble_program_object_with_features(&program, features)
                        .unwrap(),
                )
                .unwrap();
                let build = Command::new("clang")
                    .current_dir(&dir)
                    .args(["-O2", "driver.c", "code.o", "-o", "check"])
                    .output()
                    .unwrap();
                assert!(
                    build.status.success(),
                    "{}",
                    String::from_utf8_lossy(&build.stderr)
                );
                assert!(
                    Command::new(dir.join("check")).status().unwrap().success(),
                    "{} {goal:?}",
                    allocation.name()
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
