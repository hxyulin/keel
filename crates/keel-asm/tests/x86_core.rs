use keel_asm::x86::assemble_program_object_with_features;
// Core inventory includes optional CLFSH encoding witnesses, not a v1 guarantee.
fn assemble_object(source: &str) -> Result<Vec<u8>, keel_asm::Error> {
    keel_asm::x86::assemble_object_with_features(
        source,
        Features {
            clflush: true,
            ..Features {
                popcnt: true,
                sse41: true,
                ..Features::NONE
            }
        },
    )
}
use keel_asm_ir::x86::{
    CoreOpcode, Features, Instruction, Opcode, Operand, Register, RegisterClass, Syntax,
};
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;
fn text(b: &[u8]) -> Vec<u8> {
    object::File::parse(b)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn relocs(b: &[u8]) -> Vec<(u64, String, i64, String)> {
    let f = object::File::parse(b).unwrap();
    f.section_by_name(".text")
        .unwrap()
        .relocations()
        .map(|(o, r)| {
            let object::RelocationTarget::Symbol(n) = r.target() else {
                panic!()
            };
            (
                o,
                format!("{:?}", r.flags()),
                r.addend(),
                f.symbol_by_index(n).unwrap().name().unwrap().into(),
            )
        })
        .collect()
}
fn matrix(rows: Vec<String>, tag: &str) {
    let intel = format!(".intel_syntax noprefix\n.text\n{}\n", rows.join("\n"));
    let own = assemble_object(&intel).unwrap();
    let mut p = keel_asm_ir::Program::new();
    let mut att_source = String::from(".text\n");
    for row in &rows {
        if row.ends_with(':') {
            p.label(row.trim_end_matches(':'));
            att_source += row;
            att_source.push('\n');
            continue;
        }
        let i = Instruction::parse_intel(row).unwrap();
        assert_eq!(
            Instruction::parse_intel(&i.to_string()).unwrap(),
            i,
            "{row}"
        );
        assert_eq!(
            i.opcode.required_features().clflush,
            i.opcode == Opcode::Core(CoreOpcode::Clflush)
        );
        att_source += &att(&i);
        att_source.push('\n');
        p.x86_instruction(i);
    }
    assert_eq!(
        own,
        assemble_program_object_with_features(
            &p,
            Features {
                clflush: true,
                ..Features::NONE
            }
        )
        .unwrap()
    );
    let att_obj = keel_asm::x86::assemble_object_with_syntax_and_features(
        &att_source,
        Syntax::Att,
        Features {
            clflush: true,
            ..Features::NONE
        },
    )
    .unwrap();
    assert_eq!(text(&own), text(&att_obj), "Intel/ATT bytes");
    assert_eq!(relocs(&own), relocs(&att_obj));
    let dir = std::env::temp_dir().join(format!("keel-core-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (dialect, source) in [("intel", intel), ("att", att_source)] {
        let input = dir.join(format!("{dialect}.s"));
        std::fs::write(&input, &source).unwrap();
        for name in ["clang", "gnu"] {
            // LLVM has no INT1 mnemonic. This row's semantic byte oracle is
            // GNU plus pinned XED; LLVM only transports the literal byte.
            std::fs::write(
                &input,
                if name == "clang" {
                    source
                        .replace("\nint1\n", "\n.byte 0xf1\n")
                        .replace("\nint1 \n", "\n.byte 0xf1\n")
                } else {
                    source.clone()
                },
            )
            .unwrap();
            let mut c = if name == "clang" {
                let mut c = Command::new("clang");
                c.args([
                    "-target",
                    "x86_64-unknown-linux-gnu",
                    "-c",
                    "-x",
                    "assembler",
                ]);
                c
            } else {
                let mut c =
                    Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
                c.arg("--64");
                c
            };
            let output = dir.join(format!("{dialect}-{name}.o"));
            let r = c.arg(&input).arg("-o").arg(&output).output().unwrap();
            assert!(
                r.status.success(),
                "{name}/{dialect}: {}",
                String::from_utf8_lossy(&r.stderr)
            );
            let b = std::fs::read(output).unwrap();
            let a = normalize_string_prefixes(text(&own), &rows);
            let btext = normalize_string_prefixes(text(&b), &rows);
            assert_eq!(
                a.len(),
                btext.len(),
                "{name}/{dialect} size rows={rows:?} own={a:02x?} reference={btext:02x?}"
            );
            assert_eq!(
                a.iter().zip(&btext).position(|(a, b)| a != b),
                None,
                "{name}/{dialect} byte position: rows={rows:?} own={a:02x?} reference={btext:02x?}"
            );
            assert_eq!(relocs(&own), relocs(&b), "{name}/{dialect} relocations");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
fn att(i: &Instruction) -> String {
    use keel_asm_ir::x86::write_expression;
    let expression = |a: &keel_asm_ir::x86::Address| {
        let mut out = String::new();
        write_expression(&mut out, &a.expression, a.modifier).unwrap();
        out
    };
    let operands = i
        .operands
        .iter()
        .rev()
        .map(|o| match o {
            Operand::Register(r) => format!("%{r}"),
            Operand::Immediate(a) => format!("${}", expression(a)),
            Operand::Memory(m) => {
                let mut out = String::from(match m.segment {
                    Some(0x64) => "%fs:",
                    Some(0x65) => "%gs:",
                    None => "",
                    _ => unreachable!(),
                });
                out += &expression(&m.displacement);
                if m.base.is_some() || m.index.is_some() {
                    out.push('(');
                    if let Some(r) = m.base {
                        out += &format!("%{r}");
                    }
                    if let Some(r) = m.index {
                        out += &format!(",%{r},{}", m.scale);
                    }
                    out.push(')');
                }
                out
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let name = match i.opcode {
        Opcode::Core(CoreOpcode::In | CoreOpcode::Out) => {
            let w = i
                .operands
                .iter()
                .find_map(|o| match o {
                    Operand::Register(r) if r.number == 0 && r.class == RegisterClass::Gpr => {
                        Some(r.width)
                    }
                    _ => None,
                })
                .unwrap();
            format!("{}{}", i.opcode, size_suffix(w))
        }
        Opcode::Core(CoreOpcode::Lfs | CoreOpcode::Lgs | CoreOpcode::Lss) => {
            let Operand::Register(r) = &i.operands[0] else {
                unreachable!()
            };
            format!("{}{}", i.opcode, size_suffix(r.width))
        }
        Opcode::Core(CoreOpcode::CallFar | CoreOpcode::JmpFar) => {
            let Operand::Memory(m) = &i.operands[0] else {
                unreachable!()
            };
            let suffix = match m.width {
                32 => "w",
                48 => "l",
                80 => "q",
                _ => unreachable!(),
            };
            format!(
                "l{}{}",
                if matches!(i.opcode, Opcode::Core(CoreOpcode::CallFar)) {
                    "call"
                } else {
                    "jmp"
                },
                suffix
            )
        }
        Opcode::Core(CoreOpcode::RetFar { width }) => {
            format!("lret{}", size_suffix(width))
        }
        Opcode::Core(CoreOpcode::Rcl | CoreOpcode::Rcr) => format!(
            "{}{}",
            i.opcode,
            match &i.operands[0] {
                Operand::Register(r) => size_suffix(r.width),
                Operand::Memory(m) => size_suffix(m.width),
                _ => unreachable!(),
            }
        ),
        Opcode::Core(CoreOpcode::String { width: 32, .. }) => i
            .opcode
            .to_string()
            .replace("movsd", "movsl")
            .replace("stosd", "stosl")
            .replace("lodsd", "lodsl")
            .replace("cmpsd", "cmpsl")
            .replace("scasd", "scasl")
            .replace("insd", "insl")
            .replace("outsd", "outsl"),
        Opcode::Nop if !i.operands.is_empty() => format!(
            "nop{}",
            size_suffix(match &i.operands[0] {
                Operand::Register(r) => r.width,
                Operand::Memory(m) => m.width,
                _ => unreachable!(),
            })
        ),
        Opcode::Mov
            if i.operands
                .iter()
                .any(|o| matches!(o,Operand::Register(r) if r.class==RegisterClass::Segment)) =>
        {
            let w = i
                .operands
                .iter()
                .find_map(|o| match o {
                    Operand::Register(r) if r.class == RegisterClass::Gpr => Some(r.width),
                    _ => None,
                })
                .unwrap_or(16);
            format!("mov{}", size_suffix(w))
        }
        _ => i.opcode.to_string(),
    };
    let operands = if matches!(i.opcode, Opcode::Core(CoreOpcode::Enter { .. })) {
        i.operands
            .iter()
            .map(|o| {
                if let Operand::Immediate(a) = o {
                    format!("${}", expression(a))
                } else {
                    unreachable!()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    } else if i.opcode.is_direct_branch() {
        i.operands
            .iter()
            .map(|o| {
                if let Operand::Immediate(a) = o {
                    expression(a)
                } else {
                    unreachable!()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    } else {
        operands
    };
    let operands = if matches!(
        i.opcode,
        Opcode::Core(CoreOpcode::CallFar | CoreOpcode::JmpFar)
    ) {
        format!("*{operands}")
    } else {
        operands
    };
    format!("{}{name} {operands}", if i.lock { "lock " } else { "" })
}
fn size_suffix(bits: u16) -> &'static str {
    match bits {
        8 => "b",
        16 => "w",
        32 => "l",
        64 => "q",
        _ => unreachable!(),
    }
}

const NATIVE_CORE_SOURCE: &str = include_str!("data/core_native.s");

#[test]
fn native_core_source_assembles_without_external_relocations() {
    let object = assemble_object(NATIVE_CORE_SOURCE).unwrap();
    assert!(relocs(&object).is_empty());
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn native_core_scalar_branches_strings_and_guard_pages() {
    let dir = std::env::temp_dir().join(format!("keel-core-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("core.o"),
        assemble_object(NATIVE_CORE_SOURCE).unwrap(),
    )
    .unwrap();
    let driver = include_str!("data/core_native.c");
    std::fs::write(dir.join("driver.c"), driver).unwrap();
    let output = Command::new("clang")
        .arg(dir.join("driver.c"))
        .arg(dir.join("core.o"))
        .arg("-o")
        .arg(dir.join("core-smoke"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("core-smoke")).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn port_io_and_far_segment_loads_match_both_assemblers() {
    let mut rows = Vec::new();
    for acc in ["al", "ax", "eax"] {
        for port in ["0x80", "dx"] {
            rows.push(format!("in {acc},{port}"));
            rows.push(format!("out {port},{acc}"));
        }
    }
    for op in ["lfs", "lgs", "lss"] {
        rows.push(format!("{op} r9w,dword ptr [rip+0]"));
        rows.push(format!("{op} r10d,fword ptr [rip+0]"));
    }
    matrix(rows, "ports-far-segments");
    // GNU as rejects the architecturally valid REX.W far-load form. Intel's
    // long-mode opcode table and pinned XED supply this exact encoding.
    for (op, byte) in [("lfs", 0xb4), ("lgs", 0xb5), ("lss", 0xb2)] {
        assert_eq!(
            text(&assemble_object(&format!("{op} r9,[rip+0]")).unwrap()),
            [0x4c, 0x0f, byte, 0x0d, 0, 0, 0, 0]
        );
    }
}
#[test]
fn string_port_io_matches_both_assemblers() {
    let mut rows = Vec::new();
    for kind in ["ins", "outs"] {
        for suffix in ["b", "w", "d"] {
            for repeat in ["", "rep ", "repne "] {
                for address in ["", "addr32 "] {
                    let segments = if kind == "outs" {
                        &["", "fs ", "gs "][..]
                    } else {
                        &[""][..]
                    };
                    for segment in segments {
                        rows.push(format!("{address}{segment}{repeat}{kind}{suffix}"));
                    }
                }
            }
        }
    }
    matrix(rows, "string-ports");
}
#[test]
fn user_mode_descriptor_queries_match_both_assemblers() {
    let mut rows = Vec::new();
    for op in ["lar", "lsl"] {
        for dst in ["r9w", "r9d"] {
            let src = if dst == "r9" {
                "rax"
            } else if dst == "r9d" {
                "eax"
            } else {
                "ax"
            };
            rows.push(format!("{op} {dst},{src}"));
            rows.push(format!("{op} {dst},word ptr [rip+0]"));
        }
    }
    for op in ["verr", "verw"] {
        rows.push(format!("{op} r9w"));
        rows.push(format!("{op} word ptr [rip+0]"));
    }
    matrix(rows, "descriptor-queries");
    // This GNU version drops REX.W on LAR/LSL, including memory sources.
    // Pinned XED and LLVM distinguish these from the 32-bit forms.
    for (op, byte) in [("lar", 2), ("lsl", 3)] {
        assert_eq!(
            text(&assemble_object(&format!("{op} r9,rax")).unwrap()),
            [0x4c, 0x0f, byte, 0xc8]
        );
        assert_eq!(
            text(&assemble_object(&format!("{op} r9,word ptr [rip+0]")).unwrap()),
            [0x4c, 0x0f, byte, 0x0d, 0, 0, 0, 0]
        );
    }
}
#[test]
fn system_state_read_forms_match_both_assemblers() {
    let mut rows = vec!["sgdt [rip+0]".into(), "sidt [rip+0]".into()];
    for op in ["sldt", "str", "smsw"] {
        for reg in ["r9w", "r10d"] {
            rows.push(format!("{op} {reg}"));
        }
        if op == "smsw" {
            rows.push("smsw r11".into());
        }
        rows.push(format!("{op} word ptr [rip+0]"));
    }
    matrix(rows, "system-state-reads");
    // GNU drops REX.W for SLDT/STR despite the r64 spelling; LLVM and
    // pinned XED retain the requested GPRv width. No execution here (UMIP).
    for (op, modrm) in [("sldt", 0xc3), ("str", 0xcb)] {
        assert_eq!(
            text(&assemble_object(&format!("{op} r11")).unwrap()),
            [0x49, 0x0f, 0, modrm]
        );
    }
}
#[test]
fn fixed_long_mode_control_forms_match_both_assemblers() {
    matrix(
        vec![
            "pushfw".into(),
            "popfw".into(),
            "rdpmc".into(),
            "sysenter".into(),
        ],
        "fixed-core-control",
    );
    assert_eq!(text(&assemble_object("iretw").unwrap()), [0x66, 0xcf]);
    assert_eq!(text(&assemble_object("iretd").unwrap()), [0xcf]);
    assert_eq!(text(&assemble_object("udb").unwrap()), [0xd6]);
}
#[test]
fn far_control_long_mode_encodings_and_syntaxes() {
    for (source, expected) in [
        (
            "callf dword ptr [rip+0]",
            vec![0x66, 0xff, 0x1d, 0, 0, 0, 0],
        ),
        ("callf fword ptr [rip+0]", vec![0xff, 0x1d, 0, 0, 0, 0]),
        (
            "callf tbyte ptr [rip+0]",
            vec![0x48, 0xff, 0x1d, 0, 0, 0, 0],
        ),
        ("jmpf dword ptr [rip+0]", vec![0x66, 0xff, 0x2d, 0, 0, 0, 0]),
        ("jmpf fword ptr [rip+0]", vec![0xff, 0x2d, 0, 0, 0, 0]),
        ("jmpf tbyte ptr [rip+0]", vec![0x48, 0xff, 0x2d, 0, 0, 0, 0]),
        ("retf", vec![0xcb]),
        ("retf 4660", vec![0xca, 0x34, 0x12]),
        ("retfw", vec![0x66, 0xcb]),
        ("retfq", vec![0x48, 0xcb]),
        ("retfq 4660", vec![0x48, 0xca, 0x34, 0x12]),
    ] {
        let i = Instruction::parse_intel(source).unwrap();
        assert_eq!(
            text(&assemble_object(source).unwrap()),
            expected,
            "{source}"
        );
        assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
        let att = att(&i);
        let parsed = Instruction::parse_att(&att).unwrap();
        assert_eq!(
            text(
                &assemble_program_object_with_features(
                    &{
                        let mut p = keel_asm_ir::Program::new();
                        p.x86_instruction(parsed);
                        p
                    },
                    Features::NONE
                )
                .unwrap()
            ),
            expected,
            "{att}"
        );
    }
    for bad in [
        "callf rax",
        "jmpf [rax]",
        "callf qword ptr [rax]",
        "retf 65536",
        "retf -1",
    ] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
    let reference = ".text\ndata16 lcall *0(%rip)\nlcall *0(%rip)\nrex64 lcall *0(%rip)\ndata16 ljmp *0(%rip)\nljmp *0(%rip)\nrex64 ljmp *0(%rip)\nlretw\nlret\nlretq\n";
    let own = "callf dword ptr [rip+0]\ncallf fword ptr [rip+0]\ncallf tbyte ptr [rip+0]\njmpf dword ptr [rip+0]\njmpf fword ptr [rip+0]\njmpf tbyte ptr [rip+0]\nretfw\nretf\nretfq\n";
    let expected = text(&assemble_object(own).unwrap());
    let dir = std::env::temp_dir().join(format!("keel-far-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("far.s");
    std::fs::write(&input, reference).unwrap();
    for tool in ["clang", "gnu"] {
        let output = dir.join(format!("{tool}.o"));
        let mut command = if tool == "clang" {
            let mut c = Command::new("clang");
            c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            c
        } else {
            Command::new(
                std::env::var("KEEL_GNU_AS")
                    .unwrap_or_else(|_| "/opt/homebrew/bin/x86_64-elf-as".into()),
            )
        };
        let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(
            result.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(text(&std::fs::read(&output).unwrap()), expected, "{tool}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn intentional_undefined_encodings_are_explicit() {
    assert_eq!(text(&assemble_object("ud0.short").unwrap()), [0x0f, 0xff]);
    assert_eq!(
        text(&assemble_object("ud0 r9d,r10d").unwrap()),
        [0x45, 0x0f, 0xff, 0xca]
    );
    assert_eq!(
        text(&assemble_object("ud1 r9d,[rip+0]").unwrap()),
        [0x44, 0x0f, 0xb9, 0x0d, 0, 0, 0, 0]
    );
    for bad in ["ud0", "ud1", "ud0.short eax", "ud0 rax,ecx", "ud1 eax,rax"] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
}
#[test]
fn scalar_core_forms_match_both_assemblers_and_syntaxes() {
    let mut rows = Vec::new();
    for op in ["rcl", "rcr"] {
        for bits in [8, 16, 32, 64] {
            let size = match bits {
                8 => "byte",
                16 => "word",
                32 => "dword",
                _ => "qword",
            };
            let mut destinations = (0..16)
                .map(|n| {
                    if n < 8 {
                        match bits {
                            8 => ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil"][n].into(),
                            16 => ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"][n].into(),
                            32 => {
                                ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"][n].into()
                            }
                            _ => ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"][n].into(),
                        }
                    } else {
                        format!(
                            "r{n}{}",
                            match bits {
                                8 => "b",
                                16 => "w",
                                32 => "d",
                                _ => "",
                            }
                        )
                    }
                })
                .collect::<Vec<String>>();
            if bits == 8 {
                destinations.extend(["ah", "ch", "dh", "bh"].map(str::to_owned));
            }
            destinations.extend(
                [
                    "[rax]",
                    "[r13+r12*4-128]",
                    "[r12+127]",
                    "[r15+128]",
                    "[rip+external+7]",
                    "fs:[r8+8]",
                    "[rcx*8+4096]",
                    "[rip+external@GOTPCREL]",
                    "fs:[tls@TPOFF+8]",
                ]
                .map(|a| format!("{size} ptr {a}")),
            );
            for dst in destinations {
                for n in 0..=255 {
                    rows.push(format!("{op} {dst},{n}"));
                }
                rows.push(format!("{op} {dst},cl"));
            }
        }
    }
    rows.push("cwd".into());
    rows.push("int1".into());
    for vector in 0..=255 {
        rows.push(format!("int {vector}"));
    }
    for a in [
        "[rax]",
        "[r13+r12*4-128]",
        "[rip+external]",
        "fs:[r8+8]",
        "[rcx*8+4096]",
    ] {
        rows.push(format!("cmpxchg8b qword ptr {a}"));
        rows.push(format!("lock cmpxchg8b qword ptr {a}"));
        rows.push(format!("clflush byte ptr {a}"));
    }
    for frame in [0, 1, 32767, 32768, 65535] {
        for nesting in [0, 1, 31, 32, 255] {
            rows.push(format!("enter {frame},{nesting}"));
            rows.push(format!("data16 enter {frame},{nesting}"));
        }
    }
    matrix(rows, "scalar");
}
#[test]
fn rel8_core_branches_labels_relocations_and_boundaries() {
    for op in [
        "loop",
        "loope",
        "loopne",
        "jrcxz",
        "jecxz",
        "addr32 loop",
        "addr32 loope",
        "addr32 loopne",
    ] {
        matrix(
            vec![
                "back:".into(),
                format!("{op} forward"),
                format!("{op} back"),
                "forward:".into(),
                format!("{op} external+7"),
            ],
            &op.replace(' ', "-"),
        );
        let len = if op.starts_with("addr32") || op == "jecxz" {
            3
        } else {
            2
        };
        for n in [0, 1, 126, 127] {
            let s = format!("{op} target\n.zero {n}\ntarget:\n");
            assert!(assemble_object(&s).is_ok(), "{s}");
        }
        for n in [128, 129, 300] {
            let s = format!("{op} target\n.zero {n}\ntarget:\n");
            assert!(assemble_object(&s).is_err(), "accepted rel8 overflow: {s}");
        }
        for displacement in [-128, -127, -1] {
            let n = -displacement - len;
            if n < 0 {
                continue;
            }
            let s = format!("target:\n.zero {n}\n{op} target\n");
            assert!(assemble_object(&s).is_ok(), "{s}");
        }
        let s = format!("target:\n.zero {}\n{op} target\n", 129 - len);
        assert!(assemble_object(&s).is_err(), "{s}");
        for bad in [
            format!("{op} rax"),
            format!("{op} 7"),
            format!("{op} external@PLT"),
            format!("{op} [rax]"),
            format!("{op} target-other"),
            format!(".set absolute,7\n{op} absolute"),
        ] {
            assert!(assemble_object(&bad).is_err(), "{bad}");
        }
    }
}
#[test]
fn core_scalar_malformed_and_typed_validation() {
    for s in [
        "rcl xmm0,1",
        "rcr mm0,cl",
        "rcl eax,al",
        "rcr eax,ecx",
        "rcl [rax],1",
        "rcl eax,256",
        "rcr eax,-1",
        "rcl eax,external",
        "lock rcl dword ptr [rax],1",
        "cmpxchg8b rax",
        "cmpxchg8b dword ptr [rax]",
        "clflush rax",
        "clflush qword ptr [rax]",
        "lock clflush [rax]",
        "cwd rax",
        "enter -1,0",
        "enter 65536,0",
        "enter 1,256",
        "enter external,0",
        "int 256",
        "int -1",
        "int external",
        "int1 0",
        "jcxz target",
        "into",
        "in rax,dx",
        "in al,rdx",
        "in al,256",
        "out -1,al",
        "out dx,ah",
        "lfs r9d,r10d",
        "lfs r9d,qword ptr [rax]",
        "lss r9,qword ptr [rax]",
        "verr eax",
        "lar xmm0,ax",
        "sgdt rax",
        "smsw xmm0",
        "insq",
        "rep outsq",
    ] {
        assert!(assemble_object(s).is_err(), "{s}");
    }
    for s in [
        "rclb $1,%al",
        "rcrq %cl,%r15",
        "rcll %eax",
        "cwtd",
        "enterw $32,$7",
        "loopz label",
        "loopnz label",
    ] {
        let i = Instruction::parse_att(s).unwrap();
        assert!(
            assemble_program_object_with_features(
                &{
                    let mut p = keel_asm_ir::Program::new();
                    p.x86_instruction(i);
                    p
                },
                Features::NONE
            )
            .is_ok(),
            "{s}"
        );
    }
    let own = assemble_object("int.imm 3").unwrap();
    assert_eq!(text(&own), [0xcd, 3]);
    for (class, number, width, high_byte) in [
        (RegisterClass::Gpr, 16, 32, false),
        (RegisterClass::Gpr, 0, 128, false),
        (RegisterClass::Gpr, 0, 32, true),
        (RegisterClass::Mmx, 0, 64, false),
    ] {
        let mut i = Instruction::parse_intel("rcl eax,1").unwrap();
        i.operands[0] = Operand::Register(Register {
            class,
            number,
            width,
            high_byte,
        });
        let mut p = keel_asm_ir::Program::new();
        p.x86_instruction(i);
        assert!(assemble_program_object_with_features(&p, Features::NONE).is_err());
    }
}
#[test]
fn explicit_alternate_encodings_preserve_immediate_selector() {
    // XED explicitly distinguishes C0/C1 imm8 from D0/D1 implicit ONE.
    // The explicit selector retains C0/C1 even when the count is one.
    for (s, expected) in [
        ("rcl.imm al,1", vec![0xc0, 0xd0, 1]),
        ("rcr.imm r9,1", vec![0x49, 0xc1, 0xd9, 1]),
    ] {
        assert_eq!(text(&assemble_object(s).unwrap()), expected);
        let i = Instruction::parse_intel(s).unwrap();
        assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
    }
}
#[test]
fn string_and_xlat_size_repeat_segment_address_matrix() {
    let mut rows = Vec::new();
    for kind in ["movs", "stos", "lods", "cmps", "scas"] {
        for suffix in ["b", "w", "d", "q"] {
            for repeat in ["", "rep ", "repne "] {
                for addr in ["", "addr32 "] {
                    let segments = if ["movs", "lods", "cmps"].contains(&kind) {
                        vec!["", "fs ", "gs "]
                    } else {
                        vec![""]
                    };
                    for segment in segments {
                        rows.push(format!("{addr}{segment}{repeat}{kind}{suffix}"));
                    }
                }
            }
        }
    }
    for a in ["", "addr32 "] {
        for s in ["", "fs ", "gs "] {
            rows.push(format!("{a}{s}xlatb"));
        }
    }
    matrix(rows, "strings");
}
#[test]
fn explicit_string_operands_are_fixed_validated_aliases() {
    for (source, canonical) in [
        ("movsb byte ptr [rdi],byte ptr [rsi]", "movsb"),
        ("movs qword ptr [rdi],qword ptr fs:[rsi]", "fs movsq"),
        ("movsd dword ptr [edi],dword ptr [esi]", "addr32 movsd"),
        ("cmpsw word ptr gs:[rsi],word ptr es:[rdi]", "gs cmpsw"),
        ("stosb byte ptr es:[edi]", "addr32 stosb"),
        ("lodsq qword ptr fs:[esi]", "addr32 fs lodsq"),
        ("scasw word ptr [rdi]", "scasw"),
        ("xlatb byte ptr gs:[ebx]", "addr32 gs xlatb"),
    ] {
        assert_eq!(
            Instruction::parse_intel(source).unwrap(),
            Instruction::parse_intel(canonical).unwrap(),
            "{source}"
        );
    }
    for (source, canonical) in [
        ("movsb (%rsi),%es:(%rdi)", "movsb"),
        ("movsl (%esi),(%edi)", "addr32 movsd"),
        ("cmpsw %es:(%rdi),%gs:(%rsi)", "gs cmpsw"),
        ("lodsq %fs:(%rsi)", "fs lodsq"),
        ("xlatb %gs:(%ebx)", "addr32 gs xlatb"),
    ] {
        assert_eq!(
            Instruction::parse_att(source).unwrap(),
            Instruction::parse_intel(canonical).unwrap(),
            "{source}"
        );
    }
    for s in [
        "movs",
        "movsb [rax],[rsi]",
        "movsb [rdi],[rdi]",
        "movsb [edi],[rsi]",
        "movsb [rdi+8],[rsi]",
        "movsb [rdi],[rsi+external]",
        "movsb [rdi],fs:[rsi+rcx]",
        "stosb fs:[rdi]",
        "scasb [rax]",
        "lodsw byte ptr [rsi]",
        "xlatb [rax]",
        "xlatb [rbx+al]",
        "rep add eax,ebx",
        "rep int 3",
        "rep rep movsb",
        "addr32 addr32 movsb",
        "fs scasb",
        "gs stosb",
        "rep xlatb",
        "movsb xmm0,xmm1",
    ] {
        assert!(assemble_object(s).is_err(), "{s}");
    }
    use keel_asm_ir::x86::{Repeat, StringKind};
    for op in [
        CoreOpcode::String {
            kind: StringKind::Movs,
            width: 128,
            repeat: Repeat::Rep,
            address32: false,
            segment: None,
        },
        CoreOpcode::String {
            kind: StringKind::Stos,
            width: 8,
            repeat: Repeat::None,
            address32: false,
            segment: Some(0x64),
        },
        CoreOpcode::Xlat {
            address32: false,
            segment: Some(0x66),
        },
    ] {
        let mut p = keel_asm_ir::Program::new();
        p.x86_instruction(Instruction {
            opcode: Opcode::Core(op),
            operands: vec![],
            lock: false,
        });
        assert!(assemble_program_object_with_features(&p, Features::NONE).is_err());
    }
}

// GNU and LLVM order commuting string prefixes differently. Restrict this
// comparison to the known single-instruction string/XLAT ranges; never touch
// FWAIT/x87 or reorder REX, opcodes, immediates or relocation fields.
fn normalize_string_prefixes(mut bytes: Vec<u8>, rows: &[String]) -> Vec<u8> {
    if !rows.iter().any(|s| {
        Instruction::parse_intel(s).is_ok_and(|i| {
            matches!(
                i.opcode,
                Opcode::Core(CoreOpcode::String { .. } | CoreOpcode::Xlat { .. })
            )
        })
    }) {
        return bytes;
    }
    let mut offset = 0;
    for row in rows {
        if row.ends_with(':') {
            continue;
        }
        let i = Instruction::parse_intel(row).unwrap();
        let len = text(&assemble_object(row).unwrap()).len();
        if matches!(
            i.opcode,
            Opcode::Core(CoreOpcode::String { .. } | CoreOpcode::Xlat { .. })
        ) {
            let n = bytes[offset..offset + len]
                .iter()
                .take_while(|b| matches!(b, 0x64 | 0x65 | 0x66 | 0x67 | 0xf2 | 0xf3))
                .count();
            bytes[offset..offset + n].sort_unstable();
        }
        offset += len;
    }
    assert_eq!(offset, bytes.len());
    bytes
}

#[test]
fn primary_core_segment_moffs_and_nop_forms() {
    let mut rows = Vec::new();
    for segment in ["es", "cs", "ss", "ds", "fs", "gs"] {
        for dst in ["r10d", "word ptr [rip+external]"] {
            rows.push(format!("mov {dst},{segment}"));
        }
        if segment != "cs" {
            // GNU omits 66 while LLVM emits it on register-selector loads;
            // XED exact-pattern witnesses validate Keel's LLVM spelling.
            rows.push(format!("mov {segment},word ptr [rip+external]"));
        }
    }
    // LLVM Intel defaults segment PUSH/POP to 16 bits; GNU defaults to 64.
    // Exact XED witnesses below cover both widths without executing selectors.
    // AT&T segment moves infer their width from their GPR or selector.
    matrix(rows, "segments");
    matrix(
        vec![
            "nop word ptr [rax+16]".into(),
            "nop dword ptr [r13+r12*4+128]".into(),
            "nop qword ptr [rip+external]".into(),
            "nop r9w".into(),
            "nop r10d".into(),
            "nop r11".into(),
        ],
        "nop",
    );
    for (line, expected) in [
        ("movabs al,byte ptr [0x123456789abcdef0]", vec![0xa0]),
        ("movabs ax,word ptr [0x123456789abcdef0]", vec![0x66, 0xa1]),
        ("movabs eax,dword ptr [0x123456789abcdef0]", vec![0xa1]),
        (
            "movabs rax,qword ptr [0x123456789abcdef0]",
            vec![0x48, 0xa1],
        ),
        ("movabs byte ptr [0x123456789abcdef0],al", vec![0xa2]),
        ("movabs word ptr [0x123456789abcdef0],ax", vec![0x66, 0xa3]),
        ("movabs dword ptr [0x123456789abcdef0],eax", vec![0xa3]),
        (
            "movabs qword ptr [0x123456789abcdef0],rax",
            vec![0x48, 0xa3],
        ),
    ] {
        let mut expected = expected;
        expected.extend(0x123456789abcdef0_u64.to_le_bytes());
        assert_eq!(text(&assemble_object(line).unwrap()), expected, "{line}");
    }
    for line in [
        "mov cs,ax",
        "mov fs,eax",
        "push ds",
        "pop cs",
        "movabs r9,[123]",
        "movabs rax,[rdi]",
        "nop al",
        "mov ax,xmm1",
    ] {
        assert!(assemble_object(line).is_err(), "{line}");
    }
}
#[test]
fn primary_core_equal_width_extensions_and_leavew() {
    for (line, expected) in [
        ("movsx r9w,r10w", vec![0x66, 0x45, 0x0f, 0xbf, 0xca]),
        ("movzx r9w,r10w", vec![0x66, 0x45, 0x0f, 0xb7, 0xca]),
        ("movsxd r9d,r10d", vec![0x45, 0x63, 0xca]),
        ("movsxd r9w,r10w", vec![0x66, 0x45, 0x63, 0xca]),
        ("leavew", vec![0x66, 0xc9]),
    ] {
        assert_eq!(text(&assemble_object(line).unwrap()), expected, "{line}");
        let i = Instruction::parse_intel(line).unwrap();
        assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
    }
}

#[test]
fn primary_core_address32_bytes_relocations_and_guards() {
    let mut rows = Vec::new();
    for address in [
        "[eax]",
        "[r13d+r12d*4-128]",
        "[r12d+127]",
        "[r15d+128]",
        "[eip+external+7]",
        "fs:[r8d+8]",
        "[ecx*8+4096]",
        "[eax+0xffffffff]",
    ] {
        rows.push(format!("mov r9,qword ptr {address}"));
        rows.push(format!("rcl word ptr {address},7"));
        rows.push(format!("rcr byte ptr {address},cl"));
        rows.push(format!("lock cmpxchg8b qword ptr {address}"));
        rows.push(format!("clflush byte ptr {address}"));
    }
    matrix(rows, "address32");
    for line in [
        "mov rax,[eax+rcx]",
        "mov rax,[rax+ecx]",
        "mov rax,[ax]",
        "mov rax,[eip+ecx]",
        "mov rax,[eax+esp*2]",
        "mov rax,[fs]",
        "mov eax,eip",
        "vaddps xmm0,xmm1,[eax]",
    ] {
        assert!(assemble_object(line).is_err(), "{line}");
    }
    for source in [".att_syntax\npushw $7\npopw %r9w", ".att_syntax\nleavew"] {
        assert!(assemble_object(source).is_ok(), "{source}");
    }
}

#[test]
fn primary_core_address32_symbol_uses_unsigned_relocation() {
    // GNU uses R_X86_64_32 for address-size-32 absolute displacements.
    // LLVM currently emits R_X86_64_32S and unnecessarily restricts the upper
    // half of the 32-bit address space. Keep the hardware's zero extension.
    let source = "mov r9,qword ptr [ecx*4+external]";
    let own = assemble_object(source).unwrap();
    assert_eq!(text(&own), [0x67, 0x4c, 0x8b, 0x0c, 0x8d, 0, 0, 0, 0]);
    assert_eq!(
        relocs(&own),
        vec![(5, "Elf { r_type: 10 }".into(), 0, "external".into())]
    );
    let dir =
        std::env::temp_dir().join(format!("keel-core-address32-reloc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    let output = dir.join("gnu.o");
    std::fs::write(&input, format!(".intel_syntax noprefix\n.text\n{source}\n")).unwrap();
    let result = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()))
        .arg("--64")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let reference = std::fs::read(&output).unwrap();
    assert_eq!(text(&own), text(&reference));
    assert_eq!(relocs(&own), relocs(&reference));
    std::fs::remove_dir_all(dir).unwrap();
}
