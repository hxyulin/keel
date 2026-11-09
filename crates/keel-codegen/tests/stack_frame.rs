//! Frame storage is independent of declared slot identity and allocation tier.
#[path = "stack_frame/lifetime.rs"]
mod lifetime;
#[path = "stack_frame/returns.rs"]
mod returns;

use keel_codegen::{CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier};

#[test]
fn sparse_escaping_slots_assemble_on_both_targets() {
    let module = keel_ir::parse(include_str!("programs/compact_frame.keel")).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in Target::NATIVE {
        for allocator in [
            AllocationTier::Fast,
            AllocationTier::Full,
            #[cfg(feature = "experimental-regalloc")]
            AllocationTier::Local,
            #[cfg(feature = "experimental-regalloc")]
            AllocationTier::Split,
        ] {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let asm = keel_codegen::compile_with_pipeline(
                    &module,
                    target,
                    &CodegenPipeline {
                        selector: &selector,
                        allocator: &allocator,
                    },
                )
                .unwrap();
                let object = keel_asm::assemble_source_for(target, &asm).unwrap();
                assert!(!object.is_empty());
            }
        }
    }
}

#[test]
fn unused_slots_do_not_consume_frames_or_frame_limits() {
    let plain = keel_ir::parse("func @f() {\nblock0:\n return\n}").unwrap();
    let dead = keel_ir::parse(
        "func @f() {
            ss0 = stack_slot 2147483647, align 16
            ss1 = stack_slot 2147483647, align 16
        block0:
            return
        }",
    )
    .unwrap();
    let live = keel_ir::parse(
        "func @f() -> ptr {
            ss0 = stack_slot 2147483647, align 16
        block0:
            v0 = stack_addr ss0
            return v0
        }",
    )
    .unwrap();
    let debug = keel_ir::parse(
        "file f0 = \"frame.c\"\nfunc @f() {
            ss0 = stack_slot 2147483647, align 16
        block0:
            return
        }",
    )
    .unwrap();
    let tiers = [
        AllocationTier::Fast,
        AllocationTier::Full,
        #[cfg(feature = "experimental-regalloc")]
        AllocationTier::Local,
        #[cfg(feature = "experimental-regalloc")]
        AllocationTier::Split,
    ];
    for target in Target::NATIVE {
        for allocator in tiers {
            for selector in [SelectionTier::Baseline, SelectionTier::Full] {
                let pipeline = CodegenPipeline {
                    selector: &selector,
                    allocator: &allocator,
                };
                let compile =
                    |module| keel_codegen::compile_with_pipeline(module, target, &pipeline);
                assert_eq!(compile(&plain).unwrap(), compile(&dead).unwrap());
                for module in [&live, &debug] {
                    let err = compile(module).unwrap_err();
                    assert!(err.to_string().contains("too large"), "{err}");
                }
            }
        }
    }
}

fn large_arguments() -> (String, String) {
    let count = 686;
    let types = format!("{}, i8 uext", vec!["i64"; count].join(", "));
    let params = format!(
        "{}, v686: i8",
        (0..count)
            .map(|i| format!("v{i}: i64"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let args = format!("{}, v1", vec!["v0"; count].join(", "));
    let source = format!(
        "decl @c_receive({types}) -> i64
func @k_direct() -> i64 {{
block0:
 v0 = iconst.i64 7
 v1 = iconst.i8 123
 v2 = call @c_receive({args})
 return v2
}}
func @k_indirect(ptr) -> i64 {{
block0(v3: ptr):
 v0 = iconst.i64 7
 v1 = iconst.i8 123
 v2 = call_indirect v3({args}) : ({types}) -> i64
 return v2
}}
func @k_receive({types}) -> i64 {{
block0({params}):
 v687 = uext.i64 v686
 v688 = iadd v0, v685
 v689 = iadd v688, v687
 return v689
}}"
    );
    let c_params = format!(
        "{}, unsigned char tail",
        (0..count)
            .map(|i| format!("unsigned long a{i}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let c_args = format!("{}, 123", vec!["7"; count].join(", "));
    let c = format!("unsigned long c_receive({c_params}) {{ return a0 + a685 + tail; }}
extern unsigned long k_receive({c_params});
extern unsigned long k_direct(void);
extern unsigned long k_indirect(void *);
int main(void) {{ return k_direct() != 137 || k_indirect((void *)c_receive) != 137 || k_receive({c_args}) != 137; }}");
    (source, c)
}

#[test]
fn large_aarch64_arguments_assemble_and_match_native_c() {
    use std::{fs, process::Command};
    let (source, c) = large_arguments();
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let dir =
        std::env::temp_dir().join(format!("keel-large-stack-arguments-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let asm = keel_codegen::compile(&module, target).unwrap();
        let object = keel_asm::assemble_source_for(target, &asm)
            .unwrap_or_else(|error| panic!("{target:?}: {error}\n{asm}"));
        if Target::host() == Some(target) {
            let obj = dir.join("large.o");
            let driver = dir.join("driver.c");
            let exe = dir.join("run");
            fs::write(&obj, object).unwrap();
            fs::write(&driver, &c).unwrap();
            let output = Command::new("clang")
                .arg("-w")
                .arg(&driver)
                .arg(&obj)
                .arg("-o")
                .arg(&exe)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(Command::new(&exe).status().unwrap().success());
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn aarch64_large_incoming_offsets_and_aggregate_copy_tails_assemble() {
    let count = 4105;
    let types = vec!["i64"; count].join(", ");
    let params = (0..count)
        .map(|i| format!("v{i}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let prefix = vec!["i64"; 686].join(", ");
    let args = vec!["v1"; 686].join(", ");
    let source = format!(
        "decl @consume({prefix}, ptr byval {{[4097 x i8]}})
func @copy(ptr) {{
block0(v0: ptr):
 v1 = iconst.i64 7
 call @consume({args}, v0)
 return
}}
func @last({types}) -> i64 {{
block0({params}):
 return v4104
}}"
    );
    let pointer_params = (0..686)
        .map(|i| format!("v{i}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "{source}\nfunc @pointer_tail({prefix}, ptr) -> ptr {{\nblock0({pointer_params}, v686: ptr):\n return v686\n}}\n"
    );
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let asm = keel_codegen::compile(&module, target).unwrap();
        let object = keel_asm::assemble_source_for(target, &asm)
            .unwrap_or_else(|error| panic!("{target:?}: {error}\n{asm}"));
        assert!(!object.is_empty());
    }
}

#[test]
fn invoke_without_plain_calls_reserves_aggregate_storage() {
    let module = keel_ir::parse(
        "decl @personality()
 decl @callee(ptr sret {i64, i64}, ptr byval {i64, i64, i64})
 func @invoke_only(ptr, ptr) personality @personality {
 block0(v0: ptr, v1: ptr):
     invoke @callee(v0, v1), block1, block2
 block1:
     return
 block2:
     v2, v3 = landingpad cleanup
     trap
 }",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let asm = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
    assert!(!keel_asm::assemble_object(&asm).unwrap().is_empty());
}

fn assemble_aarch64(source: &str) {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
        let asm = keel_codegen::compile(&module, target).unwrap();
        let object = keel_asm::assemble_source_for(target, &asm)
            .unwrap_or_else(|error| panic!("{target:?}: {error}\n{asm}"));
        assert!(!object.is_empty());
    }
}

#[test]
fn aarch64_stack_alloc_above_large_outgoing_arguments_assembles() {
    let count = 530;
    let types = vec!["i64"; count].join(", ");
    let args = vec!["v0"; count].join(", ");
    assemble_aarch64(&format!(
        "decl @many({types}) -> i64
func @f(i64) -> i64 {{
block0(v0: i64):
 v1 = intrinsic stack_alloc(v0)
 v2 = call @many({args})
 store v2, v1
 return v2
}}"
    ));
}

#[test]
fn aarch64_float_struct_register_areas_above_large_copies_assemble() {
    assemble_aarch64(
        "decl @sink(ptr byval [16800 x i8]) -> f32
func @param(ptr byval [3 x f32], ptr byval [16800 x i8]) -> f32 {
block0(v0: ptr, v1: ptr):
 v2 = load.f32 v0
 v3 = call @sink(v1)
 v4 = fadd v2, v3
 return v4
}
func @ret(ptr sret [3 x f32], ptr byval [16800 x i8]) {
block0(v0: ptr, v1: ptr):
 v2 = call @sink(v1)
 store v2, v0
 return
}",
    );
}

#[test]
fn aarch64_epilogue_resets_sp_only_when_the_frame_moved_it() {
    let module = keel_ir::parse(
        "decl @g(i64) -> i64
func @plain(i32, i32) -> i32 cc(c) {
block0(v0: i32, v1: i32):
 v2 = iadd v0, v1
 return v2
}
func @framed(i64) -> i64 cc(c) {
    ss0 = stack_slot 8, align 8
block0(v0: i64):
 v1 = stack_addr ss0
 store v0, v1
 v2 = ptrtoint.i64 v1
 v3 = call @g(v2)
 return v3
}",
    )
    .unwrap();
    keel_ir::verify_module(&module).unwrap();
    let asm = keel_codegen::compile(&module, Target::Aarch64AppleDarwin).unwrap();
    let (plain, framed) = asm.split_once("_framed:").unwrap();
    assert!(plain.contains("\tldp\tx29, x30, [sp], #16\n"), "{asm}");
    assert!(!plain.contains("mov\tsp, x29"), "{asm}");
    assert!(framed.contains("\tmov\tsp, x29\n"), "{asm}");
}

#[test]
fn unused_entry_parameters_take_no_callee_saved_registers() {
    // Nine arguments, three read: the six unread ones would otherwise be
    // copied into callee-saved registers that the frame must save.
    let module = keel_ir::parse(
        "func @f(i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64 {
        block0(v0: i64, v1: i64, v2: i64, v3: i64, v4: i64, v5: i64, v6: i64, v7: i64, v8: i64):
            v9 = iadd v0, v4
            v10 = iadd v9, v8
            return v10
        }",
    )
    .unwrap();
    for target in Target::NATIVE {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &SelectionTier::Full,
                allocator: &allocator,
            };
            let asm = keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
            for saved in ["rbx", "r12", "r13", "r14", "r15", "x19", "x20"] {
                assert!(
                    !asm.contains(saved),
                    "{target:?} {allocator:?} saves {saved}:\n{asm}"
                );
            }
        }
    }
}

/// Slots aligned above 16 get aligned addresses on AArch64, which rounds `sp`
/// down in the prologue. The C caller varies the incoming `sp` modulo 64, and
/// the cases cover stack arguments, which are addressed from x29, and dynamic
/// allocation, which addresses the frame from x19.
#[test]
fn slots_aligned_above_sixteen_have_aligned_addresses() {
    let source = "decl @check(ptr, ptr) -> i64
func @aligned(i64) -> i64 {
    ss0 = stack_slot 8, align 8
    ss1 = stack_slot 64, align 64
    ss2 = stack_slot 32, align 32
block0(v0: i64):
    v1 = stack_addr ss0
    store v0, v1
    v2 = stack_addr ss1
    v3 = stack_addr ss2
    store v0, v2
    v4 = call @check(v2, v3)
    v5 = load.i64 v1
    v6 = load.i64 v2
    v7 = iadd v4, v5
    v8 = iadd v7, v6
    return v8
}
func @many(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) -> i64 {
    ss0 = stack_slot 64, align 64
block0(v0: i64, v1: i64, v2: i64, v3: i64, v4: i64, v5: i64, v6: i64, v7: i64, v8: i64, v9: i64):
    v10 = stack_addr ss0
    store v9, v10
    v11 = call @check(v10, v10)
    v12 = load.i64 v10
    v13 = iadd v11, v12
    v14 = iadd v13, v8
    return v14
}
func @shared(i64) -> i64 {
    ss0 = stack_slot 64, align 64
    ss1 = stack_slot 8, align 8
    ss2 = stack_slot 16, align 16
block0(v0: i64):
    v1 = stack_addr ss0
    v2 = stack_addr ss1
    v3 = stack_addr ss2
    intrinsic lifetime_start(v1)
    store v0, v1
    v4 = call @check(v1, v1)
    v5 = load.i64 v1
    intrinsic lifetime_end(v1)
    intrinsic lifetime_start(v2)
    intrinsic lifetime_start(v3)
    store v0, v2
    store v0, v3
    v6 = call @check(v1, v3)
    v7 = load.i64 v2
    v8 = load.i64 v3
    intrinsic lifetime_end(v2)
    intrinsic lifetime_end(v3)
    v9 = iadd v4, v6
    v10 = iadd v9, v5
    v11 = iadd v10, v7
    v12 = iadd v11, v8
    return v12
}
func @dynamic(i64) -> i64 {
    ss0 = stack_slot 64, align 64
block0(v0: i64):
    v1 = intrinsic stack_alloc(v0)
    v2 = stack_addr ss0
    store v0, v2
    store v0, v1
    v3 = call @check(v2, v1)
    v4 = load.i64 v2
    v5 = load.i64 v1
    v6 = iadd v3, v4
    v7 = iadd v6, v5
    return v7
}
";
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let debug = keel_ir::parse(&format!("file f0 = \"frame.c\"\n{source}")).unwrap();
    let oracle = r#"
#include <alloca.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
long aligned(long), many(long, long, long, long, long, long, long, long, long, long), dynamic(long);
/* ss2 of shared, a 16-aligned slot, takes the bytes of its 64-aligned ss0. */
long shared(long);
/* Misalignment is reported as a large value, so sums cannot hide it. */
long check(void *a, void *b) {
    return ((uintptr_t)a & 63 ? 1000000 : 0) + ((uintptr_t)b & 15 ? 1000000 : 0);
}
static long run(int pad) {
    volatile char *p = alloca(pad + 1);
    p[0] = 0;
    return aligned(7) - 14 + many(0, 0, 0, 0, 0, 0, 0, 0, 5, 9) - 14 + dynamic(24) - 48 +
           shared(5) - 15;
}
int main(void) {
    for (int pad = 0; pad < 128; pad += 16)
        if (run(pad)) { fprintf(stderr, "pad %d: %ld\n", pad, run(pad)); return 1; }
    return 0;
}
"#;
    let dir = std::env::temp_dir().join(format!("keel-aligned-slots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("oracle.c"), oracle).unwrap();
    for allocator in [AllocationTier::Fast, AllocationTier::Full] {
        for selector in [SelectionTier::Baseline, SelectionTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selector,
                allocator: &allocator,
            };
            for target in [Target::Aarch64AppleDarwin, Target::Aarch64LinuxGnu] {
                for m in [&module, &debug] {
                    let asm = keel_codegen::compile_with_pipeline(m, target, &pipeline).unwrap();
                    keel_asm::assemble_source_for(target, &asm).unwrap();
                }
            }
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            {
                let asm = keel_codegen::compile_with_pipeline(
                    &module,
                    Target::Aarch64AppleDarwin,
                    &pipeline,
                )
                .unwrap();
                assert!(asm.contains("and\tsp, x16, #0xffffffffffffffc0"), "{asm}");
                std::fs::write(dir.join("frame.s"), asm).unwrap();
                let status = std::process::Command::new("clang")
                    .arg(dir.join("oracle.c"))
                    .arg(dir.join("frame.s"))
                    .arg("-o")
                    .arg(dir.join("frame"))
                    .status()
                    .unwrap();
                assert!(status.success());
                let run = std::process::Command::new(dir.join("frame"))
                    .output()
                    .unwrap();
                assert!(
                    run.status.success(),
                    "{selector:?}/{allocator:?}: {}",
                    String::from_utf8_lossy(&run.stderr)
                );
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
