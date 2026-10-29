use keel_asm::x86::{Features, ProgramAssembler, assemble_object, assemble_program_object};
use keel_asm_ir::Program;
use object::{Object, ObjectSection, ObjectSymbol};
use std::{fs, process::Command};

fn sections(bytes: &[u8]) -> Vec<(String, u64, Vec<u8>)> {
    let file = object::File::parse(bytes).unwrap();
    [".text", ".data", ".bss"]
        .into_iter()
        .filter_map(|name| {
            let s = file.section_by_name(name)?;
            Some((name.into(), s.size(), s.data().unwrap().to_vec()))
        })
        .collect()
}

fn compare(source: &str, clang: bool) {
    let source = format!(".att_syntax prefix\n.text\n{source}\n.data\n.byte 7\n.bss\n.skip 1\n");
    let ours = assemble_object(&source).unwrap_or_else(|e| panic!("{source}: {e}"));
    let mut program = Program::new();
    program.text(&source);
    assert_eq!(ours, assemble_program_object(&program).unwrap());
    let dir = std::env::temp_dir().join(format!(
        "keel-fill-aliases-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.s"), &source).unwrap();
    let gnu = std::env::var("KEEL_GNU_AS").unwrap_or_else(|_| "as".into());
    for (tool, args) in [
        (gnu.as_str(), vec!["--64"]),
        ("clang", vec!["-target", "x86_64-unknown-linux-gnu", "-c"]),
    ] {
        if tool == "clang" && !clang {
            continue;
        }
        let output = dir.join("reference.o");
        let run = Command::new(tool)
            .args(args)
            .arg(dir.join("input.s"))
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{tool}: {}\n{source}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert_eq!(
            sections(&ours),
            sections(&fs::read(output).unwrap()),
            "{tool}: {source}"
        );
    }
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn fill_patterns_defaults_and_deferred_kernel_entries_match_gnu() {
    compare(
        r#"
.fill 3
.fill 2,2,0x12345678
.fill 2,3,-1
.fill 1,8,0x1122334455667788
.fill 1,9,-1
.fill -3,1,4
.fill 3,-1,4
.fill 0,1,7
array:
i = 0
.rept 3
.byte 0xaa
i = i + 1
.fill array + i*9 - ., 1, 0xcc
.endr
done:
.long done-array
.bss
.fill 5,4,0
.text
"#,
        false,
    );
    compare(".fill 3,2,0x1234\n.fill 2,1,0x7f\n", true);
    let bytes = assemble_object(".fill 1,8,-1").unwrap();
    assert_eq!(sections(&bytes)[0].2, [0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
}

#[test]
fn register_aliases_expand_in_instruction_operands_only() {
    let source = r#"
.set key, %rsi
.set output, %rdi
.set copy, %xmm6
.set byte, %al
.set chain, key
movups 0x10(key),copy
movq chain,output
movb $10,byte
.ascii "key copy"
key_label:
ret
"#;
    compare(source, true);
    assert_eq!(
        assemble_object(&format!(".att_syntax prefix\n{source}")).unwrap(),
        keel_asm::x86::assemble_object_with_syntax_and_features(
            source,
            keel_asm::x86::Syntax::Att,
            Features::NONE
        )
        .unwrap()
    );
    let expected = assemble_object(".att_syntax prefix\nmovups 0x10(%rsi),%xmm6\nmovq %rsi,%rdi\nmovb $10,%al\n.ascii \"key copy\"\nkey_label:\nret\n").unwrap();
    let actual = assemble_object(&format!(".att_syntax prefix\n{source}")).unwrap();
    assert_eq!(sections(&actual), sections(&expected));
    let file = object::File::parse(actual.as_slice()).unwrap();
    assert!(
        !file
            .symbols()
            .any(|s| matches!(s.name().unwrap(), "key" | "copy" | "chain"))
    );
    // GNU supports Intel register symbols; LLVM treats these as relocatable expressions.
    compare(
        ".intel_syntax noprefix\n.set key,rsi\nmov rax,key\n.set key,%rdi\nmov rax,key\n",
        false,
    );
    let mut chunks = ProgramAssembler::new(Features::NONE);
    let mut definition = Program::new();
    definition.text(".att_syntax prefix\n.set key,%rsi\n");
    chunks.push(definition);
    let mut use_ = Program::new();
    use_.text("movq key,%rax\n");
    chunks.push(use_);
    assert_eq!(
        chunks.finish().unwrap(),
        assemble_object(".att_syntax prefix\nmovq %rsi,%rax\n").unwrap()
    );
}

#[test]
fn fill_and_alias_errors_remain_explicit() {
    for source in [
        ".fill 67108865,1,0",
        ".fill 33554433,2,0",
        ".fill 3,2,unknown",
        ".bss\n.fill 1,2,1",
        ".fill missing,1,0",
    ] {
        assert!(assemble_object(source).is_err(), "{source}");
    }
    assert!(assemble_object(".set key,7\n.set key,%rsi\nmovq key,%rax\n").is_err());
    assert!(assemble_object(".set key,%rsi\n.globl key\n").is_err());
}
