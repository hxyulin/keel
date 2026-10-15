#![cfg(all(target_arch = "aarch64", target_os = "macos"))]
use keel_codegen::regalloc::{AllocationTier, Pools};
use keel_codegen::tuning::{TunedSelector, TuningCpu};
use keel_codegen::{CodegenPipeline, SelectionOptions};
use std::process::Command;

#[test]
fn tuned_integer_multiply_executes_with_optimization_allocation_and_spills() {
    let mut source = String::new();
    let mut driver = String::from("#include <stdint.h>\n");
    let mut checks = String::new();
    for bits in [32, 64] {
        for constant in [3, 5, 9] {
            for commuted in [false, true] {
                let name = format!("f{bits}_{constant}_{}", u8::from(commuted));
                let args = if commuted { "v1, v0" } else { "v0, v1" };
                source += &format!(
                    "func @{name}(i{bits}) -> i{bits} {{\nblock0(v0: i{bits}):\n v1 = iconst.i{bits} {constant}\n v2 = imul {args}\n v3 = xor v2, v1\n return v3\n}}\n"
                );
                driver += &format!("extern uint{bits}_t {name}(uint{bits}_t);\n");
                checks += &format!(
                    "if ({name}((uint{bits}_t)x) != (uint{bits}_t)(((uint{bits}_t)x * (uint{bits}_t){constant}) ^ {constant})) return 1;\n"
                );
            }
        }
    }
    driver += &format!(
        "int main(void) {{ uint64_t edge[] = {{0,1,127,128,255,65535,UINT32_MAX,UINT64_MAX,UINT64_C(1)<<63}};\n for (unsigned i=0;i<sizeof(edge)/sizeof(edge[0])+256;++i) {{ uint64_t x=i<256?i:edge[i-256]; {checks} }} return 0; }}\n"
    );
    let original = keel_ir::parse(&source).unwrap();
    let directory = std::env::temp_dir().join(format!("keel-m5-native-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("driver.c"), driver).unwrap();
    let tiny = Pools {
        caller_saved: &[9],
        callee_saved: &[],
        float_caller_saved: &[19],
        float_callee_saved: &[8],
    };
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Basic,
        keel_opt::OptimizationTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            for pools in [&keel_codegen::aarch64::REGISTERS, &tiny] {
                let mut module = original.clone();
                keel_opt::optimize_with_tier(&mut module, tier).unwrap();
                let assembly = keel_codegen::aarch64::compile_with_pipeline(
                    &module,
                    pools,
                    &CodegenPipeline {
                        selector: &TunedSelector {
                            cpu: TuningCpu::AppleM5,
                            options: SelectionOptions::ALL,
                        },
                        allocator: &allocation,
                    },
                )
                .unwrap();
                if pools.callee_saved.is_empty() {
                    assert!(
                        assembly.contains("\tstr\t"),
                        "restricted allocation must exercise spills"
                    );
                }
                std::fs::write(
                    directory.join("program.o"),
                    keel_asm::assemble_object(&assembly).unwrap(),
                )
                .unwrap();
                let output = Command::new("clang")
                    .arg("-O2")
                    .arg(directory.join("driver.c"))
                    .arg(directory.join("program.o"))
                    .arg("-o")
                    .arg(directory.join("program"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(directory.join("program"))
                        .status()
                        .unwrap()
                        .success()
                );
            }
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
