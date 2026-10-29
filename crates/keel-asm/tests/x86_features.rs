//! Regression coverage for compiler-produced Intel LOCK/shift/SSE2 forms.
#[path = "x86_features/x86_features_runtime.rs"]
mod x86_features_runtime;
use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::{
    Program,
    x86::{Features, Instruction},
};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{path::Path, process::Command};

const BINARY: &[&str] = &[
    "movdqa",
    "movdqu",
    "pxor",
    "pand",
    "pandn",
    "por",
    "pcmpeqb",
    "pcmpeqw",
    "pcmpeqd",
    "pcmpgtb",
    "pcmpgtw",
    "pcmpgtd",
    "punpcklbw",
    "punpcklwd",
    "punpckldq",
    "punpcklqdq",
    "punpckhbw",
    "punpckhwd",
    "punpckhdq",
    "punpckhqdq",
    "unpcklps",
    "unpckhps",
    "unpcklpd",
    "unpckhpd",
    "andnps",
    "andnpd",
    "minps",
    "minpd",
    "maxps",
    "maxpd",
    "pmaddwd",
    "pmulhuw",
    "pmulhw",
    "pmuludq",
    "cvtdq2ps",
    "cvtps2dq",
    "cvttps2dq",
    "cvtps2pd",
    "cvtpd2ps",
    "cvtdq2pd",
    "cvtpd2dq",
    "cvttpd2dq",
    "cmpltss",
    "cmpnltps",
];
const CONTROL: &[&str] = &[
    "shufps", "shufpd", "pshufd", "pshuflw", "pshufhw", "cmpps", "cmppd", "cmpss", "cmpsd",
];
const SHIFTS: &[&str] = &[
    "psllw", "pslld", "psllq", "psrlw", "psrld", "psrlq", "psraw", "psrad", "pslldq", "psrldq",
];

fn memory_size(op: &str) -> &str {
    match op {
        "cmpltss" | "cmpss" => "dword",
        "cmpsd" | "cvtps2pd" | "cvtdq2pd" => "qword",
        _ => "xmmword",
    }
}
fn matrix() -> Vec<String> {
    let mut cases = Vec::new();
    for &op in BINARY.iter().chain(CONTROL) {
        for dst in 0..16 {
            let mut sources = (0..16).map(|n| format!("xmm{n}")).collect::<Vec<_>>();
            for memory in [
                "[rax]",
                "[r13+r12*4-128]",
                "[r12+127]",
                "[r15+128]",
                "[rip+external+7]",
                "fs:[r8+8]",
                "[rcx*8+4096]",
            ] {
                sources.push(format!("{} ptr {memory}", memory_size(op)));
            }
            for source in sources {
                for control in if CONTROL.contains(&op) {
                    &[0, 1, 27, 255][..]
                } else {
                    &[0][..]
                } {
                    let suffix = if CONTROL.contains(&op) {
                        format!(",{control}")
                    } else {
                        String::new()
                    };
                    cases.push(format!("{op} xmm{dst},{source}{suffix}"));
                }
            }
            if matches!(op, "movdqa" | "movdqu") {
                for memory in ["[r12+r15*8+128]", "[rip+external+7]", "gs:[r9-8]"] {
                    cases.push(format!("{op} xmmword ptr {memory},xmm{dst}"));
                }
            }
        }
    }
    for &op in SHIFTS {
        for dst in 0..16 {
            for imm in [0, 1, 15, 16, 31, 32, 63, 64, 127, 255] {
                cases.push(format!("{op} xmm{dst},{imm}"));
            }
            if !matches!(op, "pslldq" | "psrldq") {
                for source in [
                    "xmm0",
                    "xmm15",
                    "xmmword ptr [r12+r13*8+128]",
                    "xmmword ptr [rip+external+7]",
                ] {
                    cases.push(format!("{op} xmm{dst},{source}"));
                }
            }
        }
    }
    for op in ["pmovmskb", "movmskps", "movmskpd"] {
        for dst in ["eax", "r9d", "r15d", "rax", "r9", "r15"] {
            for src in 0..16 {
                cases.push(format!("{op} {dst},xmm{src}"));
            }
        }
    }
    cases
}
type RelocationRecord = (u64, String, i64, String);
fn section(bytes: &[u8]) -> (Vec<u8>, Vec<RelocationRecord>) {
    let file = object::File::parse(bytes).unwrap();
    let section = file.section_by_name(".text").unwrap();
    let rel = section
        .relocations()
        .map(|(offset, r)| {
            let object::RelocationTarget::Symbol(index) = r.target() else {
                panic!("expected symbol relocation")
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
        .collect();
    (section.data().unwrap().to_vec(), rel)
}
fn run(command: &mut Command) -> String {
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}
fn differential(name: &str, cases: &[String]) {
    let source = format!(".intel_syntax noprefix\n.text\n{}\n", cases.join("\n"));
    let ours = assemble_object(&source).unwrap();
    let mut typed = Program::new();
    for case in cases {
        let i = Instruction::parse_intel(case).unwrap();
        assert_eq!(
            Instruction::parse_intel(&i.to_string()).unwrap(),
            i,
            "{case}"
        );
        assert_eq!(
            i.opcode.required_features(),
            Features::NONE,
            "baseline {case}"
        );
        typed.x86_instruction(i);
    }
    assert!(
        ours == assemble_program_object(&typed).unwrap(),
        "{name}: typed object differs"
    );
    let root = std::env::var_os("KEEL_X86_FEATURE_ARTIFACTS").map(std::path::PathBuf::from);
    let dir = root.as_ref().map_or_else(
        || std::env::temp_dir().join(format!("keel-x86-features-{}-{name}", std::process::id())),
        |p| p.join(name),
    );
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    let reference = dir.join("clang.o");
    let candidate = dir.join("keel.o");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&candidate, &ours).unwrap();
    run(Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
        .arg(&input)
        .arg("-o")
        .arg(&reference));
    let (actual_bytes, actual_relocations) = section(&ours);
    let (expected_bytes, expected_relocations) = section(&std::fs::read(&reference).unwrap());
    assert_eq!(
        actual_bytes.len(),
        expected_bytes.len(),
        "{name}: text size"
    );
    let first_difference = actual_bytes
        .iter()
        .zip(&expected_bytes)
        .position(|(a, b)| a != b);
    assert!(
        first_difference.is_none(),
        "{name}: first byte difference at {first_difference:?}"
    );
    assert!(
        actual_relocations == expected_relocations,
        "{name}: relocations differ"
    );
    let disassembler = std::env::var("KEEL_LLVM_OBJDUMP").unwrap_or_else(|_| "objdump".into());
    let disassemble = |path: &Path| run(Command::new(&disassembler).arg("-d").arg(path));
    let a = disassemble(&candidate);
    let b = disassemble(&reference);
    std::fs::write(dir.join("keel.dis"), &a).unwrap();
    std::fs::write(dir.join("clang.dis"), &b).unwrap();
    let normalize = |s: &str| {
        s.lines()
            .filter(|l| !l.contains("file format") && !l.trim().is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(normalize(&a), normalize(&b), "{name}: disassembly");
    eprintln!(
        "{name}: {} instructions, {} text bytes, {} relocations match Clang",
        cases.len(),
        section(&ours).0.len(),
        section(&ours).1.len()
    );
    if root.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn sse2_clang_bytes_relocations_disassembly_and_typed_api() {
    differential("sse2", &matrix());
}

#[test]
fn lock_whitespace_legal_rmw_and_implicit_one_shifts_match_clang() {
    let mut cases = Vec::new();
    for (size, r) in [
        ("byte", "r9b"),
        ("word", "r9w"),
        ("dword", "r9d"),
        ("qword", "r9"),
    ] {
        for op in [
            "add", "adc", "sub", "sbb", "and", "or", "xor", "xadd", "xchg", "cmpxchg",
        ] {
            cases.push(format!("lock\t\t{op} {size} ptr [r12+r13*4+128],{r}"));
            cases.push(format!("LOCK {op} {size} ptr [rip+external+7],{r}"));
        }
        for op in ["add", "adc", "sub", "sbb", "and", "or", "xor"] {
            cases.push(format!("lock\t{op} {size} ptr [rip+external+7],1"));
        }
        for op in ["inc", "dec", "neg", "not"] {
            cases.push(format!("lock\t{op} {size} ptr [r13]"));
            cases.push(format!("lock\t{op} {size} ptr fs:[r13]"));
        }
        for op in ["shl", "shr", "sar", "rol", "ror"] {
            for operand in [
                r.to_string(),
                format!("{size} ptr [r12+8*r15-128]"),
                format!("{size} ptr [rip+external+7]"),
            ] {
                cases.push(format!("{op} {operand}"));
                assert_eq!(
                    section(&assemble_object(&format!("{op} {operand}")).unwrap()).0,
                    section(&assemble_object(&format!("{op} {operand},1")).unwrap()).0
                );
            }
        }
    }
    for (size, r) in [("word", "r9w"), ("dword", "r9d"), ("qword", "r9")] {
        for op in ["bts", "btr", "btc"] {
            cases.push(format!("lock\t{op} {size} ptr [rip+external+7],{r}"));
            cases.push(format!("lock\t{op} {size} ptr [rip+external+7],5"));
        }
    }
    differential("lock-shifts", &cases);
}

fn reject(source: &str) {
    assert!(assemble_object(source).is_err(), "accepted {source}");
    if let Ok(i) = Instruction::parse_intel(source) {
        let mut typed = Program::new();
        typed.x86_instruction(i);
        assert!(
            assemble_program_object(&typed).is_err(),
            "typed API accepted {source}"
        );
    }
}

#[test]
fn invalid_lock_vector_widths_and_controls_stay_explicit() {
    // Standalone LOCK is a GNU prefix-byte statement, covered by x86_gnu_prefixes.
    for source in [
        "lock lock add [rax],eax",
        "lock add eax,ebx",
        "lock xadd eax,ebx",
        "lock cmpxchg rax,rbx",
        "lock inc eax",
        "lock neg eax",
        "lock bts rax,3",
        "lock mov [rax],eax",
        "lock cmp [rax],eax",
        "lock test [rax],eax",
        "lock bt [rax],eax",
        "lock shl dword ptr [rax]",
        "lock pxor xmm0,xmm1",
        "lock ret",
        "lock add [rax],[rbx]",
        "lock add [rax],xmm0",
        "shl [rax]",
        "shr xmm0",
        "sar rax,dl",
    ] {
        reject(source);
    }
    for op in BINARY.iter().chain(CONTROL).chain(SHIFTS) {
        for source in [
            format!("{op} rax,rbx"),
            format!("{op} xmm0,byte ptr [rax]"),
            format!("{op} [rax],[rbx]"),
            format!("{op} xmm0,xmm1,0,1"),
        ] {
            reject(&source);
        }
    }
    for &op in CONTROL.iter().chain(SHIFTS) {
        for control in ["-1", "256", "external", "al"] {
            let source = format!(
                "{op} xmm0,{}",
                if SHIFTS.contains(&op) {
                    control.to_string()
                } else {
                    format!("xmm1,{control}")
                }
            );
            reject(&source);
        }
    }
    for source in [
        "movdqa xmm0,qword ptr [rax]",
        "movdqu qword ptr [rax],xmm0",
        "pslldq xmm0,xmm1",
        "psrldq xmm0,[rax]",
        "pmovmskb ax,xmm0",
        "pmovmskb eax,[rax]",
        "movmskpd eax,[rax]",
        "cvtps2pd xmm0,xmmword ptr [rax]",
        "cvtdq2pd xmm0,xmmword ptr [rax]",
        "cmpltss xmm0,qword ptr [rax]",
        "cmpsd xmm0,dword ptr [rax],1",
        "pxor xmm0,mm0",
    ] {
        reject(source);
    }
    // Extending baseline SSE2 must not accidentally enable independent ISAs.
    for source in [
        "vaddps xmm0,xmm1,xmm2",
        "andn eax,ebx,ecx",
        "pext eax,ebx,ecx",
    ] {
        assert!(
            assemble_object(source)
                .unwrap_err()
                .to_string()
                .contains("requires x86 target features")
        );
    }
}

#[test]
fn invalid_manual_feature_sets_are_rejected_on_both_entry_points() {
    let invalid = Features {
        avx: true,
        ..Features::NONE
    };
    let mut program = Program::new();
    program.x86_instruction(Instruction::parse_intel("ret").unwrap());
    assert!(keel_asm::x86::assemble_object_with_features("ret", invalid).is_err());
    assert!(keel_asm::x86::assemble_program_object_with_features(&program, invalid).is_err());
    for source in ["popcnt rax, rcx", "roundsd xmm0, xmm1, 0"] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}
