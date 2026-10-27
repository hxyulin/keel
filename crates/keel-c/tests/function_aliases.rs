//! Function aliases preserve symbol identity without substituting target bodies.
use keel_ir::{Module, Target};
use object::{Object, ObjectSection, ObjectSymbol};
use std::{fs, process::Command};

const SOURCE: &str = r#"
int replacement(int);
int invoke(int x) { return replacement(x); }
int replacement(int) __attribute__((weak, alias("implementation")));
static int implementation(int x) { return x + 7; }
int hidden_alias(int) __attribute__((alias("implementation"), visibility("hidden")));
int chained(int) __attribute__((alias("hidden_alias")));
int (*saved)(int) = replacement;
int renamed(int) __asm__("renamed_symbol") __attribute__((alias("implementation")));
int forward(int) __attribute__((weak, alias("later_symbol")));
int later(int x) __asm__("later_symbol");
int later(int x) { return x + 11; }
"#;

fn compile(source: &str, target: Target) -> Result<Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "aliases.c",
        keel_c::Options {
            target,
            debug_info: false,
        },
    )
}

fn object(source: &str, optimize: bool, text: bool) -> Vec<u8> {
    let mut module = compile(source, Target::X86_64LinuxGnu).unwrap();
    if optimize {
        keel_opt::optimize(&mut module).unwrap();
    }
    keel_ir::verify_module(&module).unwrap();
    let reparsed = keel_ir::parse(&module.to_string()).unwrap();
    if text {
        let asm = keel_codegen::compile(&reparsed, Target::X86_64LinuxGnu).unwrap();
        keel_asm::x86::assemble_object(&asm).unwrap()
    } else {
        let program = keel_codegen::compile_program(&reparsed, Target::X86_64LinuxGnu).unwrap();
        keel_asm::x86::assemble_program_object(&program).unwrap()
    }
}

#[test]
fn aliases_match_clang_symbols_and_preserve_direct_call_binding() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("function-aliases");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("aliases.c"), SOURCE).unwrap();
    let result = Command::new("clang")
        .args(["--target=x86_64-unknown-linux-gnu", "-O2", "-c"])
        .arg(dir.join("aliases.c"))
        .arg("-o")
        .arg(dir.join("clang.o"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let reference = fs::read(dir.join("clang.o")).unwrap();
    let reference = object::File::parse(reference.as_slice()).unwrap();
    for optimize in [false, true] {
        let bytes = object(SOURCE, optimize, false);
        assert_eq!(bytes, object(SOURCE, optimize, true));
        let file = object::File::parse(bytes.as_slice()).unwrap();
        let symbol = |name| file.symbols().find(|s| s.name().unwrap() == name).unwrap();
        for (name, target) in [
            ("replacement", "implementation"),
            ("hidden_alias", "implementation"),
            ("chained", "implementation"),
            ("renamed_symbol", "implementation"),
            ("forward", "later_symbol"),
        ] {
            let s = symbol(name);
            let t = symbol(target);
            let r = reference
                .symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap();
            assert!(s.is_definition(), "{name}");
            assert_eq!(s.kind(), object::SymbolKind::Text, "{name}");
            assert_eq!(
                (s.section_index(), s.address(), s.size()),
                (t.section_index(), t.address(), t.size()),
                "{name}"
            );
            assert_eq!((s.is_weak(), s.scope()), (r.is_weak(), r.scope()), "{name}");
        }
        let alias = symbol("replacement").index();
        assert!(
            file.sections()
                .flat_map(|s| s.relocations())
                .any(|(_, r)| r.target() == object::RelocationTarget::Symbol(alias)),
            "calls/addresses retain replaceable alias"
        );
        let start = keel_asm::x86::assemble_object(".text\n.globl _start\n_start:\nret\n").unwrap();
        let replacement = object("int replacement(int x) { return x + 99; }", optimize, false);
        let inputs = [&start, &bytes, &replacement]
            .into_iter()
            .map(|bytes| keel_link::Input {
                name: "aliases.o",
                bytes,
            })
            .collect::<Vec<_>>();
        let executable = keel_link::elf::link(&inputs, Default::default()).unwrap();
        let linked = object::File::parse(executable.bytes.as_slice()).unwrap();
        let address = |name| {
            linked
                .symbols()
                .find(|s| s.name().unwrap() == name)
                .unwrap()
                .address()
        };
        assert_ne!(address("replacement"), address("hidden_alias"));
        let offset = (address("saved") - 0x400000) as usize;
        assert_eq!(
            u64::from_le_bytes(executable.bytes[offset..offset + 8].try_into().unwrap()),
            address("replacement")
        );
    }
}

#[test]
fn aliases_validate_whole_unit_types_contracts_and_definitions() {
    for (source, expected) in [
        (
            "int a(void) __attribute__((alias(\"missing\")));",
            "not defined",
        ),
        (
            "int t(void); int a(void) __attribute__((alias(\"t\")));",
            "function definition",
        ),
        (
            "int t; int a(void) __attribute__((alias(\"t\")));",
            "compatible function types",
        ),
        (
            "int t(int x) {return x;} long a(int) __attribute__((alias(\"t\")));",
            "compatible function types",
        ),
        (
            "int t(int x) {return x;} int a(long) __attribute__((alias(\"t\")));",
            "compatible function types",
        ),
        (
            "int t(int x) {return x;} int a(int) __attribute__((alias(\"t\"),noreturn));",
            "function contracts",
        ),
        (
            "int t(void) {return 1;} int a(void) __attribute__((alias(\"t\"))); long a(void);",
            "incompatible types",
        ),
        (
            "int a(void) __attribute__((alias(\"b\"))); int b(void) __attribute__((alias(\"a\")));",
            "cycle",
        ),
        ("int a(void) __attribute__((alias(\"a\")));", "cycle"),
        (
            "int t(void){return 1;} int a(void) __attribute__((alias(\"t\"))) {return 2;}",
            "function body",
        ),
        (
            "int t(void){return 1;} int a(void) __attribute__((alias(\"t\"),alias(\"t\")));",
            "duplicate alias",
        ),
        (
            "int t(void){return 1;} int a(void) __attribute__((alias(\"t\"))); int a(void) __attribute__((alias(\"t\")));",
            "duplicate alias",
        ),
        (
            "int t(void){return 1;} int a(void) __attribute__((alias(\"t\"),section(\".text.x\")));",
            "section attributes",
        ),
        (
            "int t(void){return 1;} int a(void) __asm__(\"t\") __attribute__((alias(\"t\")));",
            "assembler name conflicts",
        ),
        (
            "int t(void){return 1;} int a(void) __asm__(\"a\\n.globl injected\") __attribute__((alias(\"t\")));",
            "plain assembler",
        ),
        (
            "int t; int a __attribute__((alias(\"t\")));",
            "file-scope function",
        ),
        (
            "typedef int A __attribute__((alias(\"t\")));",
            "file-scope function",
        ),
        (
            "int f(void) { int a(void) __attribute__((alias(\"f\"))); return 0; }",
            "file-scope function",
        ),
        (
            "int t(void){return 1;} static int a(void) __attribute__((alias(\"t\"),weak));",
            "weak requires external",
        ),
    ] {
        let error = compile(source, Target::X86_64LinuxGnu).unwrap_err();
        assert!(
            error.message.contains(expected),
            "{source}: expected {expected:?}, got {error}"
        );
    }
    for target in [
        Target::Aarch64LinuxGnu,
        Target::Aarch64AppleDarwin,
        Target::Wasm32UnknownUnknown,
    ] {
        let error = compile("int a(void) __attribute__((alias(\"target\")));", target).unwrap_err();
        assert!(
            error
                .message
                .contains("function aliases require x86-64 Linux"),
            "{target:?}: {error}"
        );
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn aliases_execute_with_and_without_a_strong_override() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("function-alias-runtime");
    fs::create_dir_all(&dir).unwrap();
    for optimize in [false, true] {
        fs::write(dir.join("aliases.o"), object(SOURCE, optimize, false)).unwrap();
        for override_weak in [false, true] {
            let replacement = if override_weak {
                "int replacement(int x) { return x + 99; }"
            } else {
                ""
            };
            let expected = if override_weak { 102 } else { 10 };
            let harness = format!(
                r#"
extern int invoke(int), hidden_alias(int), chained(int), renamed_symbol(int), forward(int);
extern int (*saved)(int);
{replacement}
int main(void) {{
    /* Keep the linker's alias identity observable. Clang can fold direct
       comparisons between separately declared function names. */
    int (*volatile hidden_pointer)(int) = hidden_alias;
    int (*volatile chained_pointer)(int) = chained;
    int (*volatile renamed_pointer)(int) = renamed_symbol;
    return invoke(3) != {expected} || saved(3) != {expected}
        || hidden_alias(3) != 10 || chained(3) != 10 || renamed_symbol(3) != 10
        || forward(3) != 14 || hidden_pointer != chained_pointer
        || hidden_pointer != renamed_pointer;
}}
"#
            );
            fs::write(dir.join("main.c"), harness).unwrap();
            for compiler in ["gcc", "clang"] {
                let output = Command::new(compiler)
                    .arg(dir.join("aliases.o"))
                    .arg(dir.join("main.c"))
                    .arg("-o")
                    .arg(dir.join("run"))
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{compiler}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(
                    Command::new(dir.join("run")).status().unwrap().success(),
                    "{compiler}: optimized={optimize}, override={override_weak}"
                );
            }
        }
    }
}
