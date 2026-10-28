use keel_asm::x86::{assemble_object_with_features, assemble_program_object_with_features};
use keel_asm_ir::{
    Program,
    x86::{CpuLevel, Features, Instruction},
};
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;
type RelocationRecord = (u64, String, i64, String);
fn sections(bytes: &[u8]) -> (Vec<u8>, Vec<RelocationRecord>) {
    let o = object::File::parse(bytes).unwrap();
    let t = o.section_by_name(".text").unwrap();
    (
        t.data().unwrap().to_vec(),
        t.relocations()
            .map(|(a, r)| {
                let object::RelocationTarget::Symbol(s) = r.target() else {
                    panic!()
                };
                (
                    a,
                    format!("{:?}", r.flags()),
                    r.addend(),
                    o.symbol_by_index(s).unwrap().name().unwrap().to_string(),
                )
            })
            .collect(),
    )
}
#[test]
fn mxcsr_v1_address_matrix_and_relocations() {
    let features = Features::for_level(CpuLevel::V1);
    let mut cases = Vec::new();
    for op in ["ldmxcsr", "stmxcsr"] {
        for addr in [
            "[rax]",
            "[rsp]",
            "[r13+r12*8-128]",
            "[r15+128]",
            "[rip+external+7]",
            "fs:[r8+8]",
            "[rcx*8+4096]",
        ] {
            cases.push(format!("{op} dword ptr {addr}"));
        }
    }
    let source = format!(".intel_syntax noprefix\n.text\n{}\n", cases.join("\n"));
    let ours = assemble_object_with_features(&source, features).unwrap();
    let mut p = Program::new();
    for c in cases {
        p.x86_instruction(Instruction::parse_intel(&c).unwrap());
    }
    assert_eq!(
        ours,
        assemble_program_object_with_features(&p, features).unwrap()
    );
    for op in ["ldmxcsr", "stmxcsr"] {
        let att = Instruction::parse_att(&format!("{op} 8(%r12)")).unwrap();
        let intel = Instruction::parse_intel(&format!("{op} [r12+8]")).unwrap();
        let mut a = Program::new();
        a.x86_instruction(att);
        let mut b = Program::new();
        b.x86_instruction(intel);
        assert_eq!(
            assemble_program_object_with_features(&a, features).unwrap(),
            assemble_program_object_with_features(&b, features).unwrap()
        );
        for bad in [
            "rax",
            "xmm0",
            "qword ptr [rax]",
            "word ptr [rax]",
            "[rax],1",
        ] {
            assert!(assemble_object_with_features(&format!("{op} {bad}"), features).is_err());
        }
    }
    let dir = std::env::temp_dir().join(format!("keel-mxcsr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("test.s");
    std::fs::write(&input, source).unwrap();
    for gnu in [false, true] {
        let output = dir.join("test.o");
        let mut c = if gnu {
            Command::new(
                std::env::var("KEEL_TEST_GNU_AS")
                    .or_else(|_| std::env::var("KEEL_GNU_AS"))
                    .unwrap_or_else(|_| "as".into()),
            )
        } else {
            let mut c = Command::new("clang");
            c.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
            c
        };
        let r = c.arg(&input).arg("-o").arg(&output).output().unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(sections(&ours), sections(&std::fs::read(output).unwrap()));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
