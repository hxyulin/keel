//! Wrapping sums must count their initial value once and never read a tail
//! past the scalar bound. A protected page follows every native input.
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
};

const TINY: Pools = Pools {
    caller_saved: &[9, 10],
    callee_saved: &[19, 20],
    float_caller_saved: &[19, 20],
    float_callee_saved: &[8, 9],
};

fn source() -> String {
    let mut out = String::new();
    for bits in [32, 64] {
        for index in [32, 64] {
            for signed in [false, true] {
                let name = format!("sum_{bits}_{index}_{}", if signed { "s" } else { "u" });
                let compare = if signed { "slt" } else { "ult" };
                let extend = if signed { "sext" } else { "uext" };
                let conversion = if index == 32 {
                    format!("v7 = {extend}.i64 v4\n")
                } else {
                    String::new()
                };
                let iv = if index == 32 { "v7" } else { "v4" };
                let scale = if bits == 32 { 2 } else { 3 };
                let mask = if bits == 32 {
                    0xff00ff
                } else {
                    0xff00ff00ff00ffi64
                };
                out.push_str(&format!(
                    "func @{name}(ptr,i{index},i{index},i{bits}) -> i{bits} cc(c) {{
block0(v0: ptr,v1: i{index},v2: i{index},v3: i{bits}):
 jump block1(v2,v3)
block1(v4: i{index},v5: i{bits}):
 v6 = icmp.{compare} v4,v1
 brif v6,block2,block3
block2:
 {conversion}v8 = iconst.i64 {scale}
 v9 = shl {iv},v8
 v10 = ptradd v0,v9
 v11 = load.i{bits} v10
 v12 = iconst.i{bits} {mask}
 v13 = and v11,v12
 v14 = iadd v5,v13
 v15 = iconst.i{index} 1
 v16 = iadd v4,v15
 jump block1(v16,v14)
block3:
 return v5
}}\n"
                ));
            }
        }
    }
    out
}

fn compile(optimized: bool, allocation: AllocationTier, pools: &Pools) -> (String, Vec<u8>) {
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if optimized {
        keel_opt::run_passes_for_target(
            &mut m,
            keel_opt::DEFAULT_PIPELINE,
            keel_opt::VectorTarget::Aarch64Neon,
        )
        .unwrap();
        keel_opt::run_passes_for_target(
            &mut m,
            &["loop-reduce"],
            keel_opt::VectorTarget::Aarch64Neon,
        )
        .unwrap();
        assert!(
            m.functions.iter().all(|f| f
                .layout_insts()
                .any(|(_, i)| f.inst_data(i).is_vector_instruction())),
            "{m}"
        );
    }
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &allocation,
    };
    let text = keel_codegen::aarch64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
    let program =
        keel_codegen::aarch64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
    let object = keel_asm::assemble_program_object(&program, Default::default()).unwrap();
    assert_eq!(object, keel_asm::assemble_object(&text).unwrap());
    (text, object)
}

#[test]
fn sum_machine_objects_match_text_for_both_allocators_and_register_pressure() {
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&TINY, &keel_codegen::aarch64::REGISTERS] {
            compile(true, allocation, pools);
        }
    }
}

#[test]
fn sse2_sum_objects_match_text_with_spilled_vector_accumulators() {
    let tiny = Pools {
        caller_saved: &[6, 7],
        callee_saved: &[3],
        float_caller_saved: &[0, 1],
        float_callee_saved: &[],
    };
    let mut m = keel_ir::parse(&source()).unwrap();
    keel_opt::run_passes_for_target(
        &mut m,
        keel_opt::DEFAULT_PIPELINE,
        keel_opt::VectorTarget::X86Sse2,
    )
    .unwrap();
    keel_ir::verify_module(&m).unwrap();
    keel_opt::run_passes_for_target(&mut m, &["loop-reduce"], keel_opt::VectorTarget::X86Sse2)
        .unwrap();
    for allocation in [AllocationTier::Fast, AllocationTier::Full] {
        for pools in [&tiny, &keel_codegen::x86_64::REGISTERS] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &allocation,
            };
            let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline).unwrap();
            let program =
                keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline).unwrap();
            let object = keel_asm::x86::assemble_program_object(&program).unwrap();
            assert_eq!(object, keel_asm::x86::assemble_object(&text).unwrap());
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn multiple_faulting_streams_keep_the_scalar_first_fault() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/runtime-loops-20261020/fault-order-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("driver.c"),
        include_str!("fixtures/reduction_fault_order.c"),
    )
    .unwrap();
    for optimized in [false, true] {
        let mut m = keel_ir::parse(include_str!("fixtures/reduction_fault_order.ir")).unwrap();
        if optimized {
            keel_opt::optimize(&mut m).unwrap();
            keel_opt::run_passes(&mut m, &["loop-reduce"]).unwrap();
        }
        assert!(
            !m.functions[0]
                .layout_insts()
                .any(|(_, i)| m.functions[0].inst_data(i).is_vector_instruction())
        );
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &allocation,
            };
            let program = keel_codegen::aarch64::compile_program_with_pipeline(
                &m,
                &keel_codegen::aarch64::REGISTERS,
                &pipeline,
            )
            .unwrap();
            fs::write(
                dir.join("test.o"),
                keel_asm::assemble_program_object(&program, Default::default()).unwrap(),
            )
            .unwrap();
            let output = Command::new("clang")
                .arg("-O2")
                .arg(dir.join("driver.c"))
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
            let result = Command::new(dir.join("test")).output().unwrap();
            assert_eq!(
                result.status.code(),
                Some(20),
                "first fault changed: {}",
                String::from_utf8_lossy(&result.stdout)
            );
        }
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn native_sums_match_scalar_oracle_at_guarded_tails_and_wrapping_boundaries() {
    use std::{fs, process::Command};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/runtime-loops-20261020/native-sums-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let mut c = String::from("#include <stdint.h>\n#include <unistd.h>\n#include <sys/mman.h>\n");
    let mut body = String::from(
        "int main(void) { long page=sysconf(_SC_PAGESIZE); if(page<4096)return 3; unsigned char* region=mmap(0,2*page,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANON,-1,0); if(region==MAP_FAILED)return 4; if(mprotect(region+page,page,PROT_NONE))return 5;\n",
    );
    for bits in [32, 64] {
        for index in [32, 64] {
            for signed in [false, true] {
                let name = format!("sum_{bits}_{index}_{}", if signed { "s" } else { "u" });
                let idx = format!("{}int{index}_t", if signed { "" } else { "u" });
                let mask = if bits == 32 {
                    0xff00ff
                } else {
                    0xff00ff00ff00ffi64
                };
                c.push_str(&format!(
                    "extern uint{bits}_t {name}(void*,{idx},{idx},uint{bits}_t);\n"
                ));
                body.push_str(&format!("{{ for(int n=0;n<=65;n++) {{ uint{bits}_t* p=(void*)(region+page-n*sizeof(*p)); for(int i=-3;i<n;i++) p[i]=(uint{bits}_t)(UINT64_C(0xfedcba9876543210) ^ ((uint64_t)i*UINT64_C(0x123456789abcdef))); for(int start=0;start<=n+1;start++) for(int k=0;k<4;k++) {{ uint{bits}_t seed=k==0?0:k==1?1:k==2?(uint{bits}_t)-1:(uint{bits}_t)(UINT64_C(1)<<({bits}-1)); uint{bits}_t want=seed; for(int i=start;i<n;i++) want+=(p[i] & UINT64_C({mask})); if({name}(p,n,start,seed)!=want)return 1; }} }} }}\n"));
                if signed {
                    body.push_str(&format!("{{ uint{bits}_t* p=(void*)(region+page-64*sizeof(*p)); uint{bits}_t want=17; for(int i=-3;i<64;i++) {{ p[i]=(uint{bits}_t)(UINT64_C(0xabc123456789)*((uint64_t)i+5)); want+=(p[i] & UINT64_C({mask})); }} if({name}(p,64,-3,17)!=want || {name}(region+page,-1,0,17)!=17)return 2; }}\n"));
                }
            }
        }
    }
    body.push_str("munmap(region,2*page); return 0; }\n");
    c.push_str(&body);
    fs::write(dir.join("driver.c"), c).unwrap();
    for optimized in [false, true] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&TINY, &keel_codegen::aarch64::REGISTERS] {
                let (_, object) = compile(optimized, allocation, pools);
                fs::write(dir.join("test.o"), object).unwrap();
                let output = Command::new("clang")
                    .arg("-O2")
                    .arg(dir.join("driver.c"))
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
                    "optimized={optimized}, allocation={allocation:?}"
                );
            }
        }
    }
}
