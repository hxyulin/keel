//! Intel legacy AES-NI contract: independent GF(256) oracle, no host AES oracle.
use keel_asm_ir::x86::{CpuLevel, Features};
use keel_codegen::{
    CodegenPipeline, SelectionTier,
    regalloc::{AllocationTier, Pools},
    x86_features::FeatureSelector,
};
use std::{fs, process::Command};
#[allow(dead_code)]
#[path = "common/vector.rs"]
pub mod common;
const EMPTY: Pools = Pools {
    caller_saved: &[],
    callee_saved: &[],
    float_caller_saved: &[],
    float_callee_saved: &[],
};
const OPS: &[&str] = &[
    "aesenc",
    "aesenclast",
    "aesdec",
    "aesdeclast",
    "aesimc",
    "aeskeygenassist",
];
fn features() -> Features {
    Features::for_level(CpuLevel::V1)
        .with_features("+aes")
        .unwrap()
}
fn source() -> String {
    let mut s = String::new();
    for (i, op) in OPS.iter().enumerate() {
        let controls = if i == 5 { 256 } else { 1 };
        for control in 0..controls {
            let (types, params, args) = if i < 4 {
                ("i64x2,i64x2", "v0:i64x2,v1:i64x2", "v0,v1".to_string())
            } else if i == 4 {
                ("i64x2", "v0:i64x2", "v0".into())
            } else {
                ("i64x2", "v0:i64x2", "v0,v1".into())
            };
            s += &format!("func @aes_{i}_{control}({types})->i64x2 {{\nblock0({params}):\n");
            if i == 5 {
                s += &format!(" v1 = iconst.i32 {control}\n");
            }
            s += &format!(" v2 = intrinsic x86_{op}128({args})\n return v2\n}}\n");
            // Keep both inputs live beyond the operation and exercise aliased SSA inputs.
            if control == 0 {
                for alias in 0..2 {
                    let args = if i < 4 {
                        if alias == 0 { "v3,v4" } else { "v3,v3" }
                    } else if i == 4 {
                        "v3"
                    } else {
                        "v3,v5"
                    };
                    s += &format!(
                        "func @live_{i}_{alias}(ptr,ptr,ptr) {{\nblock0(v0:ptr,v1:ptr,v2:ptr):\n v3 = load.i64x2 align 1 v0\n v4 = load.i64x2 align 1 v1\n v5 = iconst.i32 0\n v6 = intrinsic x86_{op}128({args})\n store align 1 v6,v2\n v7 = iconst.i64 16\n v8 = ptradd v2,v7\n store align 1 v3,v8\n v9 = ptradd v8,v7\n store align 1 v4,v9\n return\n}}\n"
                    );
                }
            }
        }
    }
    // Both directions of direct/indirect register ABI; SSA args remain live across calls.
    s += "decl @callback(i64x2,i64x2)->i64x2\nfunc @direct(i64x2,i64x2)->i64x2 {\nblock0(v0:i64x2,v1:i64x2):\n v2 = call @callback(v0,v1)\n v3 = intrinsic x86_aesenc128(v0,v2)\n return v3\n}\nfunc @indirect(i64x2,i64x2,ptr)->i64x2 {\nblock0(v0:i64x2,v1:i64x2,v2:ptr):\n v3 = call_indirect v2(v0,v1) : (i64x2,i64x2)->i64x2\n v4 = intrinsic x86_aesdec128(v1,v3)\n return v4\n}\n";
    s
}
fn compile(
    src: &str,
    s: SelectionTier,
    a: AllocationTier,
    spilled: bool,
    f: Features,
) -> Result<(Vec<u8>, String), keel_codegen::CodegenError> {
    let mut m = keel_ir::parse(src).unwrap();
    keel_ir::verify_module(&m).unwrap();
    if s == SelectionTier::Full {
        keel_opt::optimize(&mut m).unwrap();
    }
    let selector = FeatureSelector {
        inner: &s,
        features: f,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: &a,
    };
    let pools = if spilled {
        &EMPTY
    } else {
        &keel_codegen::x86_64::REGISTERS
    };
    let text = keel_codegen::x86_64::compile_with_pipeline(&m, pools, &pipeline)?;
    let p = keel_codegen::x86_64::compile_program_with_pipeline(&m, pools, &pipeline)?;
    let mut chunks = keel_asm_ir::Program::default();
    keel_codegen::x86_64::compile_program_chunks_with_pipeline(&m, pools, &pipeline, &mut |p| {
        chunks.items.extend(p.items)
    })?;
    for program in [&p, &chunks] {
        for item in &program.items {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                assert!(f.contains(i.required_features()), "{i}");
            }
        }
    }
    let object = keel_asm::x86::assemble_program_object_with_features(&p, f).unwrap();
    assert_eq!(
        object,
        keel_asm::x86::assemble_object_with_features(&text, f).unwrap()
    );
    assert_eq!(
        object,
        keel_asm::x86::assemble_program_object_with_features(&chunks, f).unwrap()
    );
    Ok((object, text))
}
#[test]
fn feature_gates_signatures_controls_and_object_parity() {
    let src = source();
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        let f = Features::for_level(level);
        assert!(!f.aes);
        assert!(
            compile(
                &src,
                SelectionTier::Baseline,
                AllocationTier::Fast,
                false,
                f
            )
            .unwrap_err()
            .message
            .contains("AES")
        );
        let on = f.with_features("+aes").unwrap();
        assert_eq!(on.avx, f.avx);
        assert_eq!(on.with_features("-aes").unwrap(), f);
    }
    let pcl = "func @p(i64x2,i64x2)->i64x2 {\nblock0(v0:i64x2,v1:i64x2):\n v2 = iconst.i32 17\n v3 = intrinsic x86_pclmulqdq128(v0,v1,v2)\n return v3\n}\n";
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        for aes in [false, true] {
            for pclmul in [false, true] {
                let f = Features::for_level(level)
                    .with_features(&format!(
                        "{}aes,{}pclmul",
                        if aes { "+" } else { "-" },
                        if pclmul { "+" } else { "-" }
                    ))
                    .unwrap();
                assert_eq!(f.aes, aes);
                assert_eq!(f.pclmulqdq, pclmul);
                assert_eq!(
                    compile(
                        &src,
                        SelectionTier::Baseline,
                        AllocationTier::Fast,
                        false,
                        f
                    )
                    .is_ok(),
                    aes
                );
                assert_eq!(
                    compile(pcl, SelectionTier::Baseline, AllocationTier::Fast, false, f).is_ok(),
                    pclmul
                );
                for (line, allowed) in [
                    ("aesenc xmm8,xmm15", aes),
                    ("pclmulqdq xmm8,xmm15,17", pclmul),
                ] {
                    assert_eq!(
                        keel_asm::x86::assemble_object_with_features(line, f).is_ok(),
                        allowed
                    );
                }
            }
        }
    }
    assert!(!features().pclmulqdq);
    assert!(!features().avx && !features().avx2 && !features().sse41);
    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                compile(&src, s, a, spilled, features()).unwrap();
            }
        }
    }
    for control in ["-1", "256"] {
        let m = keel_ir::parse(&format!("func @f(i64x2)->i64x2 {{\nblock0(v0:i64x2):\n v1 = iconst.i32 {control}\n v2 = intrinsic x86_aeskeygenassist128(v0,v1)\n return v2\n}}\n")).unwrap();
        assert!(keel_ir::verify_module(&m).is_err());
    }
    let dynamic = keel_ir::parse("func @f(i64x2,i32)->i64x2 {\nblock0(v0:i64x2,v1:i32):\n v2 = intrinsic x86_aeskeygenassist128(v0,v1)\n return v2\n}\n").unwrap();
    assert!(keel_ir::verify_module(&dynamic).is_err());
    let wrong = keel_ir::parse("func @f(i32,i32)->i32 {\nblock0(v0:i32,v1:i32):\n v2 = intrinsic x86_aesenc128(v0,v1)\n return v2\n}\n").unwrap();
    assert!(keel_ir::verify_module(&wrong).is_err());
    let module = keel_ir::parse(&src).unwrap();
    let e = keel_codegen::compile_program_with_pipeline(
        &module,
        keel_ir::Target::Aarch64LinuxGnu,
        &CodegenPipeline {
            selector: &SelectionTier::Baseline,
            allocator: &AllocationTier::Fast,
        },
    )
    .unwrap_err();
    assert!(e.message.contains("x86"), "{e:?}");
    let all = keel_ir::Intrinsic::ALL;
    assert_eq!(all[766], keel_ir::Intrinsic::X86RepOuts32OwnedTerminal);
    assert_eq!(all[767], keel_ir::Intrinsic::X86Pclmulqdq128);
    assert!(all.len() >= 774);
    assert_eq!(all[768], keel_ir::Intrinsic::X86Aesenc128);
    assert_eq!(keel_ir::Intrinsic::X86Aesenc128 as u16, 768);
}
fn text(bytes: &[u8]) -> Vec<u8> {
    use object::{Object, ObjectSection};
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}
#[test]
fn preflight_rejects_unverified_aes_operand_shapes() {
    // Library callers can hand codegen parsed IR without running the verifier.
    // Diagnose malformed AES before lowering indexes or stages its operands.
    for body in [
        "v1 = iconst.i64 17\n v2 = intrinsic x86_aeskeygenassist128(v0,v1)",
        "v2 = intrinsic x86_aesenc128(v0)",
        "v2 = intrinsic x86_aesimc128(v0,v0)",
        "v1 = iconst.i64 17\n v2 = intrinsic x86_aesenc128(v0,v1)",
    ] {
        let module = keel_ir::parse(&format!(
            "func @f(i64x2)->i64x2 {{\nblock0(v0:i64x2):\n {body}\n return v2\n}}\n"
        ))
        .unwrap();
        assert!(keel_ir::verify_module(&module).is_err());
        let selector = FeatureSelector {
            inner: &SelectionTier::Baseline,
            features: features(),
        };
        let result = keel_codegen::x86_64::compile_program_with_pipeline(
            &module,
            &keel_codegen::x86_64::REGISTERS,
            &CodegenPipeline {
                selector: &selector,
                allocator: &AllocationTier::Fast,
            },
        );
        assert!(result.is_err(), "accepted malformed AES: {body}");
    }
}
#[test]
fn encoding_matches_clang_and_optional_gnu() {
    use keel_asm_ir::x86::Instruction;
    let mut src = String::from(".intel_syntax noprefix\n.text\n");
    for op in OPS {
        for dst in ["xmm0", "xmm7", "xmm8", "xmm15"] {
            for operand in [
                "xmm0",
                "xmm8",
                "xmm15",
                "xmmword ptr [r12+r13*4+127]",
                "xmmword ptr [r8-129]",
                "xmmword ptr [rip+16]",
                "xmmword ptr fs:[rax]",
            ] {
                for control in if *op == "aeskeygenassist" {
                    vec![0, 1, 127, 128, 255]
                } else {
                    vec![0]
                } {
                    let line = format!(
                        "{op} {dst},{operand}{}",
                        if *op == "aeskeygenassist" {
                            format!(",{control}")
                        } else {
                            String::new()
                        }
                    );
                    let i = Instruction::parse_intel(&line).unwrap();
                    assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
                    assert!(i.required_features().aes);
                    src += &line;
                    src += "\n";
                }
            }
        }
    }
    let ours = keel_asm::x86::assemble_object_with_features(&src, features()).unwrap();
    let mut program = keel_asm_ir::Program::new();
    for line in src.lines().skip(2) {
        program.x86_instruction(Instruction::parse_intel(line).unwrap());
    }
    assert_eq!(
        text(&ours),
        text(&keel_asm::x86::assemble_program_object_with_features(&program, features()).unwrap())
    );
    for op in OPS {
        let control = if *op == "aeskeygenassist" { ",255" } else { "" };
        let intel =
            Instruction::parse_intel(&format!("{op} xmm15,[r12+r13*4+127]{control}")).unwrap();
        let att = Instruction::parse_att(&format!(
            "{op} {}127(%r12,%r13,4),%xmm15",
            if control.is_empty() { "" } else { "$255," }
        ))
        .unwrap();
        assert_eq!(intel, att);
    }
    let dir = common::temp("aes-encoding");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("aes.s"), &src).unwrap();
    common::command(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(dir.join("aes.s"))
            .arg("-o")
            .arg(dir.join("clang.o")),
    );
    assert_eq!(text(&ours), text(&fs::read(dir.join("clang.o")).unwrap()));
    if let Ok(as_path) = std::env::var("KEEL_GNU_AS") {
        common::command(
            Command::new(as_path)
                .arg("--64")
                .arg(dir.join("aes.s"))
                .arg("-o")
                .arg(dir.join("gas.o")),
        );
        assert_eq!(text(&ours), text(&fs::read(dir.join("gas.o")).unwrap()));
    }
    for line in [
        "aesenc ymm0,ymm1",
        "aesimc xmm0,qword ptr [rax]",
        "aeskeygenassist xmm0,xmm1,256",
        "aeskeygenassist xmm0,xmm1,-1",
    ] {
        assert!(
            keel_asm::x86::assemble_object_with_features(line, Features::all_known()).is_err(),
            "{line}"
        );
    }
    assert!(
        keel_asm::x86::assemble_object_with_features(
            "vaesenc xmm0,xmm1,xmm2",
            Features::all_known()
        )
        .is_ok()
    );
    assert!(
        keel_asm::x86::assemble_object_with_features("vaesenc xmm0,xmm1,xmm2", features()).is_err()
    );
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        assert!(
            keel_asm::x86::assemble_object_with_features(&src, Features::for_level(level)).is_err()
        );
    }
    fs::remove_dir_all(dir).unwrap();
}
#[test]
fn symbolic_memory_relocations_match_independent_assemblers() {
    use object::{Object, ObjectSection, ObjectSymbol};
    let relocations = |bytes: &[u8]| {
        let file = object::File::parse(bytes).unwrap();
        file.section_by_name(".text")
            .unwrap()
            .relocations()
            .map(|(offset, r)| {
                let object::RelocationTarget::Symbol(index) = r.target() else {
                    panic!("symbol expected")
                };
                (
                    offset,
                    r.kind(),
                    r.size(),
                    r.addend(),
                    file.symbol_by_index(index)
                        .unwrap()
                        .name()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect::<Vec<_>>()
    };
    let mut source = String::from(".intel_syntax noprefix\n.text\n");
    let mut program = keel_asm_ir::Program::new();
    for op in OPS {
        let line = format!(
            "{op} xmm15,xmmword ptr [rip+external+7]{}",
            if *op == "aeskeygenassist" { ",255" } else { "" }
        );
        program.x86_instruction(keel_asm_ir::x86::Instruction::parse_intel(&line).unwrap());
        source += &format!("{line}\n");
    }
    let ours = keel_asm::x86::assemble_object_with_features(&source, features()).unwrap();
    let typed = keel_asm::x86::assemble_program_object_with_features(&program, features()).unwrap();
    assert_eq!(text(&ours), text(&typed));
    assert_eq!(relocations(&ours), relocations(&typed));
    let dir = common::temp("aes-relocations");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("aes.s"), source).unwrap();
    for tool in [Some("clang".to_owned()), std::env::var("KEEL_GNU_AS").ok()]
        .into_iter()
        .flatten()
    {
        let mut command = Command::new(&tool);
        if tool == "clang" {
            command.args(["-target", "x86_64-unknown-linux-gnu", "-c"]);
        } else {
            command.arg("--64");
        }
        common::command(
            command
                .arg(dir.join("aes.s"))
                .arg("-o")
                .arg(dir.join("oracle.o")),
        );
        let expected = fs::read(dir.join("oracle.o")).unwrap();
        assert_eq!(text(&ours), text(&expected), "{tool}");
        // KEYGENASSIST's trailing immediate must be included in the PC-relative addend.
        assert_eq!(relocations(&ours), relocations(&expected), "{tool}");
    }
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn harness() -> String {
    let mut c = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n#include <string.h>\n#include <sys/mman.h>\n#include <unistd.h>\ntypedef unsigned char u8;\ntypedef long long V __attribute__((vector_size(16)));\n",
    );
    for i in 0..6 {
        for n in 0..if i == 5 { 256 } else { 1 } {
            c += &format!(
                "extern V aes_{i}_{n}(V{});\n",
                if i < 4 { ",V" } else { "" }
            );
        }
        for alias in 0..2 {
            c += &format!("extern void live_{i}_{alias}(const void*,const void*,void*);\n");
        }
    }
    c += "static V(*kg[256])(V)={";
    for n in 0..256 {
        c += &format!("aes_5_{n},");
    }
    c += "};\n";
    for i in 0..6 {
        c += &format!("extern void aesmem_{i}(const void*,const void*,void*);\n");
    }
    c += include_str!("fixtures/x86_aes_harness.c");
    c
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn native_software_oracle_fips_vectors_aliases_abi_mxcsr_and_guards() {
    if !std::is_x86_feature_detected!("aes") || !std::is_x86_feature_detected!("sse2") {
        eprintln!("SKIP: native AES/SSE2 unavailable");
        return;
    }
    let dir = common::temp("aes-native");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("test.c"), harness()).unwrap();
    let mut memory = String::from(".intel_syntax noprefix\n.text\n");
    for (i, op) in OPS.iter().enumerate() {
        memory += &format!(".globl aesmem_{i}\naesmem_{i}:\n");
        if i < 4 {
            memory += "movdqu xmm0,xmmword ptr [rdi]\n";
        }
        memory += &format!(
            "{op} xmm0,xmmword ptr [{}]{}\nmovdqu xmmword ptr [rdx],xmm0\nret\n",
            if i < 4 { "rsi" } else { "rdi" },
            if i == 5 { ",0" } else { "" }
        );
    }
    fs::write(
        dir.join("memory.o"),
        keel_asm::x86::assemble_object_with_features(&memory, features()).unwrap(),
    )
    .unwrap();

    for s in [
        SelectionTier::Baseline,
        SelectionTier::Trees,
        SelectionTier::Full,
    ] {
        for a in [AllocationTier::Fast, AllocationTier::Full] {
            for spilled in [false, true] {
                let (o, _) = compile(&source(), s, a, spilled, features()).unwrap();
                fs::write(dir.join("test.o"), o).unwrap();
                for cc in ["clang", "gcc"] {
                    let mut command = Command::new(cc);
                    command.args(["-O2", "-march=x86-64", "-mno-avx", "-fno-tree-vectorize"]);
                    common::command(
                        command
                            .arg(dir.join("test.c"))
                            .arg(dir.join("test.o"))
                            .arg(dir.join("memory.o"))
                            .arg("-o")
                            .arg(dir.join("test")),
                    );
                    common::command(&mut Command::new(dir.join("test")));
                }
                println!("PASS AES {s:?} {a:?} spilled={spilled}");
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
