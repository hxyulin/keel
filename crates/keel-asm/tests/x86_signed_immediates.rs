//! Keep AT&T movq's signed relocation contract distinct from movabsq/movl.
use keel_asm::x86::{assemble_object, assemble_program_object};
use keel_asm_ir::{
    Program,
    x86::{CoreOpcode, Instruction, Opcode},
};
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
            "keel-signed-immediate-{}-{}",
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

fn gnu() -> String {
    std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into())
}

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    text: Vec<u8>,
    relocations: Vec<(u64, u32, String, i64)>,
}
fn snapshot(bytes: &[u8]) -> Snapshot {
    let file = object::File::parse(bytes).unwrap();
    let section = file.section_by_name(".text").unwrap();
    Snapshot {
        text: section.data().unwrap().to_vec(),
        relocations: section
            .relocations()
            .map(|(at, r)| {
                let object::RelocationFlags::Elf { r_type } = r.flags() else {
                    panic!()
                };
                let RelocationTarget::Symbol(index) = r.target() else {
                    assert_eq!(r.target(), RelocationTarget::Absolute);
                    return (at, r_type, "*ABS*".into(), r.addend());
                };
                let symbol = file.symbol_by_index(index).unwrap();
                let (name, addend) = if symbol.section() == object::SymbolSection::Absolute {
                    ("*ABS*".into(), r.addend() + symbol.address() as i64)
                } else if symbol.is_local()
                    && let Some(index) = symbol.section_index()
                {
                    (
                        file.section_by_index(index)
                            .unwrap()
                            .name()
                            .unwrap()
                            .to_owned(),
                        r.addend() + symbol.address() as i64,
                    )
                } else {
                    (symbol.name().unwrap().to_owned(), r.addend())
                };
                (at, r_type, name, addend)
            })
            .collect(),
    }
}
fn reference(source: &str, clang: bool) -> Vec<u8> {
    let dir = Scratch::new();
    let input = dir.0.join("source.s");
    let output = dir.0.join("source.o");
    std::fs::write(&input, source).unwrap();
    let mut command = Command::new(if clang { "clang".into() } else { gnu() });
    if clang {
        command.args(["--target=x86_64-linux-gnu", "-c"]);
    } else {
        command.arg("--64");
    }
    let result = command.arg(&input).arg("-o").arg(&output).output().unwrap();
    assert!(
        result.status.success(),
        "{source}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read(output).unwrap()
}
fn assert_oracles(source: &str) {
    let actual = snapshot(&assemble_object(source).unwrap_or_else(|e| panic!("{source}\n{e}")));
    for clang in [false, true] {
        assert_eq!(
            actual,
            snapshot(&reference(source, clang)),
            "clang={clang}\n{source}"
        );
    }
}

#[test]
fn signed_unsigned_and_full_width_have_literal_bytes_and_relocations() {
    let source = ".att_syntax\n.text\nmovq $external+7,%rax\nmovq $external-9,%r12\nmovl $external+5,%r8d\nmovabsq $external+11,%r15\nmovq $external+3,16(%r13)\n";
    let actual = snapshot(&assemble_object(source).unwrap());
    assert_eq!(
        actual.text,
        [
            0x48, 0xc7, 0xc0, 0, 0, 0, 0, 0x49, 0xc7, 0xc4, 0, 0, 0, 0, 0x41, 0xb8, 0, 0, 0, 0,
            0x49, 0xbf, 0, 0, 0, 0, 0, 0, 0, 0, 0x49, 0xc7, 0x45, 0x10, 0, 0, 0, 0,
        ]
    );
    assert_eq!(
        actual.relocations,
        [
            (3, 11, "external".into(), 7),
            (10, 11, "external".into(), -9),
            (16, 10, "external".into(), 5),
            (22, 1, "external".into(), 11),
            (34, 11, "external".into(), 3),
        ]
    );
    assert_oracles(source);
}

#[test]
fn all_gprs_addends_and_existing_tls_form_match_both_assemblers() {
    let mut source = String::from(".att_syntax\n.text\n");
    for reg in [
        "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12",
        "r13", "r14", "r15",
    ] {
        for addend in [-2147483648i64, -9, 0, 2147483647] {
            source.push_str(&format!("movq $external{addend:+},%{reg}\n"));
        }
    }
    source.push_str("movq $tls@TPOFF+7,%r12\n");
    source.push_str("movq %fs:0xffffffff80000000,%r12\n");
    assert_oracles(&source);
}

#[test]
fn exact_adapter_survives_formatting_typed_programs_and_alias_folding() {
    for source in ["movq $external,%rax", "mov $external+9,%r12"] {
        let i = Instruction::parse_att(source).unwrap();
        assert_eq!(i.opcode, Opcode::Core(CoreOpcode::MovImm));
        assert_eq!(i, Instruction::parse_intel(&i.to_string()).unwrap());
        let mut program = Program::new();
        program.text(".text\n");
        program.x86_instruction(i);
        assert_eq!(
            snapshot(&assemble_program_object(&program).unwrap()),
            snapshot(&assemble_object(&format!(".att_syntax\n.text\n{source}\n")).unwrap())
        );
    }
    assert_eq!(
        Instruction::parse_att("movabsq $external,%rax")
            .unwrap()
            .opcode,
        Opcode::Movabs
    );
    for (name, value) in [
        ("small", "7"),
        ("max", "2147483647"),
        ("min", "-2147483648"),
        ("high", "0xffffffff80000000"),
        ("allbits", "0xffffffffffffffff"),
    ] {
        // The forward form reaches relocation resolution rather than just the
        // instruction's absolute-constant fold. Both must retain signedness.
        for forward in [false, true] {
            let set = format!(".set {name},{value}\n");
            let body = format!("movq ${name},%r12\n");
            let source = if forward {
                format!(".att_syntax\n.text\n{body}{set}")
            } else {
                format!(".att_syntax\n.text\n{set}{body}")
            };
            assert_oracles(&source);
            let bytes = assemble_object(&source).unwrap();
            let file = object::File::parse(&*bytes).unwrap();
            let expected = if let Some(hex) = value.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).unwrap()
            } else {
                value.parse::<i64>().unwrap() as u64
            };
            assert_eq!(file.symbol_by_name(name).unwrap().address(), expected);
        }
    }
    assert_oracles(
        ".att_syntax\n.text\n.Lstart:\nmovq $.Lend-.Lstart,%r14\nnop\n.Lend:\nmovq $external-.Lstart,%r15\n",
    );
    assert_oracles(".att_syntax\n.text\n.set .Labs,4096\n.Lstart:\nmovq $.Labs-.Lstart,%rax\n");
    let mut program = Program::new();
    for (name, value) in [("value_a", -1), ("value_b", 7)] {
        program.text(&format!(".set {name},{value}\n"));
        program.x86_instruction(Instruction::parse_att(&format!("movq ${name},%r12")).unwrap());
    }
    let source =
        ".att_syntax\n.set value_a,-1\nmovq $value_a,%r12\n.set value_b,7\nmovq $value_b,%r12\n";
    assert_eq!(
        snapshot(&assemble_program_object(&program).unwrap()),
        snapshot(&assemble_object(source).unwrap())
    );
    assert_oracles(source);
}

#[test]
fn exact_signed_form_does_not_widen_on_overflow_or_invalid_relocations() {
    for source in [
        "mov.imm rax,2147483648",
        "mov.imm rax,-2147483649",
        "mov.imm rax,0xffffffff7fffffff",
        "mov.imm rax,0x8000000000000000",
        "mov.imm eax,0xffffffffffffffff",
        ".att_syntax\nmovq $external@PLT,%rax",
        ".att_syntax\nmovq $external@GOTPCREL,%rax",
        ".att_syntax\nmovq $left-right,%rax",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
}

#[test]
fn independent_linker_enforces_signed_range_and_accepts_high_kernel_addresses() {
    let assembler = PathBuf::from(gnu());
    let linker = std::env::var_os("KEEL_GNU_LD")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let name = assembler.file_name().unwrap().to_string_lossy();
            assembler.with_file_name(format!("{}ld", name.strip_suffix("as").unwrap()))
        });
    let dir = Scratch::new();
    let source = ".att_syntax\n.text\n.globl entry\nentry: movq $external,%rax\nret\n";
    let objects = [
        assemble_object(source).unwrap(),
        reference(source, false),
        reference(source, true),
    ];
    for (index, bytes) in objects.iter().enumerate() {
        let object = dir.0.join(format!("{index}.o"));
        std::fs::write(&object, bytes).unwrap();
        for (value, fits) in [
            ("0x7fffffff", true),
            ("0xffffffff80000000", true),
            ("0xffffffffffffffff", true),
            ("0x80000000", false),
            ("0xffffffff7fffffff", false),
        ] {
            let run = Command::new(&linker)
                .args(["-e", "entry", "-Ttext=0x100000"])
                .arg(format!("--defsym=external={value}"))
                .arg(&object)
                .arg("-o")
                .arg(dir.0.join("linked"))
                .output()
                .unwrap();
            assert_eq!(
                run.status.success(),
                fits,
                "object={index}, value={value}\n{}",
                String::from_utf8_lossy(&run.stderr)
            );
        }
    }
}
