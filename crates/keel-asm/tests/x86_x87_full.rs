use keel_asm_ir::x86::Instruction;
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

fn normalized(bytes: &[u8]) -> (Vec<u8>, Vec<(u64, String, i64, String)>) {
    let f = object::File::parse(bytes).unwrap();
    let text = f.section_by_name(".text").unwrap();
    let relocations = text
        .relocations()
        .map(|(offset, r)| {
            let object::RelocationTarget::Symbol(index) = r.target() else {
                panic!("symbol target")
            };
            (
                offset,
                format!("{:?}", r.flags()),
                r.addend(),
                f.symbol_by_index(index).unwrap().name().unwrap().into(),
            )
        })
        .collect();
    (text.data().unwrap().to_vec(), relocations)
}

fn cases() -> Vec<(String, String)> {
    let mut cases = Vec::new();
    let mut add = |intel: String, att: String| {
        cases.push((intel, att));
    };
    for i in 0..8 {
        for op in ["fst", "ffree", "ffreep"] {
            add(format!("{op} st({i})"), format!("{op} %st({i})"));
        }
        for op in [
            "fcmovb", "fcmove", "fcmovbe", "fcmovu", "fcmovne", "fcmovnbe", "fcmovnu",
        ] {
            add(format!("{op} st, st({i})"), format!("{op} %st({i}), %st"));
        }
        for op in ["fcom", "fcomp", "fucom", "fucomp", "fcomi", "fcomip"] {
            add(format!("{op} st({i})"), format!("{op} %st({i})"));
            if matches!(op, "fcomi" | "fcomip") {
                add(format!("{op} st, st({i})"), format!("{op} %st({i}), %st"));
            }
        }
        for (op, reverse) in [
            ("fsub", "fsubr"),
            ("fsubr", "fsub"),
            ("fdiv", "fdivr"),
            ("fdivr", "fdiv"),
        ] {
            add(format!("{op} st({i})"), format!("{op} %st({i})"));
            add(format!("{op} st, st({i})"), format!("{op} %st({i}), %st"));
            if i != 0 {
                add(
                    format!("{op} st({i}), st"),
                    format!("{reverse} %st, %st({i})"),
                );
            }
        }
        for (op, reverse) in [("faddp", "faddp"), ("fdivp", "fdivrp"), ("fdivrp", "fdivp")] {
            add(
                format!("{op} st({i}), st"),
                format!("{reverse} %st, %st({i})"),
            );
        }
    }
    for (intel, att) in [
        ("fcom", "fcom"),
        ("fcomp", "fcomp"),
        ("fucom", "fucom"),
        ("fucomp", "fucomp"),
        ("faddp", "faddp"),
        ("fdivp", "fdivrp"),
        ("fdivrp", "fdivp"),
        ("fstsw ax", "fstsw %ax"),
        ("fnstsw ax", "fnstsw %ax"),
    ] {
        add(intel.into(), att.into());
    }
    for (intel_addr, att_addr) in [
        ("[rax]", "(%rax)"),
        ("[r13+r12*4-128]", "-128(%r13,%r12,4)"),
        ("[r12+127]", "127(%r12)"),
        ("[r15+128]", "128(%r15)"),
        ("[rip+external]", "external(%rip)"),
        ("fs:[r8+8]", "%fs:8(%r8)"),
        ("[rcx*8+4096]", "4096(,%rcx,8)"),
    ] {
        for op in ["fst", "fsub", "fsubr", "fdiv", "fdivr", "fcom", "fcomp"] {
            for (width, suffix) in [("dword", "s"), ("qword", "l")] {
                add(
                    format!("{op} {width} ptr {intel_addr}"),
                    format!("{op}{suffix} {att_addr}"),
                );
            }
        }
        for op in [
            "fiadd", "fimul", "ficom", "ficomp", "fisub", "fisubr", "fidiv", "fidivr", "fist",
        ] {
            for (width, suffix) in [("word", "s"), ("dword", "l")] {
                add(
                    format!("{op} {width} ptr {intel_addr}"),
                    format!("{op}{suffix} {att_addr}"),
                );
            }
        }
        for op in ["fldenv", "fstenv", "fnstenv", "fsave", "fnsave", "frstor"] {
            add(format!("{op} {intel_addr}"), format!("{op} {att_addr}"));
            add(
                format!("data16 {op} {intel_addr}"),
                format!("data16 {op} {att_addr}"),
            );
        }
        for op in ["fstcw", "fstsw", "fnstsw"] {
            add(
                format!("{op} word ptr {intel_addr}"),
                format!("{op} {att_addr}"),
            );
        }
        for op in ["fbld", "fbstp"] {
            add(
                format!("{op} tbyte ptr {intel_addr}"),
                format!("{op} {att_addr}"),
            );
            add(format!("{op} {intel_addr}"), format!("{op} {att_addr}"));
        }
    }
    cases
}

#[test]
fn x87_architectural_register_memory_and_state_forms_match_both_references() {
    let cases = cases();
    let mut intel = String::from(".intel_syntax noprefix\n.text\n");
    let mut att = String::from(".att_syntax prefix\n.text\n");
    let mut program = keel_asm_ir::Program::new();
    for (left, right) in &cases {
        intel += &format!("{left}\n");
        att += &format!("{right}\n");
        let parsed = Instruction::parse_intel(left).unwrap();
        let other = Instruction::parse_att(right).unwrap();
        assert_eq!(parsed.opcode, other.opcode, "{left} / {right}");
        // Memory qualifiers can be inferred by a fixed-width mnemonic.
        let a = keel_asm::x86::assemble_object(left).unwrap();
        let b = keel_asm::x86::assemble_object(&format!(".att_syntax prefix\n{right}")).unwrap();
        assert_eq!(normalized(&a), normalized(&b), "{left} / {right}");
        assert_eq!(
            Instruction::parse_intel(&parsed.to_string()).unwrap(),
            parsed
        );
        program.x86_instruction(parsed);
    }
    let own = keel_asm::x86::assemble_object(&intel).unwrap();
    assert_eq!(
        own,
        keel_asm::x86::assemble_program_object(&program).unwrap()
    );
    assert_eq!(
        normalized(&own),
        normalized(&keel_asm::x86::assemble_object(&att).unwrap())
    );
    let dir = std::env::temp_dir().join(format!("keel-x87-full-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (dialect, source) in [("intel", intel), ("att", att)] {
        let input = dir.join(format!("{dialect}.s"));
        for tool in [
            "clang",
            if cfg!(target_os = "linux") {
                "as"
            } else {
                "x86_64-elf-as"
            },
        ] {
            let tool = if tool == "clang" {
                tool.to_string()
            } else {
                std::env::var("KEEL_GNU_AS").unwrap_or(tool.into())
            };
            // LLVM puts 66 before the separate FWAIT instruction for these
            // spellings. Native x86 then writes the 28-byte, not 14-byte,
            // layout. GAS and the native oracle cover these fourteen cases.
            let selected = source
                .lines()
                .filter(|line| {
                    tool != "clang"
                        || !(line.starts_with("data16 fstenv ")
                            || line.starts_with("data16 fsave "))
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            std::fs::write(&input, &selected).unwrap();
            let expected = keel_asm::x86::assemble_object(&selected).unwrap();
            let output = dir.join("reference.o");
            let mut command = Command::new(&tool);
            if tool == "clang" {
                command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            } else {
                command.arg("--64");
            }
            let p = command
                .arg(&input)
                .arg("-o")
                .arg(&output)
                .output()
                .expect("both reference assemblers required");
            assert!(
                p.status.success(),
                "{dialect} {tool}: {}",
                String::from_utf8_lossy(&p.stderr)
            );
            let canonical = |bytes: &[u8]| {
                let (mut code, relocs) = normalized(bytes);
                // Segment and operand-size prefixes may commute within one
                // instruction. Never commute a prefix across FWAIT.
                for n in 0..code.len().saturating_sub(1) {
                    if code[n] == 0x66 && code[n + 1] == 0x64 {
                        code.swap(n, n + 1);
                    }
                }
                (code, relocs)
            };
            assert_eq!(
                canonical(&expected),
                canonical(&std::fs::read(output).unwrap()),
                "{dialect} {tool}"
            );
        }
    }
    println!(
        "{} x87 forms, two dialects, Clang/GNU bytes and relocations",
        cases.len()
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn x87_extra_rejects_invalid_widths_registers_and_lock_prefixes() {
    for source in [
        "fst tbyte ptr [rax]",
        "fist qword ptr [rax]",
        "fiadd qword ptr [rax]",
        "fdiv word ptr [rax]",
        "fbld word ptr [rax]",
        "fbstp qword ptr [rax]",
        "fldenv byte ptr [rax]",
        "frstor qword ptr [rax]",
        "fstcw dword ptr [rax]",
        "fnstsw eax",
        "fstsw dx",
        "ffree",
        "ffreep st(8)",
        "fcmovne st(1), st(2)",
        "fdiv st(2), st(3)",
        "fdivp st(1), st(2)",
        "fcomi st(2), st(1)",
        "fcom st,st(1)",
        "fucomp st,st(1)",
        "fsub rax,rcx",
        "lock fnsave [rax]",
        "lock fcom st(1)",
    ] {
        assert!(keel_asm::x86::assemble_object(source).is_err(), "{source}");
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn native_waiting_environment_layout_keeps_data16_after_fwait() {
    let dir = std::env::temp_dir().join(format!("keel-x87-layout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let object = keel_asm::x86::assemble_object(
        ".text\n.globl save16\nsave16:\nfninit\ndata16 fstenv [rdi]\nret\n.globl save32\nsave32:\nfninit\nfstenv [rdi]\nret\n"
    ).unwrap();
    std::fs::write(dir.join("layout.o"), object).unwrap();
    std::fs::write(
        dir.join("layout.c"),
        r#"
#include <string.h>
extern void save16(void*), save32(void*);
int main(void) {
 unsigned char a[64],b[64]; memset(a,0xcc,64);memset(b,0xcc,64);
 save16(a);save32(b);
 int small=0,large=0;
 for(int i=0;i<14;i++)small+=a[i]!=0xcc;
 for(int i=14;i<28;i++){if(a[i]!=0xcc)return 1;large+=b[i]!=0xcc;}
 for(int i=28;i<64;i++)if(a[i]!=0xcc || b[i]!=0xcc)return 2;
 return small && large ? 0 : 3;
}
"#,
    )
    .unwrap();
    let compiled = Command::new("clang")
        .arg(dir.join("layout.c"))
        .arg(dir.join("layout.o"))
        .arg("-o")
        .arg(dir.join("layout"))
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(Command::new(dir.join("layout")).status().unwrap().success());
    std::fs::remove_dir_all(dir).unwrap();
}
