//! Bounded instruction forms observed in intact Clang compiler output.
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
