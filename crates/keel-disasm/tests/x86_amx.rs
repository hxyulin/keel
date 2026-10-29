//! Encoding and decoding only. AMX state is never enabled or executed here.
use keel_asm_ir::x86::{Features, Instruction, Operand, Register, RegisterClass, Syntax};
use keel_disasm::x86::decode;
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
fn assemble(source: &str, syntax: Syntax, features: Features) -> Vec<u8> {
    text(
        &keel_asm::x86::assemble_object_with_syntax_and_features(source, syntax, features)
            .unwrap_or_else(|e| panic!("{source}: {e}")),
    )
}
fn features() -> Features {
    Features::parse("amx_int8,amx_bf16,amx_fp16").unwrap()
}

fn att(source: &str) -> String {
    let instruction = Instruction::parse_intel(source).unwrap();
    let operands = instruction
        .operands
        .iter()
        .rev()
        .map(|operand| match operand {
            Operand::Register(r) => format!("%{r}"),
            Operand::Memory(m) => {
                let base = m.base.map_or(String::new(), |r| format!("%{r}"));
                let index = m
                    .index
                    .map_or(String::new(), |r| format!(",%{r},{}", m.scale));
                format!("{}({base}{index})", m.displacement.expression.constant)
            }
            _ => unreachable!("AMX fixture contains only register and memory operands"),
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{} {operands}", instruction.opcode)
}

#[test]
fn amx_literals_tile_registers_and_reserved_encodings() {
    for (source, expected) in [
        ("ldtilecfg [rax]", &[0xc4, 0xe2, 0x78, 0x49, 0x00][..]),
        ("tilezero tmm7", &[0xc4, 0xe2, 0x7b, 0x49, 0xf8]),
        ("tilerelease", &[0xc4, 0xe2, 0x78, 0x49, 0xc0]),
        (
            "tileloadd tmm0,[rax]",
            &[0xc4, 0xe2, 0x7b, 0x4b, 0x04, 0x20],
        ),
        ("tdpbssd tmm0,tmm1,tmm2", &[0xc4, 0xe2, 0x6b, 0x5e, 0xc1]),
    ] {
        let bytes = assemble(source, Syntax::Intel, features());
        assert_eq!(bytes, expected, "{source}");
        let decoded = decode(&bytes, 0).unwrap();
        assert_eq!(decoded.bytes, bytes);
        assert_eq!(
            assemble(&decoded.assembly(), Syntax::Intel, features()),
            bytes
        );
    }
    for n in [0, 7] {
        let r = Register::parse(&format!("tmm{n}")).unwrap();
        assert_eq!(r.class, RegisterClass::Tile);
        assert!(r.is_valid());
        assert_eq!(r.to_string(), format!("tmm{n}"));
    }
    assert!(Register::parse("tmm8").is_none());
    // R/B/vvvv must not turn three-bit tile numbers into extended registers.
    for bytes in [
        &[0xc4, 0x62, 0x7b, 0x49, 0xc0][..],
        &[0xc4, 0xc2, 0x6b, 0x5e, 0xc1],
        &[0xc4, 0xe2, 0x2b, 0x5e, 0xc1],
        &[0xc4, 0xe2, 0x7f, 0x49, 0xc0], // reserved L=1
        &[0xc4, 0xe2, 0xfb, 0x49, 0xc0], // reserved W=1
        &[0xc4, 0xe2, 0x7b, 0x4b, 0x00], // tile load requires SIB
    ] {
        assert!(decode(bytes, 0).is_none(), "{bytes:02x?}");
    }
}

#[test]
fn amx_features_and_operand_shapes_fail_closed() {
    for (source, required) in [
        ("ldtilecfg [rax]", "amx_tile"),
        ("sttilecfg [rax]", "amx_tile"),
        ("tilezero tmm0", "amx_tile"),
        ("tileloadd tmm0,[rax]", "amx_tile"),
        ("tdpbssd tmm0,tmm1,tmm2", "amx_int8"),
        ("tdpbf16ps tmm0,tmm1,tmm2", "amx_bf16"),
        ("tdpfp16ps tmm0,tmm1,tmm2", "amx_fp16"),
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(source, Features::NONE).is_err(),
            "{source}"
        );
        assemble(source, Syntax::Intel, Features::parse(required).unwrap());
        if required != "amx_tile" {
            assert!(
                keel_asm::x86::assemble_object_with_features(
                    source,
                    Features::parse("amx_tile").unwrap()
                )
                .is_err()
            );
        }
    }
    for source in [
        "tilezero xmm0",
        "tilezero tmm8",
        "tilerelease tmm0",
        "tilezero tmm0,tmm1",
        "ldtilecfg rax",
        "ldtilecfg qword ptr [rax]",
        "tdpbssd tmm0,tmm1,xmm2",
        "tileloadd tmm0,[rip+8]",
        "lock tilezero tmm0",
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(source, features()).is_err(),
            "{source}"
        );
    }
    assert!(Features::parse("amx_tile,-xsave").is_err());
    assert!(Features::parse("amx_int8,-amx_tile").is_err());
    assert!(!Features::parse("amx_tile").unwrap().avx);
}

#[test]
fn amx_matches_clang_gnu_and_decode_reassembly_in_both_dialects() {
    let mut cases = vec!["tilerelease".to_owned()];
    for t in [0, 7] {
        cases.push(format!("tilezero tmm{t}"));
        for address in [
            "[rax]",
            "[r13]",
            "[r8+r9]",
            "[r13+rcx*2+127]",
            "[r13+rcx*4+128]",
            "[r12+r15*8-129]",
            "[eax+ecx+64]",
        ] {
            for op in ["tileloadd", "tileloaddt1"] {
                cases.push(format!("{op} tmm{t},{address}"));
            }
            cases.push(format!("tilestored {address},tmm{t}"));
        }
    }
    for address in [
        "[rax]",
        "[r13+64]",
        "[r12+r14*8-129]",
        "[rip+32]",
        "[eax+ecx+64]",
    ] {
        cases.push(format!("ldtilecfg {address}"));
        cases.push(format!("sttilecfg {address}"));
    }
    for op in [
        "tdpbssd",
        "tdpbsud",
        "tdpbusd",
        "tdpbuud",
        "tdpbf16ps",
        "tdpfp16ps",
    ] {
        for (d, a, b) in [(0, 1, 7), (7, 0, 6)] {
            cases.push(format!("{op} tmm{d},tmm{a},tmm{b}"));
        }
    }
    let source = cases.join("\n");
    let bytes = assemble(&source, Syntax::Intel, features());
    let formatted = cases
        .iter()
        .map(|case| Instruction::parse_intel(case).unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(assemble(&formatted, Syntax::Intel, features()), bytes);
    let att = cases
        .iter()
        .map(|case| att(case))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(assemble(&att, Syntax::Att, features()), bytes);
    let directory = std::env::temp_dir().join(format!("keel-amx-oracle-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    for syntax in [Syntax::Intel, Syntax::Att] {
        fs::write(
            directory.join("input.s"),
            match syntax {
                Syntax::Intel => format!(".intel_syntax noprefix\n.text\n{source}\n"),
                Syntax::Att => format!(".text\n{att}\n"),
            },
        )
        .unwrap();
        for gnu in [false, true] {
            let mut command = if gnu {
                let mut c =
                    Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
                c.arg("--64");
                c
            } else {
                let mut c = Command::new("clang");
                c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
                c
            };
            let output = command
                .arg(directory.join("input.s"))
                .arg("-o")
                .arg(directory.join("oracle.o"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "GNU={gnu}, {syntax:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                text(&fs::read(directory.join("oracle.o")).unwrap()),
                bytes,
                "GNU={gnu}, {syntax:?}"
            );
        }
    }
    let mut offset = 0;
    while offset < bytes.len() {
        let i = decode(&bytes[offset..], offset as u64)
            .unwrap_or_else(|| panic!("offset {offset}: {:02x?}", &bytes[offset..]));
        assert_eq!(
            assemble(&i.assembly(), Syntax::Intel, features()),
            i.bytes,
            "{}",
            i.assembly()
        );
        offset += i.bytes.len();
    }
    fs::remove_dir_all(directory).unwrap();
}
