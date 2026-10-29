//! Size emission preserves the SysV ABI and agrees across assembly transports.
use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, SelectionTier, Target, regalloc::AllocationTier,
};
use object::{Object, ObjectSection, ObjectSymbol};

const SOURCE: &str = r#"
decl @side(i64) -> i64
func @leaf(i64) -> i64 {
block0(v0:i64):
 v1 = iconst.i64 3
 v2 = iadd v0, v1
 return v2
}
func @call_only(i64) -> i64 {
block0(v0:i64):
 v1 = call @side(v0)
 return v1
}
func @choose(i64, i64) -> i64 {
block0(v0:i64, v1:i64):
 v2 = call @side(v0)
 brif v2, block1, block2
block1:
 v3 = iadd v0, v2
 return v3
block2:
 v4 = isub v1, v2
 return v4
}
func @word_loop(ptr, ptr) -> i64 {
block0(v0:ptr, v1:ptr):
 jump block1
block1:
 v2 = load.i64 v0
 v3 = load.i64 v1
 v4 = xor v2, v3
 brif v4, block1, block2(v4)
block2(v5:i64):
 return v5
}
"#;

fn options(goal: CodegenGoal) -> EmissionOptions {
    let mut options = EmissionOptions::default();
    options.goal = goal;
    options
}

fn function<'a>(text: &'a str, name: &str) -> &'a str {
    text.split_once(&format!("\"{name}\":\n"))
        .unwrap()
        .1
        .split_once(&format!(".L{name}_end:"))
        .unwrap()
        .0
}

#[test]
fn smaller_layout_frames_and_shared_returns_preserve_transport_parity() {
    let module = keel_ir::parse(SOURCE).unwrap();
    keel_ir::verify_module(&module).unwrap();
    for selection in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for allocation in [AllocationTier::Fast, AllocationTier::Full] {
            let pipeline = CodegenPipeline {
                selector: &selection,
                allocator: &allocation,
            };
            let mut objects = Vec::new();
            for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
                let text = keel_codegen::compile_with_pipeline_and_emission(
                    &module,
                    Target::X86_64LinuxGnu,
                    &pipeline,
                    options(goal),
                )
                .unwrap();
                let program = keel_codegen::compile_program_with_pipeline_and_emission(
                    &module,
                    Target::X86_64LinuxGnu,
                    &pipeline,
                    options(goal),
                )
                .unwrap();
                let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
                assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
                let mut chunks = keel_asm_ir::Program::new();
                keel_codegen::compile_program_chunks_with_pipeline_and_emission(
                    &module,
                    Target::X86_64LinuxGnu,
                    &pipeline,
                    options(goal),
                    &mut |p| chunks.append(p),
                )
                .unwrap();
                assert_eq!(
                    bytes,
                    keel_asm::x86::assemble_program_object(&chunks).unwrap()
                );
                if goal == CodegenGoal::Size {
                    assert!(!text.contains(".p2align\t4"), "{text}");
                    assert!(!text.contains(".p2align\t5"), "{text}");
                    assert!(!function(&text, "leaf").contains("rbp"), "{text}");
                    assert!(function(&text, "call_only").contains("leave"), "{text}");
                    let shared = function(&text, "choose");
                    assert_eq!(shared.matches("\tret\n").count(), 1, "{text}");
                    assert!(shared.contains(".Lchoose_return"), "{text}");
                    if let Some(dir) = std::env::var_os("KEEL_SIZE_EXPORT_DIR") {
                        let dir = std::path::PathBuf::from(dir);
                        std::fs::create_dir_all(&dir).unwrap();
                        std::fs::write(dir.join(format!("{selection:?}-{allocation:?}.o")), &bytes)
                            .unwrap();
                    }
                } else {
                    assert!(function(&text, "leaf").contains("push\trbp"), "{text}");
                    assert!(text.contains(".p2align\t5"), "{text}");
                    assert_eq!(
                        text,
                        keel_codegen::compile_with_pipeline(
                            &module,
                            Target::X86_64LinuxGnu,
                            &pipeline,
                        )
                        .unwrap()
                    );
                }
                objects.push(bytes);
            }
            let code_size = |bytes: &[u8]| {
                object::File::parse(bytes)
                    .unwrap()
                    .section_by_name(".text")
                    .unwrap()
                    .size()
            };
            assert!(code_size(&objects[1]) < code_size(&objects[0]));
        }
    }
}

#[test]
fn explicit_alignment_and_stack_observers_keep_their_contracts() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    module.functions[0].attrs.align = Some(64);
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::compile_program_with_pipeline_and_emission(
        &module,
        Target::X86_64LinuxGnu,
        &pipeline,
        options(CodegenGoal::Size),
    )
    .unwrap();
    let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert_eq!(file.section_by_name(".text").unwrap().align(), 64);
    assert_eq!(
        file.symbols()
            .find(|s| s.name().ok() == Some("leaf"))
            .unwrap()
            .address()
            % 64,
        0
    );
    for source in [
        "func @observe() -> ptr {\nblock0:\n v0 = intrinsic frame_address()\n return v0\n}\n",
        "func @observe(i64, i64, i64, i64, i64, i64, i64) -> i64 {\nblock0(v0:i64, v1:i64, v2:i64, v3:i64, v4:i64, v5:i64, v6:i64):\n return v6\n}\n",
    ] {
        let module = keel_ir::parse(source).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let text = keel_codegen::compile_with_pipeline_and_emission(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline,
            options(CodegenGoal::Size),
        )
        .unwrap();
        assert!(text.contains("push\trbp"), "{text}");
    }
}

#[test]
fn sharing_saves_bytes_even_when_returns_need_near_jumps() {
    // Put calls before both returns, so the earlier return crosses more than
    // 127 bytes regardless of the allocator's block order.
    let mut source = SOURCE.to_owned();
    for (block, start) in [("block1", 100), ("block2", 300)] {
        let calls: String = (start..start + 64)
            .map(|i| format!(" v{i} = call @side(v0)\n"))
            .collect();
        source = source.replacen(&format!("{block}:\n"), &format!("{block}:\n{calls}"), 1);
    }
    let module = keel_ir::parse(&source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let mut sizes = Vec::new();
    for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
        let text = keel_codegen::compile_with_pipeline_and_emission(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline,
            options(goal),
        )
        .unwrap();
        let bytes = keel_asm::x86::assemble_object(&text).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let symbol = file
            .symbols()
            .find(|s| s.name().ok() == Some("choose"))
            .unwrap();
        sizes.push(symbol.size());
        if goal == CodegenGoal::Size {
            let section = file
                .section_by_index(symbol.section_index().unwrap())
                .unwrap();
            let body =
                &section.data().unwrap()[symbol.address() as usize..][..symbol.size() as usize];
            assert!(
                body.contains(&0xe9),
                "expected a near jump to the shared return"
            );
            assert_eq!(function(&text, "choose").matches("\tret\n").count(), 1);
            if let Some(dir) = std::env::var_os("KEEL_SIZE_EXPORT_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(std::path::Path::new(&dir).join("near-jump.o"), &bytes).unwrap();
            }
        }
    }
    assert!(
        sizes[1] < sizes[0],
        "near jump sharing grew the function: {sizes:?}"
    );
}

struct RbxOnly;
impl keel_codegen::regalloc::RegisterAllocator for RbxOnly {
    fn name(&self) -> &'static str {
        "rbx-only-size-test"
    }
    fn allocate(
        &self,
        input: &keel_codegen::regalloc::AllocationInput<'_>,
    ) -> keel_codegen::regalloc::Allocation {
        let pools = keel_codegen::regalloc::Pools {
            caller_saved: &[],
            callee_saved: &[3],
            float_caller_saved: &[],
            float_callee_saved: &[],
        };
        AllocationTier::Full.allocate(&keel_codegen::regalloc::AllocationInput {
            func: input.func,
            pools: &pools,
            selection: input.selection,
            is_call: input.is_call,
            returns_twice: input.returns_twice,
        })
    }
}

#[test]
fn register_only_leaf_drops_unused_call_alignment_padding() {
    let module = keel_ir::parse("func @padding_leaf(i64) -> i64 {\nblock0(v0:i64):\n v1 = iconst.i64 3\n v2 = iadd v0, v1\n return v2\n}\n").unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &RbxOnly,
    };
    for goal in [CodegenGoal::Speed, CodegenGoal::Size] {
        let text = keel_codegen::compile_with_pipeline_and_emission(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline,
            options(goal),
        )
        .unwrap();
        assert!(text.contains("push\trbx"), "{text}");
        assert_eq!(
            text.contains("sub\trsp, 8"),
            goal == CodegenGoal::Speed,
            "{text}"
        );
        let program = keel_codegen::compile_program_with_pipeline_and_emission(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline,
            options(goal),
        )
        .unwrap();
        let bytes = keel_asm::x86::assemble_program_object(&program).unwrap();
        assert_eq!(bytes, keel_asm::x86::assemble_object(&text).unwrap());
        if goal == CodegenGoal::Size
            && let Some(dir) = std::env::var_os("KEEL_SIZE_EXPORT_DIR")
        {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(std::path::Path::new(&dir).join("padding.o"), &bytes).unwrap();
        }
    }
}
