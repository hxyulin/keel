//! Kernel spelling/operand witnesses. Optional instructions are assembled, never executed.
#[path = "x86_kernel_adapters/x86_remaining_kernel_adapters.rs"]
mod x86_remaining_kernel_adapters;
use keel_asm::x86::assemble_object_with_syntax_and_features;
use keel_asm_ir::x86::{CpuLevel, Features, Instruction, Register, Syntax};
use object::{Object, ObjectSection};
use std::{fs, process::Command};

fn text(bytes: &[u8]) -> Vec<u8> {
    object::File::parse(bytes)
        .unwrap()
        .section_by_name(".text")
        .unwrap()
        .data()
        .unwrap()
        .to_vec()
}

fn assemble(source: &str, syntax: Syntax, features: Features) -> Vec<u8> {
    text(
        &assemble_object_with_syntax_and_features(source, syntax, features)
            .unwrap_or_else(|error| panic!("{source}: {error}")),
    )
}

#[test]
fn literal_kernel_spellings_use_generated_forms() {
    for (source, expected) in [
        ("rdtscp", &[0x0f, 0x01, 0xf9][..]),
        ("monitor", &[0x0f, 0x01, 0xc8]),
        ("mwait", &[0x0f, 0x01, 0xc9]),
        ("rdgsbase %r9", &[0xf3, 0x49, 0x0f, 0xae, 0xc9]),
        ("rdrand %r9w", &[0x66, 0x41, 0x0f, 0xc7, 0xf1]),
        ("fxrstorq (%r12)", &[0x49, 0x0f, 0xae, 0x0c, 0x24]),
        ("prefetchw (%rcx)", &[0x0f, 0x0d, 0x09]),
        (
            "invpcid (%r12),%r9",
            &[0x66, 0x45, 0x0f, 0x38, 0x82, 0x0c, 0x24],
        ),
        ("andb $-129,3(%r12)", &[0x41, 0x80, 0x64, 0x24, 0x03, 0x7f]),
        ("movl %eax,%ds", &[0x8e, 0xd8]),
    ] {
        assert_eq!(
            assemble(source, Syntax::Att, Features::all_known()),
            expected,
            "{source}"
        );
    }
}

#[test]
fn optional_kernel_capabilities_are_independent_of_cpu_profiles() {
    for (source, feature) in [
        ("rdtscp", "rdtscp"),
        ("monitor", "monitor"),
        ("mwait", "monitor"),
        ("rdgsbase %r9", "fsgsbase"),
        ("rdrand %r9w", "rdrand"),
        ("prefetchw (%rcx)", "prefetchw"),
        ("invpcid (%r12),%r9", "invpcid"),
    ] {
        for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
            let features = Features::for_level(level);
            assert!(
                assemble_object_with_syntax_and_features(source, Syntax::Att, features).is_err(),
                "{source} {level:?}"
            );
            assert!(
                assemble_object_with_syntax_and_features(
                    source,
                    Syntax::Att,
                    features.with_features(feature).unwrap()
                )
                .is_ok(),
                "{source} {feature}"
            );
        }
    }
    for n in 0..8 {
        assert_eq!(
            Register::parse(&format!("DB{n}")),
            Register::parse(&format!("dr{n}"))
        );
    }
    assert!(Register::parse("db00").is_none());
    assert!(Register::parse("db8").is_none());
    for source in [
        "andb $-257,(%rax)",
        "andb $256,(%rax)",
        "rdrand %al",
        "rdgsbase %ax",
        "invpcid %rax,%rcx",
        "movl %eax,%cs",
    ] {
        assert!(
            assemble_object_with_syntax_and_features(source, Syntax::Att, Features::all_known())
                .is_err(),
            "{source}"
        );
    }
}

#[test]
fn kernel_operand_matrix_matches_clang_and_gnu() {
    let mut cases = vec![
        "rdtscp".to_owned(),
        "monitor".to_owned(),
        "mwait".to_owned(),
        "prefetchw 13(%r12,%r9,4)".to_owned(),
        "fxrstorq 32(%r13)".to_owned(),
        "fxsaveq (%r12)".to_owned(),
    ];
    for reg in ["eax", "r8d", "r15d", "rax", "r8", "r15"] {
        for op in ["rdfsbase", "rdgsbase", "wrfsbase", "wrgsbase"] {
            cases.push(format!("{op} %{reg}"));
        }
    }
    for reg in [
        "ax", "r8w", "r15w", "eax", "r8d", "r15d", "rax", "r8", "r15",
    ] {
        cases.push(format!("rdrand %{reg}"));
    }
    for reg in ["rax", "r8", "r15"] {
        cases.push(format!("invpcid 32(%r12,%r9,4),%{reg}"));
    }
    for n in 0..8 {
        cases.push(format!("movq %db{n},%r9"));
        cases.push(format!("movq %r9,%db{n}"));
    }
    for value in [-256, -255, -129, -128, -1, 0, 127, 128, 255] {
        cases.push(format!("andb ${value},3(%r12)"));
    }
    for reg in ["eax", "r8d", "r15d"] {
        for segment in ["es", "ss", "ds", "fs", "gs"] {
            cases.push(format!("movl %{reg},%{segment}"));
        }
    }
    let dir = std::env::temp_dir().join(format!("keel-kernel-adapters-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("matrix.s");
    let source = format!(".text\n{}\n", cases.join("\n"));
    fs::write(&input, &source).unwrap();
    let ours = assemble(&source, Syntax::Att, Features::all_known());
    for (name, args) in [
        ("clang", vec!["-c", "-target", "x86_64-unknown-linux-gnu"]),
        ("gnu", vec!["--64"]),
    ] {
        let output = dir.join(format!("{name}.o"));
        let command = if name == "clang" {
            "clang".into()
        } else {
            std::env::var_os("KEEL_GNU_AS").unwrap_or_else(|| "as".into())
        };
        let result = Command::new(command)
            .args(args)
            .arg(&input)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(ours, text(&fs::read(output).unwrap()), "{name}");
    }
    for source in &cases {
        let parsed = Instruction::parse_att(source).unwrap();
        assert_eq!(
            assemble(source, Syntax::Att, Features::all_known()),
            assemble(&parsed.to_string(), Syntax::Intel, Features::all_known()),
            "{source}"
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn monitor_forms_keep_implicit_operands_and_long_mode_contract() {
    let features = Features::for_level(CpuLevel::V1)
        .with_features("monitor")
        .unwrap();
    for (source, expected) in [
        ("monitor", &[0x0f, 0x01, 0xc8][..]),
        ("mwait", &[0x0f, 0x01, 0xc9][..]),
    ] {
        for syntax in [Syntax::Att, Syntax::Intel] {
            assert_eq!(assemble(source, syntax, features), expected);
        }
        let form = &keel_asm_ir::x86::isa::forms(source)[0];
        assert_eq!(form.cpl, Some(0));
        for invalid in [
            format!("{source} %eax"),
            format!(".code32\n{source}"),
            format!(".code16\n{source}"),
        ] {
            assert!(
                assemble_object_with_syntax_and_features(&invalid, Syntax::Att, features).is_err(),
                "{invalid}"
            );
        }
    }
}
