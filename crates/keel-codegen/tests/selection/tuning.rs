use keel_codegen::regalloc::AllocationTier;
use keel_codegen::tuning::{TunedSelector, TuningCpu};
use keel_codegen::{CodegenPipeline, SelectionOptions, Target, compile_with_pipeline};

fn compile(source: &str, cpu: TuningCpu, options: SelectionOptions) -> String {
    let module = keel_ir::parse(source).unwrap();
    keel_ir::verify_module(&module).unwrap();
    compile_with_pipeline(
        &module,
        Target::Aarch64AppleDarwin,
        &CodegenPipeline {
            selector: &TunedSelector { cpu, options },
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap()
}

fn multiply(ty: &str, constant: i64, commuted: bool) -> String {
    let args = if commuted { "v1, v0" } else { "v0, v1" };
    let abi_ty = if matches!(ty, "i8" | "i16") {
        format!("{ty} uext")
    } else {
        ty.to_string()
    };
    format!(
        "func @f({abi_ty}) -> {abi_ty} {{\nblock0(v0: {ty}):\n v1 = iconst.{ty} {constant}\n v2 = imul {args}\n return v2\n}}"
    )
}

#[test]
fn only_measured_widths_constants_and_enabled_selection_use_m5_patterns() {
    for ty in ["i8", "i16", "i32", "i64", "i128"] {
        for constant in [2, 3, 5, 7, 9, 17, -3] {
            for commuted in [false, true] {
                let source = multiply(ty, constant, commuted);
                let generic = compile(&source, TuningCpu::Generic, SelectionOptions::ALL);
                let m5 = compile(&source, TuningCpu::AppleM5, SelectionOptions::ALL);
                if matches!(ty, "i32" | "i64") && [3, 5, 9].contains(&constant) {
                    assert!(m5.contains("lsl #"), "{m5}");
                    assert!(!m5.contains("\tmul\t"), "{m5}");
                    assert!(generic.contains("\tmul\t"), "{generic}");
                } else {
                    assert_eq!(m5, generic);
                }
                assert_eq!(
                    compile(&source, TuningCpu::AppleM5, SelectionOptions::default()),
                    compile(&source, TuningCpu::Generic, SelectionOptions::default())
                );
            }
        }
    }
}

#[test]
fn generic_tuning_preserves_existing_selector_and_m5_rejects_x86() {
    let module = keel_ir::parse(&multiply("i64", 5, false)).unwrap();
    let output = compile(
        &multiply("i64", 5, false),
        TuningCpu::Generic,
        SelectionOptions::ALL,
    );
    assert_eq!(
        output,
        keel_codegen::compile_with_options(
            &module,
            Target::Aarch64AppleDarwin,
            SelectionOptions::ALL
        )
        .unwrap()
    );
    let error = compile_with_pipeline(
        &module,
        Target::X86_64LinuxGnu,
        &CodegenPipeline {
            selector: &TunedSelector {
                cpu: TuningCpu::AppleM5,
                options: SelectionOptions::ALL,
            },
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("apple-m5 tuning requires"));
    let error = keel_codegen::x86_64::compile_with_pipeline(
        &module,
        &keel_codegen::x86_64::REGISTERS,
        &CodegenPipeline {
            selector: &TunedSelector {
                cpu: TuningCpu::AppleM5,
                options: SelectionOptions::ALL,
            },
            allocator: &AllocationTier::Full,
        },
    )
    .unwrap_err();
    assert!(error.message.contains("apple-m5 tuning requires"));
}

#[test]
fn shared_constants_survive_and_existing_multiply_add_fusion_is_preserved() {
    let shared = multiply("i64", 5, false).replace("return v2", "v3 = xor v2, v1\n return v3");
    let output = compile(&shared, TuningCpu::AppleM5, SelectionOptions::ALL);
    assert!(output.contains("#0x5"), "{output}");
    assert!(output.contains("lsl #2"), "{output}");
    let fused = multiply("i64", 5, false).replace("return v2", "v3 = iadd v2, v0\n return v3");
    let output = compile(&fused, TuningCpu::AppleM5, SelectionOptions::ALL);
    assert!(output.contains("\tmadd\t"), "{output}");
}
