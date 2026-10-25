//! Compile/encoding checks only. AVX-512 instructions never execute on the host.
use keel_asm_ir::x86::{Features, Instruction, Syntax};
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
fn own(source: &str, syntax: Syntax) -> Vec<u8> {
    text(
        &keel_asm::x86::assemble_object_with_syntax_and_features(
            source,
            syntax,
            Features::parse("avx512vl").unwrap(),
        )
        .unwrap_or_else(|e| panic!("{source}: {e}")),
    )
}
#[test]
fn evex_literals_prefix_bits_and_roundtrips() {
    for (source, expected) in [
        (
            "vaddps zmm0,zmm1,zmm2",
            &[0x62, 0xf1, 0x74, 0x48, 0x58, 0xc2][..],
        ),
        (
            "vaddps zmm0{k1}{z},zmm1,zmm2",
            &[0x62, 0xf1, 0x74, 0xc9, 0x58, 0xc2],
        ),
        (
            "vaddps zmm0{k1},zmm1,dword ptr [rax+508]{1to16}",
            &[0x62, 0xf1, 0x74, 0x59, 0x58, 0x40, 0x7f],
        ),
        (
            "vaddps zmm0,zmm1,zmmword ptr [rax+8128]",
            &[0x62, 0xf1, 0x74, 0x48, 0x58, 0x40, 0x7f],
        ),
    ] {
        let bytes = own(source, Syntax::Intel);
        assert_eq!(bytes, expected, "{source}");
        let instruction = decode(&bytes, 0).unwrap_or_else(|| panic!("{source}: {bytes:02x?}"));
        assert_eq!(instruction.bytes, bytes);
        assert_eq!(own(&instruction.assembly(), Syntax::Intel), bytes);
        assert_eq!(
            Instruction::parse_intel(source).unwrap(),
            Instruction::parse_intel(&Instruction::parse_intel(source).unwrap().to_string())
                .unwrap()
        );
    }
}
#[test]
fn evex_oracles_and_att_decorators() {
    let source = ".intel_syntax noprefix\n.text\nvaddps zmm31{k7}{z},zmm20,zmm29\nvaddps zmm0,zmm1,zmm2,{rd-sae}\nvmaxps zmm0,zmm1,zmm2,{sae}\nvaddps zmm0,zmm1,[r13+r14*4+8192]\nvaddps zmm0,zmm1,[r13+r14*4-8192]\nvgatherdps zmm17{k2},[r13+zmm23*4+508]\nvscatterdps [r13+zmm23*4+508]{k2},zmm17\nkmovw k3,r10d\n";
    let bytes = own(source, Syntax::Intel);
    let dir = std::env::temp_dir().join(format!("keel-evex-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    fs::write(&input, source).unwrap();
    for gnu in [false, true] {
        let mut cmd = if gnu {
            let mut c = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            c.arg("--64");
            c
        } else {
            let mut c = Command::new("clang");
            c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            c
        };
        let output = dir.join("oracle.o");
        let result = cmd.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(bytes, text(&fs::read(output).unwrap()), "GNU={gnu}");
    }
    let mut offset = 0;
    while offset < bytes.len() {
        let i = decode(&bytes[offset..], offset as u64)
            .unwrap_or_else(|| panic!("offset {offset}: {:02x?}", &bytes[offset..]));
        assert_eq!(
            own(&i.assembly(), Syntax::Intel),
            i.bytes,
            "{}",
            i.assembly()
        );
        offset += i.bytes.len();
    }
    assert_eq!(
        own("vaddps %zmm29, %zmm20, %zmm31{%k7}{z}", Syntax::Att),
        own("vaddps zmm31{k7}{z},zmm20,zmm29", Syntax::Intel)
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generated_binding_smoke_assembles_and_roundtrips() {
    let mut count = 0;
    let mut errors = Vec::new();
    for row in include_str!("generated/x86-evex-smoke.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        count += 1;
        let fields: Vec<_> = row.splitn(3, '|').collect();
        let source = fields[2];
        let object = match keel_asm::x86::assemble_object_with_features(
            source,
            Features::parse("avx512vl").unwrap(),
        ) {
            Ok(o) => o,
            Err(e) => {
                errors.push(format!("{source}: {e}"));
                continue;
            }
        };
        let bytes = text(&object);
        let Some(i) = decode(&bytes, 0) else {
            errors.push(format!("decode: {source}: {bytes:02x?}"));
            continue;
        };
        if i.bytes.len() != bytes.len() {
            errors.push(format!("length: {source}: {}", i.assembly()));
            continue;
        }
        let encoded = keel_asm::x86::assemble_object_with_features(
            &i.assembly(),
            Features::parse("avx512vl").unwrap(),
        );
        match encoded {
            Ok(o) if text(&o) == bytes => {}
            other => errors.push(format!("roundtrip: {source}: {}: {other:?}", i.assembly())),
        }
    }
    assert!(
        errors.is_empty(),
        "{} errors:\n{}",
        errors.len(),
        errors
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert_eq!(count, 2423);
}

fn oracle(source: &str, tag: &str, gnu: bool) -> Vec<u8> {
    let dir = std::env::temp_dir().join(format!("keel-evex-{tag}-{}-{gnu}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    fs::write(&input, source).unwrap();
    let mut command = if gnu {
        let mut c = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
        c.arg("--64");
        c
    } else {
        let mut c = Command::new("clang");
        c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        c
    };
    let output = dir.join("oracle.o");
    let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
    assert!(
        result.status.success(),
        "GNU={gnu}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let raw = fs::read(output).unwrap();
    fs::remove_dir_all(dir).unwrap();
    raw
}

#[test]
fn generated_sources_match_clang_and_gnu() {
    use object::ObjectSymbol;
    let fixtures = [
        (Syntax::Intel, include_str!("generated/x86-evex-smoke.txt")),
        (
            Syntax::Att,
            include_str!("generated/x86-evex-smoke-att.txt"),
        ),
    ];
    for (syntax, fixture) in fixtures {
        let mut cases: Vec<String> = fixture
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.splitn(3, '|').nth(2).unwrap().to_owned())
            .collect();
        if syntax == Syntax::Intel {
            cases.extend([
                "vaddps zmm17,zmm22,zmmword ptr fs:[r13d+r14d*4+8128]".to_owned(),
                "addr32 vgatherdps zmm17{k2},dword ptr [zmm23*4+508]".to_owned(),
                "vaddps zmm17,zmm22,zmmword ptr [eip+7]".to_owned(),
            ]);
            for index in 0..31 {
                cases.push(format!(
                    "vgatherdps zmm31{{k7}},dword ptr [r13+zmm{index}*4+508]"
                ));
            }
            for broadcast in [false, true] {
                for displacement in [
                    -8193, -8192, -8191, -513, -512, -1, 0, 1, 63, 64, 508, 512, 8128, 8191, 8192,
                ] {
                    let suffix = if broadcast { "{1to16}" } else { "" };
                    let size = if broadcast { "dword" } else { "zmmword" };
                    cases.push(format!(
                        "vaddps zmm17{{k2}},zmm22,{size} ptr [r13{displacement:+}]{suffix}"
                    ));
                }
            }
        }
        let expected: Vec<_> = cases.iter().map(|s| own(s, syntax)).collect();
        let mut source = if syntax == Syntax::Intel {
            ".intel_syntax noprefix\n.text\n"
        } else {
            ".att_syntax\n.text\n"
        }
        .to_owned();
        for (n, case) in cases.iter().enumerate() {
            source += &format!("w{n}: {case}\n");
        }
        for gnu in [false, true] {
            let raw = oracle(&source, &format!("generated-{syntax:?}"), gnu);
            let object = object::File::parse(raw.as_slice()).unwrap();
            let bytes = object.section_by_name(".text").unwrap().data().unwrap();
            let addresses: std::collections::HashMap<_, _> = object
                .symbols()
                .map(|s| (s.name().unwrap(), s.address() as usize))
                .collect();
            let mut errors = Vec::new();
            for (n, expected) in expected.iter().enumerate() {
                let start = addresses[format!("w{n}").as_str()];
                let end = if n + 1 == cases.len() {
                    bytes.len()
                } else {
                    addresses[format!("w{}", n + 1).as_str()]
                };
                if expected != &bytes[start..end] {
                    // VMOVQ has equivalent GPR-transfer and SIMD-transfer memory
                    // encodings. Compare the complete decoded operand description.
                    if cases[n].contains(" vmovq ") {
                        assert_eq!(
                            decode(expected, 0).unwrap().assembly(),
                            decode(&bytes[start..end], 0).unwrap().assembly(),
                            "VMOVQ alias"
                        );
                    } else {
                        errors.push(format!(
                            "{}: own={expected:02x?} ref={:02x?}",
                            cases[n],
                            &bytes[start..end]
                        ));
                    }
                }
            }
            assert!(
                errors.is_empty(),
                "{syntax:?}, GNU={gnu}: {} differences:\n{}",
                errors.len(),
                errors
                    .iter()
                    .take(50)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}

#[test]
fn features_invalid_decorators_and_reserved_prefixes() {
    use keel_asm_ir::x86::{CpuLevel, RegisterClass};
    let f = Features::parse("avx512f").unwrap();
    let vl = Features::parse("avx512vl").unwrap();
    assert!(f.avx && f.xsave && f.osxsave && !f.avx512vl);
    assert!(!Features::for_level(CpuLevel::V3).avx512f);
    for source in [
        "vaddps zmm0,zmm1,zmm2",
        "vaddss xmm17,xmm22,xmm29",
        "kmovw k1,r10d",
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(source, f).is_ok(),
            "{source}"
        );
        assert!(
            keel_asm::x86::assemble_object_with_features(source, Features::for_level(CpuLevel::V3))
                .is_err()
        );
    }
    assert!(
        keel_asm::x86::assemble_object_with_features("{evex} vaddps xmm0,xmm1,xmm2", f).is_err()
    );
    assert!(
        keel_asm::x86::assemble_object_with_features("{evex} vaddps xmm0,xmm1,xmm2", vl).is_ok()
    );
    for source in [
        "vaddps zmm0{k0},zmm1,zmm2",
        "vaddps zmm0{z},zmm1,zmm2",
        "vaddps zmm0{k1}{z}{z},zmm1,zmm2",
        "vaddps zmm0,zmm1{k1},zmm2",
        "vaddps zmm0,zmm1,zmm2{1to16}",
        "vaddps zmm0,zmm1,dword ptr [rax]{1to8}",
        "vaddps zmm0,zmm1,zmm2,{sae}",
        "vmovups zmmword ptr [rax]{k1}{z},zmm0",
        "vgatherdps zmm0,[rax+zmm1*4]",
        "vgatherdps zmm0{k1},[rax+zmm0*4]",
        "vaddps zmm32,zmm1,zmm2",
        "{evex} vaddps zmm0,zmm1,zmmword ptr [rip+x@gottpoff]",
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(source, vl).is_err(),
            "{source}"
        );
    }
    assert!(
        keel_asm::x86::assemble_object_with_features("{evex} vcvtpd2dq xmm17,[rax]", vl).is_err()
    );
    assert_ne!(
        own("{evex} vcvtpd2dq xmm17,xmmword ptr [rax]", Syntax::Intel),
        own("{evex} vcvtpd2dq xmm17,ymmword ptr [rax]", Syntax::Intel)
    );
    let good = own("vaddps zmm0,zmm1,zmm2", Syntax::Intel);
    for n in 0..good.len() {
        assert!(decode(&good[..n], 0).is_none());
    }
    let mut bad = good.clone();
    bad[1] |= 4;
    assert!(decode(&bad, 0).is_none());
    let mut bad = good.clone();
    bad[2] &= !4;
    assert!(decode(&bad, 0).is_none());
    let mut bad = good.clone();
    bad[3] |= 0x80;
    assert!(decode(&bad, 0).is_none());
    let mut bad = good.clone();
    bad[3] = (bad[3] & !0x60) | 0x60;
    assert!(decode(&bad, 0).is_none());
    for prefix in [0xf0, 0x66, 0xf2, 0xf3, 0x48] {
        let mut bad = vec![prefix];
        bad.extend(&good);
        assert!(decode(&bad, 0).is_none());
    }
    assert_eq!(RegisterClass::Zmm as u8, 9);
    assert_eq!(RegisterClass::Mask as u8, 10);
}

#[test]
fn address32_alias_folding_relocations_and_typed_transport() {
    use keel_asm_ir::Program;
    let source = "vaddps zmm31{k7}{z},zmm20,dword ptr [rax+508]{1to16}";
    let folded = own(
        ".set displacement,508\nvaddps zmm31{k7}{z},zmm20,dword ptr [rax+displacement]{1to16}",
        Syntax::Intel,
    );
    assert_eq!(folded, own(source, Syntax::Intel));
    let mut p = Program::new();
    p.x86_instruction(Instruction::parse_intel(source).unwrap());
    let features = Features::parse("avx512vl").unwrap();
    let mut plain = Instruction::parse_intel("vaddps zmm31,zmm20,zmm29").unwrap();
    plain.opcode = keel_asm_ir::x86::Opcode::Vaddps;
    assert!(plain.required_features().avx512f);
    let mut natural = Program::new();
    natural.x86_instruction(plain);
    assert_eq!(
        text(&keel_asm::x86::assemble_program_object_with_features(&natural, features).unwrap()),
        own("vaddps zmm31,zmm20,zmm29", Syntax::Intel)
    );
    assert_eq!(
        text(&keel_asm::x86::assemble_program_object_with_features(&p, features).unwrap()),
        folded
    );
    let mut chunks = keel_asm::x86::ProgramAssembler::new(features);
    chunks.push(p.clone());
    assert_eq!(text(&chunks.finish().unwrap()), folded);
    for source in [
        "vaddps zmm17,zmm22,zmmword ptr fs:[r13d+r14d*4+8128]",
        "addr32 vgatherdps zmm17{k2},[zmm23*4+508]",
        "addr32 vaddps zmm17,zmm22,zmmword ptr [rip+2147483648]",
    ] {
        let bytes = own(source, Syntax::Intel);
        assert!(bytes[0..bytes.len().min(2)].contains(&0x67));
        let i = decode(&bytes, 0).unwrap();
        assert_eq!(own(&i.assembly(), Syntax::Intel), bytes, "{}", i.assembly());
    }
}

#[test]
fn evex_pc_relative_relocation_addends_include_trailing_immediates() {
    use object::ObjectSymbol;
    let source = ".intel_syntax noprefix\n.text\nvaddps zmm17{k2},zmm22,zmmword ptr [rip+external+7]\nvcmpps k3,zmm22,zmmword ptr [rip+external+7],3\n";
    let features = Features::parse("avx512f").unwrap();
    let own = keel_asm::x86::assemble_object_with_features(source, features).unwrap();
    let relocations = |bytes: &[u8]| {
        let o = object::File::parse(bytes).unwrap();
        o.section_by_name(".text")
            .unwrap()
            .relocations()
            .map(|(offset, r)| {
                let name = match r.target() {
                    object::RelocationTarget::Symbol(index) => {
                        o.symbol_by_index(index).unwrap().name().unwrap().to_owned()
                    }
                    other => panic!("{other:?}"),
                };
                (
                    offset,
                    r.size(),
                    r.addend(),
                    name,
                    format!("{:?}", r.flags()),
                )
            })
            .collect::<Vec<_>>()
    };
    let actual = relocations(&own);
    assert_eq!(actual.iter().map(|r| r.2).collect::<Vec<_>>(), [3, 2]);
    let dir = std::env::temp_dir().join(format!("keel-evex-relocations-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    fs::write(&input, source).unwrap();
    for gnu in [false, true] {
        let mut cmd = if gnu {
            let mut c = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            c.arg("--64");
            c
        } else {
            let mut c = Command::new("clang");
            c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            c
        };
        let output = dir.join("oracle.o");
        let result = cmd.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let reference = fs::read(output).unwrap();
        assert_eq!(text(&own), text(&reference));
        assert_eq!(actual, relocations(&reference));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generated_att_bindings_match_intel() {
    let cases = |s: &'static str| {
        s.lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| l.splitn(3, '|').nth(2).unwrap())
            .collect::<Vec<_>>()
    };
    let intel = cases(include_str!("generated/x86-evex-smoke.txt"));
    let att = cases(include_str!("generated/x86-evex-smoke-att.txt"));
    assert_eq!(intel.len(), att.len());
    for (intel, att) in intel.iter().zip(att) {
        assert_eq!(own(intel, Syntax::Intel), own(att, Syntax::Att), "{att}");
    }
}
