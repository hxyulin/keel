use keel_asm::x86::{
    Syntax, assemble_object_with_features, assemble_object_with_syntax_and_features,
    assemble_program_object_with_features,
};
use keel_asm_ir::x86::{CpuLevel, Features, Instruction};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{fs, process::Command};
fn relocations(o: &[u8]) -> Vec<(u64, i64, String)> {
    let f = object::File::parse(o).unwrap();
    f.section_by_name(".text")
        .unwrap()
        .relocations()
        .map(|(offset, r)| {
            let object::RelocationTarget::Symbol(index) = r.target() else {
                panic!("symbol expected")
            };
            (
                offset,
                r.addend(),
                f.symbol_by_index(index).unwrap().name().unwrap().to_owned(),
            )
        })
        .collect()
}
fn bytes(o: &[u8]) -> Vec<u8> {
    object::File::parse(o)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn pclmul_legacy_encoding_oracles_and_feature_contract() {
    let f = Features::NONE.with_features("+pclmulqdq").unwrap();
    assert_eq!(f, Features::NONE.with_features("pclmul").unwrap());
    assert!(!f.with_features("-pclmul").unwrap().pclmulqdq);
    assert_eq!(f.names().collect::<Vec<_>>(), ["pclmulqdq"]);
    let mut cases = Vec::new();
    for d in [0, 7, 8, 15] {
        for s in [
            "xmm0",
            "xmm8",
            "xmm15",
            "xmmword ptr [rax]",
            "xmmword ptr [r13+r12*4-128]",
            "xmmword ptr fs:[r8+128]",
            "xmmword ptr [rip+16]",
            "xmmword ptr [rip+external+7]",
            "xmmword ptr [r8d+ecx*2+8]",
        ] {
            for c in 0..=255 {
                cases.push(format!("pclmulqdq xmm{d},{s},{c}"));
            }
        }
    }
    for (intel, att) in [
        ("pclmulqdq xmm8,xmm15,255", "pclmulqdq $255,%xmm15,%xmm8"),
        (
            "pclmulqdq xmm15,xmmword ptr fs:[r13+r12*4-128],238",
            "pclmulqdq $238,%fs:-128(%r13,%r12,4),%xmm15",
        ),
    ] {
        assert_eq!(
            bytes(&assemble_object_with_features(intel, f).unwrap()),
            bytes(&assemble_object_with_syntax_and_features(att, Syntax::Att, f).unwrap())
        );
    }
    let source = cases.join("\n");
    let ours = bytes(&assemble_object_with_features(&source, f).unwrap());
    assert_eq!(
        bytes(&assemble_object_with_features("pclmulqdq xmm8,xmm15,255", f).unwrap()),
        [0x66, 0x45, 0x0f, 0x3a, 0x44, 0xc7, 0xff]
    );
    let mut typed = keel_asm_ir::Program::new();
    for line in &cases {
        let i = Instruction::parse_intel(line).unwrap();
        assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
        typed.x86_instruction(i);
    }
    assert_eq!(
        ours,
        bytes(&assemble_program_object_with_features(&typed, f).unwrap())
    );
    let dir = std::env::var_os("KEEL_PCLMUL_ASM_EXPORT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("keel-pclmul-asm-{}", std::process::id()))
        });
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("keel.o"),
        assemble_object_with_features(&source, f).unwrap(),
    )
    .unwrap();
    let input = dir.join("oracle.s");
    fs::write(&input, format!(".intel_syntax noprefix\n.text\n{source}\n")).unwrap();
    for tool in [Some("clang".to_owned()), std::env::var("KEEL_GNU_AS").ok()]
        .into_iter()
        .flatten()
    {
        let out = dir.join("oracle.o");
        let mut cmd = Command::new(&tool);
        if tool == "clang" {
            cmd.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        } else {
            cmd.arg("--64");
        }
        let o = cmd.arg(&input).arg("-o").arg(&out).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let expected = fs::read(out).unwrap();
        assert_eq!(ours, bytes(&expected), "{tool}");
        assert_eq!(
            relocations(&fs::read(dir.join("keel.o")).unwrap()),
            relocations(&expected),
            "{tool} relocation addends"
        );
    }
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        let disabled = Features::for_level(level);
        assert!(!disabled.pclmulqdq);
        assert!(assemble_object_with_features(&source, disabled).is_err());
        assert!(assemble_program_object_with_features(&typed, disabled).is_err());
        assert!(
            assemble_object_with_features(
                &source,
                disabled.with_features("pclmul,-pclmul").unwrap()
            )
            .is_err()
        );
    }
    for bad in [
        "xmm0,xmm1,-1",
        "xmm0,xmm1,256",
        "xmm0,xmm1,rcx",
        "ymm0,ymm1,0",
        "xmm0,qword ptr [rax],0",
        "[rax],xmm0,0",
        "xmm0,xmm1",
        "xmm0,xmm1,unknown",
    ] {
        assert!(
            assemble_object_with_features(&format!("pclmulqdq {bad}"), f).is_err(),
            "{bad}"
        );
    }
    assert!(assemble_object_with_features("lock pclmulqdq xmm0,xmm1,0", f).is_err());
    assert!(
        assemble_object_with_features("vpclmulqdq xmm0,xmm1,xmm2,0", Features::all_known()).is_ok()
    );
    assert!(assemble_object_with_features("vpclmulqdq xmm0,xmm1,xmm2,0", f).is_err());
}
