//! Bounded instruction forms observed in intact Clang compiler output.
#[path = "x86_compiler_output/x86_x87_full.rs"]
mod x86_x87_full;
use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::x86::{Instruction, parse_program};
use object::{Object, ObjectSection, ObjectSymbol, RelocationTarget};
use std::{
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "keel-compiler-output-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn text(bytes: &[u8]) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
fn relocations(bytes: &[u8]) -> Vec<(u64, object::RelocationFlags, i64, String)> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(".text")
        .unwrap()
        .relocations()
        .map(|(offset, r)| {
            let target = match r.target() {
                RelocationTarget::Symbol(index) => file
                    .symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_owned(),
                other => panic!("unexpected relocation target {other:?}"),
            };
            (offset, r.flags(), r.addend(), target)
        })
        .collect()
}
fn gnu() -> Option<String> {
    let candidates = if let Ok(tool) = std::env::var("KEEL_GNU_AS") {
        vec![tool]
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        vec!["as".into()]
    } else {
        vec!["x86_64-elf-as".into(), "x86_64-linux-gnu-as".into()]
    };
    candidates.into_iter().find(|tool| {
        Command::new(tool).arg("--version").output().is_ok_and(|p| {
            p.status.success() && String::from_utf8_lossy(&p.stdout).contains("GNU assembler")
        })
    })
}
fn reference(source: &str, gnu: Option<&str>) -> Vec<u8> {
    let scratch = Scratch::new();
    let input = scratch.0.join("input.s");
    let output = scratch.0.join("reference.o");
    std::fs::write(&input, source).unwrap();
    let mut cmd = Command::new(gnu.unwrap_or("clang"));
    if gnu.is_some() {
        cmd.arg("--64");
    } else {
        cmd.args([
            "-c",
            "-target",
            "x86_64-unknown-linux-gnu",
            "-Wa,-mrelax-relocations=no",
        ]);
    }
    let result = cmd.arg(&input).arg("-o").arg(&output).output().unwrap();
    assert!(
        result.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(output).unwrap()
}
fn differential(intel: &str, att: &str) {
    let intel = format!(".intel_syntax noprefix\n.text\n{intel}\n");
    let att = format!(".att_syntax prefix\n.text\n{att}\n");
    let ours = assemble_object(&intel).unwrap_or_else(|e| panic!("{intel}\n{e}"));
    assert_eq!(
        ours,
        assemble_object(&att).unwrap_or_else(|e| panic!("{att}\n{e}"))
    );
    for source in [&intel, &att] {
        let program = parse_program(source).unwrap();
        assert_eq!(assemble_program_object(&program).unwrap(), ours);
        for item in &program.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), *i);
            }
        }
        let mut references = vec![reference(source, None)];
        if let Some(tool) = gnu() {
            references.push(reference(source, Some(&tool)));
        }
        for expected in references {
            assert_eq!(text(&ours), text(&expected), "{source}");
            assert_eq!(relocations(&ours), relocations(&expected), "{source}");
        }
    }
}

#[test]
fn evidenced_scalar_compare_aliases_match_both_dialects() {
    let mut intel = Vec::new();
    let mut att = Vec::new();
    for (alias, predicate) in [
        ("cmpeqsd", 0),
        ("cmplesd", 2),
        ("cmpneqsd", 4),
        ("cmpordsd", 7),
    ] {
        for dst in [0, 8, 15] {
            for (src_i, src_a) in [
                ("xmm13", "%xmm13"),
                ("qword ptr [r13+r12*4-128]", "-128(%r13,%r12,4)"),
                ("qword ptr fs:[r14*8+4096]", "%fs:4096(,%r14,8)"),
                ("qword ptr [rip+external+19]", "external+19(%rip)"),
                ("[r15+128]", "128(%r15)"),
            ] {
                let i = format!("{alias} xmm{dst}, {src_i}");
                let explicit = format!("cmpsd xmm{dst}, {src_i}, {predicate}");
                assert_eq!(
                    assemble_object(&i).unwrap(),
                    assemble_object(&explicit).unwrap()
                );
                intel.push(i);
                att.push(format!("{alias} {src_a}, %xmm{dst}"));
            }
        }
    }
    differential(&intel.join("\n"), &att.join("\n"));
}

#[test]
fn scalar_compare_aliases_reject_invalid_operands() {
    for alias in ["cmpeqsd", "cmplesd", "cmpneqsd", "cmpordsd"] {
        for operands in [
            "",
            "xmm0",
            "xmm0, dword ptr [rax]",
            "xmm0, tbyte ptr [rax]",
            "xmm0, xmm1, 2",
            "rax, xmm0",
            "qword ptr [rax], xmm0",
            "xmm0, 1",
        ] {
            let bad = format!("{alias} {operands}");
            assert!(assemble_object(&bad).is_err(), "{bad}");
        }
        assert!(assemble_object(&format!("lock {alias} xmm0, xmm1")).is_err());
        for operands in ["%eax, %xmm0", "$1, %xmm0", "%xmm0", "$2, %xmm1, %xmm0"] {
            let bad = format!(".att_syntax prefix\n{alias} {operands}");
            assert!(assemble_object(&bad).is_err(), "{bad}");
        }
    }
}

#[test]
fn x87_stack_operands_and_pop_subtraction_dialects_match_clang() {
    let mut intel = Vec::new();
    let mut att = Vec::new();
    for n in 0..8 {
        for op in [
            "fld", "fstp", "fxch", "fucomi", "fucomip", "fucompi", "fadd", "fmul", "fmulp",
        ] {
            intel.push(format!("{op} st({n})"));
            att.push(format!("{op} %st({n})"));
        }
        for op in ["fucomi", "fucomip", "fucompi", "fcmovnb", "fadd", "fmul"] {
            intel.push(format!("{op} st, st({n})"));
            att.push(format!("{op} %st({n}), %st"));
        }
        for (op_i, op_a) in [
            ("fmulp", "fmulp"),
            ("fsubp", "fsubrp"),
            ("fsubrp", "fsubp"),
            ("fadd", "fadd"),
            ("fmul", "fmul"),
        ] {
            intel.push(format!("{op_i} st({n}), st"));
            att.push(format!("{op_a} %st, %st({n})"));
            if op_i.starts_with("fsub") {
                intel.push(format!("{op_i} st({n})"));
                att.push(format!("{op_a} %st({n})"));
            }
        }
    }
    for (op_i, op_a) in [
        ("fxch", "fxch"),
        ("fucomi", "fucomi"),
        ("fucomip", "fucomip"),
        ("fucompi", "fucompi"),
        ("fmulp", "fmulp"),
        ("fsubp", "fsubrp"),
        ("fsubrp", "fsubp"),
        ("fldz", "fldz"),
    ] {
        intel.push(op_i.into());
        att.push(op_a.into());
    }
    differential(&intel.join("\n"), &att.join("\n"));
    // Register-prefix modes still apply to ST names, including ST(0)'s alias.
    let reference = assemble_object("fld st\nfucompi st, st(7)\nfstp st").unwrap();
    for source in [
        ".intel_syntax prefix\nfld %st\nfucompi %st, %st(7)\nfstp %st",
        ".att_syntax noprefix\nfld st\nfucompi st(7), st\nfstp st",
    ] {
        assert_eq!(reference, assemble_object(source).unwrap());
    }
}

#[test]
fn x87_explicit_memory_widths_addresses_and_relocations_match_clang() {
    let mut intel = Vec::new();
    let mut att = Vec::new();
    for (op, sizes) in [
        ("fld", &[("dword", "s"), ("qword", "l"), ("tbyte", "t")][..]),
        (
            "fstp",
            &[("dword", "s"), ("qword", "l"), ("tbyte", "t")][..],
        ),
        ("fadd", &[("dword", "s"), ("qword", "l")][..]),
        ("fmul", &[("dword", "s"), ("qword", "l")][..]),
        (
            "fild",
            &[("word", "s"), ("dword", "l"), ("qword", "ll")][..],
        ),
        (
            "fistp",
            &[("word", "s"), ("dword", "l"), ("qword", "ll")][..],
        ),
        ("fldcw", &[("word", "")][..]),
        ("fnstcw", &[("word", "")][..]),
    ] {
        for (size, suffix) in sizes {
            for (i, a) in [
                ("[rax]", "(%rax)"),
                ("[r13]", "(%r13)"),
                ("[r13+r12*4-128]", "-128(%r13,%r12,4)"),
                ("[rsp+128]", "128(%rsp)"),
                ("fs:[r14*8+4096]", "%fs:4096(,%r14,8)"),
                ("gs:[r15+127]", "%gs:127(%r15)"),
                ("[rip+external+19]", "external+19(%rip)"),
                ("[rip+external@GOTPCREL]", "external@GOTPCREL(%rip)"),
                ("[external+7]", "external+7"),
            ] {
                intel.push(format!("{op} {size} ptr {i}"));
                att.push(format!("{op}{suffix} {a}"));
            }
        }
    }
    differential(&intel.join("\n"), &att.join("\n"));
}

#[test]
fn x87_rejects_invalid_operands_and_typed_registers() {
    for bad in [
        "fld st(8)",
        "fld st(-1)",
        "fld st(00)",
        "fld xmm0",
        "fstp rax",
        "fucomi st(2), st(1)",
        "fucompi st(1), st",
        "fucomip qword ptr [rax]",
        "fxch st, st(1)",
        "fmulp st, st(2)",
        "fsubp st, st(2)",
        "fsubrp st, st(2)",
        "fadd st(2), st(3)",
        "fmul st(2), st(3)",
        "fcmovnb st(1), st",
        "fld qword ptr [rax], st",
        "fstp st, st(1)",
        "fldz st",
        "fild tbyte ptr [rax]",
        "fistp byte ptr [rax]",
        "fadd tbyte ptr [rax]",
        "fmul word ptr [rax]",
        "fnstcw qword ptr [rax]",
        "fldcw dword ptr [rax]",
        "fld [rax]",
        "fstp [rax]",
        "fild [rax]",
        "fistp [rax]",
        "fadd [rax]",
        "fmul [rax]",
        "fldcw [rax]",
        "fnstcw [rax]",
        "lock fxch st(1)",
        "fld qword ptr [st(0)]",
        "mov rax, st(0)",
        "add st, st(1)",
    ] {
        assert!(assemble_object(bad).is_err(), "{bad}");
    }
    for bad in [
        "fldl %st",
        "fstpt %st(1)",
        "fildll %rax",
        "fistps $1",
        "fldcw %st",
        "faddl %st(1), %st",
        "flds (%st)",
        "fucomi %st, %st(1)",
        "fmulp %st(2), %st",
        "fcmovnb %st, %st(1)",
    ] {
        assert!(
            assemble_object(&format!(".att_syntax prefix\n{bad}")).is_err(),
            "{bad}"
        );
    }
    for (number, width, high_byte) in [(8, 80, false), (0, 64, false), (0, 80, true)] {
        let mut instruction = Instruction::parse_intel("fld st(0)").unwrap();
        let keel_asm_ir::x86::Operand::Register(r) = &mut instruction.operands[0] else {
            unreachable!()
        };
        r.number = number;
        r.width = width;
        r.high_byte = high_byte;
        let mut program = keel_asm_ir::Program::new();
        program.x86_instruction(instruction);
        assert!(assemble_program_object(&program).is_err());
    }
    // A typed caller must not smuggle a stack register into an address either.
    let mut instruction = Instruction::parse_intel("fld qword ptr [rax]").unwrap();
    let keel_asm_ir::x86::Operand::Memory(m) = &mut instruction.operands[0] else {
        unreachable!()
    };
    m.base = keel_asm_ir::x86::Register::parse("st(0)");
    let mut program = keel_asm_ir::Program::new();
    program.x86_instruction(instruction);
    assert!(assemble_program_object(&program).is_err());
    for unsupported in ["fsin st", "fdiv st(2), st(3)", ".unknown_x87_directive"] {
        let error = assemble_object(&format!("nop\n{unsupported}\nret")).unwrap_err();
        assert_eq!(error.line, 2, "{error}");
    }
}

#[test]
fn clang_also_rejects_invalid_x87_register_and_width_forms() {
    let scratch = Scratch::new();
    let input = scratch.0.join("invalid.s");
    let output = scratch.0.join("invalid.o");
    for (directive, cases) in [
        (
            ".intel_syntax noprefix",
            vec![
                "fld st(8)",
                "fucomi st(2),st(1)",
                "fxch st,st(1)",
                "fmulp st(2),st(3)",
                "fsubrp st(2),st(3)",
                "fadd st(2),st(3)",
                "fcmovnb st(1),st",
                "fild tbyte ptr [rax]",
                "fistp byte ptr [rax]",
                "fldcw dword ptr [rax]",
                "fnstcw qword ptr [rax]",
            ],
        ),
        (
            ".att_syntax prefix",
            vec![
                "fldl %st",
                "fstpt %st(1)",
                "fildll %rax",
                "fistps $1",
                "fucomi %st,%st(1)",
                "fmulp %st(2),%st(3)",
                "fcmovnb %st,%st(1)",
            ],
        ),
    ] {
        for bad in &cases {
            assert!(assemble_object(&format!("{directive}\n{bad}\n")).is_err());
        }
        // Each assembler keeps going after an error, so one run per dialect
        // must report every line. GNU rewrites `fldl %st` to `fld %st` with a
        // warning; it must still never accept one of these forms silently.
        std::fs::write(&input, format!("{directive}\n{}\n", cases.join("\n"))).unwrap();
        for tool in std::iter::once("clang".to_string()).chain(gnu()) {
            let clang = tool == "clang";
            let mut command = Command::new(&tool);
            if clang {
                command.args(["-c", "-target", "x86_64-unknown-linux-gnu"]);
            } else {
                command.arg("--64");
            }
            let p = command.arg(&input).arg("-o").arg(&output).output().unwrap();
            let stderr = String::from_utf8_lossy(&p.stderr);
            for (n, bad) in cases.iter().enumerate() {
                let location = format!("{}:{}:", input.display(), n + 2);
                let diagnosed = |l: &str| {
                    l.starts_with(&location)
                        && if clang {
                            l.contains("error:")
                        } else {
                            l.contains("Error:") || l.contains("Warning:")
                        }
                };
                assert!(
                    stderr.lines().any(diagnosed),
                    "{tool} accepted {bad}\n{stderr}"
                );
            }
        }
    }
}
