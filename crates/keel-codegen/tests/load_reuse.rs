//! A successful load is reused on a conditional shift edge on both backends.
use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};
use keel_ir::InstData;

const SOURCE: &str = "func @shift(ptr, ptr, i32) cc(c) {
block0(v0: ptr, v1: ptr, v2: i32):
 v3 = load.i32 v0
 v4 = icmp.sgt v3, v2
 brif v4, block1, block2
block1:
 v5 = load.i32 v0
 store v5, v1
 return
block2:
 return
}";

#[test]
fn conditional_reload_removed_for_both_targets_and_tiers() {
    for tier in [
        keel_opt::OptimizationTier::Fast,
        keel_opt::OptimizationTier::Full,
    ] {
        for target in Target::ALL {
            let mut m = keel_ir::parse(SOURCE).unwrap();
            keel_ir::verify_module(&m).unwrap();
            keel_opt::run_passes(&mut m, tier.pipeline()).unwrap();
            keel_opt::run_passes(&mut m, &["load-reuse"]).unwrap();
            let f = &m.functions[0];
            assert_eq!(
                f.layout_insts()
                    .filter(|(_, i)| matches!(f.inst_data(*i), InstData::Load { .. }))
                    .count(),
                1
            );
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                    let pipeline = CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    };
                    let text = keel_codegen::compile_with_pipeline(&m, target, &pipeline).unwrap();
                    assert_eq!(text.matches(" = load.i32 ").count(), 1, "{text}");
                    // Exercise structured encoding too, rather than trusting IR comments.
                    let program =
                        keel_codegen::compile_program_with_pipeline(&m, target, &pipeline).unwrap();
                    let object = match target {
                        Target::Aarch64AppleDarwin => {
                            keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                        }
                        Target::X86_64LinuxGnu => {
                            keel_asm::x86::assemble_program_object(&program).unwrap()
                        }
                    };
                    assert!(!object.is_empty());
                }
            }
        }
    }
}

const MERGE_SOURCE: &str = "func @pointer_equal(ptr, ptr) -> i8 uext cc(c) {
block0(v0: ptr, v1: ptr):
 v2 = icmp.ne v0, v1
 brif v2, block1, block2
block1:
 v3 = icmp.eq v1, v0
 return v3
block2:
 v4 = icmp.eq v0, v1
 return v4
}
func @clean_merge(ptr, i8 uext) -> i64 cc(c) {
block0(v0: ptr, v1: i8):
 v2 = load.i64 v0
 brif v1, block1, block2
block1:
 jump block3
block2:
 jump block3
block3:
 v3 = load.i64 v0
 return v3
}
func @write_merge(ptr, ptr, i8 uext, i64) -> i64 cc(c) {
block0(v0: ptr, v1: ptr, v2: i8, v3: i64):
 v4 = load.i64 v0
 brif v2, block1, block2
block1:
 store v3, v1
 jump block3
block2:
 jump block3
block3:
 v5 = load.i64 v0
 return v5
}";

#[test]
fn branch_and_merge_facts_emit_on_both_targets() {
    let mut m = keel_ir::parse(MERGE_SOURCE).unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes(&mut m, &["branch-ranges", "load-reuse"]).unwrap();
    assert_eq!(
        m.functions[0]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[0].inst_data(*i), InstData::Icmp { .. }))
            .count(),
        1
    );
    assert_eq!(
        m.functions[1]
            .layout_insts()
            .filter(|(_, i)| matches!(m.functions[1].inst_data(*i), InstData::Load { .. }))
            .count(),
        1
    );
    for target in Target::ALL {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let text = keel_codegen::compile_with_pipeline(&m, target, &pipeline).unwrap();
                let program =
                    keel_codegen::compile_program_with_pipeline(&m, target, &pipeline).unwrap();
                match target {
                    Target::Aarch64AppleDarwin => assert_eq!(
                        keel_asm::assemble_object(&text).unwrap(),
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                    ),
                    Target::X86_64LinuxGnu => assert_eq!(
                        keel_asm::x86::assemble_object(&text).unwrap(),
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    ),
                }
            }
        }
    }
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn native_branch_and_merge_facts_preserve_pointer_bits_and_aliasing_stores() {
    use std::{fs, process::Command};
    let dir = std::env::temp_dir().join(format!("keel-branch-merge-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("oracle.c"),
        r#"
#include <stdint.h>
#include <stdlib.h>
extern unsigned char pointer_equal(void*,void*);
extern uint64_t clean_merge(void*,unsigned char);
extern uint64_t write_merge(void*,void*,unsigned char,uint64_t);
int main(void) {
 uintptr_t pointers[]={0,1,2,255,UINT64_MAX,UINT64_C(0x8000000000000000)};
 for(unsigned i=0;i<6;i++)for(unsigned j=0;j<6;j++)
  if(pointer_equal((void*)pointers[i],(void*)pointers[j])!=(pointers[i]==pointers[j]))abort();
 uint64_t state=17;
 for(unsigned i=0;i<10000;i++) {
  state^=state<<13;state^=state>>7;state^=state<<17;
  for(unsigned cond=0;cond<3;cond++)for(unsigned alias=0;alias<2;alias++) {
   uint64_t a=state,b=~state,newvalue=state+19;
   if(clean_merge(&a,cond)!=a)abort();
   uint64_t want=cond&&alias?newvalue:a;
   if(write_merge(&a,alias?&a:&b,cond,newvalue)!=want)abort();
   if(a!=want || b!=(cond&&!alias?newvalue:~state))abort();
  }
 }
 return 0;
}"#,
    )
    .unwrap();
    for optimized in [false, true] {
        let mut m = keel_ir::parse(MERGE_SOURCE).unwrap();
        if optimized {
            keel_opt::run_passes(&mut m, &["branch-ranges", "load-reuse"]).unwrap();
        }
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            for allocator in [AllocationTier::Fast, AllocationTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let program = keel_codegen::compile_program_with_pipeline(
                    &m,
                    Target::Aarch64AppleDarwin,
                    &pipeline,
                )
                .unwrap();
                fs::write(
                    dir.join("test.o"),
                    keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
                )
                .unwrap();
                let output = Command::new("clang")
                    .args(["-O2"])
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("test.o"))
                    .arg("-o")
                    .arg(dir.join("test"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(dir.join("test")).status().unwrap().success(),
                    "{optimized}/{selector:?}/{allocator:?}"
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
