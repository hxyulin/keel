use keel_ir::Target;
use std::{fs, process::Command};

const SOURCE: &str = r#"
unsigned long labeled_value(unsigned long input) {
    return ({ unsigned long value = input + 1; out: value; });
}
long goto_value(int skip, long *seen) {
    return ({
        long value = 3;
        if (skip) goto out;
        value += 7;
    out: last: (*seen += value), value++;
    });
}
int *labeled_pointer(int *pointer) { return ({ out: pointer; }); }
long nested_value(long input) {
    return ({ out: ({ long value = input + 2; inner: value; }); });
}
long trailing_null(long input) {
    return ({ long value = input + 4; out: value; ; ; });
}
void void_tails(void) {
    _Static_assert(__builtin_types_compatible_p(__typeof__(({ out: { 7; } })), void), "compound");
    _Static_assert(__builtin_types_compatible_p(__typeof__(({ empty: ; })), void), "null");
    _Static_assert(__builtin_types_compatible_p(__typeof__(({ 7; final: ; })), void), "labeled null");
    _Static_assert(__builtin_types_compatible_p(__typeof__(({ declared: int value; })), void), "declaration");
}
"#;

fn compile(
    source: &str,
    target: Target,
    debug_info: bool,
) -> Result<keel_ir::Module, keel_c::Diagnostic> {
    keel_c::compile(
        source,
        "statement-expression-labels.c",
        keel_c::Options { target, debug_info },
    )
}

#[test]
fn labeled_expressions_keep_their_types_and_goto_side_effects() {
    for target in Target::NATIVE {
        for debug_info in [false, true] {
            let module =
                compile(SOURCE, target, debug_info).unwrap_or_else(|e| panic!("{target:?}: {e}"));
            for optimize in [false, true] {
                let mut module = module.clone();
                if optimize {
                    keel_opt::optimize(&mut module).unwrap();
                }
                keel_ir::verify_module(&module).unwrap();
                let program = keel_codegen::compile_program(&module, target).unwrap();
                let object = match target {
                    Target::Aarch64AppleDarwin => {
                        keel_asm::assemble_program_object(&program, Default::default()).unwrap()
                    }
                    Target::Aarch64LinuxGnu => {
                        keel_asm::assemble_elf_program_object(&program, Default::default()).unwrap()
                    }
                    Target::X86_64LinuxGnu => {
                        keel_asm::x86::assemble_program_object(&program).unwrap()
                    }
                    Target::Wasm32UnknownUnknown => unreachable!("native-only test"),
                };
                if Target::host() == Some(target) {
                    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
                        "statement-expression-labels-debug{debug_info}-opt{optimize}"
                    ));
                    fs::create_dir_all(&dir).unwrap();
                    fs::write(dir.join("labels.o"), object).unwrap();
                    fs::write(
                        dir.join("main.c"),
                        r#"
extern unsigned long labeled_value(unsigned long);
extern long goto_value(int, long *);
extern int *labeled_pointer(int *);
extern long nested_value(long);
extern long trailing_null(long);
int main(void) {
    for (int input = -64; input <= 64; ++input) {
        unsigned long value = (unsigned long)input;
        if (labeled_value(value) != value + 1) return 1;
        long seen = input * 3;
        long expected = (input & 1) ? 3 : 10;
        if (goto_value(input & 1, &seen) != expected || seen != input * 3 + expected) return 2;
        if (labeled_pointer(&input) != &input) return 3;
        if (nested_value(input) != input + 2) return 4;
        if (trailing_null(input) != input + 4) return 5;
    }
    return 0;
}
"#,
                    )
                    .unwrap();
                    let executable = dir.join("run");
                    let output = Command::new("clang")
                        .arg(dir.join("labels.o"))
                        .arg(dir.join("main.c"))
                        .arg("-o")
                        .arg(&executable)
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    assert!(
                        Command::new(executable).status().unwrap().success(),
                        "debug={debug_info}, optimize={optimize}"
                    );
                }
            }
        }
    }
}

#[test]
fn labels_do_not_turn_compound_or_null_statements_into_values() {
    for tail in ["out: { 7; }", "out: ;", "7; out: ;", "out: int value;"] {
        let source = format!("int f(void) {{ return ({{ {tail} }}); }}");
        let error = compile(&source, Target::X86_64LinuxGnu, false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("incompatible types"), "{source}: {error}");
    }
}
