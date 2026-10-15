use keel_asm::x86::assemble_object;
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;
fn text(bytes: &[u8]) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn relocations(bytes: &[u8]) -> Vec<(u64, String, i64, String)> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(".text")
        .unwrap()
        .relocations()
        .map(|(offset, r)| {
            let object::RelocationTarget::Symbol(index) = r.target() else {
                panic!("symbol relocation expected")
            };
            (
                offset,
                format!("{:?}", r.flags()),
                r.addend(),
                file.symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_string(),
            )
        })
        .collect()
}
#[test]
fn packed_sse2_register_memory_matrix_matches_clang_and_gnu() {
    let mut source = String::from(".intel_syntax noprefix\n.text\n");
    for op in [
        "paddb", "paddw", "paddd", "paddq", "psubb", "psubw", "psubd", "psubq", "pmullw", "addps",
        "addpd", "subps", "subpd", "mulps", "mulpd", "divps", "divpd", "sqrtps", "sqrtpd",
    ] {
        for dst in 0..16 {
            for src in 0..16 {
                source += &format!("{op} xmm{dst}, xmm{src}\n");
            }
            for memory in [
                "[rax]",
                "[r13 + r12*4 - 128]",
                "[r12 + 127]",
                "[r15 + 128]",
                "[rip + external]",
                "fs:[r8 + 8]",
                "[rcx*8 + 4096]",
            ] {
                source += &format!("{op} xmm{dst}, xmmword ptr {memory}\n");
            }
        }
        for bad in [
            format!("{op} rax, rdx"),
            format!("{op} xmm0, qword ptr [rax]"),
            format!("{op} [rax], xmm0"),
            format!("{op} xmm0, xmm1, 0"),
        ] {
            assert!(assemble_object(&bad).is_err(), "{bad}");
        }
    }
    let dir = std::env::temp_dir().join(format!("keel-packed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("test.s");
    std::fs::write(&input, &source).unwrap();
    let expected = assemble_object(&source).unwrap();
    let output = dir.join("clang.o");
    let r = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-linux-gnu",
            "-c",
            "-x",
            "assembler",
        ])
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(text(&expected), text(&std::fs::read(&output).unwrap()));
    assert_eq!(
        relocations(&expected),
        relocations(&std::fs::read(&output).unwrap())
    );
    if cfg!(target_os = "linux") || std::env::var_os("KEEL_GNU_AS").is_some() {
        let assembler = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
        let r = Command::new(assembler)
            .arg("--64")
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(text(&expected), text(&std::fs::read(&output).unwrap()));
        assert_eq!(
            relocations(&expected),
            relocations(&std::fs::read(&output).unwrap())
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
