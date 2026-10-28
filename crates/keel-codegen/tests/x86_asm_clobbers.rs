use keel_codegen::{
    CodegenPipeline,
    regalloc::{Pools, RegisterAllocator},
};
use keel_codegen::{SelectionTier, regalloc::AllocationTier};

const SOURCE: &str = r#"
func @plain(i64) -> i64 {
entry(v0:i64):
 asm "nop", "~{memory},~{cc}"()
 v1 = iconst.i64 7
 v2 = iadd v0, v1
 return v2
}
func @clobber(i64, i64) -> i64 {
entry(v0:i64, v1:i64):
 asm "xorq %%rcx, %%rcx; xorq %%rbx, %%rbx; xorq %%r12, %%r12", "~{rcx},~{rbx},~{r12},~{cc}"()
 v2 = iadd v0, v1
 return v2
}
func @tied_live(i64, i64) -> i64 {
entry(v0:i64, v1:i64):
 v2 = asm "addq %q2, %q0", "=r,0,r,~{cc}"(v0, v1) -> i64
 v3 = iadd v0, v1
 v4 = iadd v2, v3
 return v4
}
func @memory(ptr, i64) -> i64 {
entry(v0:ptr, v1:i64):
 asm "addq %q1, %0", "mreadwrite,r,~{cc}"(v0, v1)
 v2 = load.i64 v0
 v3 = iadd v1, v2
 return v3
}
func @equal(i64, i64) -> i64 {
entry(v0:i64, v1:i64):
 v2 = asm "cmpq %q1, %q2", "=@ccz,r,r,~{cc}"(v0, v1) -> i8
 v3 = uext.i64 v2
 v4 = iadd v0, v1
 v5 = iadd v3, v4
 return v5
}
func @constant_live(i64) -> i64 {
entry(v0:i64):
 v1 = iconst.i64 1234605616436508552
 v2 = asm "addq %q2, %q0", "=r,0,r,~{cc}"(v0, v1) -> i64
 v3 = iadd v1, v2
 return v3
}
"#;

#[test]
fn selective_preservation_keeps_transport_parity_and_exports_runtime_cases() {
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (bytes, text) = compile(selector, &allocator, spilled);
                let plain = text
                    .split_once("\"plain\":\n")
                    .unwrap()
                    .1
                    .split_once(".Lplain_end:")
                    .unwrap()
                    .0;
                assert!(plain.contains("nop"), "{plain}");
                assert!(
                    !plain.contains("push r12") && !plain.contains("push rbx"),
                    "operand-free asm must not force callee saves: {plain}"
                );
                if !spilled {
                    assert!(
                        !plain.contains("[rbp -"),
                        "operand-free asm must not allocate staging: {plain}"
                    );
                }
                let clobber = text
                    .split_once("\"clobber\":\n")
                    .unwrap()
                    .1
                    .split_once(".Lclobber_end:")
                    .unwrap()
                    .0;
                assert!(
                    !clobber.contains("push r13")
                        && !clobber.contains("push r14")
                        && !clobber.contains("push r15"),
                    "undeclared callee saves: {clobber}"
                );
                if let Some(dir) = std::env::var_os("KEEL_ASM_CLOBBER_EXPORT_DIR") {
                    std::fs::create_dir_all(&dir).unwrap();
                    std::fs::write(
                        std::path::Path::new(&dir)
                            .join(format!("{selector:?}-{allocator:?}-{spilled}.o")),
                        bytes,
                    )
                    .unwrap();
                }
            }
        }
    }
}

struct RematConstants;
impl RegisterAllocator for RematConstants {
    fn name(&self) -> &'static str {
        "asm-clobber-remat"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        use keel_ir::EntityRef;
        let mut allocation = AllocationTier::Fast.allocate(input);
        for (_, inst) in input.func.layout_insts() {
            if matches!(input.func.inst_data(inst), keel_ir::InstData::Iconst { .. }) {
                for &value in input.func.inst_results(inst) {
                    allocation.locs[0][value.index()] =
                        Some(keel_codegen::regalloc::Loc::Remat(value));
                }
            }
        }
        allocation
    }
}

struct CalleeHomes;
impl RegisterAllocator for CalleeHomes {
    fn name(&self) -> &'static str {
        "asm-clobber-callee-homes"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let pools = Pools {
            caller_saved: &[],
            callee_saved: &[3, 12],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        AllocationTier::Fast.allocate(&keel_codegen::regalloc::AllocationInput {
            pools: &pools,
            ..*input
        })
    }
}

#[test]
fn staging_preserves_rematerialized_inputs_and_live_clobbered_homes() {
    for selector in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocator in [&RematConstants as &dyn RegisterAllocator, &CalleeHomes] {
            let (bytes, _) = compile(selector, allocator, false);
            if let Some(dir) = std::env::var_os("KEEL_ASM_CLOBBER_EXPORT_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{selector:?}-{}.o", allocator.name())),
                    bytes,
                )
                .unwrap();
            }
        }
    }
}

fn compile(
    selector: SelectionTier,
    allocator: &dyn RegisterAllocator,
    spilled: bool,
) -> (Vec<u8>, String) {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    if selector == SelectionTier::Full {
        keel_opt::optimize(&mut module).unwrap();
    }
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator,
    };
    let empty = Pools {
        caller_saved: &[],
        callee_saved: &[],
        float_caller_saved: &[],
        float_callee_saved: &[],
    };
    let pools = if spilled {
        &empty
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&module, pools, &pipeline).unwrap();
    let program =
        keel_codegen::x86_64::compile_program_with_pipeline(&module, pools, &pipeline).unwrap();
    let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
    assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(
        &module,
        pools,
        &pipeline,
        &mut |p| chunks.append(p),
    )
    .unwrap();
    assert_eq!(
        bytes,
        keel_asm::x86::assemble_program_object(&chunks).unwrap()
    );
    (bytes, text)
}
