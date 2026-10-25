//! Encoding witnesses only; no descriptor queries execute on the host.
use keel_asm_ir::x86::{Features, Syntax};
use object::{Object, ObjectSection};
use std::{fs, process::Command};

fn text(object: &[u8]) -> Vec<u8> {
    object::File::parse(object)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

#[test]
fn selector_register_views_match_literal_bytes_and_external_assemblers() {
    let cases = [
        (
            "lar r10w, r11w",
            "lar %r11w, %r10w",
            vec![0x66, 0x45, 0x0f, 0x02, 0xd3],
        ),
        (
            "lar r10d, r11d",
            "lar %r11d, %r10d",
            vec![0x45, 0x0f, 0x02, 0xd3],
        ),
        (
            "lar r10, r11",
            "lar %r11, %r10",
            vec![0x4d, 0x0f, 0x02, 0xd3],
        ),
        (
            "lsl r10w, r11w",
            "lsl %r11w, %r10w",
            vec![0x66, 0x45, 0x0f, 0x03, 0xd3],
        ),
        (
            "lsl r10d, r11d",
            "lsl %r11d, %r10d",
            vec![0x45, 0x0f, 0x03, 0xd3],
        ),
        (
            "lsl r10, r11",
            "lsl %r11, %r10",
            vec![0x4d, 0x0f, 0x03, 0xd3],
        ),
    ];
    let dir = std::env::temp_dir().join(format!("keel-selector-queries-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    for syntax in [Syntax::Intel, Syntax::Att] {
        let mut source = match syntax {
            Syntax::Intel => ".intel_syntax noprefix\n.text\n".to_owned(),
            Syntax::Att => ".att_syntax\n.text\n".to_owned(),
        };
        let mut gnu_source = source.clone();
        let mut expected = Vec::new();
        for (intel, att, bytes) in &cases {
            let instruction = match syntax {
                Syntax::Intel => intel,
                Syntax::Att => att,
            };
            let own = keel_asm::x86::assemble_object_with_syntax_and_features(
                instruction,
                syntax,
                Features::NONE,
            )
            .unwrap();
            assert_eq!(text(&own), *bytes, "{instruction}");
            // GNU as 2.46 omits REX.W for these 64-bit register forms,
            // in both dialects. Request it explicitly in the GNU witness;
            // Clang and Keel must produce it from the register spelling.
            if bytes[0] == 0x4d {
                gnu_source.push_str("rex.W ");
            }
            gnu_source.push_str(instruction);
            gnu_source.push('\n');
            source.push_str(instruction);
            source.push('\n');
            expected.extend_from_slice(bytes);
        }
        let input = dir.join("test.s");
        for gnu in [false, true] {
            fs::write(&input, if gnu { &gnu_source } else { &source }).unwrap();
            let mut command = if gnu {
                let mut cmd =
                    Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
                cmd.arg("--64");
                cmd
            } else {
                let mut cmd = Command::new("clang");
                cmd.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
                cmd
            };
            let output = dir.join("test.o");
            let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(text(&fs::read(output).unwrap()), expected);
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
