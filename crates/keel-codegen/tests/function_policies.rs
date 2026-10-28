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
        let text = keel_codegen::compile_with_pipeline_and_emission(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline,
            options,
        )
        .unwrap();
        assert!(text.contains(expected_align), "{attributes}: {text}");
        let program = keel_codegen::compile_program_with_pipeline_and_emission(
            &module,
            Target::X86_64LinuxGnu,
            &pipeline,
            options,
        )
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
