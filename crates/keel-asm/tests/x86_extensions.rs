//! Encoding and feature contracts only; these tests do not execute extensions.
#[path = "x86_extensions/x86_pclmul.rs"]
mod x86_pclmul;
use keel_asm::x86::{assemble_object_with_features, assemble_object_with_syntax_and_features};
use keel_asm_ir::x86::{CpuLevel, ExtensionOpcode, Features, Instruction, Syntax};
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
fn assemble(source: &str, features: Features) -> Vec<u8> {
    text(
        &assemble_object_with_features(source, features)
            .unwrap_or_else(|e| panic!("{source}: {e}")),
    )
}

#[test]
fn literal_extension_encodings_and_att_scalar_suffixes() {
    for (source, expected) in [
        ("adcx rax,rcx", &[0x66, 0x48, 0x0f, 0x38, 0xf6, 0xc1][..]),
        ("adox rax,rcx", &[0xf3, 0x48, 0x0f, 0x38, 0xf6, 0xc1]),
        ("sha1msg1 xmm0,xmm1", &[0x0f, 0x38, 0xc9, 0xc1]),
        ("vaesenc xmm0,xmm1,xmm2", &[0xc4, 0xe2, 0x71, 0xdc, 0xc2]),
        ("vaesenc ymm0,ymm1,ymm2", &[0xc4, 0xe2, 0x75, 0xdc, 0xc2]),
        (
            "vpclmulqdq xmm0,xmm1,xmm2,17",
            &[0xc4, 0xe3, 0x71, 0x44, 0xc2, 17],
        ),
    ] {
        assert_eq!(
            assemble(source, Features::all_known()),
            expected,
            "{source}"
        );
    }
    for (att, intel) in [
        ("adcxq %rcx,%rax", "adcx rax,rcx"),
        ("adoxl 8(%r12),%r9d", "adox r9d,dword ptr [r12+8]"),
    ] {
        let bytes =
            assemble_object_with_syntax_and_features(att, Syntax::Att, Features::all_known())
                .unwrap();
        assert_eq!(text(&bytes), assemble(intel, Features::all_known()));
    }
}

#[test]
fn extensions_require_independent_features_and_operand_specific_vector_bits() {
    let v1 = Features::for_level(CpuLevel::V1);
    let v3 = Features::for_level(CpuLevel::V3);
    for source in [
        "adcx eax,ecx",
        "adox rax,rcx",
        "sha1msg1 xmm0,xmm1",
        "vaesenc ymm0,ymm1,ymm2",
        "vpclmulqdq ymm0,ymm1,ymm2,0",
    ] {
        assert!(
            assemble_object_with_features(source, v3).is_err(),
            "{source}"
        );
    }
    for (source, enabled, disabled) in [
        ("adcx eax,ecx", v1.with_features("adx").unwrap(), v1),
        (
            "sha256rnds2 xmm0,xmm1",
            v1.with_features("sha").unwrap(),
            v1,
        ),
        (
            "vaesenc xmm0,xmm1,xmm2",
            v3.with_features("aes").unwrap(),
            v3.with_features("vaes").unwrap(),
        ),
        (
            "vaesenc ymm0,ymm1,ymm2",
            v3.with_features("vaes").unwrap(),
            v3.with_features("aes").unwrap(),
        ),
        (
            "vpclmulqdq xmm0,xmm1,xmm2,0",
            v3.with_features("pclmulqdq").unwrap(),
            v3.with_features("vpclmulqdq").unwrap(),
        ),
        (
            "vpclmulqdq ymm0,ymm1,ymm2,0",
            v3.with_features("vpclmulqdq").unwrap(),
            v3.with_features("pclmulqdq").unwrap(),
        ),
    ] {
        assert!(
            assemble_object_with_features(source, enabled).is_ok(),
            "{source}"
        );
        assert!(
            assemble_object_with_features(source, disabled).is_err(),
            "{source}"
        );
    }
    assert!(v1.with_features("vaes").unwrap().avx);
    assert!(v1.with_features("vpclmulqdq").unwrap().avx);
}

#[test]
fn malformed_extension_operands_are_rejected() {
    for source in [
        "adcx ax,cx",
        "adcx rax,ecx",
        "adcx rax,1",
        "lock adcx rax,[rcx]",
        "sha1msg1 ymm0,ymm1",
        "sha1msg1 xmm0,qword ptr [rax]",
        "sha1rnds4 xmm0,xmm1,256",
        "sha1rnds4 xmm0,xmm1,-1",
        "vaesenc ymm0,xmm1,ymm2",
        "vaesimc ymm0,ymm1",
        "vpclmulqdq ymm0,ymm1,xmm2,0",
        "vpclmulqdq xmm0,xmm1,xmm2,256",
    ] {
        assert!(
            assemble_object_with_features(source, Features::all_known()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn exposed_forms_match_clang_and_gnu_and_format_roundtrip() {
    let mut cases = Vec::new();
    for op in ExtensionOpcode::ALL {
        // AMX and APX have separate encoding-oracle matrices.
        if op.required_features().amx_tile || op.required_features().apx_f {
            continue;
        }
        // Unary/system adapters have dedicated independent byte matrices in
        // x86_kernel_adapters.rs and x86_remaining_kernel_adapters.rs.
        if matches!(
            op,
            ExtensionOpcode::Rdtscp
                | ExtensionOpcode::Rdfsbase
                | ExtensionOpcode::Rdgsbase
                | ExtensionOpcode::Wrfsbase
                | ExtensionOpcode::Wrgsbase
                | ExtensionOpcode::Rdrand
                | ExtensionOpcode::Prefetchw
                | ExtensionOpcode::Invpcid
                | ExtensionOpcode::Tpause
                | ExtensionOpcode::Rdseed
        ) {
            continue;
        }
        let name = op.mnemonic();
        for n in [0, 7, 8, 15] {
            if op.scalar_suffix() {
                let index = [0, 7, 8, 15].iter().position(|r| *r == n).unwrap();
                for (dst, src, width) in [
                    (["eax", "edi", "r8d", "r15d"][index], "ecx", "dword"),
                    (["rax", "rdi", "r8", "r15"][index], "r15", "qword"),
                ] {
                    cases.push(format!("{name} {dst},{src}"));
                    for addr in [
                        "[rax]",
                        "[rbp]",
                        "[r13+r12*4-129]",
                        "[r12+127]",
                        "[r12+128]",
                        "fs:[r8-128]",
                        "[rip+32]",
                    ] {
                        cases.push(format!("{name} {dst},{width} ptr {addr}"));
                    }
                }
            } else {
                let widths = if matches!(
                    name,
                    "vaesenc" | "vaesenclast" | "vaesdec" | "vaesdeclast" | "vpclmulqdq"
                ) {
                    vec!["xmm", "ymm"]
                } else {
                    vec!["xmm"]
                };
                for class in widths {
                    let width = if class == "xmm" { "xmmword" } else { "ymmword" };
                    let sources = [
                        format!("{class}{}", 15 - n),
                        format!("{width} ptr [r13+r12*4-129]"),
                        format!("{width} ptr [rip+32]"),
                    ];
                    for src in sources {
                        let middle = if matches!(
                            name,
                            "vaesenc" | "vaesenclast" | "vaesdec" | "vaesdeclast" | "vpclmulqdq"
                        ) {
                            format!(",{class}8")
                        } else {
                            String::new()
                        };
                        let controls =
                            if matches!(name, "sha1rnds4" | "vaeskeygenassist" | "vpclmulqdq") {
                                vec![",0", ",17", ",255"]
                            } else {
                                vec![""]
                            };
                        for imm in controls {
                            cases.push(format!("{name} {class}{n}{middle},{src}{imm}"));
                        }
                    }
                }
            }
        }
    }
    let source = cases.join("\n");
    let expected = assemble(&source, Features::all_known());
    let formatted = cases
        .iter()
        .map(|s| Instruction::parse_intel(s).unwrap().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(assemble(&formatted, Features::all_known()), expected);
    let dir = std::env::temp_dir().join(format!("keel-extension-oracles-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    fs::write(&input, format!(".intel_syntax noprefix\n.text\n{source}\n")).unwrap();
    for (name, mut command) in [
        ("clang", {
            let mut c = Command::new("clang");
            c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            c
        }),
        ("gnu", {
            let mut c = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            c.arg("--64");
            c
        }),
    ] {
        let output = dir.join(format!("{name}.o"));
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            text(&fs::read(output).unwrap()),
            expected,
            "{name}, {} cases",
            cases.len()
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
