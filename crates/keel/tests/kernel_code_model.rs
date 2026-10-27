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
        for model in ["pic", "kernel"] {
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
                if model == "kernel" { 2 } else { 0 }
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
