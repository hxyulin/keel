//! Function-local selection and layout override invocation defaults.
use keel_codegen::{
    CodegenGoal, CodegenPipeline, EmissionOptions, SelectionTier, Target, regalloc::AllocationTier,
};

#[test]
fn size_and_speed_presets_override_layout_and_keep_transport_parity() {
    for (attributes, default_goal, expected_align) in [
        ("optimize(\"Os\")", CodegenGoal::Speed, "\t.p2align\t0"),
        ("optimize(2)", CodegenGoal::Size, "\t.p2align\t4"),
        ("optnone", CodegenGoal::Size, "\t.p2align\t4"),
        ("cold", CodegenGoal::Speed, "\t.p2align\t0"),
    ] {
        let module = keel_ir::parse(&format!("func @f(i64) -> i64 {attributes} {{\nblock0(v0:i64):\n v1 = iconst.i64 3\n v2 = iadd v0, v1\n return v2\n}}\n")).unwrap();
        let pipeline = CodegenPipeline {
            selector: &SelectionTier::Full,
            allocator: &AllocationTier::Full,
        };
        let mut options = EmissionOptions::default();
        options.goal = default_goal;
        let text = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(options)
            .text(&module)
            .unwrap();
        assert!(text.contains(expected_align), "{attributes}: {text}");
        let program = keel_codegen::Request::new(Target::X86_64LinuxGnu, pipeline)
            .emission(options)
            .program(&module)
            .unwrap();
        let expected = keel_asm::x86::assemble_object(&text).unwrap();
        let actual = keel_asm::x86::assemble_program_object(&program).unwrap();
        assert_eq!(expected, actual, "{attributes}");
    }
}

#[test]
fn hot_and_cold_sections_preserve_explicit_sections_and_alignment() {
    for target in [Target::X86_64LinuxGnu, Target::Aarch64LinuxGnu] {
        let module = keel_ir::parse("func @c() cold {\nblock0:\n return\n}\nfunc @h() hot {\nblock0:\n return\n}\nfunc @s() cold section \".init.text\" align 64 {\nblock0:\n return\n}\n").unwrap();
        let text = keel_codegen::compile(&module, target).unwrap();
        assert!(
            text.contains(".text.unlikely")
                && text.contains(".text.hot")
                && text.contains(".init.text"),
            "{target:?}: {text}"
        );
        assert!(text.contains("p2align\t6"), "{text}");
    }
}

#[test]
fn unknown_optimization_options_reject_before_emission() {
    let module = keel_ir::parse(
        "func @f() optimize(\"no-imaginary-optimization\") {\nblock0:\n return\n}\n",
    )
    .unwrap();
    for target in Target::NATIVE {
        let error = keel_codegen::compile(&module, target)
            .unwrap_err()
            .to_string();
        assert!(error.contains("imaginary-optimization"), "{error}");
    }
}

#[test]
fn optional_selection_uses_function_presets() {
    use keel_codegen::{InstructionSelector, select};
    use std::cell::RefCell;
    struct Observed(RefCell<Vec<String>>);
    impl InstructionSelector for Observed {
        fn name(&self) -> &'static str {
            "observed"
        }
        fn select(
            &self,
            function: &keel_ir::Function,
            target: select::Target,
            folds: &dyn Fn(&keel_ir::InstData) -> Vec<keel_ir::Value>,
        ) -> select::Selection {
            self.0.borrow_mut().push(function.name.clone());
            SelectionTier::Full.select(function, target, folds)
        }
    }
    for target in Target::NATIVE {
        let module = keel_ir::parse("func @default(i64) -> i64 {\nblock0(v0:i64):\n return v0\n}\nfunc @off(i64) -> i64 optnone {\nblock0(v0:i64):\n return v0\n}\nfunc @size(i64) -> i64 optimize(\"Os\") {\nblock0(v0:i64):\n return v0\n}\n").unwrap();
        let selector = Observed(RefCell::new(Vec::new()));
        keel_codegen::compile_with_pipeline(
            &module,
            target,
            &CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Full,
            },
        )
        .unwrap();
        assert_eq!(*selector.0.borrow(), ["default"], "{target:?}");
    }
}

#[test]
fn elf_debug_ranges_cover_function_sections_and_hot_cold_sections() {
    use keel_ir::{EntityRef, SourceFile, SourceLoc};
    use object::{Object, ObjectSection};
    for target in [Target::Aarch64LinuxGnu, Target::X86_64LinuxGnu] {
        for (function_sections, second_attrs) in [(true, ""), (false, "cold")] {
            let mut module = keel_ir::parse(&format!(
                "func @a() {{\nblock0:\n return\n}}\nfunc @b() {second_attrs} {{\nblock0:\n return\n}}\n"
            )).unwrap();
            module.files.push("sections.c".into());
            for (index, function) in module.functions.iter_mut().enumerate() {
                let inst = function.block_insts(function.entry_block().unwrap())[0];
                function.set_inst_loc(
                    inst,
                    Some(SourceLoc {
                        file: SourceFile::new(0),
                        line: index as u32 + 1,
                        col: 1,
                        scope: None,
                    }),
                );
            }
            let mut options = EmissionOptions::default();
            options.function_sections = function_sections;
            let program = keel_codegen::Request::new(
                target,
                CodegenPipeline {
                    selector: &SelectionTier::Covers,
                    allocator: &AllocationTier::Full,
                },
            )
            .emission(options)
            .program(&module)
            .unwrap();
            let bytes = keel_asm::assemble_program_for(target, &program).unwrap();
            let object = object::File::parse(bytes.as_slice()).unwrap();
            let section = object
                .section_by_name(".debug_ranges")
                .expect("disjoint CU needs ranges");
            let ranges = section.data().unwrap();
            assert_eq!(ranges.len(), 80, "{target:?}, sections={function_sections}");
            assert_eq!(&ranges[..8], &[0xff; 8]);
            assert_eq!(&ranges[32..40], &[0xff; 8]);
            assert_eq!(&ranges[64..], &[0; 16]);
            assert_eq!(
                section.relocations().count(),
                2,
                "each function gets its own base address relocation"
            );
        }
    }
}
