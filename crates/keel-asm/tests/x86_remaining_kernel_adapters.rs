//! Encoding-only witnesses; these instructions are never executed by the tests.
use keel_asm::x86::assemble_object_with_syntax_and_features;
use keel_asm_ir::x86::{CpuLevel, Features, Instruction, Syntax};
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
            .unwrap_or_else(|e| panic!("{source}: {e}")),
    )
}

#[test]
fn literal_sysretl_rdseed_and_tpause_forms() {
    for (source, bytes) in [
        ("sysretl", &[0x0f, 0x07][..]),
        ("tpause %r9d", &[0x66, 0x41, 0x0f, 0xae, 0xf1]),
        ("rdseed %r9w", &[0x66, 0x41, 0x0f, 0xc7, 0xf9]),
        ("rdseed %r9d", &[0x41, 0x0f, 0xc7, 0xf9]),
        ("rdseed %r9", &[0x49, 0x0f, 0xc7, 0xf9]),
    ] {
        assert_eq!(
            assemble(source, Syntax::Att, Features::all_known()),
            bytes,
            "{source}"
        );
    }
}

#[test]
fn optional_capabilities_and_operand_rules_remain_explicit() {
    for level in [CpuLevel::V1, CpuLevel::V2, CpuLevel::V3] {
        let features = Features::for_level(level);
        assert_eq!(assemble("sysretl", Syntax::Att, features), [0x0f, 0x07]);
        for (source, feature) in [("rdseed %r9", "rdseed"), ("tpause %r9d", "waitpkg")] {
            assert!(
                assemble_object_with_syntax_and_features(source, Syntax::Att, features).is_err()
            );
            let enabled = features.with_features(feature).unwrap();
            assert!(assemble_object_with_syntax_and_features(source, Syntax::Att, enabled).is_ok());
        }
    }
    for source in [
        "sysretl %eax",
        "rdseed %al",
        "rdseed (%rax)",
        "tpause %rax",
        "tpause %ax",
        "tpause (%rax)",
        "tpausel %eax",
        "rdrandw %ax",
        "rdrandl %eax",
        "rdrandq %rax",
        "rdseedw %ax",
        "rdseedl %eax",
        "rdseedq %rax",
    ] {
        assert!(
            assemble_object_with_syntax_and_features(source, Syntax::Att, Features::all_known())
                .is_err(),
            "{source}"
        );
    }
}

#[test]
fn kernel_exit_wait_and_seed_matrix_matches_clang_and_gnu() {
    let mut cases = vec![
        "sysret".to_owned(),
        "sysretl".to_owned(),
        "sysretq".to_owned(),
    ];
    for reg in ["eax", "edi", "r8d", "r15d"] {
        cases.push(format!("tpause %{reg}"));
    }
    for registers in [
        ["ax", "di", "r8w", "r15w"],
        ["eax", "edi", "r8d", "r15d"],
        ["rax", "rdi", "r8", "r15"],
    ] {
        for reg in registers {
            cases.push(format!("rdseed %{reg}"));
        }
    }
    let dir = std::env::temp_dir().join(format!(
        "keel-remaining-kernel-adapters-{}",
        std::process::id()
    ));
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
