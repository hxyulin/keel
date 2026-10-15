//! AVX XMM encoding, feature legality, and differential coverage.
use keel_asm::x86::{
    Features, assemble_object, assemble_object_with_features, assemble_program_object_with_features,
};
use keel_asm_ir::{Program, x86::Instruction};
use object::{Object, ObjectSection, ObjectSymbol};
use std::process::Command;

fn avx() -> Features {
    Features {
        avx: true,
        ..Features::default()
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
fn assemble(source: &str) -> Vec<u8> {
    assemble_object_with_features(source, avx()).unwrap()
}
fn relocations(bytes: &[u8]) -> Vec<(u64, object::RelocationFlags, i64, String)> {
    let file = object::File::parse(bytes).unwrap();
    file.section_by_name(".text")
        .unwrap()
        .relocations()
        .map(|(offset, r)| {
            let name = match r.target() {
                object::RelocationTarget::Symbol(index) => file
                    .symbol_by_index(index)
                    .unwrap()
                    .name()
                    .unwrap()
                    .to_owned(),
                other => panic!("unexpected relocation target {other:?}"),
            };
            (offset, r.flags(), r.addend(), name)
        })
        .collect()
}

#[test]
fn vex_prefixes_reserved_bits_and_fixup_bias() {
    for (source, expected) in [
        ("vaddss xmm0,xmm1,xmm2", vec![0xc5, 0xf2, 0x58, 0xc2]),
        ("vaddsd xmm8,xmm15,xmm2", vec![0xc5, 0x03, 0x58, 0xc2]),
        (
            "vaddss xmm8,xmm15,xmm10",
            vec![0xc4, 0x41, 0x02, 0x58, 0xc2],
        ),
        ("vucomiss xmm0,xmm1", vec![0xc5, 0xf8, 0x2e, 0xc1]),
        ("vmovsd xmm0,[rax]", vec![0xc5, 0xfb, 0x10, 0x00]),
        ("vzeroupper", vec![0xc5, 0xf8, 0x77]),
    ] {
        assert_eq!(text(&assemble(source)), expected, "{source}");
    }
    let object = assemble("vroundsd xmm9,xmm14,[rip+external+7],11");
    assert_eq!(
        relocations(&object),
        vec![(
            5,
            object::RelocationFlags::Elf { r_type: 2 },
            2,
            "external".into()
        )]
    );
}

#[test]
fn avx_feature_gating_roundtrips_and_invalid_forms() {
    for source in [
        "vaddss xmm0,xmm1,xmm2",
        "vsqrtsd xmm15,xmm14,[r12]",
        "vucomisd xmm8,xmm9",
        "vmovss [rax],xmm15",
        "vroundss xmm0,xmm1,xmm2,8",
        "vzeroupper",
    ] {
        let i = Instruction::parse_intel(source).unwrap();
        assert_eq!(Instruction::parse_intel(&i.to_string()).unwrap(), i);
        assert_eq!(i.opcode.required_features(), avx());
        let e = assemble_object(source).unwrap_err().to_string();
        assert!(e.contains("avx"), "{e}");
        let mut p = Program::new();
        p.x86_instruction(i);
        let error = keel_asm::x86::assemble_program_object(&p)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("requires x86 target features: avx"),
            "{error}"
        );
        assert_eq!(
            assemble(source),
            assemble_program_object_with_features(&p, avx()).unwrap()
        );
        assert!(
            assemble_object_with_features(
                source,
                Features {
                    bmi1: true,
                    bmi2: true,
                    ..Features::default()
                }
            )
            .is_err()
        );
    }
    for source in [
        "vaddss xmm0,xmm1",
        "vaddsd xmm0,xmm1,xmm2,xmm3",
        "vaddss xmm0,rax,xmm1",
        "vaddss xmm0,xmm1,qword ptr [rax]",
        "vaddsd [rax],xmm1,xmm2",
        "vaddss xmm0,[rax],xmm1",
        "vaddss xmm0,xmm1,7",
        "vaddps ymm0,ymm1,ymm2",
        "vsqrtss xmm0,xmm1",
        "vsqrtsd xmm0,xmm1,dword ptr [rax]",
        "vucomiss xmm0,xmm1,xmm2",
        "vucomisd [rax],xmm1",
        "vmovss xmm0,xmm1",
        "vmovsd xmm0,xmm1,[rax]",
        "vmovsd [rax],xmm1,xmm2",
        "vmovaps xmm0,xmm1,xmm2",
        "vmovupd xmm0,qword ptr [rax]",
        "vmovss [rax],[rbx]",
        "vandps xmm0,xmm1,qword ptr [rax]",
        "vroundsd xmm0,xmm1,xmm2",
        "vroundss xmm0,xmm1,xmm2,256",
        "vroundss xmm0,xmm1,xmm2,-129",
        "vzeroupper xmm0",
        "lock vaddss xmm0,xmm1,xmm2",
    ] {
        assert!(
            assemble_object_with_features(source, avx()).is_err(),
            "accepted {source}"
        );
    }
}

#[test]
fn typed_avx_rejects_malformed_registers_and_memory() {
    use keel_asm_ir::x86::Operand;
    for (operand, number, width) in [(0, 16, 128), (1, 1, 64), (2, 17, 128)] {
        let mut i = Instruction::parse_intel("vaddsd xmm0,xmm1,xmm2").unwrap();
        if let Operand::Register(r) = &mut i.operands[operand] {
            r.number = number;
            r.width = width;
        }
        let mut p = Program::new();
        p.x86_instruction(i);
        assert!(assemble_program_object_with_features(&p, avx()).is_err());
    }
    let mut i = Instruction::parse_intel("vaddsd xmm0,xmm1,[r12+r13]").unwrap();
    if let Operand::Memory(m) = &mut i.operands[2] {
        m.scale = 3;
    }
    let mut p = Program::new();
    p.x86_instruction(i);
    assert!(assemble_program_object_with_features(&p, avx()).is_err());
}

fn differential_cases() -> Vec<String> {
    let mut cases = Vec::new();
    for (op, width) in [
        ("vaddss", 32),
        ("vaddsd", 64),
        ("vsubss", 32),
        ("vsubsd", 64),
        ("vmulss", 32),
        ("vmulsd", 64),
        ("vdivss", 32),
        ("vdivsd", 64),
        ("vsqrtss", 32),
        ("vsqrtsd", 64),
        ("vminss", 32),
        ("vminsd", 64),
        ("vmaxss", 32),
        ("vmaxsd", 64),
        ("vandps", 128),
        ("vandpd", 128),
        ("vorps", 128),
        ("vorpd", 128),
        ("vxorps", 128),
        ("vxorpd", 128),
        ("vroundss", 32),
        ("vroundsd", 64),
    ] {
        let imm = if op.starts_with("vround") { ",11" } else { "" };
        // All register numbers in every field; independent src/dst and alias cases.
        for d in 0..16 {
            for a in 0..16 {
                for b in [0, 7, 8, 15, d, a] {
                    cases.push(format!("{op} xmm{d},xmm{a},xmm{b}{imm}"));
                }
            }
        }
        let size = match width {
            32 => "dword",
            64 => "qword",
            _ => "xmmword",
        };
        for address in [
            "rax",
            "rsp",
            "rbp",
            "r8",
            "r12",
            "r13",
            "r12+8*r15-128",
            "r13+4*r8+128",
            "2*r9+129",
            "rax+2*r12-129",
            "rip+external+7",
            "rip+external@GOTPCREL",
            "rip+local",
            "0x12345678",
        ] {
            for d in [0, 8, 15] {
                cases.push(format!("{op} xmm{d},xmm15,{size} ptr [{address}]{imm}"));
            }
        }
    }
    for (op, size, scalar) in [
        ("vmovss", "dword", true),
        ("vmovsd", "qword", true),
        ("vmovaps", "xmmword", false),
        ("vmovups", "xmmword", false),
        ("vmovapd", "xmmword", false),
        ("vmovupd", "xmmword", false),
    ] {
        for d in 0..16 {
            for a in 0..16 {
                if scalar {
                    for b in 0..16 {
                        cases.push(format!("{op} xmm{d},xmm{a},xmm{b}"));
                    }
                } else {
                    cases.push(format!("{op} xmm{d},xmm{a}"));
                }
            }
            for address in ["rax", "r12+8*r15-16", "rip+external+9", "rip+local"] {
                cases.push(format!("{op} xmm{d},{size} ptr [{address}]"));
                cases.push(format!("{op} {size} ptr [{address}],xmm{d}"));
            }
        }
    }
    for (op, size) in [("vucomiss", "dword"), ("vucomisd", "qword")] {
        for d in 0..16 {
            for b in 0..16 {
                cases.push(format!("{op} xmm{d},xmm{b}"));
            }
            for address in ["r12+8*r15-16", "rip+external+9", "rip+local"] {
                cases.push(format!("{op} xmm{d},{size} ptr [{address}]"));
            }
        }
    }
    cases.extend(
        [
            "vmovss xmm15,dword ptr fs:[r12+8*r9+17]",
            "vaddsd xmm8,xmm15,qword ptr gs:[r13]",
            "vzeroupper",
        ]
        .map(str::to_owned),
    );
    cases
}

#[test]
fn avx_matrix_matches_clang_and_gnu() {
    let cases = differential_cases();
    assert!(cases.len() > 45_000);
    let source = format!(
        ".intel_syntax noprefix\n.text\n{}\nlocal: .zero 16\n",
        cases.join("\n")
    );
    let ours = assemble(&source);
    let mut program = Program::new();
    for case in &cases {
        program.x86_instruction(Instruction::parse_intel(case).unwrap());
    }
    program.label("local");
    program.directive(keel_asm_ir::Directive::Space(16));
    assert_eq!(program.stats().text_bytes, 0);
    assert_eq!(
        ours,
        assemble_program_object_with_features(&program, avx()).unwrap()
    );
    for case in &cases {
        let instruction = Instruction::parse_intel(case).unwrap();
        assert_eq!(instruction.opcode.required_features(), avx());
        assert_eq!(
            Instruction::parse_intel(&instruction.to_string()).unwrap(),
            instruction
        );
    }
    let temp = std::env::temp_dir().join(format!("keel-avx-diff-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("matrix.s");
    std::fs::write(&input, &source).unwrap();
    let mut tools = vec![(
        "clang".to_string(),
        vec!["-target", "x86_64-unknown-linux-gnu", "-c"],
    )];
    // Linux requires GNU as. Other hosts may supply a cross-assembler explicitly.
    if let Some(gnu) = std::env::var_os("KEEL_TEST_GNU_AS") {
        tools.push((gnu.to_str().unwrap().to_owned(), vec!["--64"]));
    } else if cfg!(target_os = "linux") {
        tools.push(("as".into(), vec!["--64"]));
    }
    for (tool, flags) in tools {
        // GNU as retains the written bitwise order and uses load-direction moves
        // by default; LLVM and keel choose VEX2 where possible. Request the same
        // equivalent encoding explicitly, without changing FP arithmetic order.
        if tool != "clang" {
            let normalized = cases
                .iter()
                .map(|case| {
                    use keel_asm_ir::x86::{Opcode::*, Operand::Register};
                    let mut i = Instruction::parse_intel(case).unwrap();
                    let register = |n: usize| match i.operands.get(n) {
                        Some(Register(r)) => Some(r.number),
                        _ => None,
                    };
                    let short_move = matches!(
                        i.opcode,
                        Vmovaps | Vmovups | Vmovapd | Vmovupd | Vmovss | Vmovsd
                    ) && register(0).is_some_and(|r| r < 8)
                        && register(i.operands.len() - 1).is_some_and(|r| r >= 8);
                    if matches!(i.opcode, Vandps | Vandpd | Vorps | Vorpd | Vxorps | Vxorpd)
                        && register(1).is_some_and(|r| r < 8)
                        && register(2).is_some_and(|r| r >= 8)
                    {
                        i.operands.swap(1, 2);
                    }
                    format!("{}{i}", if short_move { "{store} " } else { "" })
                })
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(
                &input,
                format!(".intel_syntax noprefix\n.text\n{normalized}\nlocal: .zero 16\n"),
            )
            .unwrap();
        }
        let output = temp.join("reference.o");
        let result = Command::new(&tool)
            .args(flags)
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{tool}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let reference = std::fs::read(output).unwrap();
        let (a, b) = (text(&ours), text(&reference));
        if a != b {
            let offset = a
                .iter()
                .zip(&b)
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            let mut start = 0;
            for case in &cases {
                let length = text(&assemble(case)).len();
                if offset < start + length {
                    panic!(
                        "{tool}: byte {offset} in {case}; ours {:?}, reference {:?}",
                        &a[offset.saturating_sub(4)..(offset + 8).min(a.len())],
                        &b[offset.saturating_sub(4)..(offset + 8).min(b.len())]
                    );
                }
                start += length;
            }
            panic!("{tool}: size differs: {} vs {}", a.len(), b.len());
        }
        assert_eq!(relocations(&ours), relocations(&reference), "{tool}");
    }
    std::fs::remove_dir_all(temp).unwrap();
}

fn compile_fixture(source: &str, opt: bool, fast: bool, features: Features) -> (String, Program) {
    use keel_codegen::{
        CodegenPipeline, SelectionTier, Target, regalloc::AllocationTier,
        x86_features::FeatureSelector,
    };
    let mut module = keel_ir::parse(source).unwrap();
    if opt {
        keel_opt::optimize(&mut module).unwrap();
    }
    let selector = FeatureSelector {
        inner: if opt {
            &SelectionTier::Full
        } else {
            &SelectionTier::Baseline
        },
        features,
    };
    let pipeline = CodegenPipeline {
        selector: &selector,
        allocator: if fast {
            &AllocationTier::Fast
        } else {
            &AllocationTier::Full
        },
    };
    (
        keel_codegen::compile_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline).unwrap(),
        keel_codegen::compile_program_with_pipeline(&module, Target::X86_64LinuxGnu, &pipeline)
            .unwrap(),
    )
}

#[test]
fn feature_enabled_backend_fixtures_have_identical_typed_and_text_objects() {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    let mut count = 0;
    for path in std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|s| s == "keel"))
    {
        let source = std::fs::read_to_string(&path).unwrap();
        if source
            .lines()
            .find_map(|s| s.strip_prefix("; target: "))
            .is_some_and(|s| s != "x86_64-unknown-linux-gnu")
        {
            continue;
        }
        for (opt, fast) in [(false, false), (true, false), (true, true)] {
            let (text, program) = compile_fixture(&source, opt, fast, avx());
            assert_eq!(program.stats().text_bytes, 0, "{}", path.display());
            assert_eq!(
                assemble(&text),
                assemble_program_object_with_features(&program, avx()).unwrap(),
                "{}",
                path.display()
            );
            count += 1;
        }
    }
    assert!(count > 100, "{count}");
}

#[test]
fn independent_destinations_remove_scalar_copies_and_baseline_stays_legacy() {
    let source = include_str!("fixtures/x86_avx_arithmetic.keel");
    let (baseline, _) = compile_fixture(source, false, false, Features::default());
    let (enabled, program) = compile_fixture(source, false, false, avx());
    assert!(baseline.contains("subsd"));
    assert!(!baseline.contains("vsubsd"));
    let arithmetic: Vec<_> = program
        .items
        .iter()
        .filter_map(|item| {
            if let keel_asm_ir::Item::X86Instruction(i) = item {
                Some(i)
            } else {
                None
            }
        })
        .filter(|i| i.opcode == keel_asm_ir::x86::Opcode::Vsubsd)
        .collect();
    assert_eq!(arithmetic.len(), 1, "{enabled}");
    assert_eq!(arithmetic[0].operands.len(), 3);
    assert!(
        enabled.matches("movaps").count() < baseline.matches("movaps").count(),
        "baseline:\n{baseline}\nAVX:\n{enabled}"
    );
    assert!(!enabled.contains("vzeroupper"));
    assert!(enabled.contains("vaddss") && enabled.contains("vsqrtss"));
    assert!(enabled.contains("vdivsd") && enabled.contains("vsqrtsd"));
    assert_eq!(
        assemble(&enabled),
        assemble_program_object_with_features(&program, avx()).unwrap()
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn avx_float_fixture_executes_with_nan_signed_zero_conversions_and_calls() {
    if !std::is_x86_feature_detected!("avx") {
        return;
    }
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../keel-codegen/tests/programs");
    let source = std::fs::read_to_string(dir.join("floats.keel")).unwrap();
    let temp = std::env::temp_dir().join(format!("keel-avx-floats-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    for (opt, fast) in [(false, false), (true, false), (true, true)] {
        let (text, program) = compile_fixture(&source, opt, fast, avx());
        assert!(text.contains("vaddsd") && text.contains("vsqrtss"));
        let object = temp.join("floats.o");
        std::fs::write(
            &object,
            assemble_program_object_with_features(&program, avx()).unwrap(),
        )
        .unwrap();
        let exe = temp.join("floats");
        let result = Command::new("clang")
            .arg(&object)
            .arg(dir.join("floats.c"))
            .args(["-lm", "-o"])
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(&exe).output().unwrap();
        assert!(
            result.status.success(),
            "opt={opt} fast={fast}: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn avx_xmm_merge_load_zeroing_and_high_registers_execute() {
    if !std::is_x86_feature_detected!("avx") {
        return;
    }
    let source = r#"
.intel_syntax noprefix
.text
.globl probe
.type probe,@function
probe:
vmovups xmm8,[rsi]
vmovups xmm9,[rsi+16]
mov r8,rsi
mov r9,8
vmovups xmm10,[r8+4*r9]
vmovss xmm11,xmm8,xmm9
vmovups [rdi],xmm11
vmovsd xmm12,xmm8,xmm9
vmovups [rdi+16],xmm12
vmovss xmm13,[rsi]
vmovups [rdi+32],xmm13
vxorps xmm14,xmm8,xmm9
vmovups [rdi+48],xmm14
vroundss xmm15,xmm8,[rip+round_input],11
vmovups [rdi+64],xmm15
vsqrtss xmm10,xmm8,xmm10
vmovups [rdi+80],xmm10
vucomiss xmm8,xmm9
setp al
movzx eax,al
vzeroupper
ret
.size probe,.-probe
.section .rodata
round_input: .long 0xc0700000
.section .note.GNU-stack,"",@progbits
"#;
    let driver = r#"
#include <stdint.h>
extern int probe(uint32_t *, const uint32_t *);
int main(void) {
    uint32_t in[12] = {0x7fc01234,0x12345678,0x87654321,0x11223344,
                       0x3f800000,0x55667788,0x99887766,0xaabbccdd,
                       0x41100000,0,0,0};
    uint32_t out[24] = {0};
    if (probe(out,in) != 1) return 1;
    for (int j=0;j<4;j++) {
        if (out[j] != (j==0 ? in[4] : in[j])) return 2;
        if (out[4+j] != (j<2 ? in[4+j] : in[j])) return 3;
        if (out[8+j] != (j==0 ? in[0] : 0)) return 4;
        if (out[12+j] != (in[j]^in[4+j])) return 5;
        if (out[16+j] != (j==0 ? 0xc0400000 : in[j])) return 6;
        if (out[20+j] != (j==0 ? 0x40400000 : in[j])) return 7;
    }
    return 0;
}
"#;
    let temp = std::env::temp_dir().join(format!("keel-avx-probe-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let input = temp.join("probe.s");
    let c = temp.join("driver.c");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&c, driver).unwrap();
    let reference = temp.join("clang.o");
    let result = Command::new("clang")
        .arg("-c")
        .arg(&input)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let ours = temp.join("ours.o");
    std::fs::write(&ours, assemble(source)).unwrap();
    for (name, obj) in [("ours", ours), ("clang", reference)] {
        let exe = temp.join(name);
        let result = Command::new("clang")
            .arg(obj)
            .arg(&c)
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            Command::new(exe).status().unwrap().code(),
            Some(0),
            "{name}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
