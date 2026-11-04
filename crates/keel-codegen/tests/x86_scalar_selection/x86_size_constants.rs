use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, SelectionTier, Target,
    regalloc::{Allocation, AllocationInput, AllocationTier, RegisterAllocator},
};

const VALUES: &[u64] = &[
    0xffff_ffff_0000_0000,
    0xffff_ffff_7fff_ffff,
    0xffff_ffff_8000_0000,
    0xffff_ffff_ffff_ffff,
    0xffff_fffe_ffff_ffff,
    0x0000_0000_ffff_ffff,
];

fn emission(goal: CodegenGoal) -> EmissionOptions {
    let mut options = EmissionOptions::default();
    options.goal = goal;
    options
}

struct Restricted(&'static [u8]);
impl RegisterAllocator for Restricted {
    fn name(&self) -> &'static str {
        if self.0.is_empty() {
            "constant-spilled"
        } else {
            "constant-high"
        }
    }
    fn allocate(&self, input: &AllocationInput<'_>) -> Allocation {
        let pools = keel_codegen::Pools {
            caller_saved: self.0,
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
fn allocators() -> [&'static dyn RegisterAllocator; 4] {
    [
        &AllocationTier::Fast,
        &AllocationTier::Full,
        &Restricted(&[8]),
        &Restricted(&[]),
    ]
}

fn source() -> String {
    let mut source = String::from("decl @sink(i64,i64,i64,i64,i64,i64) -> i64\n");
    for (i, &k) in VALUES.iter().enumerate() {
        source += &format!(
            "func @constant{i}() -> i64 {{\nentry:\n v0 = iconst.i64 {}\n return v0\n}}\n\
             func @choose{i}(i8) -> i64 {{\nentry(v0:i8):\n v1 = iconst.i64 {}\n v2 = iconst.i64 37\n v3 = select v0,v1,v2\n return v3\n}}\n",
            k as i64, k as i64,
        );
    }
    source += &format!(
        "func @arguments() -> i64 {{\nentry:\n v0 = iconst.i64 {}\n v1 = call @sink(v0,v0,v0,v0,v0,v0)\n return v1\n}}\n",
        VALUES[0] as i64,
    );
    source
}

#[test]
fn size_constants_use_shorter_flag_preserving_sequences_and_keep_boundaries() {
    let module = keel_ir::parse(&source()).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in allocators() {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: allocation,
            };
            for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                let text = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(emission(goal))
                    .text(&module)
                    .unwrap();
                let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(emission(goal))
                    .program(&module)
                    .unwrap();
                assert_eq!(
                    keel_asm::x86::assemble_program_object(&program).unwrap(),
                    keel_asm::x86::assemble_object(&text).unwrap(),
                );
                for (i, &k) in VALUES.iter().enumerate() {
                    let body = text
                        .split_once(&format!("\"constant{i}\":\n"))
                        .unwrap()
                        .1
                        .split_once(&format!(".Lconstant{i}_end:"))
                        .unwrap()
                        .0;
                    let shorter = goal == CodegenGoal::Size && i < 2;
                    assert_eq!(body.contains("\tnot\t"), shorter, "{k:#x} {goal:?}\n{body}");
                    if shorter {
                        assert!(!body.contains("movabs"), "{body}");
                    }
                }
                if goal == CodegenGoal::Size {
                    if allocation.name() == "constant-high" {
                        assert!(text.contains("\tnot\tr8\n"), "{text}");
                    }
                } else {
                    assert!(!text.contains("\tnot\t"), "{text}");
                }
            }
        }
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_size_constants_preserve_values_argument_registers_and_select_flags() {
    use std::{fs, process::Command};
    let module = keel_ir::parse(&source()).unwrap();
    let dir = std::env::temp_dir().join(format!("keel-size-constants-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut c = String::from("#include <stdint.h>\n#include <stdlib.h>\n");
    c += &format!(
        "uint64_t sink(uint64_t a,uint64_t b,uint64_t c,uint64_t d,uint64_t e,uint64_t f) {{ if(a!={0}ULL||b!=a||c!=a||d!=a||e!=a||f!=a) abort(); return a; }}\nextern uint64_t arguments(void);\n",
        VALUES[0]
    );
    for i in 0..VALUES.len() {
        c += &format!("extern uint64_t constant{i}(void),choose{i}(uint8_t);\n");
    }
    c += "int main(void) { const uint8_t flags[]={0,1,2,127,128,255};\n";
    for (i, k) in VALUES.iter().enumerate() {
        c += &format!(
            "if(constant{i}()!={k}ULL) return 1; for(unsigned j=0;j<sizeof(flags);j++) if(choose{i}(flags[j])!=(flags[j]?{k}ULL:37ULL)) return 2;\n"
        );
    }
    c += &format!("return arguments()!={}ULL; }}\n", VALUES[0]);
    fs::write(dir.join("driver.c"), c).unwrap();
    for selection in [SelectionTier::Baseline, SelectionTier::Full] {
        for allocation in allocators() {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: allocation,
            };
            for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
                    .emission(emission(goal))
                    .program(&module)
                    .unwrap();
                fs::write(
                    dir.join("code.o"),
                    keel_asm::x86::assemble_program_object(&program).unwrap(),
                )
                .unwrap();
                let build = Command::new("clang")
                    .current_dir(&dir)
                    .args(["driver.c", "code.o", "-o", "check"])
                    .output()
                    .unwrap();
                assert!(
                    build.status.success(),
                    "{}",
                    String::from_utf8_lossy(&build.stderr)
                );
                assert!(
                    Command::new(dir.join("check")).status().unwrap().success(),
                    "{selection:?} {} {goal:?}",
                    allocation.name()
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
