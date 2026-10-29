//! Driver flags independently control compiler-created unwind metadata.
use object::{Object, ObjectSection};
use std::{fs, path::Path, process::Command};

fn compile(dir: &Path, target: &str, flags: &[&str]) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(dir)
        .args([
            "cc",
            "input.c",
            "--target",
            target,
            "--emit-object",
            "-o",
            "output.o",
        ])
        .args(flags)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{target}/{flags:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::read(dir.join("output.o")).unwrap()
}

#[test]
fn flags_are_independent_ordered_and_preserve_debug_code_and_kernel_model() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("unwind-flags");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.c"), "extern long helper(long); long value; long function(long x) { return helper(x) + x + value; }\n").unwrap();
    for target in ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"] {
        let baseline = compile(&dir, target, &[]);
        let base = object::File::parse(baseline.as_slice()).unwrap();
        assert!(base.section_by_name(".eh_frame").is_some());
        for (flags, frames) in [
            (vec!["-fno-asynchronous-unwind-tables"], false),
            (vec!["-fno-unwind-tables"], true),
            (
                vec!["-fno-asynchronous-unwind-tables", "-fno-unwind-tables"],
                false,
            ),
            (
                vec!["-fno-asynchronous-unwind-tables", "-funwind-tables"],
                true,
            ),
            (
                vec!["-funwind-tables", "-fno-asynchronous-unwind-tables"],
                true,
            ),
            (
                vec![
                    "-funwind-tables",
                    "-fno-asynchronous-unwind-tables",
                    "-fno-unwind-tables",
                ],
                false,
            ),
            (
                vec![
                    "-fno-asynchronous-unwind-tables",
                    "-fasynchronous-unwind-tables",
                ],
                true,
            ),
        ] {
            let bytes = compile(&dir, target, &flags);
            let file = object::File::parse(bytes.as_slice()).unwrap();
            assert_eq!(
                file.section_by_name(".eh_frame").is_some(),
                frames,
                "{target}/{flags:?}"
            );
            assert_eq!(
                file.section_by_name(".text").unwrap().data().unwrap(),
                base.section_by_name(".text").unwrap().data().unwrap()
            );
        }
        let bytes = compile(&dir, target, &["-g", "-fno-asynchronous-unwind-tables"]);
        let file = object::File::parse(bytes.as_slice()).unwrap();
        assert!(file.section_by_name(".eh_frame").is_none());
        assert!(file.section_by_name(".debug_info").is_some());
        assert!(file.section_by_name(".debug_line").is_some());
    }
    let bytes = compile(
        &dir,
        "x86_64-unknown-linux-gnu",
        &[
            "-Os",
            "--code-model=kernel",
            "-fno-asynchronous-unwind-tables",
        ],
    );
    let file = object::File::parse(bytes.as_slice()).unwrap();
    assert!(file.section_by_name(".eh_frame").is_none());
    assert!(
        file.section_by_name(".text")
            .unwrap()
            .relocations()
            .any(|(_, r)| r.flags() == object::RelocationFlags::Elf { r_type: 11 })
    );
    let output = Command::new(env!("CARGO_BIN_EXE_keel"))
        .current_dir(&dir)
        .args([
            "cc",
            "input.c",
            "--target",
            "wasm32-unknown-unknown",
            "--emit-wasm",
            "-fno-asynchronous-unwind-tables",
            "-o",
            "out.wasm",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported with --emit-wasm"));
    fs::remove_dir_all(dir).unwrap();
}
