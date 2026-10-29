use keel_asm_ir::x86::{Instruction, Register, RegisterClass};

#[test]
fn ymm_transport_identity_and_format() {
    for n in 0..32 {
        let ymm = Register::parse(&format!("ymm{n}")).unwrap();
        let xmm = Register::parse(&format!("xmm{n}")).unwrap();
        assert_eq!(ymm.class, RegisterClass::Ymm);
        assert_eq!(ymm.width, 256);
        assert_eq!(ymm.number, xmm.number);
        assert_ne!(ymm, xmm);
        let source = format!("vaddps ymm{n},ymm1,ymmword ptr [r12+16]");
        let i = Instruction::parse_intel(&source).unwrap();
        assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
    }
    assert!(Register::parse("ymm32").is_none());
}

use keel_asm::x86::{
    Features, assemble_object_with_features, assemble_program_object_with_features,
};
use keel_asm_ir::{
    Program,
    x86::{V3_FORMS, V3Operand},
};
use object::{Object, ObjectSection};
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
fn cases() -> Vec<String> {
    let registers = [
        [9, 10, 11, 12],
        [0, 1, 2, 3],
        [8, 1, 15, 0],
        [0, 9, 10, 15],
        [15, 14, 13, 12],
        [1, 1, 1, 1],
        [9, 10, 11, 12],
        [15, 0, 7, 8],
    ];
    let addresses = [
        "r12+r13*4+127",
        "rax",
        "r8+128",
        "r13-128",
        "r15+r9*8-129",
        "r12+r13*2",
        "r13*4+2147483647",
        "rip+37",
    ];
    let mut out = Vec::new();
    for f in V3_FORMS {
        for (profile, regs) in registers.iter().enumerate() {
            if f.vsib != 0 && profile == 5 {
                continue;
            }
            let index = if matches!(profile, 1 | 4 | 7) { 4 } else { 14 };
            let vector_index = format!("{}{}", if f.vsib == 128 { "xmm" } else { "ymm" }, index);
            let address = if f.vsib == 0 {
                addresses[profile].to_string()
            } else {
                match profile {
                    1 => format!("rax+{vector_index}"),
                    6 => format!("{vector_index}*4+2147483647"),
                    7 => format!("{vector_index}*8+37"),
                    _ => format!("r12+{vector_index}*4+127"),
                }
            };
            let ops = f
                .operands
                .iter()
                .enumerate()
                .map(|(n, o)| match o {
                    V3Operand::X => format!("xmm{}", regs[n]),
                    V3Operand::Y => format!("ymm{}", regs[n]),
                    V3Operand::D => {
                        if regs[n] < 8 {
                            ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"]
                                [regs[n] as usize]
                                .into()
                        } else {
                            format!("r{}d", regs[n])
                        }
                    }
                    V3Operand::Q => {
                        if regs[n] < 8 {
                            ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"]
                                [regs[n] as usize]
                                .into()
                        } else {
                            format!("r{}", regs[n])
                        }
                    }
                    V3Operand::I => {
                        if profile % 2 == 0 {
                            "255".into()
                        } else {
                            "0".into()
                        }
                    }
                    V3Operand::M(w) => format!(
                        "{} ptr {}[{address}]",
                        match w {
                            8 => "byte",
                            16 => "word",
                            32 => "dword",
                            64 => "qword",
                            128 => "xmmword",
                            256 => "ymmword",
                            _ => panic!("width {w}"),
                        },
                        if profile == 4 { "gs:" } else { "" }
                    ),
                })
                .collect::<Vec<_>>()
                .join(",");
            let source = format!("{} {}", f.opcode, ops).trim().to_string();
            out.push(source);
        }
    }
    out
}
#[test]
fn every_xed_vector_form_typed_text_roundtrip() {
    for source in cases() {
        let i = Instruction::parse_intel(&source).unwrap();
        assert_eq!(
            Instruction::parse_intel(&i.to_string()).unwrap(),
            i,
            "{source}"
        );
        let mut p = Program::new();
        p.x86_instruction(i);
        let a = assemble_object_with_features(&source, Features::all_known())
            .unwrap_or_else(|e| panic!("{source}: {e}"));
        assert_eq!(
            a,
            assemble_program_object_with_features(&p, Features::all_known()).unwrap(),
            "{source}"
        );
        assert!(
            assemble_object_with_features(&source, Features::NONE).is_err(),
            "{source}"
        );
    }
}
#[test]
fn every_xed_vector_form_matches_clang_and_gnu() {
    differential(cases(), "vector");
}
fn differential(cases: Vec<String>, label: &str) {
    differential_syntax(cases, label, false);
}
fn differential_syntax(cases: Vec<String>, label: &str, att_syntax: bool) {
    let evidence = std::env::var_os("KEEL_V3_EVIDENCE_DIR");
    let dir = evidence.as_ref().map_or_else(
        || std::env::temp_dir().join(format!("keel-v3-{}-{label}", std::process::id())),
        |root| std::path::PathBuf::from(root).join(label),
    );
    std::fs::create_dir_all(&dir).unwrap();
    // Check both source orders for known symmetric instructions: LLVM and GAS
    // differ on whether they exchange sources to save a VEX byte. Each accepted
    // encoding is still compared byte-for-byte to an independent assembler.
    let source = cases
        .iter()
        .enumerate()
        .map(|(n, s)| {
            let alias = oracle_alias(s).map_or(String::new(), |a| {
                format!(".section .alias{n},\"ax\"\n{a}\n.section .vex3{n},\"ax\"\n{{vex3}} {s}\n")
            });
            format!(".section .case{n},\"ax\"\n{s}\n{alias}")
        })
        .collect::<String>();
    let source = if att_syntax {
        source
            .lines()
            .map(|line| {
                if line.starts_with('.') {
                    line.to_string()
                } else if let Some(line) = line.strip_prefix("{vex3} ") {
                    format!("{{vex3}} {}", att(&Instruction::parse_intel(line).unwrap()))
                } else {
                    att(&Instruction::parse_intel(line).unwrap())
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        source
    };
    let syntax = if att_syntax {
        ".att_syntax prefix"
    } else {
        ".intel_syntax noprefix"
    };
    let input = dir.join("input.s");
    std::fs::write(&input, format!("{syntax}\n{source}")).unwrap();
    let gnu_as = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
    let mut differences = Vec::new();
    for tool in ["clang", gnu_as.as_str()] {
        {
            let gnu = ["vpcmpestri", "vpcmpestrm", "vpcmpistri"].iter().fold(
                source.clone(),
                |s, base| {
                    s.replace(&format!("{base}64 "), &format!("{base}q "))
                        .replace(&format!("rex64 {base} "), &format!("{base}q "))
                },
            );
            let gnu = gnu
                .lines()
                .map(|s| {
                    if s.starts_with("vpcmpistriq ")
                        || tool == "clang"
                            && (s.starts_with("vpcmpestriq ") || s.starts_with("vpcmpestrmq "))
                    {
                        "nop"
                    } else {
                        s
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(&input, format!("{syntax}\n{gnu}")).unwrap();
        }
        if evidence.is_some() {
            std::fs::copy(
                &input,
                dir.join(if tool == "clang" { "clang.s" } else { "gnu.s" }),
            )
            .unwrap();
        }
        let output = dir.join(if tool == "clang" { "clang.o" } else { "gnu.o" });
        let mut cmd = Command::new(tool);
        if tool == "clang" {
            cmd.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        } else {
            cmd.arg("--64");
        }
        let result = cmd.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&result.stderr[..result.stderr.len().min(10000)])
        );
        let bytes = std::fs::read(output).unwrap();
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let sections = file
            .sections()
            .map(|s| (s.name().unwrap(), s.data().unwrap()))
            .collect::<std::collections::HashMap<_, _>>();
        for (n, s) in cases.iter().enumerate() {
            if s.starts_with("vpcmpistri64 ")
                || tool == "clang"
                    && (s.starts_with("vpcmpestri64 ") || s.starts_with("vpcmpestrm64 "))
            {
                continue;
            }
            let expected = sections[format!(".case{n}").as_str()];
            let own_object = assemble_object_with_features(s, Features::all_known()).unwrap();
            let ours = text(&own_object);
            if label == "relocations" {
                assert_eq!(
                    relocations(&own_object, ".text"),
                    relocations(&bytes, &format!(".case{n}")),
                    "{tool}: relocations {s}"
                );
            }
            if ours != expected
                && sections
                    .get(format!(".alias{n}").as_str())
                    .is_none_or(|alias| ours != *alias)
                && sections
                    .get(format!(".vex3{n}").as_str())
                    .is_none_or(|alias| ours != *alias)
            {
                differences.push(format!(
                    "{tool}: row {n}: {s} ours={ours:02x?} expected={expected:02x?}"
                ));
            }
        }
    }
    std::fs::write(dir.join("differences.txt"), differences.join("\n")).unwrap();
    assert!(
        differences.is_empty(),
        "{}",
        differences
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    if evidence.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

fn scalar_cases() -> Vec<String> {
    let mut out = Vec::new();
    for r in ["ax", "r9w", "eax", "r9d", "rax", "r9"] {
        for src in [r, "[r12+r13*4+128]", "fs:[rbp]"] {
            out.push(format!("lzcnt {r},{src}"));
        }
        for mem in ["[r12+r13*4+128]", "fs:[rbp]"] {
            out.push(format!("movbe {r},{mem}"));
            out.push(format!("movbe {mem},{r}"));
        }
    }
    for op in ["xsave", "xsave64", "xrstor", "xrstor64"] {
        for mem in ["[rax]", "[r12+r13*4+128]", "fs:[rbp]"] {
            out.push(format!("{op} {mem}"));
        }
    }
    for dst in ["ax", "r9w", "r15w"] {
        for src in [
            "bx",
            "r14w",
            "word ptr [r12+r13*4+127]",
            "word ptr fs:[rbp]",
        ] {
            out.push(format!("tzcnt {dst},{src}"));
        }
    }
    out.extend(["xgetbv".into(), "xsetbv".into()]);
    out
}
#[test]
fn scalar_v3_families_match_both_assemblers_and_typed_transport() {
    let cases = scalar_cases();
    for s in &cases {
        let i = Instruction::parse_intel(s).unwrap();
        let mut p = Program::new();
        p.x86_instruction(i.clone());
        assert_eq!(
            assemble_object_with_features(s, Features::all_known()).unwrap(),
            assemble_program_object_with_features(&p, Features::all_known()).unwrap(),
            "{s}"
        );
        assert!(
            assemble_object_with_features(s, Features::NONE).is_err(),
            "{s}"
        );
        let mut f = Features::NONE;
        let r = i.required_features();
        f.bmi1 = r.bmi1;
        f.lzcnt = r.lzcnt;
        f.movbe = r.movbe;
        f.xsave = r.xsave;
        assert!(assemble_object_with_features(s, f).is_ok(), "{s}");
        assert!(!r.popcnt && !r.sse41);
    }
    differential(cases, "scalar");
}

fn oracle_alias(s: &str) -> Option<String> {
    let mut i = Instruction::parse_intel(s).unwrap();
    let name = i.opcode.to_string();
    let symmetric = matches!(
        name.as_str(),
        "vaddpd"
            | "vaddps"
            | "vmulpd"
            | "vmulps"
            | "vandps"
            | "vandpd"
            | "vorps"
            | "vorpd"
            | "vxorps"
            | "vxorpd"
            | "vpor"
            | "vpand"
            | "vpxor"
            | "vpaddb"
            | "vpaddw"
            | "vpaddd"
            | "vpaddq"
            | "vpaddsb"
            | "vpaddsw"
            | "vpaddusb"
            | "vpaddusw"
            | "vpavgb"
            | "vpavgw"
            | "vpcmpeqb"
            | "vpcmpeqw"
            | "vpcmpeqd"
            | "vpmulhuw"
            | "vpmulhw"
            | "vpmullw"
            | "vpmuludq"
            | "vpsadbw"
            | "vpmaxsw"
            | "vpmaxub"
            | "vpminsw"
            | "vpminub"
            | "vpmaddwd"
    ) || matches!(name.as_str(), "vcmpps" | "vcmppd")
        && matches!(i.operands.get(3),Some(keel_asm_ir::x86::Operand::Immediate(a)) if matches!(a.expression.constant,0|255));
    if symmetric
        && i.operands.len() >= 3
        && matches!(i.operands[1], keel_asm_ir::x86::Operand::Register(_))
        && matches!(i.operands[2], keel_asm_ir::x86::Operand::Register(_))
    {
        i.operands.swap(1, 2);
        Some(i.to_string())
    } else {
        None
    }
}

fn att(i: &Instruction) -> String {
    use keel_asm_ir::x86::{Opcode, Operand, V3Opcode};
    let mut name = i.opcode.to_string();
    if matches!(i.opcode, Opcode::V3(V3Opcode::Lzcnt | V3Opcode::Movbe)) {
        let w = i
            .operands
            .iter()
            .find_map(|o| match o {
                Operand::Register(r) => Some(r.width),
                _ => None,
            })
            .unwrap();
        name.push(match w {
            16 => 'w',
            32 => 'l',
            64 => 'q',
            _ => unreachable!(),
        });
    }
    if matches!(
        i.opcode,
        Opcode::V3(V3Opcode::Vcvtsi2ss | V3Opcode::Vcvtsi2sd)
    ) {
        let w = match &i.operands[2] {
            Operand::Register(r) => r.width,
            Operand::Memory(m) => m.width,
            _ => unreachable!(),
        };
        name.push(if w == 64 { 'q' } else { 'l' });
    }
    if matches!(
        i.opcode,
        Opcode::V3(V3Opcode::Vcvtpd2dq | V3Opcode::Vcvttpd2dq | V3Opcode::Vcvtpd2ps)
    ) && let Operand::Memory(m) = &i.operands[1]
    {
        name.push(if m.width == 256 { 'y' } else { 'x' });
    }
    for base in ["vpcmpestri", "vpcmpestrm", "vpcmpistri"] {
        name = name.replace(&format!("{base}64"), &format!("rex64 {base}"));
    }
    let operands = i
        .operands
        .iter()
        .rev()
        .map(|o| match o {
            Operand::Register(r) => format!("%{r}"),
            Operand::Immediate(a) => format!("${a}"),
            Operand::Memory(m) => {
                let base = m.base.map_or(String::new(), |r| format!("%{r}"));
                let index = m
                    .index
                    .map_or(String::new(), |r| format!(",%{r},{}", m.scale));
                format!(
                    "{}{}({base}{index})",
                    match m.segment {
                        Some(0x64) => "%fs:",
                        Some(0x65) => "%gs:",
                        _ => "",
                    },
                    m.displacement
                )
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{name} {operands}")
}
#[test]
fn all_forms_att_and_intel_encode_identically() {
    use keel_asm::x86::{Syntax, assemble_object_with_syntax_and_features};
    let mut sources = cases();
    sources.extend(scalar_cases());
    for source in sources {
        let i = Instruction::parse_intel(&source).unwrap();
        let att = att(&i);
        let a = assemble_object_with_features(&source, Features::all_known()).unwrap();
        let b = assemble_object_with_syntax_and_features(&att, Syntax::Att, Features::all_known())
            .unwrap_or_else(|e| panic!("{source} -> {att}: {e}"));
        assert_eq!(text(&a), text(&b), "{source} -> {att}");
    }
}

#[test]
fn malformed_typed_and_parsed_operands_are_rejected() {
    use keel_asm_ir::x86::{Address, Modifier, Operand};
    let reject = |i: Instruction| {
        let mut p = Program::new();
        p.x86_instruction(i.clone());
        assert!(
            assemble_program_object_with_features(&p, Features::all_known()).is_err(),
            "accepted {i:?}"
        );
    };
    for source in cases().into_iter().step_by(8).chain(scalar_cases()) {
        let original = Instruction::parse_intel(&source).unwrap();
        let mut i = original.clone();
        i.lock = true;
        reject(i);
        for (n, o) in original.operands.iter().enumerate() {
            match o {
                Operand::Register(_) => {
                    for field in 0..3 {
                        let mut i = original.clone();
                        let Operand::Register(r) = &mut i.operands[n] else {
                            unreachable!()
                        };
                        match field {
                            0 => r.number = 32,
                            1 => r.high_byte = true,
                            _ => {
                                r.class = RegisterClass::Rip;
                                r.width = 64;
                                r.number = 0
                            }
                        };
                        reject(i);
                    }
                }
                Operand::Memory(_) => {
                    for field in 0..5 {
                        let mut i = original.clone();
                        let Operand::Memory(m) = &mut i.operands[n] else {
                            unreachable!()
                        };
                        match field {
                            0 => m.width = 1,
                            1 => m.scale = 3,
                            2 => m.segment = Some(0x66),
                            3 => m.base = Register::parse("xmm0"),
                            _ => m.index = Register::parse("rsp"),
                        };
                        reject(i);
                    }
                }
                Operand::Immediate(_) => {
                    for value in [-129, 256] {
                        let mut i = original.clone();
                        i.operands[n] = Operand::Immediate(Address {
                            expression: keel_asm_ir::Expression::constant(value),
                            modifier: Modifier::None,
                        });
                        reject(i);
                    }
                }
            }
        }
    }
    for source in [
        "vaddps ymm0,xmm1,ymm2",
        "vaddps xmm0,xmm1,ymm2",
        "vaddss ymm0,ymm1,ymm2",
        "vaddps ymm0,ymm1,xmmword ptr [rax]",
        "vaddps ymm0,ymm1,[xmm1]",
        "vaddps ymm0,ymm1,[rax+ymm4]",
        "vpgatherdd ymm0,[rax+ymm0*4],ymm1",
        "vpgatherdd ymm0,[rax+ymm1*4],ymm1",
        "vpgatherdd ymm0,[rax+ymm2*4],ymm0",
        "vpgatherdd ymm0,[rax+xmm2*4],ymm1",
        "vpgatherdd ymm0,[rip+ymm2*4],ymm1",
        "vpgatherdd ymm0,[rax+r12*4],ymm1",
        "vpermq xmm0,xmm1,3",
        "vextracti128 ymm0,ymm1,0",
        "vcvtps2ph ymm0,ymm1,0",
        "vfmadd132ss ymm0,ymm1,ymm2",
        "vfmadd213pd ymm0,xmm1,ymm2",
        "lzcnt ax,eax",
        "movbe eax,ecx",
        "movbe eax,qword ptr [rax]",
        "xsave rax",
        "xsave qword ptr [rax]",
        "xgetbv eax",
        "vpsrld ymm15,ymmword ptr [rip+external+7],255",
        "vaddps ymm32,ymm0,ymm1",
        "vaddps ymm0,ymm1,[rax+ymm16*4]",
    ] {
        assert!(
            assemble_object_with_features(source, Features::all_known()).is_err(),
            "{source}"
        );
    }
}
#[test]
fn form_specific_features_never_inherit_legacy_bits() {
    use keel_asm_ir::x86::V3Opcode;
    for source in cases() {
        let i = Instruction::parse_intel(&source).unwrap();
        let required = i.required_features();
        assert!(!required.popcnt && !required.sse41, "{source}");
        assert!(
            assemble_object_with_features(&source, required).is_ok(),
            "{source}"
        );
        let mut disabled = Features::all_known();
        if required.fma {
            disabled.fma = false;
        } else if required.f16c {
            disabled.f16c = false;
        } else if required.avx2 {
            disabled.avx2 = false;
        } else {
            disabled.avx = false;
            disabled.avx2 = false;
            disabled.fma = false;
            disabled.f16c = false;
        }
        assert!(
            assemble_object_with_features(&source, disabled).is_err(),
            "{source}"
        );
    }
    let avx = Features {
        xsave: true,
        osxsave: true,
        avx: true,
        ..Features::NONE
    };
    for source in [
        "vbroadcastss ymm0,dword ptr [rax]",
        "vbroadcastsd ymm0,qword ptr [rax]",
        "vaddps ymm0,ymm1,ymm2",
        "vpaddd xmm0,xmm1,xmm2",
    ] {
        assert!(
            assemble_object_with_features(source, avx).is_ok(),
            "{source}"
        );
    }
    for source in [
        "vbroadcastss ymm0,xmm1",
        "vbroadcastsd ymm0,xmm1",
        "vpaddd ymm0,ymm1,ymm2",
    ] {
        assert!(
            assemble_object_with_features(source, avx).is_err(),
            "{source}"
        );
    }
    for opcode in V3Opcode::ALL {
        assert!(
            ![
                "andn", "bextr", "blsi", "blsmsk", "blsr", "tzcnt", "shlx", "shrx", "sarx", "rorx",
                "bzhi", "pext", "pdep", "mulx"
            ]
            .contains(&opcode.mnemonic())
        );
    }
}

fn relocations(bytes: &[u8], section: &str) -> Vec<(u64, object::RelocationFlags, i64, String)> {
    use object::ObjectSymbol;
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(section)
        .unwrap()
        .relocations()
        .map(|(offset, r)| {
            let object::RelocationTarget::Symbol(index) = r.target() else {
                panic!("unexpected relocation")
            };
            (
                offset,
                r.flags(),
                r.addend(),
                file.symbol_by_index(index).unwrap().name().unwrap().into(),
            )
        })
        .collect()
}
#[test]
fn symbolic_ymm_vsib_and_trailing_immediates_match_external_relocations() {
    use keel_asm::x86::{Syntax, assemble_object_with_syntax_and_features};
    let sources = [
        "vaddps ymm15,ymm14,ymmword ptr [rip+external+7]",
        "vroundps ymm15,ymmword ptr [rip+external+7],255",
        "vblendvps ymm15,ymm14,ymmword ptr [rip+external+7],ymm13",
        "vshufps ymm15,ymm14,ymmword ptr [rip+external+7],255",
        "vpermq ymm15,ymmword ptr [rip+external+7],255",
        "vpshufd ymm15,ymmword ptr [rip+external+7],255",
        "vpsrld ymm15,ymm14,xmmword ptr [rip+external+7]",
        "vextractf128 xmmword ptr [rip+external+7],ymm15,255",
        "vextracti128 xmmword ptr [rip+external+7],ymm15,255",
        "vfmadd231pd ymm15,ymm14,ymmword ptr [rip+external+7]",
        "vcvtps2ph xmmword ptr [rip+external+7],ymm15,255",
        "vcvtph2ps ymm15,xmmword ptr [rip+external+7]",
        "vpgatherdd ymm15,dword ptr [r12+ymm4*4+external+7],ymm14",
        "vgatherqps xmm15,dword ptr [ymm4*4+external+7],xmm14",
        "lzcnt r15,qword ptr [rip+external+7]",
        "movbe qword ptr [rip+external+7],r15",
        "xsave64 [rip+external+7]",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect::<Vec<_>>();
    differential(sources.clone(), "relocations");
    for source in sources {
        let i = Instruction::parse_intel(&source).unwrap();
        let mut p = Program::new();
        p.x86_instruction(i.clone());
        let a = assemble_object_with_features(&source, Features::all_known()).unwrap();
        assert_eq!(
            a,
            assemble_program_object_with_features(&p, Features::all_known()).unwrap()
        );
        let b =
            assemble_object_with_syntax_and_features(&att(&i), Syntax::Att, Features::all_known())
                .unwrap();
        assert_eq!(text(&a), text(&b));
        assert_eq!(relocations(&a, ".text"), relocations(&b, ".text"));
    }
}

#[test]
fn exact_reserved_bits_and_xed_w1_string_aliases() {
    for (source, expected) in [
        ("vzeroall", vec![0xc5, 0xfc, 0x77]),
        ("vzeroupper", vec![0xc5, 0xf8, 0x77]),
        ("vmovups ymm8,ymm9", vec![0xc4, 0x41, 0x7c, 0x10, 0xc1]),
        (
            "vfmadd132pd ymm8,ymm15,ymm14",
            vec![0xc4, 0x42, 0x85, 0x98, 0xc6],
        ),
        (
            "vpcmpestri64 xmm9,xmm10,255",
            vec![0xc4, 0x43, 0xf9, 0x61, 0xca, 0xff],
        ),
        (
            "vpcmpestrm64 xmm9,xmm10,255",
            vec![0xc4, 0x43, 0xf9, 0x60, 0xca, 0xff],
        ),
        (
            "vpcmpistri64 xmm9,xmm10,255",
            vec![0xc4, 0x43, 0xf9, 0x63, 0xca, 0xff],
        ),
        (
            "vpgatherdd ymm0,[r12+ymm4*4],ymm15",
            vec![0xc4, 0xc2, 0x05, 0x90, 0x04, 0xa4],
        ),
    ] {
        assert_eq!(
            text(&assemble_object_with_features(source, Features::all_known()).unwrap()),
            expected,
            "{source}"
        );
    }
    for base in ["vpcmpestri", "vpcmpestrm", "vpcmpistri"] {
        let i = Instruction::parse_intel(&format!("{base}64 xmm9,xmm10,255")).unwrap();
        assert_eq!(
            i,
            Instruction::parse_intel(&format!("rex64 {base} xmm9,xmm10,255")).unwrap()
        );
        assert_eq!(
            i,
            Instruction::parse_intel(&format!("{base}q xmm9,xmm10,255")).unwrap()
        );
    }
}

#[test]
fn all_att_forms_match_both_assemblers() {
    let mut sources = cases();
    sources.extend(scalar_cases());
    differential_syntax(sources, "att", true);
}

#[test]
fn tls_relaxation_is_not_a_generic_v3_memory_relocation() {
    for source in [
        "lzcnt rax, qword ptr [rip+external@GOTTPOFF]",
        "movbe eax, dword ptr [rip+external@GOTTPOFF]",
        "vldmxcsr [rip+external@GOTTPOFF]",
        "xsave [rip+external@GOTTPOFF]",
        "vmovups ymm0, [rip+external@GOTTPOFF]",
    ] {
        let i = Instruction::parse_intel(source).unwrap();
        let mut program = Program::new();
        program.x86_instruction(i);
        assert!(
            assemble_object_with_features(source, Features::all_known()).is_err(),
            "{source}"
        );
        assert!(
            assemble_program_object_with_features(&program, Features::all_known()).is_err(),
            "{source}"
        );
        // Ordinary PC-relative address relocations remain valid.
        assert!(
            assemble_object_with_features(&source.replace("@GOTTPOFF", ""), Features::all_known())
                .is_ok(),
            "{source}"
        );
    }
}
