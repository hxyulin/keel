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
            Features::parse("avx512vl,avx512dq,avx512bw,avx512cd,vpclmulqdq").unwrap(),
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
    for row in include_str!("../generated/x86-evex-smoke.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
    {
        count += 1;
        let fields: Vec<_> = row.splitn(3, '|').collect();
        let source = fields[2];
        let object = match keel_asm::x86::assemble_object_with_features(
            source,
            Features::parse("avx512vl,avx512dq,avx512bw,avx512cd,vpclmulqdq").unwrap(),
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
            Features::parse("avx512vl,avx512dq,avx512bw,avx512cd,vpclmulqdq").unwrap(),
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
    assert_eq!(count, 3385);
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
        (
            Syntax::Intel,
            include_str!("../generated/x86-evex-smoke.txt"),
        ),
        (
            Syntax::Att,
            include_str!("../generated/x86-evex-smoke-att.txt"),
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
    let intel = cases(include_str!("../generated/x86-evex-smoke.txt"));
    let att = cases(include_str!("../generated/x86-evex-smoke-att.txt"));
    assert_eq!(intel.len(), att.len());
    for (intel, att) in intel.iter().zip(att) {
        assert_eq!(own(intel, Syntax::Intel), own(att, Syntax::Att), "{att}");
    }
}

#[test]
fn evex_dq_bw_cd_independent_bytes_and_decode_roundtrips() {
    // This is a hand-selected boundary matrix, independent of generated smoke
    // fixtures. These instructions are assembled and decoded, never executed.
    let mut cases: Vec<String> = [
        "vpmullq zmm31{k7}{z},zmm16,zmm29",
        "vcvtpd2qq zmm31{k7}{z},zmm29,{rn-sae}",
        "vcvttpd2qq zmm17{k2},zmm30,{sae}",
        "vcvttps2uqq zmm31{k7}{z},ymm20,{sae}",
        "vcvtuqq2ps ymm31{k7}{z},zmm20,{rd-sae}",
        "vcvtqq2pd zmm31{k7}{z},zmm20,{ru-sae}",
        "vextractf64x2 xmm31{k1}{z},zmm20,3",
        "vinserti64x2 zmm31{k7}{z},zmm20,xmm29,3",
        "kandb k7,k6,k5",
        "knotb k7,k6",
        "kmovb k7,r15d",
        "kmovb r15d,k7",
        "ktestw k7,k6",
        "vpaddb zmm31{k7}{z},zmm20,zmm29",
        "vpcmpb k7{k6},zmm20,zmm29,255",
        "vpcmpuw k7{k6},zmm20,zmm29,3",
        "vpmovb2m k7,zmm31",
        "vpmovm2b zmm31,k7",
        "vpbroadcastb zmm31{k7}{z},r15d",
        "vpbroadcastw zmm31,r15d",
        "vpmovswb ymm31{k7}{z},zmm20",
        "kandq k7,k6,k5",
        "kandd k7,k6,k5",
        "kshiftlq k7,k6,63",
        "kshiftrd k7,k6,31",
        "kunpckwd k7,k6,k5",
        "kunpckdq k7,k6,k5",
        "kmovq k7,r15",
        "kmovq r15,k7",
        "kmovq k7,qword ptr [r13+127]",
        "kmovq qword ptr [r13+128],k7",
        "kmovd k7,r15d",
        "kmovd r15d,k7",
        "vpconflictd zmm31{k7}{z},zmm20",
        "vpconflictq zmm31{k7}{z},zmm20",
        "vplzcntd zmm31{k7}{z},zmm20",
        "vpmullq ymm31{k7}{z},ymm16,ymm29",
        "vpaddb xmm31{k7}{z},xmm20,xmm29",
        "vpconflictd ymm31{k7}{z},ymm20",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    // Displacements straddle compressed disp8*N limits. Distinct short-memory
    // tuples catch use of the ZMM destination size as an access/scale shortcut.
    for (template, scale) in [
        ("vpmullq zmm31{k7}{z},zmm20,qword ptr [r13+DISP]{1to8}", 8),
        ("vplzcntq zmm31{k7}{z},qword ptr [r13+DISP]{1to8}", 8),
        ("vbroadcastf64x2 zmm31{k7}{z},xmmword ptr [r13+DISP]", 16),
        ("vpmovsxbw zmm31{k7}{z},ymmword ptr [r13+DISP]", 32),
        ("vpmovwb ymmword ptr [r13+DISP]{k7},zmm20", 32),
        ("vpmovuswb ymmword ptr [r13+DISP]{k7},zmm20", 32),
        ("vpsubw zmm31{k7}{z},zmm20,zmmword ptr [r13+DISP]", 64),
        ("vpconflictd zmm31{k7}{z},zmmword ptr [r13+DISP]", 64),
        ("vpbroadcastb zmm31{k7}{z},byte ptr [r13+DISP]", 1),
        ("vpbroadcastw zmm31{k7}{z},word ptr [r13+DISP]", 2),
    ] {
        for displacement in [-129 * scale, -128 * scale, -1, 127 * scale, 128 * scale] {
            cases.push(template.replace("DISP", &displacement.to_string()));
        }
    }
    let features = Features::parse("avx512dq,avx512bw,avx512cd,avx512vl").unwrap();
    let encode = |source: &str, syntax| {
        text(
            &keel_asm::x86::assemble_object_with_syntax_and_features(source, syntax, features)
                .unwrap_or_else(|e| panic!("{source}: {e}")),
        )
    };
    let mut source = String::from(".intel_syntax noprefix\n.text\n");
    let mut expected = Vec::new();
    for case in &cases {
        source.push_str(case);
        source.push('\n');
        let bytes = encode(case, Syntax::Intel);
        let decoded = decode(&bytes, 0).unwrap_or_else(|| panic!("{case}: {bytes:02x?}"));
        assert_eq!(decoded.bytes.len(), bytes.len(), "{case}");
        assert_eq!(
            encode(&decoded.assembly(), Syntax::Intel),
            bytes,
            "{case}: {}",
            decoded.assembly()
        );
        let parsed = Instruction::parse_intel(case).unwrap();
        assert_eq!(
            Instruction::parse_intel(&parsed.to_string()).unwrap(),
            parsed,
            "{case}"
        );
        expected.extend(bytes);
    }
    for gnu in [false, true] {
        assert_eq!(
            text(&oracle(&source, "dq-bw-cd", gnu)),
            expected,
            "GNU={gnu}"
        );
    }
    for (att, intel) in [
        (
            "vpmullq %zmm29,%zmm16,%zmm31{%k7}{z}",
            "vpmullq zmm31{k7}{z},zmm16,zmm29",
        ),
        (
            "vcvtpd2qq {rn-sae},%zmm29,%zmm31{%k7}{z}",
            "vcvtpd2qq zmm31{k7}{z},zmm29,{rn-sae}",
        ),
        (
            "vpmovwb %zmm20,4064(%r13){%k7}",
            "vpmovwb ymmword ptr [r13+4064]{k7},zmm20",
        ),
        ("kmovq %r15,%k7", "kmovq k7,r15"),
        (
            "vplzcntq 1016(%r13){1to8},%zmm31{%k7}{z}",
            "vplzcntq zmm31{k7}{z},qword ptr [r13+1016]{1to8}",
        ),
    ] {
        let bytes = encode(intel, Syntax::Intel);
        assert_eq!(encode(att, Syntax::Att), bytes, "{att}");
        let source = format!(".att_syntax\n.text\n{att}\n");
        for gnu in [false, true] {
            assert_eq!(
                text(&oracle(&source, "dq-bw-cd-att", gnu)),
                bytes,
                "GNU={gnu}, {att}"
            );
        }
    }
}

#[test]
fn evex_dq_bw_cd_feature_permissions_and_invalid_forms() {
    let assemble =
        |source: &str, features| keel_asm::x86::assemble_object_with_features(source, features);
    let f = Features::parse("avx512f").unwrap();
    let vl = Features::parse("avx512vl").unwrap();
    for (feature, wide, narrow) in [
        (
            "avx512dq",
            "vpmullq zmm31{k7}{z},zmm20,zmm29",
            "vpmullq ymm31{k7}{z},ymm20,ymm29",
        ),
        (
            "avx512bw",
            "vpaddb zmm31{k7}{z},zmm20,zmm29",
            "vpaddb xmm31{k7}{z},xmm20,xmm29",
        ),
        (
            "avx512cd",
            "vpconflictq zmm31{k7}{z},zmm20",
            "vpconflictq ymm31{k7}{z},ymm20",
        ),
    ] {
        let enabled = Features::parse(feature).unwrap();
        assert!(enabled.avx512f && !enabled.avx512vl);
        assert!(assemble(wide, enabled).is_ok(), "{wide}");
        assert!(assemble(wide, f).is_err(), "{wide}");
        assert!(assemble(wide, vl).is_err(), "{wide}");
        assert!(assemble(narrow, enabled).is_err(), "{narrow}");
        assert!(
            assemble(narrow, enabled.with_features("avx512vl").unwrap()).is_ok(),
            "{narrow}"
        );
        let required = Instruction::parse_intel(wide).unwrap().required_features();
        assert!(
            required.avx512f && !required.avx512vl,
            "{wide}: {required:?}"
        );
    }
    for (source, feature) in [
        ("kmovb k7,r15d", "avx512dq"),
        ("ktestw k7,k6", "avx512dq"),
        ("kmovd k7,r15d", "avx512bw"),
        ("kmovq k7,r15", "avx512bw"),
        ("kandq k7,k6,k5", "avx512bw"),
        ("kandd k7,k6,k5", "avx512bw"),
    ] {
        assert!(
            assemble(source, Features::parse(feature).unwrap()).is_ok(),
            "{source}"
        );
        assert!(assemble(source, f).is_err(), "{source}");
    }
    let all = Features::parse("avx512dq,avx512bw,avx512cd,avx512vl").unwrap();
    for source in [
        "vpmullq zmm31{k7}{z},zmm20,zmm29,{sae}",
        "vpaddb zmm31{k7}{z},zmm20,byte ptr [rax]{1to64}",
        "vpmovwb ymmword ptr [rax]{k7}{z},zmm20",
        "vpbroadcastw zmm31{k7}{z},byte ptr [rax]",
        "vplzcntq zmm31{k7}{z},qword ptr [rax]{1to16}",
        "vcvttpd2qq zmm31{k7}{z},zmm20,{rn-sae}",
        "kmovq k7,r15d",
        "kmovd k7,r15",
    ] {
        assert!(assemble(source, all).is_err(), "{source}");
    }
}
