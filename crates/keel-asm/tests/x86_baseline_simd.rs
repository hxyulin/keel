#[path = "x86_baseline_simd/x86_packed.rs"]
mod x86_packed;
#[path = "x86_baseline_simd/x86_v1_mxcsr.rs"]
mod x86_v1_mxcsr;
use keel_asm::x86::{assemble_object, assemble_program_object_with_features};
use keel_asm_ir::x86::{Features, Instruction, Operand, Register, RegisterClass};
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
    let source = format!(".intel_syntax noprefix\n.text\n{}\n", rows.join("\n"));
    let own = assemble_object(&source).unwrap();
    let mut p = keel_asm_ir::Program::new();
    for row in &rows {
        let i = Instruction::parse_intel(row).unwrap();
        assert_eq!(
            Instruction::parse_intel(&i.to_string()).unwrap(),
            i,
            "{row}"
        );
        assert_eq!(i.opcode.required_features(), Features::NONE);
        p.x86_instruction(i);
    }
    let typed = assemble_program_object_with_features(&p, Features::NONE).unwrap();
    assert_eq!(own, typed, "text/direct parity");
    let dir = std::env::temp_dir().join(format!("keel-baseline-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.s");
    std::fs::write(&input, &source).unwrap();
    for (name, mut c) in [
        ("clang", {
            let mut c = Command::new("clang");
            c.args([
                "-target",
                "x86_64-unknown-linux-gnu",
                "-c",
                "-x",
                "assembler",
            ]);
            c
        }),
        ("gnu", {
            let mut c = Command::new(std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into()));
            c.arg("--64");
            c
        }),
    ] {
        let out = dir.join(format!("{name}.o"));
        let r = c.arg(&input).arg("-o").arg(&out).output().unwrap();
        assert!(
            r.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let b = std::fs::read(out).unwrap();
        assert_eq!(text(&own), text(&b), "{name} bytes");
        let a = relocs(&own);
        let b = relocs(&b);
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(&b) {
            assert_eq!(a, b, "{name} relocation");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn mmx_and_sse2_packed_forms() {
    let mut rows = Vec::new();
    for op in [
        "packssdw",
        "packsswb",
        "packuswb",
        "paddb",
        "paddw",
        "paddd",
        "paddq",
        "paddsb",
        "paddsw",
        "paddusb",
        "paddusw",
        "psubb",
        "psubw",
        "psubd",
        "psubq",
        "psubsb",
        "psubsw",
        "psubusb",
        "psubusw",
        "pavgb",
        "pavgw",
        "pmaxsw",
        "pmaxub",
        "pminsw",
        "pminub",
        "psadbw",
        "pand",
        "pandn",
        "por",
        "pxor",
        "pcmpeqb",
        "pcmpeqw",
        "pcmpeqd",
        "pcmpgtb",
        "pcmpgtw",
        "pcmpgtd",
        "pmaddwd",
        "pmulhuw",
        "pmulhw",
        "pmullw",
        "pmuludq",
        "punpcklbw",
        "punpcklwd",
        "punpckldq",
        "punpckhbw",
        "punpckhwd",
        "punpckhdq",
    ] {
        for (class, n, size) in [("mm", 8, "qword"), ("xmm", 16, "xmmword")] {
            for a in 0..n {
                for b in 0..n {
                    rows.push(format!("{op} {class}{a}, {class}{b}"));
                }
                // Low MMX unpack memory operands consume 32 bits (XED MEM0:r:d).
                let size = if class == "mm" && ["punpcklbw", "punpcklwd", "punpckldq"].contains(&op)
                {
                    "dword"
                } else {
                    size
                };
                for addr in [
                    "[r13+r12*4-128]",
                    "[rip+external+7]",
                    "fs:[r8+128]",
                    "[rcx*8+4096]",
                ] {
                    rows.push(format!("{op} {class}{a}, {size} ptr {addr}"));
                }
            }
        }
        for bad in [
            format!("{op} mm0, xmm0"),
            format!("{op} xmm0, mm0"),
            format!("{op} mm0, word ptr [rax]"),
            format!("{op} [rax], mm0"),
        ] {
            assert!(assemble_object(&bad).is_err(), "{bad}");
        }
    }
    for op in [
        "psllw", "pslld", "psllq", "psrlw", "psrld", "psrlq", "psraw", "psrad",
    ] {
        for a in 0..8 {
            for b in 0..8 {
                rows.push(format!("{op} mm{a}, mm{b}"));
            }
            rows.push(format!("{op} mm{a}, qword ptr [rip+external]"));
            for k in [0, 1, 63, 255] {
                rows.push(format!("{op} mm{a}, {k}"));
            }
        }
    }
    for a in 0..8 {
        rows.extend([
            format!("pshufw mm{a}, mm7, 255"),
            format!("pshufw mm{a}, qword ptr [rip+external], 3"),
            format!("pmovmskb r9d, mm{a}"),
        ]);
        for b in 0..8 {
            rows.push(format!("movq mm{a}, mm{b}"));
        }
        for r in ["eax", "r9d", "rax", "r15"] {
            let op = if r == "eax" || r == "r9d" {
                "movd"
            } else {
                "movq"
            };
            rows.push(format!("{op} mm{a}, {r}"));
            rows.push(format!("{op} {r}, mm{a}"));
        }
        rows.extend([
            format!("movq mm{a}, qword ptr [rip+external]"),
            format!("movq qword ptr [r12], mm{a}"),
            format!("movd mm{a}, dword ptr [r13]"),
            format!("movd dword ptr [rip+external], mm{a}"),
        ]);
    }
    for class in ["mm", "xmm"] {
        for r in ["eax", "r9d", "rax", "r15"] {
            rows.push(format!("pextrw {r}, {class}7, 255"));
            rows.push(format!("pinsrw {class}7, {r}, 255"));
        }
        rows.push(format!("pinsrw {class}7, word ptr [rip+external], 3"));
    }
    matrix(rows, "packed");
}
#[test]
fn mmx_typed_and_parsed_rejections() {
    for n in 0..8 {
        let r = Register::parse(&format!("MM{n}")).unwrap();
        assert_eq!(r.class, RegisterClass::Mmx);
        assert_eq!(r.width, 64);
    }
    for s in ["mm8", "mm16", "mm01", "mm-1"] {
        assert!(Register::parse(s).is_none());
    }
    for bad in [
        "paddb mm8, mm0",
        "paddb mm0, mm1, 1",
        "psllw mm0, 256",
        "pshufw mm0, mm1, -1",
        "movd mm0, xmm0",
        "movq mm0, xmm0",
        "pinsrw mm0, ax, 0",
        "pextrw word ptr [rax], mm0, 0",
        "paddb mm0, [mm1]",
    ] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
    for (number, width, high_byte) in [(8, 64, false), (0, 128, false), (0, 64, true)] {
        let mut i = Instruction::parse_intel("paddb mm0, mm1").unwrap();
        i.operands[0] = Operand::Register(Register {
            class: RegisterClass::Mmx,
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
fn baseline_scalar_conversions_moves_and_state() {
    let mut rows = Vec::new();
    for (op, size) in [
        ("comiss", "dword"),
        ("comisd", "qword"),
        ("rcpps", "xmmword"),
        ("rcpss", "dword"),
        ("rsqrtps", "xmmword"),
        ("rsqrtss", "dword"),
    ] {
        for a in 0..16 {
            for b in 0..16 {
                rows.push(format!("{op} xmm{a}, xmm{b}"));
            }
            for addr in ["[r13+r12*4-128]", "[rip+external+7]", "gs:[r9+128]"] {
                rows.push(format!("{op} xmm{a}, {size} ptr {addr}"));
            }
        }
    }
    for suffix in ["ps", "pd", "ss", "sd"] {
        let size = if suffix == "ss" {
            "dword"
        } else if suffix == "sd" {
            "qword"
        } else {
            "xmmword"
        };
        for pred in ["eq", "lt", "le", "unord", "neq", "nlt", "nle", "ord"] {
            for a in [0, 7, 8, 15] {
                for b in [0, 7, 8, 15] {
                    rows.push(format!("cmp{pred}{suffix} xmm{a}, xmm{b}"));
                }
                rows.push(format!(
                    "cmp{pred}{suffix} xmm{a}, {size} ptr [rip+external+5]"
                ));
            }
        }
    }
    for (op, size) in [("cvtss2si", "dword"), ("cvtsd2si", "qword")] {
        for r in ["eax", "r9d", "rax", "r15"] {
            for b in 0..16 {
                rows.push(format!("{op} {r}, xmm{b}"));
            }
            rows.push(format!("{op} {r}, {size} ptr [rip+external]"));
        }
    }
    for op in ["cvtpi2ps", "cvtpi2pd"] {
        for a in 0..16 {
            for b in 0..8 {
                rows.push(format!("{op} xmm{a}, mm{b}"));
            }
            rows.push(format!("{op} xmm{a}, qword ptr [rip+external]"));
        }
    }
    for (op, size) in [
        ("cvtps2pi", "qword"),
        ("cvttps2pi", "qword"),
        ("cvtpd2pi", "xmmword"),
        ("cvttpd2pi", "xmmword"),
    ] {
        for a in 0..8 {
            for b in 0..16 {
                rows.push(format!("{op} mm{a}, xmm{b}"));
            }
            rows.push(format!("{op} mm{a}, {size} ptr [rip+external]"));
        }
    }
    for a in 0..16 {
        for op in ["movhps", "movlps", "movhpd", "movlpd"] {
            rows.push(format!("{op} xmm{a}, qword ptr [rip+external]"));
            rows.push(format!("{op} qword ptr [r13+r12*4], xmm{a}"));
        }
        for b in 0..16 {
            rows.push(format!("movlhps xmm{a}, xmm{b}"));
            rows.push(format!("maskmovdqu xmm{a}, xmm{b}"));
        }
        for b in 0..8 {
            rows.push(format!("movdq2q mm{b}, xmm{a}"));
            rows.push(format!("movq2dq xmm{a}, mm{b}"));
        }
        for op in ["movntps", "movntpd", "movntdq"] {
            rows.push(format!("{op} xmmword ptr [rip+external], xmm{a}"));
        }
        rows.push(format!("movq xmm{a}, qword ptr [rip+external]"));
        rows.push(format!("movq qword ptr [r13+r12*4], xmm{a}"));
    }
    for a in 0..8 {
        for b in 0..8 {
            rows.push(format!("maskmovq mm{a}, mm{b}"));
        }
        rows.push(format!("movntq qword ptr [rip+external], mm{a}"));
    }
    for r in ["eax", "r9d", "rax", "r15"] {
        let size = if r == "eax" || r == "r9d" {
            "dword"
        } else {
            "qword"
        };
        rows.push(format!("movnti {size} ptr [rip+external], {r}"));
    }
    for op in [
        "fxsave",
        "fxrstor",
        "fxsave64",
        "fxrstor64",
        "prefetchnta",
        "prefetcht0",
        "prefetcht1",
        "prefetcht2",
    ] {
        for addr in [
            "[r13+r12*4-128]",
            "[rip+external+7]",
            "fs:[r9+128]",
            "[rcx*8+4096]",
        ] {
            rows.push(format!("{op} {addr}"));
        }
    }
    matrix(rows, "extras");
    for bad in [
        "comiss xmm0, qword ptr [rax]",
        "cvtss2si ax, xmm0",
        "cvtsd2si rax, dword ptr [rax]",
        "cvtps2pi mm0, dword ptr [rax]",
        "cvtpi2pd xmm0, xmm1",
        "movq2dq mm0, xmm1",
        "movdq2q mm0, [rax]",
        "movhps xmm0, xmm1",
        "movlpd xmm0, xmm1",
        "movntdq xmm0, [rax]",
        "movntq [rax], xmm0",
        "movnti [rax], ax",
        "maskmovq mm0, [rax]",
        "maskmovdqu xmm0, mm0",
        "fxsave rax",
        "fxrstor qword ptr [rax]",
        "prefetcht0 xmm0",
        "prefetchnta qword ptr [rax]",
    ] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
}

#[test]
fn baseline_att_mmx_and_conversion_aliases() {
    for (att, intel) in [
        ("movq %mm1, %mm0", "movq mm0, mm1"),
        ("movq %rax, %mm0", "movq mm0, rax"),
        ("pinsrw $3, %eax, %mm0", "pinsrw mm0, eax, 3"),
        ("pextrw $3, %mm0, %eax", "pextrw eax, mm0, 3"),
        ("cvtss2siq %xmm1, %r9", "cvtss2si r9, xmm1"),
        ("cvtsd2sil (%rax), %r9d", "cvtsd2si r9d, [rax]"),
    ] {
        assert_eq!(
            Instruction::parse_att(att).unwrap(),
            Instruction::parse_intel(intel).unwrap(),
            "{att}"
        );
    }
}

#[test]
fn baseline_relocations_and_invalid_encoding_selectors() {
    matrix(
        vec![
            "pshufw mm7, qword ptr [rip+external@GOTPCREL+8], 255".into(),
            "pinsrw mm7, word ptr [rip+external@GOTPCREL], 255".into(),
            "paddusb xmm15, xmmword ptr fs:[external@TPOFF+8]".into(),
            "movq mm7, qword ptr [external+8]".into(),
            "movntdq xmmword ptr [r13+external+8], xmm15".into(),
            "cmpeqss xmm15, dword ptr [rip+external@GOTPCREL], 255".replace(", 255", ""),
        ],
        "relocations",
    );
    for bad in [
        "movq.6e xmm0, xmm1",
        "movq.7e mm0, xmm1",
        "movq.d6 mm0, mm1",
        "movq.7f xmm0, xmm1",
        "movss.store xmm0, mm0",
        "movsd.store dword ptr [rax], xmm0",
        "pshufw mm0, [rip+external@TPOFF], 7",
        "pinsrw mm0, [rip+external@PLT], 0",
        "pinsrw mm0, [rip+external@GOTTPOFF], 0",
        "pshufw mm0, mm1, external",
        "cmpeqps xmm0, xmm1, 0",
        "cmpps xmm0, xmm1, 256",
        "psllw mm0, mm1, 0",
        "movq mm0, qword ptr [xmm1]",
        "movntq [rax+rip], mm0",
    ] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
}

#[test]
fn pextrw_baseline_and_sse4_form_routing() {
    use keel_asm::x86::assemble_object_with_features;
    use keel_asm_ir::x86::{CpuLevel, Opcode, V2Opcode};
    for s in ["pextrw eax, xmm15, 3", "pextrw r9d, mm7, 3"] {
        assert_eq!(Instruction::parse_intel(s).unwrap().opcode, Opcode::Pextrw);
        assert!(assemble_object_with_features(s, Features::NONE).is_ok());
    }
    let memory = "pextrw word ptr [rip+external], xmm15, 3";
    let i = Instruction::parse_intel(memory).unwrap();
    assert_eq!(i.opcode, Opcode::V2(V2Opcode::PextrwSse4));
    assert_eq!(
        Instruction::parse_att("pextrw $3, %xmm15, external(%rip)")
            .unwrap()
            .opcode,
        i.opcode
    );
    assert!(assemble_object_with_features(memory, Features::NONE).is_err());
    assert!(assemble_object_with_features(memory, Features::for_level(CpuLevel::V2)).is_ok());
    let explicit = "pextrw.s eax, xmm15, 3";
    assert_eq!(Instruction::parse_intel(explicit).unwrap().opcode, i.opcode);
    assert!(assemble_object_with_features(explicit, Features::NONE).is_err());
    assert!(assemble_object_with_features(explicit, Features::for_level(CpuLevel::V2)).is_ok());
}

#[test]
fn xed_negative_typed_operand_shapes() {
    for s in [
        "movq mm0,mm1",
        "paddb mm0,[rax]",
        "cvtdq2ps xmm0,xmm1",
        "pshufd xmm0,xmm1,3",
        "movaps xmm0,[rax]",
    ] {
        let i = Instruction::parse_intel(s).unwrap();
        assert!(assemble_object(&format!("lock {s}")).is_err(), "LOCK: {s}");
        for (n, operand) in i.operands.iter().enumerate() {
            let mut bad = i.clone();
            match &mut bad.operands[n] {
                Operand::Register(r) => r.width += 1,
                Operand::Memory(m) => m.width = if m.width == 0 { 16 } else { 8 },
                Operand::Immediate(a) => a.expression.constant = 256,
            }
            let mut p = keel_asm_ir::Program::new();
            p.x86_instruction(bad);
            assert!(
                assemble_program_object_with_features(&p, Features::NONE).is_err(),
                "invalid operand {n}: {s}"
            );
            if matches!(operand, Operand::Memory(_)) {
                let mut bad = i.clone();
                let Operand::Memory(m) = &mut bad.operands[n] else {
                    unreachable!()
                };
                m.base = Some(Register::parse("mm0").unwrap());
                let mut p = keel_asm_ir::Program::new();
                p.x86_instruction(bad);
                assert!(
                    assemble_program_object_with_features(&p, Features::NONE).is_err(),
                    "MMX address: {s}"
                );
            }
        }
    }
}
