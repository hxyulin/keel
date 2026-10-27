use object::{Object, ObjectSection};
use std::{fs, process::Command};

#[test]
fn assembly_and_c_object_commands_resolve_binary_inputs_from_working_directory() {
    let dir = std::env::temp_dir().join(format!("keel-incbin-cli-{}", std::process::id()));
    fs::create_dir_all(dir.join("nested")).unwrap();
    fs::write(dir.join("payload.bin"), [0, 255, 1, 128, 0]).unwrap();
    fs::write(
        dir.join("nested/input.s"),
        ".section .blob,\"a\"\n.incbin \"payload.bin\"\n",
    )
    .unwrap();
    fs::write(dir.join("nested/input.c"), "asm(\".section .blob,\\\"a\\\"\\n.incbin \\\"payload.bin\\\"\\n.text\\n\"); int value(void) { return 7; }").unwrap();
    for (command, input, output, flags) in [
        ("asm", "nested/input.s", "asm.o", &[][..]),
        ("cc", "nested/input.c", "c.o", &["--emit-object"][..]),
    ] {
        let run = Command::new(env!("CARGO_BIN_EXE_keel"))
            .current_dir(&dir)
            .args([
                command,
                "--target",
                "x86_64-unknown-linux-gnu",
                input,
                "-o",
                output,
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let bytes = fs::read(dir.join(output)).unwrap();
        let object = object::File::parse(bytes.as_slice()).unwrap();
        assert_eq!(
            object.section_by_name(".blob").unwrap().data().unwrap(),
            [0, 255, 1, 128, 0]
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
