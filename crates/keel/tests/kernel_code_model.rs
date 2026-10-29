#[path = "kernel_code_model/unwind_flags.rs"]
mod unwind_flags;

use object::{Object, ObjectSection};
use std::{fs, process::Command};

#[test]
fn c_and_ir_cli_choose_kernel_addresses_and_reject_incompatible_targets() {
    let dir = std::env::temp_dir().join(format!("keel-kernel-model-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("address.c"), "extern int external; int defined; void *address(void){return &external;} void *local(void){return &defined;}").unwrap();
    for command in ["cc", "compile"] {
        let input = if command == "cc" {
            "address.c"
        } else {
            "address.keel"
        };
        if command == "compile" {
            let ir = Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args([
                    "cc",
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--emit-ir",
                    "address.c",
                ])
                .output()
                .unwrap();
            assert!(
                ir.status.success(),
                "{}",
                String::from_utf8_lossy(&ir.stderr)
            );
            fs::write(dir.join(input), ir.stdout).unwrap();
        }
        for model in ["pic", "small", "kernel"] {
            let output = format!("{command}-{model}.o");
            let result = Command::new(env!("CARGO_BIN_EXE_keel"))
                .current_dir(&dir)
                .args([
                    command,
                    "--target",
                    "x86_64-unknown-linux-gnu",
                    "--code-model",
                    model,
                    "--emit-object",
                    input,
                    "-o",
                    &output,
                ])
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let bytes = fs::read(dir.join(output)).unwrap();
            let file = object::File::parse(&*bytes).unwrap();
            let kinds: Vec<_> = file
                .section_by_name(".text")
                .unwrap()
                .relocations()
                .map(|(_, r)| r.kind())
                .collect();
            assert_eq!(
                kinds
                    .iter()
                    .filter(|&&kind| kind == object::RelocationKind::Absolute)
                    .count(),
                if model == "pic" { 0 } else { 2 }
            );
        }
    }
    for flags in [
        vec![
            "--target",
            "aarch64-unknown-linux-gnu",
            "--code-model",
            "kernel",
        ],
        vec![
            "--target",
            "x86_64-unknown-linux-gnu",
            "--code-model",
            "kernel",
            "--link-intent",
            "shared",
        ],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .arg("cc")
            .args(flags)
            .arg("address.c")
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("code model"));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn static_code_models_link_only_when_absolute_address_relocations_fit() {
    use keel_codegen::{
        CodegenPipeline, SelectionTier,
        regalloc::AllocationTier,
        x86_64::{self, CodeModel},
    };
    use keel_link::{Input, elf};
    let module = keel_ir::parse("data @value = align 8 { i64 7 }\nfunc @_start() -> ptr {\nblock0:\n v0 = symbol_addr @value\n return v0\n}\n").unwrap();
    let pipeline = CodegenPipeline {
        selector: &SelectionTier::Full,
        allocator: &AllocationTier::Full,
    };
    for (model, relocation) in [(CodeModel::Small, 10), (CodeModel::Kernel, 11)] {
        let source = x86_64::compile_with_pipeline_and_code_model(
            &module,
            &x86_64::REGISTERS,
            &pipeline,
            model,
        )
        .unwrap();
        let bytes = keel_asm::x86::assemble_object(&source).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        assert!(
            file.section_by_name(".text")
                .unwrap()
                .relocations()
                .any(|(_, r)| r.flags() == object::RelocationFlags::Elf { r_type: relocation }),
            "{source}"
        );
        for address in [0x1000000u64, 0xffffffff81000000, 0x100000000] {
            let script = format!(
                "ENTRY(physical_entry) PHDRS {{ text PT_LOAD FLAGS(7); }} SECTIONS {{ . = {address}; .text : AT(0x1000000) {{ *(.text) }} :text physical_entry = 0x1000000; .data : AT(ADDR(.data) - {address} + 0x1000000) {{ *(.data) *(.bss) }} :text /DISCARD/ : {{ *(.eh_frame) }} }}"
            );
            let result = elf::link(
                &[Input {
                    name: "model.o",
                    bytes: &bytes,
                }],
                elf::Options {
                    script: Some(script),
                    ..Default::default()
                },
            );
            let fits = match model {
                CodeModel::Small => address < (1u64 << 32),
                CodeModel::Kernel => {
                    address <= i32::MAX as u64 || address >= i32::MIN as i64 as u64
                }
                _ => unreachable!(),
            };
            if fits {
                result.unwrap();
            } else {
                assert!(
                    result.unwrap_err().contains("overflow"),
                    "{model:?}/{address:#x}"
                );
            }
        }
    }
}
