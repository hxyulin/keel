//! GNU aliases in 64-bit mode. Word spellings keep their operand-size prefix.
use object::{Object, ObjectSection};
use std::{fs, process::Command};

fn text(bytes: &[u8]) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn flags_aliases_match_literal_bytes_clang_and_gnu() {
    let source = ".att_syntax prefix\n.text\npushf\npopf\npushfq\npopfq\npushfw\npopfw\n";
    let own = keel_asm::x86::assemble_object(source).unwrap();
    assert_eq!(text(&own), [0x9c, 0x9d, 0x9c, 0x9d, 0x66, 0x9c, 0x66, 0x9d]);
    let dir = std::env::temp_dir().join(format!("keel-flags-aliases-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("test.s");
    fs::write(&input, source).unwrap();
    for gnu in [false, true] {
        let mut command = if gnu {
            let mut c = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            c.arg("--64");
            c
        } else {
            let mut c = Command::new("clang");
            c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            c
        };
        let output = dir.join("test.o");
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(text(&own), text(&fs::read(output).unwrap()));
    }
    for source in ["pushfl", "popfl", "pushf %rax", "popf %rax"] {
        assert!(
            keel_asm::x86::assemble_object(&format!(".att_syntax prefix\n{source}")).is_err(),
            "{source}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
