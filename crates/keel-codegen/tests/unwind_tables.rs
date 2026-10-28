//! Unwind emission changes metadata, not instruction bytes or user assembly.
use keel_codegen::{
    CodegenPipeline, EmissionOptions, SelectionTier, Target, UnwindTables, regalloc::AllocationTier,
};
use object::{Object, ObjectSection};

const SOURCE: &str = "decl @external(i64) -> i64\nfunc @leaf(i64) -> i64 cc(fast) local {\nblock0(v0:i64):\n v1 = iconst.i64 3\n v2 = iadd v0, v1\n return v2\n}\nfunc @chain(i64) -> i64 cc(fast) local {\nblock0(v0:i64):\n v1 = call @leaf(v0)\n return v1\n}\nfunc @root(i64) -> i64 {\nblock0(v0:i64):\n v1 = call @chain(v0)\n v2 = call @external(v1)\n v3 = iadd v0, v2\n return v3\n}\n";

fn options(policy: UnwindTables) -> EmissionOptions {
    let mut result = EmissionOptions::default();
    result.unwind_tables = policy;
    result
}

fn assemble(program: &keel_asm_ir::Program, target: Target) -> Vec<u8> {
    match target {
        Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(program).unwrap(),
        Target::Aarch64LinuxGnu => {
            keel_asm::assemble_elf_program_object(program, Default::default()).unwrap()
        }
        Target::Aarch64AppleDarwin => {
            keel_asm::assemble_program_object(program, Default::default()).unwrap()
        }
        _ => unreachable!(),
    }
}

#[test]
fn text_typed_and_chunks_agree_and_only_optional_frame_metadata_disappears() {
    let module = keel_ir::parse(SOURCE).unwrap();
    for target in [
        Target::X86_64LinuxGnu,
        Target::Aarch64LinuxGnu,
        Target::Aarch64AppleDarwin,
    ] {
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
                let default =
                    keel_codegen::compile_with_pipeline(&module, target, &pipeline).unwrap();
                let explicit = keel_codegen::compile_with_pipeline_and_emission(
                    &module,
                    target,
                    &pipeline,
                    Default::default(),
                )
                .unwrap();
                assert_eq!(default, explicit);
                assert!(default.contains(".cfi_startproc"));
                let mut objects = Vec::new();
                for policy in [UnwindTables::All, UnwindTables::RequiredOnly] {
                    let text = keel_codegen::compile_with_pipeline_and_emission(
                        &module,
                        target,
                        &pipeline,
                        options(policy),
                    )
                    .unwrap();
                    assert_eq!(
                        text.contains(".cfi_"),
                        policy == UnwindTables::All,
                        "{target:?}/{selection:?}/{allocation:?}: {text}"
                    );
                    let program = keel_codegen::compile_program_with_pipeline_and_emission(
                        &module,
                        target,
                        &pipeline,
                        options(policy),
                    )
                    .unwrap();
                    let bytes = assemble(&program, target);
                    let source_bytes = match target {
                        Target::X86_64LinuxGnu => keel_asm::x86::assemble_object(&text).unwrap(),
                        Target::Aarch64LinuxGnu => keel_asm::assemble_elf_object(&text).unwrap(),
                        _ => keel_asm::assemble_object(&text).unwrap(),
                    };
                    assert_eq!(bytes, source_bytes, "{target:?}/{policy:?}");
                    let mut chunks = keel_asm_ir::Program::new();
                    keel_codegen::compile_program_chunks_with_pipeline_and_emission(
                        &module,
                        target,
                        &pipeline,
                        options(policy),
                        &mut |p| chunks.append(p),
                    )
                    .unwrap();
                    assert_eq!(bytes, assemble(&chunks, target));
                    let file = object::File::parse(bytes.as_slice()).unwrap();
                    let unwind_name = if target == Target::Aarch64AppleDarwin {
                        "__compact_unwind"
                    } else {
                        ".eh_frame"
                    };
                    assert_eq!(
                        file.section_by_name(unwind_name).is_some(),
                        policy == UnwindTables::All
                    );
                    objects.push(bytes);
                }
                let enabled = object::File::parse(objects[0].as_slice()).unwrap();
                let disabled = object::File::parse(objects[1].as_slice()).unwrap();
                let text_name = if target == Target::Aarch64AppleDarwin {
                    "__text"
                } else {
                    ".text"
                };
                assert_eq!(
                    enabled.section_by_name(text_name).unwrap().data().unwrap(),
                    disabled.section_by_name(text_name).unwrap().data().unwrap(),
                    "{target:?}/{selection:?}/{allocation:?}"
                );
            }
        }
    }
}

#[test]
fn explicit_module_assembly_cfi_survives() {
    let mut module = keel_ir::parse(SOURCE).unwrap();
    module.assembly.push(
        ".text\n.globl user_function\nuser_function:\n.cfi_startproc\nret\n.cfi_endproc\n".into(),
    );
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let program = keel_codegen::compile_program_with_pipeline_and_emission(
        &module,
        Target::X86_64LinuxGnu,
        &pipeline,
        options(UnwindTables::RequiredOnly),
    )
    .unwrap();
    assert!(!program.items.iter().any(|item| matches!(
        item,
        keel_asm_ir::Item::Directive(keel_asm_ir::Directive::Cfi(_))
    )));
    let bytes = assemble(&program, Target::X86_64LinuxGnu);
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert!(file.section_by_name(".eh_frame").is_some());
}

#[test]
fn required_exception_frames_are_retained() {
    let module = keel_ir::parse(include_str!("programs/exceptions.keel")).unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    let required = keel_codegen::compile_program_with_pipeline_and_emission(
        &module,
        Target::Aarch64AppleDarwin,
        &pipeline,
        options(UnwindTables::RequiredOnly),
    )
    .unwrap();
    let all =
        keel_codegen::compile_program_with_pipeline(&module, Target::Aarch64AppleDarwin, &pipeline)
            .unwrap();
    assert_eq!(
        assemble(&required, Target::Aarch64AppleDarwin),
        assemble(&all, Target::Aarch64AppleDarwin)
    );
    assert!(required.items.iter().any(|item| matches!(
        item,
        keel_asm_ir::Item::Directive(keel_asm_ir::Directive::Cfi(keel_asm_ir::Cfi::Personality(
            _
        )))
    )));

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        use std::{fs, process::Command};
        let dir =
            std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("required-unwind-exceptions");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("exceptions.o"),
            assemble(&required, Target::Aarch64AppleDarwin),
        )
        .unwrap();
        let result = Command::new("/usr/bin/clang++")
            .args(["-std=c++17", "exceptions.o"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/programs/exceptions.cpp"
            ))
            .args(["-o", "exceptions"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            Command::new(dir.join("exceptions"))
                .status()
                .unwrap()
                .success()
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
