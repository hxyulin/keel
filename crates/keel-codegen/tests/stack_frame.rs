//! Frame storage is independent of declared slot identity and allocation tier.
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
            #[cfg(feature = "experimental-regalloc")]
            AllocationTier::Weighted,
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
                let object = match target {
                    Target::Aarch64AppleDarwin => keel_asm::assemble_object(&asm).unwrap(),
                    Target::Aarch64LinuxGnu => keel_asm::assemble_elf_object(&asm).unwrap(),
                    Target::X86_64LinuxGnu => keel_asm::x86::assemble_object(&asm).unwrap(),
                };
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
        #[cfg(feature = "experimental-regalloc")]
        AllocationTier::Weighted,
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
