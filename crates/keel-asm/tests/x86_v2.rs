//! Encoding gates, not execution/semantic coverage. Native Linux belongs to the
//! parent runner. Clang and GNU as are independent encoding/relocation oracles.
use keel_asm::x86::{
    assemble_object_with_features, assemble_object_with_syntax_and_features,
    assemble_program_object_with_features,
};
use keel_asm_ir::{
    Program,
    x86::{CpuLevel, Features, Instruction, Opcode, Operand, Register, Syntax, V2Form, V2Opcode},
};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{path::Path, process::Command};

fn features() -> Features {
    Features::for_level(CpuLevel::V2)
}
fn size(bits: u16) -> &'static str {
    match bits {
        8 => "byte",
        16 => "word",
        32 => "dword",
        64 => "qword",
        128 => "xmmword",
        _ => unreachable!(),
    }
}
fn addresses() -> [&'static str; 9] {
    [
        "[rax]",
        "[rbp]",
        "[r13+r12*4-128]",
        "[r12+127]",
        "[r15+128]",
        "[rip+external+7]",
        "fs:[r8+8]",
        "gs:[r13-129]",
        "[rcx*8+4096]",
    ]
}
fn sources(bits: u16, registers: bool) -> Vec<String> {
    let mut src = if registers {
        (0..16).map(|r| format!("xmm{r}")).collect()
    } else {
        Vec::new()
    };
    src.extend(addresses().map(|a| format!("{} ptr {a}", size(bits))));
    src
}
fn cases(op: V2Opcode) -> Vec<String> {
    let s = op.spec();
    let mut out = Vec::new();
    match s.form {
        V2Form::Binary | V2Form::Memory | V2Form::Control | V2Form::Control64 | V2Form::Blend => {
            for dst in 0..16 {
                for src in sources(s.memory_bits, s.form != V2Form::Memory) {
                    let base = format!("{} xmm{dst},{src}", s.mnemonic);
                    if matches!(s.form, V2Form::Control | V2Form::Control64) {
                        for n in [0, 1, 15, 127, 128, 255] {
                            out.push(format!("{base},{n}"));
                        }
                    } else {
                        out.push(base.clone());
                        if s.form == V2Form::Blend {
                            out.push(format!("{base},xmm0"));
                        }
                    }
                }
            }
            if matches!(s.form, V2Form::Control | V2Form::Control64) {
                for n in 0..=255 {
                    out.push(format!("{} xmm15,xmm8,{n}", s.mnemonic));
                    out.push(format!(
                        "{} xmm8,{} ptr [rip+external+7],{n}",
                        s.mnemonic,
                        size(s.memory_bits)
                    ));
                }
            }
        }
        V2Form::Extract | V2Form::Insert => {
            for xmm in 0..16 {
                let rb = if s.memory_bits <= 16 {
                    32
                } else {
                    s.memory_bits
                };
                let mut integer = (0..16).map(|r| gpr(r, rb)).collect::<Vec<_>>();
                integer.extend(sources(s.memory_bits, false));
                for i in integer {
                    for n in [0, 1, 15, 127, 128, 255] {
                        out.push(if s.form == V2Form::Extract {
                            format!("{} {i},xmm{xmm},{n}", s.mnemonic)
                        } else {
                            format!("{} xmm{xmm},{i},{n}", s.mnemonic)
                        });
                    }
                }
            }
            let rb = if s.memory_bits <= 16 {
                32
            } else {
                s.memory_bits
            };
            for n in 0..=255 {
                for integer in [
                    gpr(15, rb),
                    format!("{} ptr [rip+external+7]", size(s.memory_bits)),
                ] {
                    out.push(if s.form == V2Form::Extract {
                        format!("{} {integer},xmm8,{n}", s.mnemonic)
                    } else {
                        format!("{} xmm8,{integer},{n}", s.mnemonic)
                    });
                }
            }
        }
        V2Form::Fisttp => {
            for bits in [16, 32, 64] {
                for a in addresses() {
                    out.push(format!("fisttp {} ptr {a}", size(bits)));
                }
            }
        }
        V2Form::Cx16 => {
            for a in addresses() {
                out.push(format!("cmpxchg16b xmmword ptr {a}"));
                out.push(format!("lock cmpxchg16b xmmword ptr {a}"));
            }
        }
        V2Form::Nullary => out.push(s.mnemonic.into()),
        V2Form::Crc32 => {
            for dst in 0..16 {
                for dst_bits in [32, 64] {
                    for bits in [8, 16, 32, 64] {
                        if dst_bits == 64 && !matches!(bits, 8 | 64) || dst_bits == 32 && bits == 64
                        {
                            continue;
                        }
                        let mut src = (0..16).map(|r| gpr(r, bits)).collect::<Vec<_>>();
                        if bits == 8 {
                            src.extend(["ah", "ch", "dh", "bh"].map(str::to_string));
                        }
                        src.extend(sources(bits, false));
                        for src in src {
                            // High-byte sources are encodable only with no REX prefix.
                            if ["ah", "ch", "dh", "bh"].contains(&src.as_str())
                                && (dst >= 8 || dst_bits == 64)
                            {
                                continue;
                            }
                            out.push(format!("crc32 {},{src}", gpr(dst, dst_bits)));
                        }
                    }
                }
            }
        }
    }
    out
}
fn gpr(n: usize, bits: u16) -> String {
    if n >= 8 {
        return format!(
            "r{n}{}",
            match bits {
                8 => "b",
                16 => "w",
                32 => "d",
                64 => "",
                _ => unreachable!(),
            }
        );
    }
    let names = match bits {
        8 => ["al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil"],
        16 => ["ax", "cx", "dx", "bx", "sp", "bp", "si", "di"],
        32 => ["eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi"],
        64 => ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"],
        _ => unreachable!(),
    };
    names[n].into()
}
fn section(bytes: &[u8]) -> (Vec<u8>, Vec<(u64, String, i64, String)>) {
    let f = object::File::parse(bytes).unwrap();
    let s = f.section_by_name(".text").unwrap();
    let rel = s
        .relocations()
        .map(|(off, r)| {
            let object::RelocationTarget::Symbol(idx) = r.target() else {
                panic!("symbol expected")
            };
            (
                off,
                format!("{:?}", r.flags()),
                r.addend(),
                f.symbol_by_index(idx).unwrap().name().unwrap().to_string(),
            )
        })
        .collect();
    (s.data().unwrap().to_vec(), rel)
}
fn run(c: &mut Command) {
    let r = c.output().unwrap();
    assert!(
        r.status.success(),
        "{c:?}: {}",
        String::from_utf8_lossy(&r.stderr)
            .lines()
            .take(12)
            .collect::<Vec<_>>()
            .join("\n")
    );
}
fn oracle(cases: &[String], dir: &Path) -> (usize, usize) {
    std::fs::create_dir_all(dir).unwrap();
    let source = format!(".intel_syntax noprefix\n.text\n{}\n", cases.join("\n"));
    let ours = assemble_object_with_features(&source, features()).unwrap();
    let mut typed = Program::new();
    let mut att_source = String::new();
    for case in cases {
        let i = Instruction::parse_intel(case).unwrap();
        assert_eq!(
            Instruction::parse_intel(&i.to_string()).unwrap(),
            i,
            "{case}"
        );
        att_source += &att(&i);
        att_source.push('\n');
        typed.x86_instruction(i);
    }
    assert_eq!(
        ours,
        assemble_program_object_with_features(&typed, features()).unwrap(),
        "direct/text objects"
    );
    let att_object =
        assemble_object_with_syntax_and_features(&att_source, Syntax::Att, features()).unwrap();
    assert_eq!(section(&ours), section(&att_object), "Intel/AT&T parity");
    std::fs::write(dir.join("att.s"), &att_source).unwrap();
    let input = dir.join("input.s");
    std::fs::write(&input, source).unwrap();
    std::fs::write(dir.join("keel.o"), &ours).unwrap();
    let actual = section(&ours);
    for (assembler, dialect) in [
        ("clang", Syntax::Intel),
        ("gnu", Syntax::Intel),
        ("clang", Syntax::Att),
        ("gnu", Syntax::Att),
    ] {
        if assembler == "clang" && cases.first().is_some_and(|c| {
            matches!(Instruction::parse_intel(c).unwrap().opcode, Opcode::V2(v) if v.spec().form == V2Form::Control64)
        }) { continue; } // LLVM exposes no opcode spelling that sets string-form REX.W.
        // LLVM chooses the SSE2 register encoding for PEXTRW and has no
        // spelling to force SSE4. GNU pextrw.s independently checks those
        // exact bytes. LLVM still checks every SSE4 memory destination.
        let reference_cases = cases
            .iter()
            .filter(|case| {
                assembler != "clang" || !case.starts_with("pextrw.s ") || case.contains('[')
            })
            .cloned()
            .collect::<Vec<_>>();
        let reference_source = reference_cases
            .iter()
            .map(|case| {
                let mut case = if dialect == Syntax::Att {
                    att(&Instruction::parse_intel(case).unwrap())
                } else {
                    case.clone()
                };
                for base in ["pcmpestri", "pcmpestrm", "pcmpistri", "pcmpistrm"] {
                    case = case.replace(&format!("{base}64 "), &format!("rex64 {base} "));
                }
                if assembler == "clang" {
                    case = case.replace("pextrw.s ", "pextrw ");
                }
                case
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            &input,
            format!(
                "{}\n.text\n{reference_source}\n",
                if dialect == Syntax::Att {
                    ".att_syntax prefix"
                } else {
                    ".intel_syntax noprefix"
                }
            ),
        )
        .unwrap();
        let actual = if reference_cases.len() == cases.len() {
            actual.clone()
        } else {
            section(
                &assemble_object_with_features(&reference_cases.join("\n"), features()).unwrap(),
            )
        };
        let output = dir.join(format!("{assembler}.o"));
        if assembler == "clang" {
            run(Command::new("clang")
                .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
                .arg(&input)
                .arg("-o")
                .arg(&output));
        } else {
            let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
            run(Command::new(gnu)
                .arg("--64")
                .arg(&input)
                .arg("-o")
                .arg(&output));
        }
        let expected = section(&std::fs::read(output).unwrap());
        assert_eq!(actual.0.len(), expected.0.len(), "{assembler} text size");
        assert_eq!(
            actual.0.iter().zip(&expected.0).position(|(a, b)| a != b),
            None,
            "{assembler} first byte mismatch"
        );
        assert_eq!(actual.1, expected.1, "{assembler} relocation tuples");
    }
    (actual.0.len(), actual.1.len())
}
#[test]
fn every_inventory_form_matches_clang_gnu_and_typed_text() {
    let root = std::env::var_os("KEEL_V2_ARTIFACTS").map(std::path::PathBuf::from);
    let dir = root
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join(format!("keel-v2-{}", std::process::id())));
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/v2-coverage/inventory.json")).unwrap();
    assert_eq!(
        inventory["instructions"].as_array().unwrap().len(),
        V2Opcode::ALL.len()
    );
    let mut report = Vec::new();
    for &op in V2Opcode::ALL {
        let s = op.spec();
        assert!(
            inventory["instructions"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["mnemonic"] == s.mnemonic)
        );
        let cases = cases(op);
        let (bytes, relocations) = oracle(&cases, &dir.join(s.mnemonic));
        report.push(serde_json::json!({"mnemonic":s.mnemonic,"family":s.family,"cases":cases.len(),"gnu_cases_per_dialect":cases.len(),"clang_cases_per_dialect":if s.form==V2Form::Control64 {0} else if s.mnemonic=="pextrw.s" {cases.iter().filter(|c|c.contains('[')).count()} else {cases.len()},"bytes":bytes,"relocations":relocations,"clang":if s.mnemonic=="pextrw.s" {"memory_forms_pass_register_force_unavailable"} else if s.form == V2Form::Control64 {"rex_w_force_unavailable"} else {"pass"},"gnu":"pass","typed_text":"pass","format_roundtrip":"pass","intel_att":"pass","native_semantics":"pending_parent_linux"}));
        eprintln!(
            "{}: {} forms, {bytes} bytes, {relocations} relocations",
            s.mnemonic,
            cases.len()
        );
    }
    for (name, family) in [
        ("roundss", "sse41"),
        ("roundsd", "sse41"),
        ("popcnt", "popcnt"),
    ] {
        let mut cases = Vec::new();
        if name == "popcnt" {
            for bits in [16, 32, 64] {
                for dst in 0..16 {
                    let mut src = (0..16).map(|r| gpr(r, bits)).collect::<Vec<_>>();
                    src.extend(sources(bits, false));
                    for source in src {
                        cases.push(format!("popcnt {},{source}", gpr(dst, bits)));
                    }
                }
            }
        } else {
            for dst in 0..16 {
                for source in sources(if name == "roundss" { 32 } else { 64 }, true) {
                    for n in [0, 1, 3, 4, 8, 15, 127, 128, 255] {
                        cases.push(format!("{name} xmm{dst},{source},{n}"));
                    }
                }
            }
        }
        let (bytes, relocations) = oracle(&cases, &dir.join(name));
        assert!(
            assemble_object_with_features(&cases[0], Features::for_level(CpuLevel::V1)).is_err()
        );
        report.push(serde_json::json!({"mnemonic":name,"family":family,"cases":cases.len(),"bytes":bytes,"relocations":relocations,"clang":"pass","gnu":"pass","typed_text":"pass","intel_att":"pass","native_semantics":"pending_parent_linux","existing_identity":true}));
    }
    std::fs::write(
        dir.join("encoding-results.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    if root.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn gates_and_invalid_forms_are_explicit() {
    for &op in V2Opcode::ALL {
        let s = op.spec();
        let case = cases(op).remove(0);
        assert!(
            assemble_object_with_features(&case, Features::for_level(CpuLevel::V1))
                .unwrap_err()
                .to_string()
                .contains("requires x86 target features"),
            "{case}"
        );
        assert!(op.required_features().names().eq([s.family]));
        for bad in [
            format!("{} ymm0,ymm1", s.mnemonic),
            format!("{} mm8,mm1", s.mnemonic),
            format!("{} xmm0,byte ptr [rax],0,1", s.mnemonic),
            format!("lock {case}"),
        ] {
            if s.form == V2Form::Cx16 && bad.starts_with("lock ") {
                continue;
            }
            reject(&bad);
        }
        if matches!(
            s.form,
            V2Form::Control | V2Form::Control64 | V2Form::Extract | V2Form::Insert
        ) {
            let example = cases(op).remove(0);
            let (base, _) = example.rsplit_once(',').unwrap();
            for n in ["-1", "256", "external", "al"] {
                reject(&format!("{base},{n}"));
            }
        }
        if matches!(
            s.form,
            V2Form::Binary | V2Form::Memory | V2Form::Control | V2Form::Control64 | V2Form::Blend
        ) {
            let imm = if matches!(s.form, V2Form::Control | V2Form::Control64) {
                ",0"
            } else {
                ""
            };
            for bits in [8, 16, 32, 64, 128] {
                if bits != s.memory_bits {
                    reject(&format!(
                        "{} xmm0,{} ptr [rax]{imm}",
                        s.mnemonic,
                        size(bits)
                    ));
                }
            }
            reject(&format!("{} [rax],xmm0{imm}", s.mnemonic));
        }
    }
    for bad in [
        "rex64 pshufb xmm0,xmm1",
        "rex64 lock pcmpestri xmm0,xmm1,0",
        "rex64 add eax,ecx",
        "crc32 rax,ah",
        "crc32 r8d,ah",
        "crc32 eax,rax",
        "crc32 rax,ecx",
        "crc32 rax,cx",
        "crc32 eax,[rax]",
        "crc32 ax,cl",
        "crc32 eax,xmm0",
        "pextrb al,xmm0,0",
        "pextrb rax,xmm0,0",
        "pinsrb xmm0,al,0",
        "pinsrd xmm0,rax,0",
        "pinsrq xmm0,eax,0",
        "cmpxchg16b rax",
        "cmpxchg16b qword ptr [rax]",
        "fisttp [rax]",
        "fisttp byte ptr [rax]",
        "lddqu xmm0,xmm1",
        "movntdqa xmm0,xmm1",
        "pblendvb xmm0,xmm1,xmm2",
        "blendvps xmm0,xmm1,0",
        "lahf 0",
        "sahf eax",
    ] {
        reject(bad);
    }
    // Construct one instruction without parsing; the typed identity is public.
    let mut p = Program::new();
    p.x86_instruction(Instruction {
        opcode: Opcode::V2(V2Opcode::Pshufb),
        operands: [
            Operand::Register(Register::parse("xmm15").unwrap()),
            Operand::Register(Register::parse("xmm8").unwrap()),
        ]
        .into_iter()
        .collect(),
        lock: false,
    });
    assert_eq!(
        section(&assemble_program_object_with_features(&p, features()).unwrap()).0,
        vec![0x66, 0x45, 0x0f, 0x38, 0x00, 0xf8]
    );
}
fn reject(s: &str) {
    assert!(
        assemble_object_with_features(s, features()).is_err(),
        "accepted {s}"
    );
    if let Ok(i) = Instruction::parse_intel(s) {
        let mut p = Program::new();
        p.x86_instruction(i);
        assert!(
            assemble_program_object_with_features(&p, features()).is_err(),
            "typed accepted {s}"
        );
    }
}
#[test]
fn att_forms_have_the_same_operands_bytes_and_relocations() {
    for (intel, att) in [
        (
            "pcmpestri64 xmm15,xmmword ptr [rip+external+7],255",
            "rex64 pcmpestri $255,external+7(%rip),%xmm15",
        ),
        ("pextrw word ptr [r12],xmm15,1", "pextrw $1,%xmm15,(%r12)"),
        ("pshufb xmm15,xmm8", "pshufb %xmm8,%xmm15"),
        (
            "palignr xmm9,xmmword ptr [rip+external+7],255",
            "palignr $255,external+7(%rip),%xmm9",
        ),
        ("pinsrb xmm15,eax,3", "pinsrb $3,%eax,%xmm15"),
        ("pextrq qword ptr [r12],xmm15,1", "pextrq $1,%xmm15,(%r12)"),
        ("blendvps xmm9,xmm8,xmm0", "blendvps %xmm0,%xmm8,%xmm9"),
        (
            "crc32 eax,byte ptr [rip+external+7]",
            "crc32b external+7(%rip),%eax",
        ),
        ("crc32 eax,word ptr [r12]", "crc32w (%r12),%eax"),
        ("crc32 r9,qword ptr [r12]", "crc32q (%r12),%r9"),
        ("fisttp word ptr [r12]", "fisttps (%r12)"),
        ("fisttp dword ptr [r12]", "fisttpl (%r12)"),
        ("fisttp qword ptr [r12]", "fisttpll (%r12)"),
        (
            "lock cmpxchg16b xmmword ptr [r12]",
            "lock cmpxchg16b (%r12)",
        ),
        ("lahf", "lahf"),
        ("sahf", "sahf"),
    ] {
        let a = assemble_object_with_features(intel, features()).unwrap();
        let b = assemble_object_with_syntax_and_features(att, Syntax::Att, features()).unwrap();
        assert_eq!(section(&a), section(&b), "{att}");
        assert_eq!(
            Instruction::parse_att(att).unwrap().opcode,
            Instruction::parse_intel(intel).unwrap().opcode
        );
    }
}

fn att(i: &Instruction) -> String {
    use keel_asm_ir::x86::{V2Opcode::*, write_expression};
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
        Opcode::Popcnt => format!(
            "popcnt{}",
            match &i.operands[0] {
                Operand::Register(r) => suffix(r.width),
                _ => unreachable!(),
            }
        ),
        Opcode::V2(Crc32) => format!(
            "crc32{}",
            suffix(match &i.operands[1] {
                Operand::Register(r) => r.width,
                Operand::Memory(m) => m.width,
                _ => unreachable!(),
            })
        ),
        Opcode::V2(Fisttp) => format!(
            "fisttp{}",
            match &i.operands[0] {
                Operand::Memory(m) => match m.width {
                    16 => "s",
                    32 => "l",
                    64 => "ll",
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            }
        ),
        _ => i.opcode.to_string(),
    };
    format!("{}{name} {operands}", if i.lock { "lock " } else { "" })
}
fn suffix(bits: u16) -> &'static str {
    match bits {
        8 => "b",
        16 => "w",
        32 => "l",
        64 => "q",
        _ => unreachable!(),
    }
}

fn mmx_cases(op: V2Opcode) -> Vec<String> {
    assert!(op.supports_mmx());
    let s = op.spec();
    let mut rows = Vec::new();
    for dst in 0..8 {
        let mut sources = (0..8).map(|r| format!("mm{r}")).collect::<Vec<_>>();
        sources.extend(addresses().map(|a| format!("qword ptr {a}")));
        sources.extend(
            [
                "qword ptr [external+11]",
                "qword ptr [rip+external@GOTPCREL+8]",
                "qword ptr fs:[tls@TPOFF+8]",
                "qword ptr [r13+external+8]",
            ]
            .map(str::to_owned),
        );
        for src in sources {
            let base = format!("{} mm{dst},{src}", s.mnemonic);
            if s.form == V2Form::Control {
                // Every immediate byte, every MMX register pair and address class.
                for n in 0..=255 {
                    rows.push(format!("{base},{n}"));
                }
            } else {
                rows.push(base);
            }
        }
    }
    rows
}

#[test]
fn ssse3_mmx_inventory_matches_clang_gnu_and_typed_text() {
    let root = std::env::var_os("KEEL_V2_MMX_ARTIFACTS").map(std::path::PathBuf::from);
    let dir = root.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!("keel-v2-mmx-{}", std::process::id()))
    });
    let inventory: serde_json::Value =
        serde_json::from_str(include_str!("../../../docs/v2-coverage/inventory.json")).unwrap();
    let mut report = Vec::new();
    for &op in V2Opcode::ALL.iter().filter(|op| op.supports_mmx()) {
        let s = op.spec();
        let entry = inventory["instructions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["mnemonic"] == s.mnemonic)
            .unwrap();
        assert_eq!(entry["mmx_encoding"]["prefix"], 0);
        assert_eq!(entry["mmx_encoding"]["memory_bits"], 64);
        assert_eq!(entry["mmx_encoding"]["map"], s.map);
        assert_eq!(entry["mmx_encoding"]["byte"], s.byte);
        assert_eq!(entry["operand_forms"].as_array().unwrap().len(), 4);
        let rows = mmx_cases(op);
        let (bytes, relocations) = oracle(&rows, &dir.join(s.mnemonic));
        report.push(serde_json::json!({"mnemonic":s.mnemonic,"family":"ssse3","register_family":"MMX","xed_forms":2,"cases":rows.len(),"bytes":bytes,"relocations":relocations,"clang":"pass","gnu":"pass","clang_cases_per_dialect":rows.len(),"gnu_cases_per_dialect":rows.len(),"typed_text":"pass","format_roundtrip":"pass","intel_att":"pass","native_semantics":"pending_parent_linux"}));
    }
    assert_eq!(report.len(), 16);
    std::fs::write(
        dir.join("mmx-encoding-results.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    if root.is_none() {
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn ssse3_mmx_feature_gates_and_malformed_operands() {
    use keel_asm_ir::x86::RegisterClass;
    let only_ssse3 = Features {
        ssse3: true,
        ..Features::NONE
    };
    for &op in V2Opcode::ALL.iter().filter(|op| op.supports_mmx()) {
        let name = op.spec().mnemonic;
        let suffix = if op.spec().form == V2Form::Control {
            ",7"
        } else {
            ""
        };
        assert!(op.required_features().names().eq(["ssse3"]));
        for src in ["mm1", "qword ptr [rip+external]"] {
            let s = format!("{name} mm0,{src}{suffix}");
            for f in [
                Features::NONE,
                Features::default(),
                Features {
                    ssse3: false,
                    ..features()
                },
                Features {
                    sse3: true,
                    ..Features::NONE
                },
                Features {
                    sse41: true,
                    ..Features::NONE
                },
            ] {
                let e = assemble_object_with_features(&s, f)
                    .unwrap_err()
                    .to_string();
                assert!(e.contains("requires x86 target features"), "{s}: {e}");
            }
            assert!(assemble_object_with_features(&s, only_ssse3).is_ok());
            let i = Instruction::parse_intel(&s).unwrap();
            assert_eq!(i.opcode, Opcode::V2(op));
            for f in [Features::NONE, Features::default()] {
                let mut p = Program::new();
                p.x86_instruction(i.clone());
                assert!(assemble_program_object_with_features(&p, f).is_err());
            }
        }
        for bad in [
            format!("{name} mm0,xmm1{suffix}"),
            format!("{name} xmm0,mm1{suffix}"),
            format!("{name} mm8,mm1{suffix}"),
            format!("{name} mm0,mm8{suffix}"),
            format!("{name} rax,mm1{suffix}"),
            format!("{name} [rax],mm1{suffix}"),
            format!("{name} mm0,[mm1]{suffix}"),
            format!("{name} mm0,[rax+rcx*3]{suffix}"),
            format!("{name} mm0,[rip+rcx]{suffix}"),
            format!("{name} mm0,[rip+external@GOTTPOFF]{suffix}"),
            format!("lock {name} mm0,mm1{suffix}"),
            format!("{name} mm0,mm1{suffix},0"),
        ] {
            assert!(
                assemble_object_with_features(&bad, only_ssse3).is_err(),
                "{bad}"
            );
        }
        for bits in [8, 16, 32, 128] {
            let bad = format!("{name} mm0,{} ptr [rax]{suffix}", size(bits));
            reject(&bad);
        }
        if op.spec().form == V2Form::Control {
            for control in ["-1", "256", "external", "rax"] {
                reject(&format!("{name} mm0,mm1,{control}"));
            }
            reject(&format!("{name} mm0,mm1"));
        }
        let original = Instruction::parse_intel(&format!("{name} mm0,mm1{suffix}")).unwrap();
        for (number, width, high_byte) in [(8, 64, false), (0, 128, false), (0, 64, true)] {
            let mut i = original.clone();
            i.operands[0] = Operand::Register(Register {
                class: RegisterClass::Mmx,
                number,
                width,
                high_byte,
            });
            let mut p = Program::new();
            p.x86_instruction(i);
            assert!(assemble_program_object_with_features(&p, only_ssse3).is_err());
        }
    }
    // Every other v2 identity must continue to reject MMX operands.
    for &op in V2Opcode::ALL.iter().filter(|op| !op.supports_mmx()) {
        let suffix = if matches!(
            op.spec().form,
            V2Form::Control | V2Form::Control64 | V2Form::Insert | V2Form::Extract
        ) {
            ",7"
        } else {
            ""
        };
        reject(&format!("{} mm0,mm1{suffix}", op.spec().mnemonic));
    }
}

#[test]
fn ssse3_mmx_malformed_operands_match_independent_assemblers() {
    let dir = std::env::temp_dir().join(format!("keel-v2-mmx-invalid-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("invalid.s");
    let output = dir.join("invalid.o");
    for &op in V2Opcode::ALL.iter().filter(|op| op.supports_mmx()) {
        let name = op.spec().mnemonic;
        let suffix = if op.spec().form == V2Form::Control {
            ",7"
        } else {
            ""
        };
        for bad in [
            format!("{name} mm0,xmm1{suffix}"),
            format!("{name} xmm0,mm1{suffix}"),
            format!("{name} mm0,dword ptr [rax]{suffix}"),
            format!("{name} mm0,xmmword ptr [rax]{suffix}"),
            format!("{name} [rax],mm1{suffix}"),
        ] {
            reject(&bad);
            std::fs::write(&input, format!(".intel_syntax noprefix\n{bad}\n")).unwrap();
            let mut clang = Command::new("clang");
            clang.args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-c",
                "-x",
                "assembler",
            ]);
            let mut gnu =
                Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            gnu.arg("--64");
            for mut command in [clang, gnu] {
                let r = command.arg(&input).arg("-o").arg(&output).output().unwrap();
                assert!(
                    !r.status.success(),
                    "independent assembler accepted {bad}: {command:?}"
                );
            }
        }
        let att = format!(
            "{name} {}%xmm1,%mm0",
            if suffix.is_empty() { "" } else { "$7," }
        );
        assert!(assemble_object_with_syntax_and_features(&att, Syntax::Att, features()).is_err());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
